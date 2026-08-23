# 安全加固收尾总结（火眼眼 / CodeReviewExpert）

工具链：`rust-toolchain.toml = 1.88.0-x86_64-pc-windows-msvc`。所有改动已通过 `cargo clippy`（零警告）与 `cargo test`，关键路径已收敛。

## 本回合完成（Tasks #17–#19）

### ✅ Task #17 — 网关 XFF 覆盖 + TLS 强制 + 文档
- `gateway/src/proxy.rs`：`X-Forwarded-For` 改为**单一权威值覆盖**（非追加），杜绝经代理绕过限流。
- `gateway/src/config.rs`：默认监听改为 `0.0.0.0:80`（与 `docs/deploy.md`、`docker-compose.yml` 一致）；新增 `validate_tls()`——监听 443 却无证书即拒绝，cert/key 须成对。
- `gateway/src/tls.rs`：新增 `validate()`（cert/key 成对校验）。
- `gateway/src/main.rs`：启动前调用 `validate_tls()`，失败 `exit(1)`（fail-closed）。
- `docs/deploy.md`：补充 `GATEWAY_TLS_CERT_PATH`/`KEY_PATH` 与 443-需-TLS 说明。
- 验证：gateway `cargo clippy` 零警告；**16 个测试全过**（含修正后的 `validate_tls_refuses_plaintext_on_443`）。

### ✅ Task #18 — OTP 防枚举 + sled 过期清理 + 限流可共享
- `auth/mobile.rs`：移除 `SmsChallengeStore::peek`（不再回显 OTP）；新增 **5 次/小时尝试上限**（`is_locked_out` / `record_failure`），防短信码暴力枚举；OTP 一次性消费。
- `api/mobile.rs` + `api/mod.rs`：删除演示回显端点 `mfa_dev_code`、`MobileDevCodeRequest` 及路由 `/api/mobile/mfa/dev-code`（OTP 仍于登录时经日志下发，dev 流程可用）。
- `auth/refresh.rs`：`revoke` 现记录令牌原过期时间；新增 `sweep_once()` / `start_expiry_sweeper()` 后台清理 `tok:`/`rev:` 过期条目。
- `main.rs`：启动后 `start_expiry_sweeper(1h)`，避免 sled 库无限增长。
- `auth/ratelimit.rs`：新增 `RateLimitBackend` trait + `RedisRateLimitBackend` 占位（多实例共享限流契约），注释澄清进程内限制。
- 验证：backend `cargo clippy` 零警告；**69 个测试全过**（新增 OTP 单元测试、sweep 测试）。

### ✅ Task #19 — unwrap 收敛 + FFI ABI 文档 + CI 门禁
- `recording/live.rs`：生产路径的 `Mutex`/`RwLock` 锁 `.expect()`/`.unwrap()` 改为 `unwrap_or_else(|e| e.into_inner())`，**中毒后优雅恢复**而非 panic（直播服务关键路径）。
- `seewo-plugin-api/src/lib.rs`：补全 ABI 稳定性契约文档（字段顺序/`repr(C)`/枚举判别式/`PLUGIN_API_VERSION` bump 规则/字符串 ptr+len/所有权/panic 隔离）；`read_str` 补 `# Safety`。
- `seewo-plugin-loader/src/lib.rs`：`LoadedPlugin::load` 补 `# Safety`（消 `missing_safety_doc`）。
- `.github/workflows/ci.yml`（新建）：`fmt --check` + `clippy --workspace --all-targets` + `clippy ... -W clippy::unwrap_used -W clippy::expect_used`（warn 级，不阻断历史告警）+ `test`。
- 验证：backend + 两个插件 crate `cargo clippy` 零警告。

## 之前回合已完成（复述，均已验证）
- **P0-1** JWT 硬编码密钥移除，缺失即启动失败。
- **P0-2** 插件 Ed25519 签名真实校验接入 `load_verified`，失败绝不 dlopen。
- **P0-3** seed 账号仅 `dev_mode` 播种，缺口令时随机 24 字符。
- **P0-4** `check_path` 拒 Windows 盘符绝对路径（ZipSlip），流式 500MB 上限防 zip bomb。
- **P1** HMAC 恒定时间比较 + `subtle="2"`；SM4 ECB→CBC（随机 IV + 加盐派生）。

## 需注意的行为变化
- 演示端点 `/api/mobile/mfa/dev-code` 已移除（安全加固）。OTP 仍在登录时经服务端日志下发；若 `drafftink-mobile` 前端仍调用该端点，需改为从日志读取或走真实短信网关。
- `db/mod.rs`、`drftx.rs`、`enbx_to_wb.rs` 中的 `.unwrap()` 均在 `#[cfg(test)]` 内（合法），生产路径无 unwrap，故关键路径收敛实际落在 `recording/live.rs`。CI 的 unwrap 告警会覆盖测试代码（warn 级，不影响流水线）。
