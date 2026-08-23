//! 密码加密服务：基于 Argon2id 的安全哈希与校验。
//!
//! 使用 Argon2id（默认参数 + 随机盐），符合 NIST/OWASP 对密码存储的建议，
//! 满足教育数据安全的合规要求（明文密码不落盘）。

use argon2::password_hash::{
    rand_core::OsRng, PasswordHash, PasswordHasher, PasswordVerifier, SaltString,
};
use argon2::Argon2;

/// 使用 Argon2id 对密码进行加盐哈希，返回可安全存储的 PHC 字符串。
pub fn hash_password(password: &str) -> String {
    let salt = SaltString::generate(&mut OsRng);
    let argon2 = Argon2::default(); // 默认即为 Argon2id
    argon2
        .hash_password(password.as_bytes(), &salt)
        .expect("Argon2 哈希失败（系统资源不足？）")
        .to_string()
}

/// 校验明文密码与存储的 Argon2 哈希是否匹配。
///
/// 任何解析/校验错误都返回 `false`，避免泄露具体失败原因。
pub fn verify_password(password: &str, hash: &str) -> bool {
    let parsed = match PasswordHash::new(hash) {
        Ok(p) => p,
        Err(_) => return false,
    };
    Argon2::default()
        .verify_password(password.as_bytes(), &parsed)
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_hash_and_verify() {
        let h = hash_password("S3cr3t-Pass!");
        assert_ne!(h, "S3cr3t-Pass!");
        assert!(verify_password("S3cr3t-Pass!", &h));
        assert!(!verify_password("wrong", &h));
    }

    #[test]
    fn test_hash_is_salted_unique() {
        let a = hash_password("same");
        let b = hash_password("same");
        assert_ne!(a, b, "相同密码的哈希必须因随机盐而不同");
    }
}
