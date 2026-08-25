//! # minio — MinIO（S3 兼容）存储后端
//!
//! 实现符合 S3 API 的 MinIO 客户端，复用现有 [`Storage`](crate::storage::Storage)
//! 抽象，对接现有 MinIO 不重复造轮子。采用纯 Rust 标准库（`std::net::TcpStream`）
//! 实现 HTTP/1.1 传输与 AWS Signature V4 签名，**无 C 依赖**。
//!
//! > 说明：默认仅支持 `http://` 端点（内网 MinIO 部署常用）。如需 HTTPS 可在此
//! > 之上扩展 TLS 连接器，但会引入额外 crate，离线环境暂不启用。

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{anyhow, Result};
use chrono::Utc;
use sha2::{Digest, Sha256};

use crate::storage::Storage;

/// MinIO / S3 兼容存储配置。
#[derive(Debug, Clone)]
pub struct MinioConfig {
    /// 端点，例如 `http://127.0.0.1:9000`
    pub endpoint: String,
    /// 存储桶
    pub bucket: String,
    /// Access Key
    pub access_key: String,
    /// Secret Key
    pub secret_key: String,
    /// 区域（默认 `cn-north-1`）
    pub region: String,
}

fn default_region() -> String {
    "cn-north-1".to_string()
}

impl MinioConfig {
    /// 从环境变量加载 MinIO 配置；若端点 / AccessKey / SecretKey 未全部设置则返回 `None`。
    ///
    /// 支持的环境变量：`MINIO_ENDPOINT`、`MINIO_BUCKET`、`MINIO_ACCESS_KEY`、
    /// `MINIO_SECRET_KEY`、`MINIO_REGION`。
    pub fn from_env() -> Option<Self> {
        let endpoint = std::env::var("MINIO_ENDPOINT").ok()?;
        let access_key = std::env::var("MINIO_ACCESS_KEY").ok()?;
        let secret_key = std::env::var("MINIO_SECRET_KEY").ok()?;
        let bucket = std::env::var("MINIO_BUCKET").unwrap_or_else(|_| "courseware".to_string());
        let region = std::env::var("MINIO_REGION").unwrap_or_else(|_| default_region());
        Some(Self {
            endpoint,
            bucket,
            access_key,
            secret_key,
            region,
        })
    }
}

/// MinIO（S3 兼容）存储实现，复用 [`Storage`] 抽象。
pub struct MinioStorage {
    host: String,
    port: u16,
    bucket: String,
    access_key: String,
    secret_key: String,
    region: String,
}

impl MinioStorage {
    /// 由配置构造客户端；解析端点（仅支持 `http://`）。
    pub fn new(cfg: &MinioConfig) -> Result<Self> {
        let ep = cfg.endpoint.trim();
        let without_scheme = ep
            .strip_prefix("http://")
            .ok_or_else(|| anyhow!("MinIO 端点仅支持 http:// 方案: {ep}"))?;
        let (host, port) = match without_scheme.split_once(':') {
            Some((h, p)) => (
                h.to_string(),
                p.parse::<u16>().map_err(|_| anyhow!("非法端口: {p}"))?,
            ),
            None => (without_scheme.to_string(), 9000),
        };
        let region = if cfg.region.is_empty() {
            default_region()
        } else {
            cfg.region.clone()
        };
        Ok(Self {
            host,
            port,
            bucket: cfg.bucket.clone(),
            access_key: cfg.access_key.clone(),
            secret_key: cfg.secret_key.clone(),
            region,
        })
    }

    /// 计算 AWS 签名所需的 amzdate（`%Y%m%dT%H%M%SZ`）与 date（`%Y%m%d`）。
    fn amz_time() -> (String, String) {
        let now = Utc::now();
        (
            now.format("%Y%m%dT%H%M%SZ").to_string(),
            now.format("%Y%m%d").to_string(),
        )
    }

    /// 构造 AWS Signature V4 签名与 scope。
    fn sign_v4(
        &self,
        method: &str,
        key: &str,
        payload: &[u8],
        amzdate: &str,
        date: &str,
    ) -> (String, String) {
        let canonical_uri = format!("/{}/{}", self.bucket, key);
        let canonical_query = "";
        let payload_hash = sha256_hex(payload);
        let host_header = format!("{}:{}", self.host, self.port);
        let canonical_headers = format!(
            "host:{host_header}\nx-amz-content-sha256:{payload_hash}\nx-amz-date:{amzdate}\n"
        );
        let signed_headers = "host;x-amz-content-sha256;x-amz-date";
        let canonical_request = format!(
            "{method}\n{canonical_uri}\n{canonical_query}\n{canonical_headers}\n{signed_headers}\n{payload_hash}"
        );
        let scope = format!("{}/{}/s3/aws4_request", date, self.region);
        let string_to_sign = format!(
            "AWS4-HMAC-SHA256\n{}\n{}\n{}",
            amzdate,
            scope,
            sha256_hex(canonical_request.as_bytes())
        );
        let k_date = hmac_sha256(
            format!("AWS4{}", self.secret_key).as_bytes(),
            date.as_bytes(),
        );
        let k_region = hmac_sha256(&k_date, self.region.as_bytes());
        let k_service = hmac_sha256(&k_region, b"s3");
        let k_signing = hmac_sha256(&k_service, b"aws4_request");
        let signature = hex::encode(hmac_sha256(&k_signing, string_to_sign.as_bytes()));
        (signature, scope)
    }

    /// 构造完整 HTTP/1.1 请求头块（不含 PUT 请求体）。
    fn request_head(&self, method: &str, key: &str, payload: &[u8], content_type: &str) -> String {
        let (amzdate, date) = Self::amz_time();
        let payload_hash = sha256_hex(payload);
        let (signature, scope) = self.sign_v4(method, key, payload, &amzdate, &date);
        let host_header = format!("{}:{}", self.host, self.port);
        let authorization = format!(
            "AWS4-HMAC-SHA256 Credential={}/{}, SignedHeaders=host;x-amz-content-sha256;x-amz-date, Signature={}",
            self.access_key, scope, signature
        );
        let canonical_uri = format!("/{}/{}", self.bucket, key);
        let mut req = format!(
            "{method} {canonical_uri} HTTP/1.1\r\n\
             Host: {host_header}\r\n\
             X-Amz-Date: {amzdate}\r\n\
             X-Amz-Content-Sha256: {payload_hash}\r\n\
             Authorization: {authorization}\r\n"
        );
        if !content_type.is_empty() {
            req.push_str(&format!("Content-Type: {content_type}\r\n"));
        }
        req.push_str(&format!("Content-Length: {}\r\n", payload.len()));
        req.push_str("Connection: close\r\n\r\n");
        req
    }

    /// 通过 TCP 发送请求并接收完整响应。
    fn send_recv(
        &self,
        method: &str,
        key: &str,
        payload: &[u8],
        content_type: &str,
    ) -> Result<Vec<u8>> {
        let mut stream = TcpStream::connect((self.host.as_str(), self.port))
            .map_err(|e| anyhow!("连接 MinIO 失败 ({}:{}): {e}", self.host, self.port))?;
        let head = self.request_head(method, key, payload, content_type);
        stream
            .write_all(head.as_bytes())
            .map_err(|e| anyhow!("写入 MinIO 请求失败: {e}"))?;
        if !payload.is_empty() {
            stream
                .write_all(payload)
                .map_err(|e| anyhow!("写入 MinIO 请求体失败: {e}"))?;
        }
        stream
            .flush()
            .map_err(|e| anyhow!("刷新 MinIO 请求失败: {e}"))?;
        let mut buf = Vec::new();
        stream
            .read_to_end(&mut buf)
            .map_err(|e| anyhow!("读取 MinIO 响应失败: {e}"))?;
        Ok(buf)
    }
}

impl Storage for MinioStorage {
    fn save(&self, key: &str, data: Vec<u8>) -> Result<()> {
        let resp = self.send_recv("PUT", key, &data, "application/octet-stream")?;
        let code = parse_status(&resp)?;
        if (200..300).contains(&code) {
            Ok(())
        } else {
            Err(anyhow!("MinIO PUT 失败，状态码 {code}"))
        }
    }

    fn load(&self, key: &str) -> Result<Vec<u8>> {
        let resp = self.send_recv("GET", key, &[], "")?;
        let code = parse_status(&resp)?;
        if (200..300).contains(&code) {
            Ok(extract_body(&resp))
        } else {
            Err(anyhow!("MinIO GET 失败，状态码 {code}"))
        }
    }

    fn delete(&self, key: &str) -> Result<()> {
        let resp = self.send_recv("DELETE", key, &[], "")?;
        let code = parse_status(&resp)?;
        if (200..300).contains(&code) || code == 404 {
            Ok(())
        } else {
            Err(anyhow!("MinIO DELETE 失败，状态码 {code}"))
        }
    }

    fn exists(&self, key: &str) -> Result<bool> {
        // 简化实现：尝试读取，成功即存在
        Ok(self.load(key).is_ok())
    }
}

/// HMAC-SHA256 计算（纯 Rust 实现，仅依赖 `sha2`，无 C 依赖 / 无额外 crate）。
///
/// 标准 HMAC：H(K ⊕ opad ‖ H(K ⊕ ipad ‖ msg))，SHA256 块长 64 字节。
fn hmac_sha256(key: &[u8], data: &[u8]) -> Vec<u8> {
    const BLOCK: usize = 64;
    let mut k = [0u8; BLOCK];
    if key.len() > BLOCK {
        let h = Sha256::digest(key);
        k[..32].copy_from_slice(h.as_slice());
    } else {
        k[..key.len()].copy_from_slice(key);
    }
    let mut ipad = [0x36u8; BLOCK];
    let mut opad = [0x5cu8; BLOCK];
    for i in 0..BLOCK {
        ipad[i] ^= k[i];
        opad[i] ^= k[i];
    }
    let mut inner = Sha256::new();
    inner.update(ipad);
    inner.update(data);
    let inner_hash = inner.finalize();
    let mut outer = Sha256::new();
    outer.update(opad);
    outer.update(inner_hash);
    outer.finalize().as_slice().to_vec()
}

/// SHA256 十六进制摘要。
fn sha256_hex(data: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hex::encode(hasher.finalize())
}

/// 解析 HTTP 响应状态码。
fn parse_status(buf: &[u8]) -> Result<u16> {
    let text = String::from_utf8_lossy(buf);
    let first_line = text.lines().next().unwrap_or("");
    // 形如 "HTTP/1.1 200 OK"
    let mut parts = first_line.split_whitespace();
    let _version = parts.next();
    let code = parts
        .next()
        .ok_or_else(|| anyhow!("非法 HTTP 响应: {first_line}"))?
        .parse::<u16>()
        .map_err(|_| anyhow!("非法 HTTP 状态码: {first_line}"))?;
    Ok(code)
}

/// 提取 HTTP 响应体（头与体以 `\r\n\r\n` 分隔）。
fn extract_body(buf: &[u8]) -> Vec<u8> {
    const SEP: &[u8] = b"\r\n\r\n";
    buf.windows(SEP.len())
        .position(|w| w == SEP)
        .map(|pos| buf[pos + SEP.len()..].to_vec())
        .unwrap_or_default()
}

/// 计算 Unix 秒级时间戳（供测试/扩展使用）。
#[allow(dead_code)]
fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_parse_http_endpoint() {
        let cfg = MinioConfig {
            endpoint: "http://127.0.0.1:9000".into(),
            bucket: "courseware".into(),
            access_key: "ak".into(),
            secret_key: "sk".into(),
            region: "cn-north-1".into(),
        };
        let client = MinioStorage::new(&cfg).unwrap();
        assert_eq!(client.host, "127.0.0.1");
        assert_eq!(client.port, 9000);
        assert_eq!(client.bucket, "courseware");
    }

    #[test]
    fn config_rejects_https() {
        let cfg = MinioConfig {
            endpoint: "https://minio.example.com".into(),
            bucket: "b".into(),
            access_key: "ak".into(),
            secret_key: "sk".into(),
            region: "cn-north-1".into(),
        };
        assert!(MinioStorage::new(&cfg).is_err());
    }

    #[test]
    fn signature_v4_is_deterministic() {
        let cfg = MinioConfig {
            endpoint: "http://127.0.0.1:9000".into(),
            bucket: "b".into(),
            access_key: "ak".into(),
            secret_key: "sk".into(),
            region: "cn-north-1".into(),
        };
        let client = MinioStorage::new(&cfg).unwrap();
        let (sig1, scope1) = client.sign_v4("PUT", "k", b"data", "20260812T000000Z", "20260812");
        let (sig2, _scope2) = client.sign_v4("PUT", "k", b"data", "20260812T000000Z", "20260812");
        assert_eq!(sig1, sig2);
        assert_eq!(scope1, "20260812/cn-north-1/s3/aws4_request");
    }

    #[test]
    fn extract_body_splits_headers() {
        let resp = b"HTTP/1.1 200 OK\r\nContent-Length: 3\r\n\r\nabc".to_vec();
        assert_eq!(extract_body(&resp), b"abc".to_vec());
    }
}
