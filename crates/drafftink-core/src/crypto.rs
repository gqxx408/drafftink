//! # 加密与认证模块
//!
//! 提供以下能力：
//! - **Ed25519 签名/验证** — 复用 `plugin::signing` 已有实现
//! - **JWT 生成/校验** — 纯 Rust（jsonwebtoken），绑定设备指纹
//! - **设备指纹** — 脱敏采集，SHA-256 哈希，跨平台
//! - **密钥管理** — 密钥对生成、安全存储接口

use anyhow::{anyhow, bail, Result};
use chrono::{Duration, Utc};
use ed25519_dalek::{Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use subtle::ConstantTimeEq;
use uuid::Uuid;

pub use crate::plugin::signing::{generate_keypair, hash_bytes, sign_data};

/// 配置加载错误。
///
/// 用于「缺失即拒绝启动」的安全闸门：例如 JWT 密钥未配置时，
/// 进程应在 `axum::serve` 之前显式退出，而非静默回退到默认（硬编码）密钥。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    /// JWT 密钥未配置（环境变量缺失或为空），或仍在使用已知的默认硬编码密钥。
    MissingJwtSecret,
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::MissingJwtSecret => {
                write!(
                    f,
                    "JWT secret not configured (missing/empty env var or known default secret)"
                )
            }
        }
    }
}

impl std::error::Error for ConfigError {}

// ════════════════════════════════════════════════════════════════════════════
//  JWT
// ════════════════════════════════════════════════════════════════════════════

/// JWT Claims — 包含用户身份、角色、租户、设备指纹和过期时间。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JwtClaims {
    /// 用户 ID
    pub sub: String,
    /// 用户名
    pub name: String,
    /// 角色（admin / teacher / student）
    pub role: String,
    /// 班级 ID（学生专用，老师为空）
    pub class_id: Option<String>,
    /// 设备指纹（SHA-256 哈希的十六进制字符串）
    pub device_fp: String,
    /// 租户 ID（学校 ID），用于多租户数据隔离
    #[serde(default)]
    pub tenant_id: String,
    /// Token 类型（"access" / "refresh"），用于区分访问令牌与刷新令牌
    #[serde(default)]
    pub typ: Option<String>,
    /// 签发时间（Unix 时间戳）
    pub iat: i64,
    /// 过期时间（Unix 时间戳）
    pub exp: i64,
    /// JWT 唯一 ID（用于吊销）
    pub jti: String,
}

/// JWT 配置
#[derive(Debug, Clone)]
pub struct JwtConfig {
    /// 签名密钥（HMAC-SHA256）
    pub secret: Vec<u8>,
    /// 有效期（小时）
    pub expiry_hours: i64,
    /// 签发者
    pub issuer: String,
}

impl Default for JwtConfig {
    fn default() -> Self {
        // 注意：不再提供任何硬编码密钥。`secret` 默认为空，
        // 必须由 `JwtConfig::from_env()` 或显式构造填入，且必须经过
        // `validate_not_default()` 校验，否则进程应拒绝启动。
        Self {
            secret: Vec::new(),
            expiry_hours: 24,
            issuer: "drafftink-gateway".to_string(),
        }
    }
}

impl JwtConfig {
    /// 历史上使用过的默认硬编码密钥（仅用于检测“仍在使用默认密钥”的误配置）。
    ///
    /// 这些字面量一旦出现在运行中即意味着安全漏洞，因此列入黑名单用于校验。
    pub const KNOWN_DEFAULT_SECRETS: &'static [&'static [u8]] = &[
        b"drafftink-backend-default-secret",
        b"drafftink-default-secret-change-me",
    ];

    /// 从统一环境变量 `DRAFTTINK_JWT_SECRET` 读取 JWT 密钥。
    ///
    /// 缺失或为空时返回 [`ConfigError::MissingJwtSecret`]，**绝不**静默回退到默认密钥。
    /// 网关与后端必须读取同一个环境变量，以避免信任错配。
    pub fn from_env() -> Result<Self, ConfigError> {
        match std::env::var("DRAFTTINK_JWT_SECRET") {
            Ok(s) if !s.is_empty() => Ok(Self {
                secret: s.into_bytes(),
                expiry_hours: 24,
                issuer: "drafftink-gateway".to_string(),
            }),
            _ => Err(ConfigError::MissingJwtSecret),
        }
    }

    /// 校验当前密钥不是空密钥，也不是已知的默认硬编码密钥。
    ///
    /// 应在进程启动（`axum::serve` 之前）调用，作为「拒绝启动」闸门：
    /// 返回 `Err` 时，调用方应打印明确错误并执行 `std::process::exit(1)`。
    pub fn validate_not_default(&self) -> Result<(), ConfigError> {
        if self.secret.is_empty() {
            return Err(ConfigError::MissingJwtSecret);
        }
        if Self::KNOWN_DEFAULT_SECRETS.contains(&self.secret.as_slice()) {
            return Err(ConfigError::MissingJwtSecret);
        }
        Ok(())
    }
}

/// 生成 JWT token。
///
/// # 参数
/// - `user_id` — 用户唯一 ID
/// - `name` — 用户名
/// - `role` — 角色
/// - `class_id` — 班级 ID（可选）
/// - `device_fp` — 设备指纹
/// - `config` — JWT 配置
pub fn generate_jwt(
    user_id: &str,
    name: &str,
    role: &str,
    class_id: Option<&str>,
    device_fp: &str,
    config: &JwtConfig,
) -> Result<String> {
    let now = Utc::now();
    let claims = JwtClaims {
        sub: user_id.to_string(),
        name: name.to_string(),
        role: role.to_string(),
        class_id: class_id.map(|s| s.to_string()),
        device_fp: device_fp.to_string(),
        tenant_id: String::new(),
        typ: None,
        iat: now.timestamp(),
        exp: (now + Duration::hours(config.expiry_hours)).timestamp(),
        jti: Uuid::new_v4().to_string(),
    };

    // 手动构建 JWT（Header + Payload + Signature），避免 jsonwebtoken 依赖冲突
    let header = serde_json::json!({"alg": "HS256", "typ": "JWT"});
    let header_b64 = url_safe_b64(&serde_json::to_vec(&header)?);
    let payload_b64 = url_safe_b64(&serde_json::to_vec(&claims)?);
    let signing_input = format!("{header_b64}.{payload_b64}");

    let sig = hmac_sha256(&config.secret, signing_input.as_bytes());
    let sig_b64 = url_safe_b64(&sig);

    Ok(format!("{signing_input}.{sig_b64}"))
}

/// 验证 JWT token，返回 Claims。
///
/// # 参数
/// - `token` — JWT token 字符串
/// - `config` — JWT 配置（密钥必须与签发时一致）
/// - `expected_device_fp` — 期望的设备指纹（用于绑定验证）
pub fn verify_jwt(token: &str, config: &JwtConfig, expected_device_fp: &str) -> Result<JwtClaims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        bail!("无效 JWT 格式: 期望 3 段, 实际 {} 段", parts.len());
    }

    // 验证签名
    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let expected_sig = hmac_sha256(&config.secret, signing_input.as_bytes());
    let actual_sig = url_safe_b64_decode(parts[2])?;
    // 常量时间比较，避免时序侧信道泄露签名是否「接近」正确。
    if !bool::from(expected_sig.as_slice().ct_eq(actual_sig.as_slice())) {
        bail!("JWT 签名验证失败");
    }

    // 解析 Claims
    let payload_bytes = url_safe_b64_decode(parts[1])?;
    let claims: JwtClaims =
        serde_json::from_slice(&payload_bytes).map_err(|e| anyhow!("JWT payload 解析失败: {e}"))?;

    // 验证过期时间
    let now = Utc::now().timestamp();
    if claims.exp < now {
        bail!("JWT 已过期: 过期时间 {}", claims.exp);
    }

    // 验证设备指纹
    if claims.device_fp != expected_device_fp {
        bail!(
            "设备指纹不匹配: 期望 {}, 实际 {}",
            expected_device_fp,
            claims.device_fp
        );
    }

    Ok(claims)
}

/// 不验证设备指纹的 JWT 校验（用于管理后台等场景）。
pub fn verify_jwt_unchecked(token: &str, config: &JwtConfig) -> Result<JwtClaims> {
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() != 3 {
        bail!("无效 JWT 格式: 期望 3 段, 实际 {} 段", parts.len());
    }

    let signing_input = format!("{}.{}", parts[0], parts[1]);
    let expected_sig = hmac_sha256(&config.secret, signing_input.as_bytes());
    let actual_sig = url_safe_b64_decode(parts[2])?;
    // 常量时间比较，避免时序侧信道。
    if !bool::from(expected_sig.as_slice().ct_eq(actual_sig.as_slice())) {
        bail!("JWT 签名验证失败");
    }

    let payload_bytes = url_safe_b64_decode(parts[1])?;
    let claims: JwtClaims =
        serde_json::from_slice(&payload_bytes).map_err(|e| anyhow!("JWT payload 解析失败: {e}"))?;

    let now = Utc::now().timestamp();
    if claims.exp < now {
        bail!("JWT 已过期");
    }

    Ok(claims)
}

// ════════════════════════════════════════════════════════════════════════════
//  设备指纹
// ════════════════════════════════════════════════════════════════════════════

/// 生成设备指纹。
///
/// 采集脱敏的设备信息（操作系统、架构、主机名哈希），
/// 用 SHA-256 哈希后返回十六进制字符串。
/// 不采集 MAC 地址/IMEI 等敏感信息，仅用系统特征做设备绑定。
pub fn generate_device_fingerprint() -> Result<String> {
    let mut fingerprint_input = String::new();

    // 操作系统信息（脱敏）
    fingerprint_input.push_str(std::env::consts::OS);
    fingerprint_input.push('|');
    fingerprint_input.push_str(std::env::consts::ARCH);
    fingerprint_input.push('|');

    // 主机名（哈希后使用，不直接暴露）
    if let Ok(hostname) = std::env::var("COMPUTERNAME") {
        fingerprint_input.push_str(&hostname);
    } else if let Ok(hostname) = std::env::var("HOSTNAME") {
        fingerprint_input.push_str(&hostname);
    }
    fingerprint_input.push('|');

    // 用户名（哈希后使用）
    if let Ok(user) = std::env::var("USERNAME") {
        fingerprint_input.push_str(&user);
    } else if let Ok(user) = std::env::var("USER") {
        fingerprint_input.push_str(&user);
    }
    fingerprint_input.push('|');

    // 进程 PID（增加唯一性）
    fingerprint_input.push_str(&std::process::id().to_string());

    // SHA-256 哈希
    let hash = sha256_hex(fingerprint_input.as_bytes());
    Ok(hash)
}

/// 生成持久化设备 ID（首次运行时生成，后续复用）。
///
/// 在指定目录下创建 `.device_id` 文件，存储一个 UUID。
/// 后续调用时读取并返回该 UUID。
pub fn get_or_create_device_id(storage_dir: &std::path::Path) -> Result<String> {
    let device_id_file = storage_dir.join(".device_id");

    if device_id_file.exists() {
        let content = std::fs::read_to_string(&device_id_file)
            .map_err(|e| anyhow!("读取设备 ID 失败: {e}"))?;
        let id = content.trim().to_string();
        if !id.is_empty() {
            return Ok(id);
        }
    }

    // 生成新 ID
    let new_id = Uuid::new_v4().to_string();

    // 确保目录存在
    std::fs::create_dir_all(storage_dir).map_err(|e| anyhow!("创建存储目录失败: {e}"))?;

    // 原子写入
    let tmp = device_id_file.with_extension("tmp");
    std::fs::write(&tmp, &new_id).map_err(|e| anyhow!("写入设备 ID 失败: {e}"))?;
    std::fs::rename(&tmp, &device_id_file).map_err(|e| anyhow!("重命名设备 ID 文件失败: {e}"))?;

    Ok(new_id)
}

/// 结合设备指纹和设备 ID 生成最终的设备绑定标识。
pub fn generate_device_binding(fingerprint: &str, device_id: &str) -> String {
    let combined = format!("{fingerprint}:{device_id}");
    sha256_hex(combined.as_bytes())
}

// ════════════════════════════════════════════════════════════════════════════
//  Ed25519 签名（扩展接口）
// ════════════════════════════════════════════════════════════════════════════

/// 用 Ed25519 私钥对数据签名（直接签名，不先哈希）。
///
/// 与 `plugin::signing::sign_data` 不同，此函数直接对数据签名，
/// 而非先 SHA-512 哈希再签名。适用于 drftx 等已预计算哈希的场景。
pub fn sign_raw(private_key: &[u8; 32], data: &[u8]) -> [u8; 64] {
    let signing_key = SigningKey::from_bytes(private_key);
    let sig = signing_key.sign(data);
    sig.to_bytes()
}

/// 验证 Ed25519 签名（直接验证，不先哈希）。
pub fn verify_raw(public_key: &[u8; 32], data: &[u8], signature: &[u8; 64]) -> bool {
    let verifying_key = match VerifyingKey::from_bytes(public_key) {
        Ok(k) => k,
        Err(_) => return false,
    };
    let sig = ed25519_dalek::Signature::from_bytes(signature);
    verifying_key.verify(data, &sig).is_ok()
}

// ════════════════════════════════════════════════════════════════════════════
//  内部辅助函数
// ════════════════════════════════════════════════════════════════════════════

/// HMAC-SHA256（纯 Rust 实现，无外部依赖）
fn hmac_sha256(key: &[u8], message: &[u8]) -> Vec<u8> {
    const BLOCK_SIZE: usize = 64;

    // 密钥处理：过长则哈希，过短则补零
    let processed_key = if key.len() > BLOCK_SIZE {
        sha256_bytes(key).to_vec()
    } else {
        let mut k = key.to_vec();
        k.resize(BLOCK_SIZE, 0);
        k
    };

    // ipad / opad
    let ipad: Vec<u8> = processed_key.iter().map(|b| b ^ 0x36).collect();
    let opad: Vec<u8> = processed_key.iter().map(|b| b ^ 0x5c).collect();

    // inner = H(ipad || message)
    let mut inner = Sha256::new();
    inner.update(&ipad);
    inner.update(message);
    let inner_hash = inner.finalize();

    // outer = H(opad || inner)
    let mut outer = Sha256::new();
    outer.update(&opad);
    outer.update(inner_hash);
    outer.finalize().to_vec()
}

/// SHA-256 哈希，返回字节数组
fn sha256_bytes(data: &[u8]) -> [u8; 32] {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().into()
}

/// SHA-256 哈希，返回十六进制字符串
fn sha256_hex(data: &[u8]) -> String {
    let hash = sha256_bytes(data);
    hash.iter().map(|b| format!("{b:02x}")).collect()
}

/// URL-safe Base64 编码（无 padding）
fn url_safe_b64(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

/// URL-safe Base64 解码（无 padding）
fn url_safe_b64_decode(s: &str) -> Result<Vec<u8>> {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(s)
        .map_err(|e| anyhow!("Base64 解码失败: {e}"))
}

// ════════════════════════════════════════════════════════════════════════════
//  单元测试
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_jwt_generate_and_verify() {
        let config = JwtConfig::default();
        let device_fp = generate_device_fingerprint().unwrap();

        let token =
            generate_jwt("user-123", "张老师", "teacher", None, &device_fp, &config).unwrap();

        let claims = verify_jwt(&token, &config, &device_fp).unwrap();
        assert_eq!(claims.sub, "user-123");
        assert_eq!(claims.name, "张老师");
        assert_eq!(claims.role, "teacher");
        assert_eq!(claims.device_fp, device_fp);
    }

    #[test]
    fn test_jwt_wrong_device_fp_rejected() {
        let config = JwtConfig::default();
        let fp1 = "fingerprint-a";
        let fp2 = "fingerprint-b";

        let token = generate_jwt("u1", "学生", "student", Some("class-1"), fp1, &config).unwrap();

        let result = verify_jwt(&token, &config, fp2);
        assert!(result.is_err(), "设备指纹不匹配应被拒绝");
    }

    #[test]
    fn test_jwt_tampered_payload_rejected() {
        let config = JwtConfig::default();
        let fp = generate_device_fingerprint().unwrap();

        let token = generate_jwt("u1", "老师", "teacher", None, &fp, &config).unwrap();

        // 篡改 payload 部分
        let parts: Vec<&str> = token.split('.').collect();
        let tampered = format!("{}.{}.{}", parts[0], "tampered", parts[2]);

        let result = verify_jwt(&tampered, &config, &fp);
        assert!(result.is_err(), "篡改后的 JWT 应被拒绝");
    }

    #[test]
    fn test_jwt_expired_rejected() {
        let config = JwtConfig {
            expiry_hours: -1, // 已过期
            ..Default::default()
        };

        let fp = generate_device_fingerprint().unwrap();
        let token = generate_jwt("u1", "老师", "teacher", None, &fp, &config).unwrap();

        let result = verify_jwt(&token, &config, &fp);
        assert!(result.is_err(), "过期的 JWT 应被拒绝");
    }

    #[test]
    fn test_jwt_verify_unchecked() {
        let config = JwtConfig::default();
        let fp = generate_device_fingerprint().unwrap();

        let token = generate_jwt("u1", "学生", "student", Some("c1"), &fp, &config).unwrap();

        // 不验证设备指纹
        let claims = verify_jwt_unchecked(&token, &config).unwrap();
        assert_eq!(claims.sub, "u1");
        assert_eq!(claims.class_id, Some("c1".to_string()));
    }

    #[test]
    fn test_jwt_config_from_env_missing_returns_err() {
        // 确保环境变量未设置，验证缺失即返回 Err（拒绝静默回退）。
        std::env::remove_var("DRAFTTINK_JWT_SECRET");
        assert!(
            JwtConfig::from_env().is_err(),
            "缺失 DRAFTTINK_JWT_SECRET 时必须返回 Err"
        );
    }

    #[test]
    fn test_jwt_config_from_env_present_returns_ok() {
        std::env::set_var("DRAFTTINK_JWT_SECRET", "a-strong-random-secret");
        let cfg = JwtConfig::from_env().expect("应设置成功");
        assert_eq!(cfg.secret, b"a-strong-random-secret".to_vec());
        std::env::remove_var("DRAFTTINK_JWT_SECRET");
    }

    #[test]
    fn test_validate_not_default_empty_rejected() {
        let cfg = JwtConfig {
            secret: Vec::new(),
            ..Default::default()
        };
        assert!(cfg.validate_not_default().is_err(), "空密钥必须被拒绝");
    }

    #[test]
    fn test_validate_not_default_known_secret_rejected() {
        let cfg = JwtConfig {
            secret: b"drafftink-default-secret-change-me".to_vec(),
            ..Default::default()
        };
        assert!(
            cfg.validate_not_default().is_err(),
            "已知默认硬编码密钥必须被拒绝"
        );
    }

    #[test]
    fn test_validate_not_default_valid_ok() {
        let cfg = JwtConfig {
            secret: b"a-strong-random-secret".to_vec(),
            ..Default::default()
        };
        assert!(cfg.validate_not_default().is_ok(), "真实密钥应通过校验");
    }

    #[test]
    fn test_device_fingerprint_consistent() {
        // 同一进程内多次调用应返回相同结果
        let fp1 = generate_device_fingerprint().unwrap();
        let fp2 = generate_device_fingerprint().unwrap();
        assert_eq!(fp1, fp2);
        assert_eq!(fp1.len(), 64); // SHA-256 hex = 64 chars
    }

    #[test]
    fn test_device_id_persistence() {
        let temp_dir = std::env::temp_dir().join("drafftink_test_device_id");
        let _ = std::fs::remove_dir_all(&temp_dir);

        // 首次创建
        let id1 = get_or_create_device_id(&temp_dir).unwrap();
        assert!(!id1.is_empty());

        // 第二次读取（应相同）
        let id2 = get_or_create_device_id(&temp_dir).unwrap();
        assert_eq!(id1, id2);

        // 清理
        let _ = std::fs::remove_dir_all(&temp_dir);
    }

    #[test]
    fn test_device_binding() {
        let fp = "abc123";
        let device_id = "550e8400-e29b-41d4-a716-446655440000";
        let binding = generate_device_binding(fp, device_id);
        assert_eq!(binding.len(), 64);

        // 相同输入应产生相同绑定
        let binding2 = generate_device_binding(fp, device_id);
        assert_eq!(binding, binding2);

        // 不同输入应产生不同绑定
        let binding3 = generate_device_binding("different", device_id);
        assert_ne!(binding, binding3);
    }

    #[test]
    fn test_ed25519_sign_and_verify_raw() {
        let (sk, pk) = generate_keypair();
        let data = b"test data for signing";

        let signature = sign_raw(&sk, data);
        assert_eq!(signature.len(), 64);

        assert!(verify_raw(&pk, data, &signature));
    }

    #[test]
    fn test_ed25519_wrong_key_rejected() {
        let (sk1, _) = generate_keypair();
        let (_, pk2) = generate_keypair();
        let data = b"test data";

        let signature = sign_raw(&sk1, data);
        assert!(!verify_raw(&pk2, data, &signature), "错误公钥应验证失败");
    }

    #[test]
    fn test_ed25519_tampered_data_rejected() {
        let (sk, pk) = generate_keypair();
        let original = b"original data";
        let tampered = b"tampered data";

        let signature = sign_raw(&sk, original);
        assert!(!verify_raw(&pk, tampered, &signature), "篡改数据应验证失败");
    }

    #[test]
    fn test_hmac_sha256_known_vector() {
        // RFC 4231 Test Case 1
        let key = &[0x0bu8; 20];
        let data = b"Hi There";
        let expected = [
            0xb0, 0x34, 0x4c, 0x61, 0xd8, 0xdb, 0x38, 0x53, 0x5c, 0xa8, 0xaf, 0xce, 0xaf, 0x0b,
            0xf1, 0x2b, 0x88, 0x1d, 0xc2, 0x00, 0xc9, 0x83, 0x3d, 0xa7, 0x26, 0xe9, 0x37, 0x6c,
            0x2e, 0x32, 0xcf, 0xf7,
        ];
        let result = hmac_sha256(key, data);
        assert_eq!(result, expected);
    }
}
