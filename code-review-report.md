# 代码审查报告 — seewo-class-mvp（Rust 工作区）

> 审查范围：32 个 crate、约 258 个 `.rs` 文件、~40k 行。重点聚焦认证/授权、密码学、文件导入（ZipSlip）、原生插件加载、网关与传输层。
> 审查方式：静态源码走查 + 危险模式扫描（`unsafe` / `unwrap` / 硬编码密钥 / 路径处理）。**未执行编译或 `cargo clippy`**（工作区依赖 wgpu/egui 等需系统 C 库，当前环境编译成本过高），建议后续补充。

---

## 一、总体印象

**技术栈**：纯 Rust 后端（`axum` + `tokio` + `sled`），egui/eframe 桌面端，wgpu 渲染，自研 SM4/Ed25519 国密实现，原生 `cdylib` 插件系统。

**优点很扎实**：密码学基础（Argon2id + OS RNG 盐）、JWT 设计（过期校验、类型 `typ` 区分、`jti` 吊销/轮换）、多租户隔离与 RBAC、SM4 标准测试向量（KAT）自检、插件 FFI 的 panic 隔离，都做得到位——说明团队有安全意识。

**但存在几个「可被直接利用」的硬伤**，集中在三处：**默认密钥硬编码、插件信任模型失效、传输层未加密**。这些不是风格问题，而是部署即用即危的漏洞。下面按优先级展开。

---

## 二、🔴 必须修复（Blockers）

### 1. 三处硬编码 JWT 默认密钥 —— 可伪造任意令牌（含管理员）
- `crates/drafftink-backend/src/config.rs:37` → `b"drafftink-backend-default-secret"`
- `crates/drafftink-gateway/src/config.rs:39` 与 `:64` → `b"drafftink-default-secret-change-me"`
- `crates/drafftink-core/src/crypto.rs:63`（`JwtConfig::default()`）→ 同上

**Why**：当环境变量（`DRAFTTINK_JWT_SECRET` / `GATEWAY_JWT_SECRET`）未设置时，`from_env()` 直接回退到这些**众所周知的字面量**。攻击者在知道默认值的情况下，可本地用该密钥签发任意 `sub`/`role`/`tenant_id` 的访问令牌——包括 `Role::Admin`。网关的 `verify_jwt` 正是用 `JwtConfig`（默认密钥）做鉴权闸门，后端 `verify_access_token` 同理。这意味着**默认部署下，任何人都可伪造管理员身份**。

**Suggestion**：
- 启动时若检测到密钥为默认值（或环境变量缺失），**直接拒绝启动**（或随机生成并持久化到文件，重启沿用）。
- 至少：移除 `Default` 中的字面量，改由 `from_env()` 在缺失时 `panic!`/返回错误，而非静默回退。
- 网关与后端必须共用同一密钥来源，避免一方用默认、一方用真实值导致信任错配。

### 2. 插件加载「签名校验」形同虚设 —— `trusted_key` 从未被使用
- `crates/drafftink-core/src/plugin/loader.rs:40-41` 提供 `with_trusted_key(...)`，但 `load_verified`（`:127`）**从始至终未读取 `self.trusted_key`**。
- 校验逻辑实质是：`dev_known = allowed_devs.iter().any(|d| d == &manifest.author)`（`:184`）。而 `manifest.author` 来自插件**自身清单**，完全由攻击者控制。
- 更糟：`DrafftinkPluginLoader::load_all` / `PluginManager::load_all`（`:238`、`:420`）对插件目录下**任意** `.dll/.so/.dylib` 直接 `dlopen` 并执行 `initialize()`，无任何签名要求。

**Why**：`signing.rs` 里完整的 Ed25519 校验逻辑（`verify_plugin_signature`）确实存在，却没接到加载流程。结果是——只要把一个 `author` 字段写成信任列表里的名字（如 `"official"`），恶意 DLL 就会被自动加载并执行任意原生代码。这是典型的**供应链/代码执行信任失效**。

**Suggestion**：
- 在 `load_verified` 中真正接入 `trusted_key`：加载前先用 Ed25519 公钥验证插件二进制/清单签名，失败即拒绝。
- 或将 `load_all` 改为仅加载**已签名且通过校验**的插件；对 `DrafftinkPluginLoader` 同样加签名门。
- 当前这套「按 author 字符串信任」应视为 placeholder，不可用于生产。

### 3. 默认演示账号弱口令，且无 dev_mode 闸门
- `crates/drafftink-backend/src/main.rs:158-226`：`seed_default_data` 在数据库为空时**无条件**播种 `admin/admin123`、`teacher01/teacher123`、`student01/student123`。

**Why**：种子函数未检查 `config.dev_mode`，生产环境首次启动即创建弱口令管理员。叠加第 1 条（默认密钥），攻击者可用 `admin/admin123` 直接登录（或干脆伪造令牌）。这是经典「默认凭据」高危项。

**Suggestion**：
- 用 `dev_mode` 闸门包裹种子逻辑；生产默认不播种。
- 若必须播种，强制首次运行改密（如随机生成一次性密码并写入运维日志/交互式输入），或要求环境变量覆盖初始口令。

### 4. Zip 导入路径校验不完整（盘符绝对路径漏检）—— ZipSlip 隐患
- `crates/enbx_importer/src/security.rs:10-15` `check_path` 仅拒绝 `..`、前导 `/`、前导 `\`。
- 对比 `crates/drafftink-backend/src/storage/local.rs:29-46` `resolve_path`：额外拒绝了**盘符绝对路径**（`C:` 等）。

**Why**：`check_path` 漏掉了 `C:\windows\system32\evil.dll` 这类 Windows 绝对路径（不以 `/` 或 `\` 开头）。若该文件名出现在压缩包条目中，它会绕过 `check_zip_bomb` 的校验。当前 `extract_resources`（`lib.rs:153-187`）用 `dest.join(fname)` 落盘，且 `fname` 来自 `Reference.xml`（攻击者可控），虽因 `Resources/{fname}` 前缀间接限制了实际写入，但防御链条脆弱、依赖巧合。

**Suggestion**：
- 复用/统一到与 `LocalStorage::resolve_path` 等价的标准逻辑（拒绝 `..`、绝对路径、**盘符**）。
- 对 `extract_resources` 中来自 `ref_map` 的 `fname` 也调用同一 `check_path`，而非仅校验 zip 条目标称名。
- 更稳妥：对所有写出路径做 `canonicalize()` 后确认前缀仍位于 `dest` 内（canonicalization guard）。

---

## 三、🟡 应当修复（Suggestions）

### 5. JWT 签名比较非恒定时间 —— 时序侧信道
- `crates/drafftink-core/src/crypto.rs:129` 与 `:162`：`if expected_sig != actual_sig`。

**Why**：`Vec<u8>` 的 `!=` 不是恒定时间比较。理论上攻击者可通过测量响应耗时逐字节爆破 HMAC。虽利用门槛高，但是密码学实现的基本准则违反，且修复成本极低。

**Suggestion**：改用 `subtle::ConstantTimeEq`（`expected_sig.ct_eq(&actual_sig).into()`），或换用 `hmac` 库的 `verify_slice`。

### 6. SM4 使用 ECB 模式 + 弱密钥派生 —— 敏感数据信封加密不足
- `crates/drafftink-core/src/sm4.rs:8` 注释明确「默认 ECB + PKCS#7」；`crates/drafftink-backend/src/auth/mobile.rs:144-162` 用它加密 JSON 载荷。
- 密钥派生 `derive_sm4_key`（`mobile.rs:134-142`）：`SHA256(device_fp + server_secret)` 取前 16 字节。`device_fp` 是公开的设备标识（请求头里传来），并非秘密。

**Why**：ECB 不隐藏明文模式，相同明文/相同密钥 → 相同密文，结构化数据（如审批意见）会泄露结构。密钥材料里一半是公开量（device_fp），一旦 `server_secret` 用了第 1 条的默认硬编码值，整条信封加密即失效。

**Suggestion**：
- 改用 SM4-CBC 或 SM4-GCM，附带随机 IV/nonce（GCM 还能提供完整性）。
- 密钥派生加盐（如 `HKDF`），不要直接 `SHA256(public || secret)` 截断。
- 注意：`sm4.rs` 实现本身正确性良好（KAT 全过），问题在**使用方式**而非算法实现。

### 7. 网关 X-Forwarded-For 透传 + 追加 —— 绕过后端限流
- `crates/drafftink-gateway/src/proxy.rs:78-83`：先遍历转发**原始**所有请求头（含客户端自带的 `x-forwarded-for`），又 `.header("x-forwarded-for", client_ip)` 再追加一条。
- 后端 `client_ip`（`auth/mod.rs:135-144`）取 `x-forwarded-for` 的**第一个**值（`split(',').next()`）。

**Why**：原始 XFF 排在前面 → 后端读到的「客户端 IP」是攻击者可伪造的值。后端登录限流按该 IP 计数，于是攻击者可不断变更 XFF 头部**绕过每 IP 限流**做暴力破解。

**Suggestion**：网关应**覆盖**而非追加 XFF（用 `set` 而非转发原值），并仅在可信入口场景拼接真实客户端 IP；后端限流 IP 应取网关直连的对端地址。

### 8. WAF 为黑名单，易被绕过（仅纵深防御）
- `crates/drafftink-gateway/src/security.rs:83-178`：基于少量固定子串的 SQLi/XSS/路径遍历检测。

**Why**：子串匹配可被轻易绕过（`OR 1=1`、`UNION ALL SELECT`、`/**/`、`SLEEP(`、双重编码、分块载荷等均未覆盖）；且只扫 path+body，不处理 URL 解码后的 query/header。值得肯定的是它提供了基础防御，但**绝不能**当作主防线。

**Suggestion**：明确其「尽力而为」定位；真正的安全来自参数化/结构化存储（后端用的是 sled KV，无 SQL 注入面）与输出编码。可考虑升级到更全的规则集或专用 WAF。

### 9. 传输层未加密（网关 HTTP-only 占位）
- `crates/drafftink-gateway/src/tls.rs` 仅是占位文档，`TlsConfig` 默认 `None`；`config.rs:41` `tls_cert_path: None`。
- 但 `cookies` 设置了 `Secure; SameSite=Strict`（`api/auth.rs:221-225`）。

**Why**：默认无 TLS，凭据、JWT、SM4 密文均以明文传输；而 `Secure` Cookie 在 HTTP 下又根本不会发送——自相矛盾。监听地址默认 `0.0.0.0:443` 却跑明文，是危险的配置错觉。

**Suggestion**：生产必须通过前置 TLS 终止代理或启用 `rustls`；强制 `GATEWAY_TLS_*` 缺失时拒绝以明文监听 443。

### 10. 短信 OTP 偏弱 + 存在 `peek` 回显
- `crates/drafftink-backend/src/auth/mobile.rs:41-48`：6 位码；`:67-69` 提供 `peek()` 直接读取当前验证码。
- 注释承认「演示环境以日志/接口回显」。

**Why**：6 位仅 ~10⁶ 空间，且 `peek`/回显接口在演示外存在即构成泄露面；OTP 尝试未单独限流。

**Suggestion**：生产移除 `peek`；OTP 校验增加限流与尝试次数上限；考虑更长随机码或 TOTP。

### 11. 刷新令牌 sled 存储无过期清理 —— 无限增长
- `crates/drafftink-backend/src/auth/refresh.rs:64-77`：`store` 写入 `tok:` 键但**从不清理**，吊销只写 `rev:` 占位。

**Why**：长期运行后 `tok:`/`rev:` 键只增不减，sled 文件无限膨胀。

**Suggestion**：周期性删除 `exp < now` 的 `tok:` 键；或干脆用 TTL 语义的实现。

### 12. 登录限流为进程内 —— 多实例失效
- `crates/drafftink-backend/src/auth/ratelimit.rs` 与 `gateway/security.rs` 均为内存 `HashMap`。

**Why**：多副本部署时每实例独立计数，攻击者分散请求即可绕过。代码注释已承认，需提示运维。

**Suggestion**：共享存储（Redis）或网关层统一限流；至少在文档中标注该限制。

### 13. `unwrap`/`expect` 集中，长寿命服务存在 panic 风险
按文件统计（部分）：`db/mod.rs:20`、`drftx.rs:27`、`enbx_to_wb.rs:15`、`drafftink-enbx/generator.rs:13`、`mapper.rs:19`、`cache.rs:15`、`recording/live.rs:13`、`drafftink-core/crypto.rs:15` 等。

**Why**：后端是常驻服务，关键路径上的 `unwrap`/`expect` 会让单个异常输入/状态导致整个进程崩溃（DoS）。

**Suggestion**：将初始化/解析路径的 `unwrap` 改为 `?` 传播或显式错误处理；对 `Option`/`Result` 显式处理。可借助 `cargo clippy -W clippy::unwrap_used` 在 CI 中强制收敛。

### 14. Zip-Bomb 检查依赖攻击者可控的元数据
- `crates/enbx_importer/src/security.rs:17-41`：`f.size()`、`f.compressed_size()` 取自 zip 头部。

**Why**：压缩大小可由攻击者伪造，比率检查（`uncomp/comp > 100`）可被绕过；解压时的真实膨胀无法仅靠头部预估。

**Suggestion**：边解压边累计已解压字节并设置硬上限（streaming 上限），超限立即中止。

### 15. 插件 FFI `Box<dyn Plugin>` 跨 cdylib 边界 —— ABI 脆弱
- `crates/drafftink-core/src/plugin/loader.rs:88,178`、`seewo-plugin-loader`：把 `Box<dyn Plugin>` 跨动态库边界传递。

**Why**：Rust 没有稳定 ABI，`Box<dyn Trait>` 的 vtable 布局在不同编译器版本/优化下可能错位，导致内存不安全或崩溃。这是稳定性/内存安全隐患（非纯逻辑）。

**Suggestion**：跨 FFI 边界改用 `#[repr(C)]` 的 C 风格结构体 + `extern "C"` 函数指针（如 `seewo-plugin-loader` 的做法），或统一用相同工具链编译插件。

---

## 四、💭 细节 / 建议（Nits）

- **有意 DLL 泄漏**：`loader.rs:300-302,438,471` 用 `std::mem::forget(_lib)` 规避 Windows `FreeLibrary` 死锁。属于已知取舍，记录备查；长期应考虑线程安全的卸载方案。
- **插件 `execute` 固定 256KB 输出缓冲**（`seewo-plugin-loader:84`）：依赖插件不越界写入——在当前「信任插件」边界内可接受，但一旦结合第 2 条的签名缺失，恶意插件可造成堆溢出。建议加边界校验。
- **`.ok()` 吞错误**：`enbx_importer/src/lib.rs:62,80,93,127` 等多处 `read_to_end(...).ok()`，读取失败静默继续，可能导致解析到空/残缺数据而不报错。
- **`recording/live.rs:249,265` 的 `panic!("应为媒体帧")`**：非穷尽 `match` 的兜底 panic，处于媒体流热路径，异常帧会直接崩进程。
- **`generate_device_fingerprint`（`crypto.rs:187`）** 将主机名、用户名、PID 拼入指纹——同一机器同用户指纹稳定，但 PID 在重启后变化，导致「设备绑定」跨进程不稳定（功能而非安全）。

---

## 五、亮点（值得表扬）👍

- **密码哈希**：`password.rs` Argon2id + `OsRng` 随机盐，校验失败统一返回 `false` 不泄露原因——标准做法。
- **JWT 设计**：`jwt.rs` 显式 `validate_exp=true`、区分 `access`/`refresh` 的 `typ`、刷新令牌 `jti` 吊销与轮换——完整且正确。
- **多租户隔离 + RBAC**：`rbac.rs` 的 `ensure_tenant_access` / `check_teacher_owns_class_in_tenant` 形成双层兜底，注释清楚。
- **存储路径校验**：`storage/local.rs` 正确拦截 `..`、绝对路径、**盘符**，且有单元测试——应作为全仓路径校验的范本（见第 4 条）。
- **国密实现质量**：`sm4.rs` 自带 GM/T 0002-2012 标准向量与 S-box 自检，正确性有保障。
- **插件 panic 隔离**：`catch_unwind(AssertUnwindSafe(...))` 包裹 FFI 调用，避免插件 panic 拖垮宿主。
- **测试覆盖**：auth/jwt/rbac/ratelimit/security 均有针对性单测，WAF 规则也有回归用例。

---

## 六、优先修复路线图

| 优先级 | 项 | 工作量 |
|---|---|---|
| P0 | 1. 启动拒绝默认 JWT 密钥 / 强制环境变量 | 小 |
| P0 | 2. 插件接入真实 Ed25519 签名校验 | 中 |
| P0 | 3. 闸控/移除默认演示账号弱口令 | 小 |
| P0 | 4. 统一路径校验（复用 `resolve_path` 逻辑） | 小 |
| P1 | 5. HMAC 恒定时间比较 | 极小 |
| P1 | 6. SM4 改 CBC/GCM + 加盐派生 | 中 |
| P1 | 7. 网关覆盖而非透传 XFF；后端限流取真实对端 IP | 小 |
| P1 | 9. 启用 TLS（拒绝明文监听 443） | 中 |
| P2 | 10/11/12. OTP 限流与 `peek` 移除、sled 过期清理、限流共享化 | 中 |
| P2 | 13/14/15. `unwrap` 收敛、流式 zip 上限、FFI ABI 加固 | 中~大 |
| 全程 | 补充 `cargo clippy`（`-W unwrap_used, clippy::too_many_*`）与 `cargo audit` 到 CI | 小 |

---

## 七、附录：关键统计

- 工作区 crate 数：32；`.rs` 文件数：~258；总行数：~40k。
- 危险模式扫描：
  - `unsafe {` 出现约 35 处，集中于原生插件加载（`libloading`）、窗口句柄 FFI、wgpu 渲染（多为合理用途，但插件加载面见第 2 条）。
  - `unwrap()`/`expect()`/`panic!` 在 `db/mod.rs`、`drftx.rs`、`enbx_to_wb.rs`、`generator.rs`、`cache.rs`、`live.rs` 等集中（详见第 13 条）。
- 密码学依赖：`argon2`、`jsonwebtoken`、`ed25519-dalek`、`sha2`、`hmac`（自实现）、`ecb` crate（在依赖树中，SM4 ECB 使用见第 6 条）。
- 尚未审查（建议后续跟进）：`drafftink-display` / `drafftink-app` / `drafftink-desktop` 的 UI 与渲染层、`drafftink-quiz` 的 WebSocket、`drafftink-etl`、`drafftink-migrator`、移动端 `drafftink-wasm`。这些模块体量更大，且涉及较多 `unsafe` 与文件 IO，宜单独成轮审查。
