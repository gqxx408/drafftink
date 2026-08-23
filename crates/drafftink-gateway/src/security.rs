//! Security modules: rate limiting and WAF (Web Application Firewall).
//!
//! The gateway implements a sliding-window rate limiter per IP and a
//! pattern-based WAF checker for SQL injection, XSS, and path traversal.

use std::collections::HashMap;

// ════════════════════════════════════════════════════════════════════════════
//  Rate Limiter
// ════════════════════════════════════════════════════════════════════════════

/// Sliding-window rate limiter keyed by IP address.
///
/// Stores a vector of request timestamps per IP. Timestamps older than
/// 60 seconds are pruned on each check. Call `cleanup()` periodically
/// to evict IPs with no recent activity and keep memory usage low.
pub struct RateLimiter {
    requests: HashMap<String, Vec<i64>>,
}

impl RateLimiter {
    /// Create a new empty rate limiter.
    pub fn new() -> Self {
        Self {
            requests: HashMap::new(),
        }
    }

    /// Check if the given IP is within the rate limit.
    ///
    /// Returns `true` if the request is allowed (and records the timestamp),
    /// `false` if rate-limited. Timestamps older than 1 minute are removed
    /// before checking.
    pub fn check_rate(&mut self, ip: &str, limit: u32) -> bool {
        let now = chrono::Utc::now().timestamp();
        let window_start = now - 60;

        let timestamps = self.requests.entry(ip.to_string()).or_default();
        timestamps.retain(|&ts| ts > window_start);

        if timestamps.len() >= limit as usize {
            return false;
        }

        timestamps.push(now);
        true
    }

    /// Remove all entries whose timestamps are entirely older than 1 minute.
    ///
    /// Call this periodically (e.g. every 60 seconds) to keep the
    /// `HashMap` from growing unbounded.
    pub fn cleanup(&mut self) {
        let now = chrono::Utc::now().timestamp();
        let window_start = now - 60;

        self.requests.retain(|_, timestamps| {
            timestamps.retain(|&ts| ts > window_start);
            !timestamps.is_empty()
        });
    }

    /// Returns the number of currently tracked IPs.
    pub fn tracked_ip_count(&self) -> usize {
        self.requests.len()
    }
}

impl Default for RateLimiter {
    fn default() -> Self {
        Self::new()
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  WAF Checker
// ════════════════════════════════════════════════════════════════════════════

/// Pattern-based Web Application Firewall.
///
/// Inspects request method, path, and body for common attack signatures:
/// SQL injection, XSS, and path traversal.
pub struct WafChecker {
    sql_injection_patterns: Vec<&'static str>,
    xss_patterns: Vec<&'static str>,
    path_traversal_patterns: Vec<&'static str>,
}

impl WafChecker {
    /// Create a new WAF checker with default rule sets.
    pub fn new() -> Self {
        Self {
            sql_injection_patterns: vec![
                "' OR ",
                "' OR'",
                "UNION SELECT",
                "DROP TABLE",
                "DROP DATABASE",
                "; --",
                "'--",
                "' #",
                "INSERT INTO",
                "DELETE FROM",
                "UPDATE SET",
                "EXEC(",
                "EXECUTE(",
                "xp_cmdshell",
            ],
            xss_patterns: vec![
                "<script",
                "</script",
                "javascript:",
                "onerror=",
                "onload=",
                "onclick=",
                "onmouseover=",
                "<iframe",
                "<object",
                "<embed",
                "alert(",
                "document.cookie",
                "eval(",
            ],
            path_traversal_patterns: vec![
                "../",
                "..\\",
                "..%2f",
                "..%5c",
                "%2e%2e%2f",
                "%2e%2e%5c",
            ],
        }
    }

    /// Check a request for malicious patterns.
    ///
    /// Returns `Ok(())` if the request passes all checks, or
    /// `Err(reason)` with a human-readable description of the violation.
    pub fn check_request(&self, method: &str, path: &str, body: &[u8]) -> Result<(), String> {
        // Check path for traversal patterns.
        let path_lower = path.to_lowercase();
        for pattern in &self.path_traversal_patterns {
            if path_lower.contains(&pattern.to_lowercase()) {
                return Err(format!("Path traversal detected: {pattern}"));
            }
        }

        // Check body for SQL injection and XSS.
        let body_str = String::from_utf8_lossy(body);
        let body_lower = body_str.to_lowercase();

        for pattern in &self.sql_injection_patterns {
            if body_lower.contains(&pattern.to_lowercase()) {
                return Err(format!("SQL injection pattern detected: {pattern}"));
            }
        }

        for pattern in &self.xss_patterns {
            if body_lower.contains(&pattern.to_lowercase()) {
                return Err(format!("XSS pattern detected: {pattern}"));
            }
        }

        // Also scan the full request line (method + path) for injection.
        let full = format!("{method} {path}").to_lowercase();
        for pattern in &self.sql_injection_patterns {
            if full.contains(&pattern.to_lowercase()) {
                return Err(format!("SQL injection in path: {pattern}"));
            }
        }
        for pattern in &self.xss_patterns {
            if full.contains(&pattern.to_lowercase()) {
                return Err(format!("XSS pattern in path: {pattern}"));
            }
        }

        Ok(())
    }
}

impl Default for WafChecker {
    fn default() -> Self {
        Self::new()
    }
}

// ════════════════════════════════════════════════════════════════════════════
//  Tests
// ════════════════════════════════════════════════════════════════════════════

#[cfg(test)]
mod tests {
    use super::*;

    // ── RateLimiter tests ──────────────────────────────────────────────

    #[test]
    fn test_rate_limiter_allows_within_limit() {
        let mut limiter = RateLimiter::new();
        for _ in 0..60 {
            assert!(limiter.check_rate("192.168.1.1", 60));
        }
    }

    #[test]
    fn test_rate_limiter_blocks_over_limit() {
        let mut limiter = RateLimiter::new();
        for _ in 0..60 {
            assert!(limiter.check_rate("192.168.1.1", 60));
        }
        // 61st request should be blocked.
        assert!(!limiter.check_rate("192.168.1.1", 60));
    }

    #[test]
    fn test_rate_limiter_separate_ips() {
        let mut limiter = RateLimiter::new();
        for _ in 0..60 {
            assert!(limiter.check_rate("192.168.1.1", 60));
        }
        // Different IP gets its own counter.
        assert!(limiter.check_rate("192.168.1.2", 60));
    }

    #[test]
    fn test_rate_limiter_cleanup() {
        let mut limiter = RateLimiter::new();
        limiter.check_rate("10.0.0.1", 100);
        assert_eq!(limiter.tracked_ip_count(), 1);
        // Fresh entries are not removed by cleanup.
        limiter.cleanup();
        assert_eq!(limiter.tracked_ip_count(), 1);
    }

    #[test]
    fn test_rate_limiter_zero_limit() {
        let mut limiter = RateLimiter::new();
        assert!(!limiter.check_rate("10.0.0.1", 0));
    }

    // ── WAF tests ──────────────────────────────────────────────────────

    #[test]
    fn test_waf_allows_normal_request() {
        let waf = WafChecker::new();
        assert!(waf
            .check_request(
                "POST",
                "/api/auth/login",
                br#"{"username":"teacher","password":"hashed"}"#,
            )
            .is_ok());
    }

    #[test]
    fn test_waf_blocks_sql_injection() {
        let waf = WafChecker::new();
        assert!(waf
            .check_request("POST", "/api/auth/login", b"' OR 1=1 --")
            .is_err());
        assert!(waf
            .check_request("POST", "/api", b"UNION SELECT * FROM users")
            .is_err());
        assert!(waf
            .check_request("POST", "/api", b"DROP TABLE users")
            .is_err());
        assert!(waf
            .check_request("POST", "/api", b"; -- comment")
            .is_err());
    }

    #[test]
    fn test_waf_blocks_xss() {
        let waf = WafChecker::new();
        assert!(waf
            .check_request("POST", "/api", b"<script>alert('xss')</script>")
            .is_err());
        assert!(waf
            .check_request("POST", "/api", b"javascript:alert(1)")
            .is_err());
        assert!(waf
            .check_request("POST", "/api", b"<img onerror=alert(1)>")
            .is_err());
        assert!(waf
            .check_request("POST", "/api", b"<iframe src=evil.com>")
            .is_err());
    }

    #[test]
    fn test_waf_blocks_path_traversal() {
        let waf = WafChecker::new();
        assert!(waf
            .check_request("GET", "/api/../etc/passwd", b"")
            .is_err());
        assert!(waf
            .check_request("GET", "/api/..\\windows\\system32", b"")
            .is_err());
    }

    #[test]
    fn test_waf_allows_safe_path() {
        let waf = WafChecker::new();
        assert!(waf.check_request("GET", "/api/homework/123", b"").is_ok());
        assert!(waf.check_request("POST", "/api/homework/submit", b"").is_ok());
    }

    #[test]
    fn test_waf_allows_empty_body() {
        let waf = WafChecker::new();
        assert!(waf.check_request("GET", "/api/homework/result/abc", b"").is_ok());
    }
}
