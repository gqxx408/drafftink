//! # drftx — 防篡改作业文件格式
//!
//! 三层架构：快照层 → 签名层 → 批注层，提交即冻结，学生无法二次编辑。
//!
//! ## 文件布局
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │ Header  (8 bytes)                           │
//! │   magic: "DRFT" (4B)                        │
//! │   major_version: u16 (2B)                   │
//! │   minor_version: u16 (2B)                   │
//! ├─────────────────────────────────────────────┤
//! │ Snapshot Layer                              │
//! │   snapshot_len: u32 (4B)                    │
//! │   snapshot_data: [u8; snapshot_len]         │
//! │   (bincode 编码的 ExerciseSnapshot)          │
//! ├─────────────────────────────────────────────┤
//! │ Signature Layer                             │
//! │   signature_len: u32 (4B)                   │
//! │   signature_data: [u8; signature_len]       │
//! │   (bincode 编码的 ExerciseSignature)         │
//! ├─────────────────────────────────────────────┤
//! │ Annotation Layer (可选)                     │
//! │   annotation_len: u32 (4B)                  │
//! │   annotation_data: [u8; annotation_len]     │
//! │   (bincode 编码的 TeacherAnnotation)         │
//! ├─────────────────────────────────────────────┤
//! │ CRC32 校验尾 (4B)                           │
//! └─────────────────────────────────────────────┘
//! ```

use anyhow::{anyhow, bail, Result};
use chrono::{DateTime, Utc};
use crc32fast::Hasher as CrcHasher;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use uuid::Uuid;

use crate::emgi::EmgiDataset;
use crate::recording::RecordingMetadata;

// ════════════════════════════════════════════════════════════════════════════
//  常量
// ════════════════════════════════════════════════════════════════════════════

/// 文件魔数
pub const DRFTX_MAGIC: [u8; 4] = *b"DRFT";

/// 主版本号
pub const DRFTX_MAJOR_VERSION: u16 = 1;

/// 次版本号（1：引入 EMGI 合规元数据区，见 [`EmgiDataset`]）
pub const DRFTX_MINOR_VERSION: u16 = 1;

/// 文件头大小 (magic 4B + major 2B + minor 2B)
const HEADER_SIZE: usize = 8;

/// 单层最大大小 (16 MB)，防止恶意超大文件
const MAX_LAYER_SIZE: usize = 16 * 1024 * 1024;

// ════════════════════════════════════════════════════════════════════════════
//  数据结构
// ════════════════════════════════════════════════════════════════════════════

/// 快照层 — 学生提交时冻结的作业内容，提交后不可修改。
///
/// 包含作业 ID、学生 ID、作答数据、提交时间戳和内容哈希。
/// `content_hash` 是 `answer_data` 的 SHA-256，用于签名验证。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExerciseSnapshot {
    /// 作业唯一 ID
    pub homework_id: Uuid,
    /// 学生唯一 ID
    pub student_id: Uuid,
    /// 作答数据（bincode 编码的答案内容）
    pub answer_data: Vec<u8>,
    /// 提交时间戳（UTC）
    pub submitted_at: DateTime<Utc>,
    /// answer_data 的 SHA-256 哈希（32 字节）
    pub content_hash: [u8; 32],
}

impl ExerciseSnapshot {
    /// 创建新的快照，自动计算 content_hash。
    pub fn new(homework_id: Uuid, student_id: Uuid, answer_data: Vec<u8>) -> Self {
        let content_hash = sha256_bytes(&answer_data);
        Self {
            homework_id,
            student_id,
            answer_data,
            submitted_at: Utc::now(),
            content_hash,
        }
    }

    /// 验证 content_hash 与 answer_data 是否一致。
    pub fn verify_hash(&self) -> bool {
        let computed = sha256_bytes(&self.answer_data);
        computed == self.content_hash
    }
}

/// 签名层 — 学生用 Ed25519 私钥对快照哈希的数字签名。
///
/// 提交时生成，验证后证明作业确实由该学生提交且未被篡改。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct ExerciseSignature {
    /// 签名算法标识（0 = Ed25519）
    pub algorithm: u8,
    /// 学生公钥（32 字节）
    pub public_key: [u8; 32],
    /// Ed25519 签名（64 字节，对 content_hash 签名）
    pub signature: Vec<u8>,
    /// 签名时间戳（UTC）
    pub signed_at: DateTime<Utc>,
}

/// 批注层 — 老师批改时添加的评注，不修改快照层。
///
/// 包含分数、评语、批注数据（墨迹/几何修正）和可选的教师签名。
/// 写入批注层不会影响快照层的完整性。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Default)]
pub struct TeacherAnnotation {
    /// 教师唯一 ID
    pub teacher_id: Uuid,
    /// 分数（0-100，可选）
    pub score: Option<f32>,
    /// 文字评语
    pub comments: String,
    /// 批注数据（bincode 编码的墨迹/标注）
    pub annotation_data: Vec<u8>,
    /// 批注时间戳（UTC）
    pub annotated_at: DateTime<Utc>,
    /// 教师签名（可选，对批注内容的 Ed25519 签名）
    pub teacher_signature: Option<Vec<u8>>,
}

/// 完整的 drftx 文件 — 包含所有三层数据，以及可选的 EMGI 合规元数据。
///
/// `emgi` 字段承载符合 JY/T 1002-2012 的教育管理基础信息（学校/学生/教职工等），
/// 作为文件头中的元数据区写入（见 `to_bytes`/`from_bytes`），与既有三层格式兼容。
#[derive(Debug, Clone, PartialEq)]
pub struct DrftxFile {
    /// 快照层（必需）
    pub snapshot: ExerciseSnapshot,
    /// 签名层（必需）
    pub signature: ExerciseSignature,
    /// 批注层（可选，未批改时为 None）
    pub annotation: Option<TeacherAnnotation>,
    /// EMGI 合规元数据（可选，符合 JY/T 1002-2012），作为文件头元数据区。
    pub emgi: Option<EmgiDataset>,
    /// 录播 BERM 元数据（可选，符合 DB34/T 2318-2015），作为文件头元数据区。
    pub recording: Option<RecordingMetadata>,
}

// ════════════════════════════════════════════════════════════════════════════
//  序列化 / 反序列化
// ════════════════════════════════════════════════════════════════════════════

impl DrftxFile {
    /// 序列化为字节流（含文件头、三层、CRC32）。
    pub fn to_bytes(&self) -> Result<Vec<u8>> {
        let mut buf = Vec::with_capacity(256);

        // ── 文件头 ──
        buf.extend_from_slice(&DRFTX_MAGIC);
        buf.extend_from_slice(&DRFTX_MAJOR_VERSION.to_le_bytes());
        buf.extend_from_slice(&DRFTX_MINOR_VERSION.to_le_bytes());

        // ── 快照层 ──
        let snapshot_bytes =
            bincode::serialize(&self.snapshot).map_err(|e| anyhow!("快照序列化失败: {e}"))?;
        write_layer(&mut buf, &snapshot_bytes)?;

        // ── 签名层 ──
        let sig_bytes =
            bincode::serialize(&self.signature).map_err(|e| anyhow!("签名序列化失败: {e}"))?;
        write_layer(&mut buf, &sig_bytes)?;

        // ── EMGI 合规元数据区（可选）──
        // 用一个 1 字节的 meta_flags 明确标识后续可选层的有无，避免歧义：
        //   bit0 = 含批注层；bit1 = 含 emgi 层；bit2 = 含录播 BERM 层。
        //   顺序为 emgi 层 → 批注层 → 录播层。
        let has_ann = self.annotation.is_some();
        let has_emgi = self.emgi.is_some();
        let has_rec = self.recording.is_some();
        let meta_flags: u8 =
            u8::from(has_ann) | (u8::from(has_emgi) << 1) | (u8::from(has_rec) << 2);
        buf.push(meta_flags);

        if let Some(ref emgi) = self.emgi {
            let emgi_bytes =
                bincode::serialize(emgi).map_err(|e| anyhow!("EMGI 序列化失败: {e}"))?;
            write_layer(&mut buf, &emgi_bytes)?;
        }

        // ── 批注层 ──
        let annotation_bytes = if let Some(ref ann) = self.annotation {
            bincode::serialize(ann).map_err(|e| anyhow!("批注序列化失败: {e}"))?
        } else {
            Vec::new()
        };
        write_layer(&mut buf, &annotation_bytes)?;

        // ── 录播 BERM 层（可选，符合 DB34/T 2318-2015）──
        if let Some(ref rec) = self.recording {
            let rec_bytes =
                bincode::serialize(rec).map_err(|e| anyhow!("录播元数据序列化失败: {e}"))?;
            write_layer(&mut buf, &rec_bytes)?;
        }

        // ── CRC32 校验尾 ──
        let mut crc = CrcHasher::new();
        crc.update(&buf);
        let crc_val = crc.finalize();
        buf.extend_from_slice(&crc_val.to_le_bytes());

        Ok(buf)
    }

    /// 从字节流反序列化，验证魔数、版本、CRC32 和签名。
    ///
    /// # 参数
    /// - `verify_signature` — 是否验证 Ed25519 签名
    pub fn from_bytes(data: &[u8], verify_signature: bool) -> Result<Self> {
        if data.len() < HEADER_SIZE + 4 {
            bail!("文件过短: {} 字节（最少 {}）", data.len(), HEADER_SIZE + 4);
        }

        // ── 校验魔数 ──
        let magic = &data[..4];
        if magic != DRFTX_MAGIC {
            bail!("无效魔数: 期望 {DRFTX_MAGIC:?}, 实际 {magic:?}");
        }

        // ── 解析版本 ──
        let major = u16::from_le_bytes([data[4], data[5]]);
        let minor = u16::from_le_bytes([data[6], data[7]]);
        if major != DRFTX_MAJOR_VERSION {
            bail!("不支持的版本: {major}.{minor}（当前支持 {DRFTX_MAJOR_VERSION}.{DRFTX_MINOR_VERSION}）");
        }

        // ── 校验 CRC32 ──
        let crc_stored = u32::from_le_bytes([
            data[data.len() - 4],
            data[data.len() - 3],
            data[data.len() - 2],
            data[data.len() - 1],
        ]);
        let mut crc = CrcHasher::new();
        crc.update(&data[..data.len() - 4]);
        let crc_computed = crc.finalize();
        if crc_stored != crc_computed {
            bail!("CRC32 校验失败: 文件可能已损坏或被篡改 (期望 {crc_stored:#010x}, 实际 {crc_computed:#010x})");
        }

        // ── 解析三层 ──
        let mut offset = HEADER_SIZE;
        let (snapshot, snapshot_end) = read_layer(data, offset, "快照")?;
        offset = snapshot_end;

        let (signature, sig_end) = read_layer(data, offset, "签名")?;
        offset = sig_end;

        // ── 元数据区（可选）──
        // 读取 1 字节 meta_flags：bit0 = 批注层，bit1 = emgi 层，bit2 = 录播 BERM 层。
        if offset >= data.len() - 4 {
            bail!("文件在签名层之后缺少元数据标志位");
        }
        let meta_flags = data[offset];
        offset += 1;
        let has_emgi = meta_flags & 0b10 != 0;
        let has_ann = meta_flags & 0b01 != 0;
        let has_rec = meta_flags & 0b100 != 0;

        let mut emgi: Option<EmgiDataset> = None;

        let emgi_start = if has_emgi {
            let (emgi_bytes, next) = read_layer(data, offset, "EMGI")?;
            emgi = Some(bincode_decode(&emgi_bytes, "EMGI")?);
            next
        } else {
            offset
        };

        let annotation = if has_ann {
            let (ann, _) = read_layer(data, emgi_start, "批注")?;
            Some(ann)
        } else {
            None
        };

        let rec_start = if has_ann {
            // annotation 已消费 emgi_start 之后的层，其结束偏移需重新计算
            let (_, next) = read_layer(data, emgi_start, "批注")?;
            next
        } else {
            emgi_start
        };

        let recording = if has_rec {
            let (rec_bytes, _) = read_layer(data, rec_start, "录播")?;
            Some(bincode_decode(&rec_bytes, "录播")?)
        } else {
            None
        };

        let snapshot: ExerciseSnapshot = bincode_decode(&snapshot, "快照")?;
        let signature: ExerciseSignature = bincode_decode(&signature, "签名")?;

        // ── 验证快照哈希 ──
        if !snapshot.verify_hash() {
            bail!("快照内容哈希不匹配: answer_data 可能被篡改");
        }

        // ── 验证签名 ──
        if verify_signature {
            verify_exercise_signature(&snapshot, &signature)?;
        }

        let annotation: Option<TeacherAnnotation> = if let Some(ann_bytes) = annotation {
            Some(bincode_decode(&ann_bytes, "批注")?)
        } else {
            None
        };

        Ok(Self {
            snapshot,
            signature,
            annotation,
            emgi,
            recording,
        })
    }

    /// 添加教师批注（不修改快照层）。
    ///
    /// 返回新的 DrftxFile，原文件不受影响。
    pub fn with_annotation(mut self, annotation: TeacherAnnotation) -> Self {
        self.annotation = Some(annotation);
        self
    }

    /// 附加符合 JY/T 1002-2012 的 EMGI 合规元数据（作为文件头元数据）。
    ///
    /// 返回新的 DrftxFile，原文件不受影响。
    pub fn with_emgi(mut self, emgi: EmgiDataset) -> Self {
        self.emgi = Some(emgi);
        self
    }

    /// 附加符合 DB34/T 2318-2015 的录播 BERM 元数据（作为文件头元数据）。
    ///
    /// 返回新的 DrftxFile，原文件不受影响。
    pub fn with_recording(mut self, recording: RecordingMetadata) -> Self {
        self.recording = Some(recording);
        self
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  签名验证
// ════════════════════════════════════════════════════════════════════════════

/// 验证 Ed25519 签名是否匹配快照内容。
fn verify_exercise_signature(
    snapshot: &ExerciseSnapshot,
    signature: &ExerciseSignature,
) -> Result<()> {
    use ed25519_dalek::{Verifier, VerifyingKey};

    if signature.algorithm != 0 {
        bail!(
            "不支持的签名算法: {}（仅支持 Ed25519 = 0）",
            signature.algorithm
        );
    }

    let verifying_key =
        VerifyingKey::from_bytes(&signature.public_key).map_err(|e| anyhow!("无效公钥: {e}"))?;

    let sig = ed25519_dalek::Signature::try_from(signature.signature.as_slice())
        .map_err(|e| anyhow!("无效签名字节: {e}"))?;

    match verifying_key.verify(&snapshot.content_hash, &sig) {
        Ok(()) => Ok(()),
        Err(_) => bail!("签名验证失败: 作业可能被篡改或签名不匹配"),
    }
}

/// 用学生私钥对快照签名，生成 ExerciseSignature。
pub fn sign_snapshot(
    snapshot: &ExerciseSnapshot,
    private_key: &[u8; 32],
) -> Result<ExerciseSignature> {
    use ed25519_dalek::Signer;

    let signing_key = ed25519_dalek::SigningKey::from_bytes(private_key);
    let verifying_key = signing_key.verifying_key();
    let sig = signing_key.sign(&snapshot.content_hash);

    Ok(ExerciseSignature {
        algorithm: 0,
        public_key: verifying_key.to_bytes(),
        signature: sig.to_bytes().to_vec(),
        signed_at: Utc::now(),
    })
}

// ════════════════════════════════════════════════════════════════════════════
//  辅助函数
// ════════════════════════════════════════════════════════════════════════════

/// 计算数据的 SHA-256 哈希。
fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// 写入一层：4 字节长度 + 数据。
fn write_layer(buf: &mut Vec<u8>, data: &[u8]) -> Result<()> {
    if data.len() > MAX_LAYER_SIZE {
        bail!("层数据过大: {} 字节（上限 {}）", data.len(), MAX_LAYER_SIZE);
    }
    buf.extend_from_slice(&(data.len() as u32).to_le_bytes());
    buf.extend_from_slice(data);
    Ok(())
}

/// 读取一层：4 字节长度 + 数据，返回 (数据, 结束偏移)。
fn read_layer(data: &[u8], offset: usize, name: &str) -> Result<(Vec<u8>, usize)> {
    if offset + 4 > data.len() {
        bail!("{name}层长度字段越界");
    }
    let len = u32::from_le_bytes([
        data[offset],
        data[offset + 1],
        data[offset + 2],
        data[offset + 3],
    ]) as usize;
    if len > MAX_LAYER_SIZE {
        bail!("{name}层数据过大: {len} 字节（上限 {MAX_LAYER_SIZE}）");
    }
    let start = offset + 4;
    let end = start + len;
    if end > data.len() {
        bail!("{name}层数据越界: 需要 {end} 字节，仅有 {}", data.len());
    }
    Ok((data[start..end].to_vec(), end))
}

/// bincode 反序列化辅助。
fn bincode_decode<'de, T: Deserialize<'de>>(data: &'de [u8], name: &str) -> Result<T> {
    bincode::deserialize(data).map_err(|e| anyhow!("{name}反序列化失败: {e}"))
}

// ════════════════════════════════════════════════════════════════════════════
//  单元测试
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    /// 生成测试用密钥对
    fn test_keypair() -> ([u8; 32], [u8; 32]) {
        crate::plugin::signing::generate_keypair()
    }

    #[test]
    fn test_snapshot_creation_and_hash() {
        let hw_id = Uuid::new_v4();
        let stu_id = Uuid::new_v4();
        let answer = b"student answer data".to_vec();

        let snapshot = ExerciseSnapshot::new(hw_id, stu_id, answer.clone());

        assert_eq!(snapshot.homework_id, hw_id);
        assert_eq!(snapshot.student_id, stu_id);
        assert_eq!(snapshot.answer_data, answer);
        assert!(snapshot.verify_hash());
    }

    #[test]
    fn test_snapshot_tamper_detection() {
        let snapshot =
            ExerciseSnapshot::new(Uuid::new_v4(), Uuid::new_v4(), b"original answer".to_vec());

        // 篡改 answer_data 但不更新 hash
        let mut tampered = snapshot.clone();
        tampered.answer_data = b"tampered answer!!!".to_vec();

        assert!(!tampered.verify_hash());
        assert!(snapshot.verify_hash());
    }

    #[test]
    fn test_full_roundtrip_with_signature() {
        let (sk, _pk) = test_keypair();
        let hw_id = Uuid::new_v4();
        let stu_id = Uuid::new_v4();

        let snapshot = ExerciseSnapshot::new(hw_id, stu_id, b"my homework answer".to_vec());
        let signature = sign_snapshot(&snapshot, &sk).unwrap();

        let file = DrftxFile {
            snapshot,
            signature,
            annotation: None,
            emgi: None,
            recording: None,
        };

        let bytes = file.to_bytes().unwrap();
        let restored = DrftxFile::from_bytes(&bytes, true).unwrap();

        assert_eq!(file, restored);
    }

    #[test]
    fn test_tampered_bytes_rejected() {
        let (sk, _) = test_keypair();
        let snapshot = ExerciseSnapshot::new(Uuid::new_v4(), Uuid::new_v4(), b"answer".to_vec());
        let signature = sign_snapshot(&snapshot, &sk).unwrap();

        let file = DrftxFile {
            snapshot,
            signature,
            annotation: None,
            emgi: None,
            recording: None,
        };

        let mut bytes = file.to_bytes().unwrap();

        // 篡改一个字节（在快照数据区域）
        if bytes.len() > 20 {
            bytes[15] ^= 0xFF;
        }

        let result = DrftxFile::from_bytes(&bytes, true);
        assert!(result.is_err(), "篡改后的文件应被拒绝");
    }

    #[test]
    fn test_wrong_key_rejected() {
        let (sk1, _) = test_keypair();
        let (_, _pk2) = test_keypair();

        let snapshot = ExerciseSnapshot::new(Uuid::new_v4(), Uuid::new_v4(), b"answer".to_vec());
        let signature = sign_snapshot(&snapshot, &sk1).unwrap();

        let file = DrftxFile {
            snapshot,
            signature,
            annotation: None,
            emgi: None,
            recording: None,
        };

        let bytes = file.to_bytes().unwrap();

        // 篡改签名中的公钥
        let mut tampered = bytes.clone();
        // 找到签名层中的公钥位置并篡改
        // 签名层在快照层之后，公钥是前 32 字节
        let snapshot_len_start = HEADER_SIZE;
        let snapshot_len = u32::from_le_bytes([
            tampered[snapshot_len_start],
            tampered[snapshot_len_start + 1],
            tampered[snapshot_len_start + 2],
            tampered[snapshot_len_start + 3],
        ]) as usize;
        let sig_data_start = HEADER_SIZE + 4 + snapshot_len + 4;
        if sig_data_start + 32 < tampered.len() {
            tampered[sig_data_start] ^= 0x01;
        }

        // 重新计算 CRC32
        let crc_offset = tampered.len() - 4;
        let mut crc = CrcHasher::new();
        crc.update(&tampered[..crc_offset]);
        let crc_val = crc.finalize();
        tampered[crc_offset..].copy_from_slice(&crc_val.to_le_bytes());

        let result = DrftxFile::from_bytes(&tampered, true);
        assert!(result.is_err(), "使用错误公钥的签名应被拒绝");
    }

    #[test]
    fn test_with_annotation() {
        let (sk, _) = test_keypair();
        let snapshot = ExerciseSnapshot::new(Uuid::new_v4(), Uuid::new_v4(), b"answer".to_vec());
        let signature = sign_snapshot(&snapshot, &sk).unwrap();

        let annotation = TeacherAnnotation {
            teacher_id: Uuid::new_v4(),
            score: Some(95.0),
            comments: "做得很好！".to_string(),
            annotation_data: vec![1, 2, 3],
            annotated_at: Utc::now(),
            teacher_signature: None,
        };

        let file = DrftxFile {
            snapshot,
            signature,
            annotation: None,
            emgi: None,
            recording: None,
        }
        .with_annotation(annotation);

        let bytes = file.to_bytes().unwrap();
        let restored = DrftxFile::from_bytes(&bytes, true).unwrap();

        assert!(restored.annotation.is_some());
        assert_eq!(restored.annotation.unwrap().score, Some(95.0));
    }

    #[test]
    fn test_annotation_does_not_affect_snapshot() {
        let (sk, _) = test_keypair();
        let snapshot = ExerciseSnapshot::new(Uuid::new_v4(), Uuid::new_v4(), b"answer".to_vec());
        let signature = sign_snapshot(&snapshot, &sk).unwrap();

        // 创建无批注的文件
        let file_no_ann = DrftxFile {
            snapshot: snapshot.clone(),
            signature: signature.clone(),
            annotation: None,
            emgi: None,
            recording: None,
        };
        let bytes_no_ann = file_no_ann.to_bytes().unwrap();

        // 添加批注
        let file_with_ann = DrftxFile {
            snapshot: snapshot.clone(),
            signature: signature.clone(),
            annotation: Some(TeacherAnnotation {
                teacher_id: Uuid::new_v4(),
                score: Some(80.0),
                comments: "不错".to_string(),
                annotation_data: vec![],
                annotated_at: Utc::now(),
                teacher_signature: None,
            }),
            emgi: None,
            recording: None,
        };
        let bytes_with_ann = file_with_ann.to_bytes().unwrap();

        // 快照层应该完全相同
        let restored_no_ann = DrftxFile::from_bytes(&bytes_no_ann, true).unwrap();
        let restored_with_ann = DrftxFile::from_bytes(&bytes_with_ann, true).unwrap();

        assert_eq!(restored_no_ann.snapshot, restored_with_ann.snapshot);
        assert_eq!(restored_no_ann.signature, restored_with_ann.signature);
    }

    #[test]
    fn test_invalid_magic_rejected() {
        let (sk, _) = test_keypair();
        let snapshot = ExerciseSnapshot::new(Uuid::new_v4(), Uuid::new_v4(), b"x".to_vec());
        let signature = sign_snapshot(&snapshot, &sk).unwrap();
        let file = DrftxFile {
            snapshot,
            signature,
            annotation: None,
            emgi: None,
            recording: None,
        };

        let mut bytes = file.to_bytes().unwrap();
        bytes[0] = b'X'; // 破坏魔数

        let result = DrftxFile::from_bytes(&bytes, false);
        assert!(result.is_err());
    }

    #[test]
    fn test_empty_answer_data() {
        let (sk, _) = test_keypair();
        let snapshot = ExerciseSnapshot::new(Uuid::new_v4(), Uuid::new_v4(), Vec::new());
        let signature = sign_snapshot(&snapshot, &sk).unwrap();

        let file = DrftxFile {
            snapshot,
            signature,
            annotation: None,
            emgi: None,
            recording: None,
        };
        let bytes = file.to_bytes().unwrap();
        let restored = DrftxFile::from_bytes(&bytes, true).unwrap();

        assert!(restored.snapshot.answer_data.is_empty());
        assert!(restored.snapshot.verify_hash());
    }

    #[test]
    fn test_emgi_metadata_roundtrip() {
        use crate::emgi::{EmgiDataset, SchoolBasic};

        let (sk, _) = test_keypair();
        let snapshot = ExerciseSnapshot::new(Uuid::new_v4(), Uuid::new_v4(), b"answer".to_vec());
        let signature = sign_snapshot(&snapshot, &sk).unwrap();

        let mut emgi = EmgiDataset::new();
        emgi.with_record(&SchoolBasic {
            school_id: Some("S1101080001".into()),
            school_name: Some("示范学校".into()),
            school_nature: Some("3".into()),
            ..Default::default()
        });

        let file = DrftxFile {
            snapshot,
            signature,
            annotation: None,
            emgi: Some(emgi),
            recording: None,
        };

        let bytes = file.to_bytes().unwrap();
        let restored = DrftxFile::from_bytes(&bytes, true).unwrap();

        let restored_emgi = restored.emgi.expect("EMGI 元数据应随文件保留");
        assert_eq!(restored_emgi.records.len(), 1);
        assert_eq!(restored_emgi.records[0].class_id, "JCXX0101");
        assert!(restored_emgi.validate().is_empty());
    }

    #[test]
    fn test_crc_tamper_detection() {
        let (sk, _) = test_keypair();
        let snapshot = ExerciseSnapshot::new(Uuid::new_v4(), Uuid::new_v4(), b"data".to_vec());
        let signature = sign_snapshot(&snapshot, &sk).unwrap();
        let file = DrftxFile {
            snapshot,
            signature,
            annotation: None,
            emgi: None,
            recording: None,
        };

        let mut bytes = file.to_bytes().unwrap();
        // 篡改 CRC32 尾部
        let last = bytes.len() - 1;
        bytes[last] ^= 0xFF;

        let result = DrftxFile::from_bytes(&bytes, false);
        assert!(result.is_err(), "CRC32 篡改应被检测");
    }
}
