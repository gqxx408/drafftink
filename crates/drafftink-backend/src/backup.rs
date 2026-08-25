//! # 自动备份
//!
//! 每日在指定时间备份 sled 数据库和资源存储目录，保留最近 7 个备份。

use std::path::{Path, PathBuf};

use chrono::Duration;
use chrono::{Datelike, Local, NaiveTime, Timelike};
use tracing::{error, info};

/// 启动备份后台任务
///
/// 每日在 `backup_hour` 时备份 `db_path` 和 `storage_path` 到 `backup_path`。
/// 保留最近 7 个备份，自动删除更早的备份。
pub fn start_backup_task(
    db_path: PathBuf,
    storage_path: PathBuf,
    backup_path: PathBuf,
    backup_hour: u32,
) {
    tokio::spawn(async move {
        loop {
            // 计算到下次备份的等待时间
            let wait = seconds_until_next_backup(backup_hour);
            info!("下次备份将在 {} 秒后执行", wait);
            tokio::time::sleep(std::time::Duration::from_secs(wait)).await;

            // 在阻塞线程中执行备份（涉及大量文件 I/O）
            let db_path = db_path.clone();
            let storage_path = storage_path.clone();
            let backup_path_clone = backup_path.clone();

            let result = tokio::task::spawn_blocking(move || {
                perform_backup(&db_path, &storage_path, &backup_path_clone)
            })
            .await;

            match result {
                Ok(Ok(())) => info!("备份完成"),
                Ok(Err(e)) => error!("备份失败: {e}"),
                Err(e) => error!("备份任务异常: {e}"),
            }

            // 清理旧备份
            if let Err(e) = cleanup_old_backups(&backup_path) {
                error!("清理旧备份失败: {e}");
            }
        }
    });
}

/// 计算到下次备份时间的秒数
fn seconds_until_next_backup(backup_hour: u32) -> u64 {
    let now = Local::now();
    let target_time = NaiveTime::from_hms_opt(backup_hour.min(23), 0, 0)
        .unwrap_or_else(|| NaiveTime::from_hms_opt(0, 0, 0).unwrap_or(NaiveTime::MIN));

    let today_target = now
        .date_naive()
        .and_time(target_time)
        .and_local_timezone(Local)
        .single();

    let next = match today_target {
        Some(dt) if dt > now => dt,
        _ => {
            // 今天的时间已过，计算明天
            let tomorrow = now.date_naive() + Duration::days(1);
            tomorrow
                .and_time(target_time)
                .and_local_timezone(Local)
                .single()
                .unwrap_or(now + Duration::hours(24))
        }
    };

    let diff = next - now;
    diff.num_seconds().max(1) as u64
}

/// 执行一次备份
fn perform_backup(db_path: &Path, storage_path: &Path, backup_path: &Path) -> anyhow::Result<()> {
    let now = Local::now();
    let timestamp = format!(
        "{:04}{:02}{:02}_{:02}{:02}{:02}",
        now.year(),
        now.month(),
        now.day(),
        now.hour(),
        now.minute(),
        now.second()
    );
    let backup_dir = backup_path.join(format!("backup_{timestamp}"));

    std::fs::create_dir_all(&backup_dir)?;

    // 备份数据库
    if db_path.exists() {
        let db_backup = backup_dir.join("db");
        copy_dir_recursive(db_path, &db_backup)?;
        info!("数据库已备份到 {}", db_backup.display());
    }

    // 备份存储
    if storage_path.exists() {
        let storage_backup = backup_dir.join("storage");
        copy_dir_recursive(storage_path, &storage_backup)?;
        info!("存储已备份到 {}", storage_backup.display());
    }

    Ok(())
}

/// 递归复制目录
fn copy_dir_recursive(src: &Path, dst: &Path) -> std::io::Result<()> {
    std::fs::create_dir_all(dst)?;

    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let path = entry.path();
        let file_type = entry.file_type()?;

        let dest = dst.join(entry.file_name());

        if file_type.is_dir() {
            copy_dir_recursive(&path, &dest)?;
        } else if file_type.is_file() {
            std::fs::copy(&path, &dest)?;
        }
        // 忽略符号链接等其他类型
    }

    Ok(())
}

/// 清理旧备份，保留最近 7 个
fn cleanup_old_backups(backup_path: &Path) -> anyhow::Result<()> {
    if !backup_path.exists() {
        return Ok(());
    }

    let mut backups: Vec<PathBuf> = std::fs::read_dir(backup_path)?
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.file_name()
                .to_str()
                .map(|s| s.starts_with("backup_"))
                .unwrap_or(false)
        })
        .map(|e| e.path())
        .collect();

    // 按名称排序（名称包含时间戳，字典序 = 时间序）
    backups.sort();

    // 保留最后 7 个，删除其余
    let keep_count = 7;
    if backups.len() > keep_count {
        let to_delete = &backups[..backups.len() - keep_count];
        for dir in to_delete {
            if let Err(e) = std::fs::remove_dir_all(dir) {
                error!("删除旧备份失败 {}: {e}", dir.display());
            } else {
                info!("已删除旧备份: {}", dir.display());
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_seconds_until_next_backup_returns_positive() {
        let secs = seconds_until_next_backup(2);
        assert!(secs > 0);
        // 最多不超过 24 小时 = 86400 秒
        assert!(secs <= 86400);
    }

    #[test]
    fn test_copy_dir_recursive() {
        let src = std::env::temp_dir().join(format!(
            "drafftink_test_backup_src_{}",
            uuid::Uuid::new_v4()
        ));
        let dst = std::env::temp_dir().join(format!(
            "drafftink_test_backup_dst_{}",
            uuid::Uuid::new_v4()
        ));

        std::fs::create_dir_all(src.join("subdir")).unwrap();
        std::fs::write(src.join("a.txt"), b"hello").unwrap();
        std::fs::write(src.join("subdir/b.txt"), b"world").unwrap();

        copy_dir_recursive(&src, &dst).unwrap();

        assert!(dst.join("a.txt").exists());
        assert!(dst.join("subdir/b.txt").exists());

        let _ = std::fs::remove_dir_all(&src);
        let _ = std::fs::remove_dir_all(&dst);
    }

    #[test]
    fn test_cleanup_keeps_last_7() {
        let backup_dir = std::env::temp_dir().join(format!(
            "drafftink_test_backup_cleanup_{}",
            uuid::Uuid::new_v4()
        ));
        std::fs::create_dir_all(&backup_dir).unwrap();

        // 创建 10 个备份目录
        for i in 0..10 {
            let dir = backup_dir.join(format!("backup_2026010{i}_020000"));
            std::fs::create_dir_all(&dir).unwrap();
            std::fs::write(dir.join("data.txt"), b"backup").unwrap();
        }

        cleanup_old_backups(&backup_dir).unwrap();

        let remaining = std::fs::read_dir(&backup_dir)
            .unwrap()
            .filter(|e| {
                e.as_ref()
                    .map(|e| {
                        e.file_name()
                            .to_str()
                            .map(|s| s.starts_with("backup_"))
                            .unwrap_or(false)
                    })
                    .unwrap_or(false)
            })
            .count();

        assert_eq!(remaining, 7);

        let _ = std::fs::remove_dir_all(&backup_dir);
    }
}
