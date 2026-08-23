//! 构建脚本：把本地预编译 FFmpeg 的运行时可执行文件（ffmpeg.exe / ffprobe.exe /
//! 依赖 DLL）拷贝到构建产物旁，使老师获得零安装体验。视频解码由运行时直接调用
//! `ffmpeg.exe` 完成（见 `src/video_player.rs`），不再依赖任何 Rust FFmpeg 绑定。

use std::path::PathBuf;

fn main() {
    // 解析预编译 FFmpeg 包路径：
    //   优先采用 `.cargo/config.toml` 中的 `FFMPEG_DIR` 环境变量；
    //   否则回退到相对于本 crate 清单目录（上溯两级 = 工作区根）的路径。
    let ffmpeg_dir = std::env::var("FFMPEG_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|_| {
            let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
            manifest.join("../../third_party/ffmpeg")
        });

    let bin_dir = ffmpeg_dir.join("bin");

    // 把运行时所需文件拷贝到 target/<profile>/ 旁，与 drafftink-desktop.exe 同级，
    // 使运行时能按「与当前 exe 同级」规则找到 ffmpeg.exe。
    #[cfg(target_os = "windows")]
    {
        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
        // CARGO_MANIFEST_DIR 为 crates/drafftink-desktop；target/<profile> 在上两级。
        let target_dir = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap())
            .join("../../target")
            .join(&profile);

        // 暴露绝对目标目录，供 video_player 在运行时作为兜底路径定位 ffmpeg.exe。
        println!("cargo:rustc-env=FFMPEG_BIN_DIR={}", target_dir.display());

        if let Ok(entries) = std::fs::read_dir(&bin_dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                let ext = path.extension().and_then(|s| s.to_str()).unwrap_or("");
                // ffmpeg.exe / ffprobe.exe 本体 + 全部 DLL 依赖。
                if ext == "dll" || ext == "exe" {
                    let _ = std::fs::copy(&path, target_dir.join(entry.file_name()));
                }
            }
        }
    }

    println!("cargo:rerun-if-changed={}", bin_dir.display());
}
