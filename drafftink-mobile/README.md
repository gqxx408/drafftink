# 校园移动办公平台（drafftink-mobile）

校本教学套件（seewo-class-mvp）的移动办公 PWA 前端，对接 `drafftink-backend` 的移动办公
REST 接口。纯前端、无专有依赖，可安装到手机主屏离线使用。

## 技术栈

- **React 18 + TypeScript + Vite 5**
- **PWA**：手写 `manifest.webmanifest` + `sw.js`（离线缓存应用外壳，零额外构建依赖）
- **SM4 信封解密**：`sm-crypto`（ECB + PKCS#7），密钥由设备指纹与校内共享密钥本地派生
- 路由：`react-router-dom`（`HashRouter`，静态托管免服务端回退）
- 样式：原生 CSS（移动优先、轻量）

## 功能页面（7 + 登录/MFA）

| 页面 | 路由 | 后端接口 |
|------|------|----------|
| 登录 | `/login` | `POST /api/mobile/login` |
| 短信二次验证 | `/mfa` | `POST /api/mobile/mfa/verify`（演示码 `POST /api/mobile/mfa/dev-code`） |
| 工作台 | `/` | todos / announcements / messages 概览 |
| 待办审批 | `/approvals` | `GET /api/mobile/todos`、`POST /api/mobile/workflow/approve` |
| 公文流转 | `/official-doc` | `POST /api/mobile/workflow/start`（official_doc） |
| 通知公告 | `/announcements` | `GET /api/mobile/announcements` |
| 会议预约 | `/meeting` | `POST /api/mobile/meeting/book` |
| 用印申请 | `/seal` | `POST /api/mobile/seal/apply` |
| 消息中心 | `/messages` | `GET /api/mobile/messages`（SM4 解密正文） |
| AI 学情看板 | `/ai` | 离线聚合 todos/announcements/messages 的启发式洞察 |
| 我的 | `/me` | 个人资料、退出登录 |

## 运行

```bash
cd drafftink-mobile
npm install

# 开发模式（热更新）
npm run dev

# 生产构建
npm run build        # 输出到 dist/（注意：构建前需先 rm -rf dist，见下方说明）
npm run preview      # 本地预览 dist
```

### 与后端联调

后端默认监听 `0.0.0.0:8080` 且 CORS 允许任意来源，因此前端 `.env` 中
`VITE_API_BASE=http://localhost:8080` 即可直接对接，**无需开发代理**。

**演示模式**（无真实短信通道时完成 MFA）：

```bash
# 启动时开启开发模式，暴露短信验证码回显接口
DRAFTTINK_DEV_MODE=true cargo run -p drafftink-backend
```

随后在 MFA 页面点击「获取演示验证码」自动填入；生产环境务必删除该环境变量。

### 演示账号（后端 seed 数据）

| 角色 | 用户名 | 密码 |
|------|--------|------|
| 管理员 | `admin` | `admin123` |
| 教师 | `teacher01` | `teacher123` |
| 学生 | `student01` | `student123` |

> 学生仅有查看权限；待办审批/公文流转/用印申请需教师或管理员角色（RBAC）。

## SM4 信封密钥对齐

消息正文在后端以 `SM4(ECB + PKCS#7)` 加密（GB/T 32907-2016），密钥派生为
`SHA256(device_fp ‖ jwt_secret)[0..16]`，与后端 `auth::mobile::derive_sm4_key` 完全一致。

前端在 `.env` 中以 `VITE_SM4_SECRET` 预置「校内共享密钥」，其取值**必须与后端
`DRAFTTINK_JWT_SECRET` 相同**，否则消息正文无法解密。默认值 `drafftink-backend-default-secret`
与后端默认密钥一致。这是「预置到内部应用的信封密钥」模式，明文仅在设备本地出现，数据不出校。

## 构建说明（沙箱环境）

当前构建配置已设 `build.emptyOutDir: false`。若需重新构建，请先手动清理：

```bash
rm -rf dist && npm run build
```

## 安全与合规

- 访问令牌绑定设备指纹（`x-device-fp`），MFA 校验设备一致性。
- 敏感接口需 JWT 鉴权 + RBAC（角色：admin / teacher / student）。
- 短信验证码一次性消费；SSO 令牌符合 GB/T 36342-2018。
- 多租户数据按 `tenant_id` 隔离。
- 全部数据在校内闭环，不外发。
