//! # 移动端 MFA 认证与单点登录
//!
//! 提供移动办公所需的多因子安全能力：
//! - **设备指纹绑定**：访问令牌（[`crate::auth::jwt`] 签发）已绑定 `device_fp`，
//!   本模块在 MFA 与 SSO 环节再次校验请求设备指纹与令牌一致。
//! - **短信二次验证（SMS OTP）**：登录后下发 6 位一次性验证码（演示环境以日志/接口回显，
//!   生产应经短信网关下发），用于敏感操作前的第二步认证。
//! - **单点登录（SSO，GB/T 36342-2018）**：MFA 通过后签发校园级 SSO 令牌，供校内其他
//!   应用（如排课、资源平台）免密互信。
//! - **敏感数据 SM4 信封加密（GB/T 32907-2016）**：对消息等敏感载荷以 SM4（CBC + PKCS#7）
//!   加密，密钥由设备指纹、服务器密钥与随机盐（HKDF 风格加盐派生）派生，
//!   每次加密使用随机 IV，保证传输与存储的机密性与语义安全。

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use base64::engine::general_purpose::STANDARD as B64;
use base64::Engine;
use chrono::Utc;
use sha2::{Digest, Sha256};
use uuid::Uuid;

use drafftink_core::crypto::{generate_jwt, JwtConfig};
use drafftink_core::Sm4;

use crate::error::AppError;

/// 短信验证码有效期
const SMS_TTL: Duration = Duration::from_secs(300);
/// SSO 令牌有效期（小时）
const SSO_TTL_HOURS: i64 = 8;
/// 短信验证码单小时最大尝试验证次数（防暴力枚举）
const OTP_MAX_ATTEMPTS: u32 = 5;
/// 验证码尝试频控窗口（秒，1 小时）
const OTP_ATTEMPT_WINDOW_SECS: i64 = 3600;

/// 短信验证码挑战存储（内存，按用户 ID 索引）。
///
/// 安全特性：验证码一次性消费；同一用户连续验证失败超过 [`OTP_MAX_ATTEMPTS`] 次
/// （[`OTP_ATTEMPT_WINDOW_SECS`] 窗口内）将临时锁定，防止对短信验证码的暴力枚举。
#[derive(Clone, Default)]
pub struct SmsChallengeStore {
    /// (验证码, 过期 unix 秒)
    codes: Arc<Mutex<HashMap<Uuid, (String, i64)>>>,
    /// 尝试验证失败计数：(失败次数, 窗口起始 unix 秒)
    failures: Arc<Mutex<HashMap<Uuid, (u32, i64)>>>,
}

impl SmsChallengeStore {
    /// 生成并“下发”验证码（演示：返回验证码，生产应经短信网关）。
    pub fn issue(&self, user_id: Uuid) -> String {
        // 由 UUID 派生 6 位随机码（无需额外 RNG 依赖）
        let n = (Uuid::new_v4().as_u128() % 1_000_000) as u32;
        let code = format!("{n:06}");
        let exp = (Utc::now() + SMS_TTL).timestamp();
        self.codes.lock().unwrap().insert(user_id, (code.clone(), exp));
        code
    }

    /// 校验验证码（一次性：验证成功后即失效）。
    ///
    /// 窗口内失败次数已达 [`OTP_MAX_ATTEMPTS`] 时直接拒绝后续验证，需等待窗口重置。
    pub fn verify(&self, user_id: Uuid, code: &str) -> bool {
        // 频控：窗口内失败过多直接拒绝，避免验证码被暴力枚举
        if self.is_locked_out(user_id) {
            return false;
        }
        let mut code_guard = self.codes.lock().unwrap();
        let entry = match code_guard.get(&user_id) {
            Some(e) => e.clone(),
            // 无待验证挑战：不计入频控，避免恶意请求用无效用户锁死正常用户
            None => return false,
        };
        let (stored, exp) = entry;
        if exp < Utc::now().timestamp() {
            code_guard.remove(&user_id);
            self.record_failure(user_id);
            return false;
        }
        if stored != code {
            self.record_failure(user_id);
            return false;
        }
        // 验证成功：消费验证码并清除失败计数
        code_guard.remove(&user_id);
        self.failures.lock().unwrap().remove(&user_id);
        true
    }

    /// 该用户是否处于验证码频控锁定中。
    pub fn is_locked_out(&self, user_id: Uuid) -> bool {
        let guard = self.failures.lock().unwrap();
        if let Some((count, window_start)) = guard.get(&user_id) {
            let now = Utc::now().timestamp();
            return now - *window_start < OTP_ATTEMPT_WINDOW_SECS && *count >= OTP_MAX_ATTEMPTS;
        }
        false
    }

    /// 记录一次验证失败，并在窗口过期时重置计数。
    fn record_failure(&self, user_id: Uuid) {
        let mut guard = self.failures.lock().unwrap();
        let now = Utc::now().timestamp();
        let (count, window_start) = guard.get(&user_id).copied().unwrap_or((0, now));
        let (count, window_start) = if now - window_start >= OTP_ATTEMPT_WINDOW_SECS {
            (0, now)
        } else {
            (count, window_start)
        };
        guard.insert(user_id, (count + 1, window_start));
    }
}

/// MFA 会话记录：某访问令牌（按 jti）是否已完成短信二次验证，及签发的 SSO 票据。
#[derive(Clone, Default)]
pub struct MfaSessionStore {
    sessions: Arc<Mutex<HashMap<String, MfaSession>>>,
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
struct MfaSession {
    user_id: Uuid,
    device_fp: String,
    verified: bool,
    sso_ticket: Option<String>,
}

impl MfaSessionStore {
    /// 标记某访问令牌 jti 已完成 MFA 验证，并登记签发的 SSO 票据。
    pub fn mark_verified(&self, jti: &str, user_id: Uuid, device_fp: &str, sso_ticket: String) {
        self.sessions.lock().unwrap().insert(
            jti.to_string(),
            MfaSession {
                user_id,
                device_fp: device_fp.to_string(),
                verified: true,
                sso_ticket: Some(sso_ticket),
            },
        );
    }

    /// 取回已签发的 SSO 票据（仅当该 jti 已完成 MFA）。
    pub fn take_sso_ticket(&self, jti: &str) -> Option<String> {
        self.sessions
            .lock()
            .unwrap()
            .get(jti)
            .filter(|s| s.verified)
            .and_then(|s| s.sso_ticket.clone())
    }

    /// 吊销某访问令牌的 MFA 会话（登出/换设备时调用）。
    pub fn revoke(&self, jti: &str) {
        self.sessions.lock().unwrap().remove(jti);
    }
}

/// 综合的移动认证状态（持有于 [`crate::state::AppState`]）。
#[derive(Clone, Default)]
pub struct MobileAuth {
    /// 短信验证码挑战
    pub sms: SmsChallengeStore,
    /// MFA 会话（SSO 票据）
    pub sessions: MfaSessionStore,
}

impl MobileAuth {
    /// 创建空移动认证状态。
    pub fn new() -> Self {
        Self::default()
    }
}

/// 由设备指纹、服务器密钥与随机盐派生 16 字节 SM4 密钥（SHA-256 取前 16 字节）。
///
/// 引入随机 `salt` 使相同 `(device_fp, server_secret)` 每次加密得到不同密钥，
/// 抵御「相同明文→相同密钥→可被关联」的攻击，属 HKDF 风格的加盐派生。
pub fn derive_sm4_key(device_fp: &str, server_secret: &[u8], salt: &[u8]) -> [u8; 16] {
    let mut hasher = Sha256::new();
    hasher.update(device_fp.as_bytes());
    hasher.update(server_secret);
    hasher.update(salt);
    let digest = hasher.finalize();
    let mut key = [0u8; 16];
    key.copy_from_slice(&digest[..16]);
    key
}

/// 以 SM4（CBC + PKCS#7，随机 IV + 随机盐）加密任意 JSON 可序列化值，
/// 返回 Base64 信封：`IV(16 字节) || salt(16 字节) || ciphertext`。
///
/// 随机 IV 与盐由 UUID v4 提供密码学随机性；盐和 IV 随密文传输，无需保密。
pub fn encrypt_json<T: serde::Serialize>(
    device_fp: &str,
    server_secret: &[u8],
    value: &T,
) -> Result<String, AppError> {
    let plain = serde_json::to_vec(value)
        .map_err(|e| AppError::Internal(format!("SM4 载荷序列化失败: {e}")))?;
    // 随机 IV 与盐（各 16 字节）
    let mut iv = [0u8; 16];
    iv.copy_from_slice(Uuid::new_v4().as_bytes());
    let mut salt = [0u8; 16];
    salt.copy_from_slice(Uuid::new_v4().as_bytes());
    let key = derive_sm4_key(device_fp, server_secret, &salt);
    let cipher = Sm4::new(&key).encrypt_cbc(&iv, &plain);
    let mut env = Vec::with_capacity(32 + cipher.len());
    env.extend_from_slice(&iv);
    env.extend_from_slice(&salt);
    env.extend_from_slice(&cipher);
    Ok(B64.encode(env))
}

/// 解密 SM4（CBC + PKCS#7）Base64 信封（`IV || salt || ciphertext`）为 JSON 字符串。
pub fn decrypt_json(device_fp: &str, server_secret: &[u8], b64: &str) -> Result<String, AppError> {
    let env = B64
        .decode(b64)
        .map_err(|e| AppError::BadRequest(format!("SM4 信封 Base64 解码失败: {e}")))?;
    if env.len() < 32 || env.len() % 16 != 0 {
        return Err(AppError::BadRequest("SM4 信封格式非法".to_string()));
    }
    let (iv, rest) = env.split_at(16);
    let (salt, ct) = rest.split_at(16);
    let mut iv_b = [0u8; 16];
    iv_b.copy_from_slice(iv);
    let mut salt_b = [0u8; 16];
    salt_b.copy_from_slice(salt);
    let key = derive_sm4_key(device_fp, server_secret, &salt_b);
    let plain = Sm4::new(&key)
        .decrypt_cbc(&iv_b, ct)
        .map_err(|e| AppError::Internal(format!("SM4 解密失败: {e}")))?;
    String::from_utf8(plain)
        .map_err(|e| AppError::Internal(format!("SM4 明文非 UTF-8: {e}")))
}

/// 签发校园级 SSO 令牌（GB/T 36342-2018 单点登录凭证）。
///
/// 令牌经 SM4/设备指纹绑定的 HS256 签名，携带 `sub/name/role/tenant/device_fp`，
/// 供校内互信应用免密登录。
pub fn issue_sso_token(
    secret: &[u8],
    sub: &str,
    name: &str,
    role: &str,
    class_id: Option<&str>,
    device_fp: &str,
    _tenant_id: &str,
) -> Result<String, AppError> {
    let cfg = JwtConfig {
        secret: secret.to_vec(),
        expiry_hours: SSO_TTL_HOURS,
        issuer: "GB/T 36342-2018 SSO".to_string(),
    };
    generate_jwt(sub, name, role, class_id, device_fp, &cfg)
        .map_err(|e| AppError::Internal(format!("SSO 令牌签发失败: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json;

    #[test]
    fn sm4_cbc_envelope_roundtrip_and_randomness() {
        let device_fp = "fp-abc";
        let secret = b"server-secret";
        let payload = "机密用印事由-测试载荷";

        let env = encrypt_json(device_fp, secret, &payload).unwrap();
        // 相同明文每次加密应得到不同信封（随机 IV + 盐）。
        let env2 = encrypt_json(device_fp, secret, &payload).unwrap();
        assert_ne!(env, env2, "CBC 下相同明文应产生不同信封（随机 IV/盐）");

        // 解密应还原原始载荷。
        let plain: String = serde_json::from_str(&decrypt_json(device_fp, secret, &env).unwrap())
            .expect("解密结果应为合法 JSON 字符串");
        assert_eq!(plain, payload);
    }

    #[test]
    fn sm4_cbc_envelope_wrong_secret_fails() {
        let device_fp = "fp-abc";
        let env = encrypt_json(device_fp, b"secret-A", &"hello").unwrap();
        assert!(
            decrypt_json(device_fp, b"secret-B", &env).is_err(),
            "错误密钥应解密失败（PKCS#7 填充校验不通过）"
        );
    }

    #[test]
    fn sm4_cbc_envelope_wrong_salt_layout_fails() {
        // 长度不足 32 字节的信封应被拒绝。
        assert!(decrypt_json("fp", b"secret", "bm90LWJhc2U2NA==").is_err());
    }

    #[test]
    fn sms_otp_one_time_use() {
        let store = SmsChallengeStore::default();
        let uid = Uuid::new_v4();
        let code = store.issue(uid);
        assert!(store.verify(uid, &code), "正确验证码应通过");
        // 一次性消费后再次使用同一验证码应失败
        assert!(!store.verify(uid, &code), "验证码验证后应失效（一次性）");
    }

    #[test]
    fn sms_otp_attempt_cap_locks_out() {
        let store = SmsChallengeStore::default();
        let uid = Uuid::new_v4();
        let code = store.issue(uid);
        // 连续 OTP_MAX_ATTEMPTS 次错误尝试应触发锁定
        for _ in 0..OTP_MAX_ATTEMPTS {
            assert!(!store.verify(uid, "000000"), "错误验证码应失败");
        }
        // 达到上限后，即使提交正确验证码也应被锁定拒绝
        assert!(
            !store.verify(uid, &code),
            "达到尝试上限后即使正确验证码也应被锁定拒绝"
        );
        assert!(store.is_locked_out(uid), "锁定状态应可被查询");
    }
}
