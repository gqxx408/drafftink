# 校本教学套件 API 文档

本文档详细说明校本教学套件（seewo-class-mvp）的全部 HTTP API 接口。

---

## 目录

1. [通用说明](#1-通用说明)
2. [认证接口](#2-认证接口)
3. [作业接口](#3-作业接口)
4. [资源接口](#4-资源接口)
5. [健康检查](#5-健康检查)
6. [网关接口](#6-网关接口)
7. [错误码参考](#7-错误码参考)

---

## 1. 通用说明

### 1.1 基础地址

| 访问方式 | 基础地址 | 说明 |
|---------|---------|------|
| 内网直连 | `http://<服务器内网IP>:8080` | 在校内网络中直接访问后端 |
| 公网网关 | `http://<公网域名或IP>` | 通过网关访问，经过安全检查 |

### 1.2 请求格式

- 除文件上传接口使用 `multipart/form-data` 外，所有接口均使用 `application/json` 格式。
- 请求体编码为 UTF-8。
- 所有日期时间使用 ISO 8601 / RFC 3339 格式（如 `2024-03-15T14:30:00Z`）。
- 所有 ID 使用 UUID v4 格式（如 `550e8400-e29b-41d4-a716-446655440000`）。

### 1.3 认证方式

除登录和健康检查外，所有接口需要在请求头中携带 JWT 令牌：

```
Authorization: Bearer <token>
```

通过公网网关访问时，还需额外携带设备指纹头：

```
X-Device-FP: <设备指纹>
```

设备指纹是客户端生成的设备标识（SHA-256 哈希的十六进制字符串），登录时提交，后续请求中必须与登录时一致，否则网关将拒绝请求。

### 1.4 角色说明

系统采用基于角色的访问控制（RBAC），共三种角色：

| 角色 | 标识 | 权限范围 |
|------|------|---------|
| 管理员 | `admin` | 全校数据访问权限，可管理所有班级和用户 |
| 老师 | `teacher` | 仅访问自己负责的班级数据，可创建/批改作业 |
| 学生 | `student` | 仅访问自己所在班级的作业，可提交作业 |

---

## 2. 认证接口

### 2.1 用户登录

用户通过用户名和密码登录，获取 JWT 令牌。

| 项目 | 说明 |
|------|------|
| 方法 | `POST` |
| 路径 | `/api/auth/login` |
| 认证 | 不需要 |
| 角色 | 任意 |

**请求体：**

```json
{
  "username": "teacher01",
  "password": "teacher123",
  "device_fp": "a1b2c3d4e5f6..."
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `username` | string | 是 | 用户名 |
| `password` | string | 是 | 密码 |
| `device_fp` | string | 是 | 设备指纹（SHA-256 哈希的十六进制字符串，64 字符） |

**成功响应（200 OK）：**

```json
{
  "token": "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9...",
  "user": {
    "id": "550e8400-e29b-41d4-a716-446655440000",
    "username": "teacher01",
    "display_name": "王老师",
    "role": "teacher",
    "class_id": null
  }
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `token` | string | JWT 令牌，后续请求需在 Authorization 头中携带 |
| `user.id` | string (UUID) | 用户唯一 ID |
| `user.username` | string | 用户名 |
| `user.display_name` | string | 显示名称 |
| `user.role` | string | 角色（`admin` / `teacher` / `student`） |
| `user.class_id` | string (UUID) \| null | 班级 ID（学生才有值，老师和管理员为 null） |

**错误响应：**

| 状态码 | error | 说明 |
|--------|-------|------|
| 401 | `unauthorized` | 用户名或密码错误 |
| 403 | `forbidden` | 账号已被禁用 |

**示例（curl）：**

```bash
curl -X POST http://localhost:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{
    "username": "teacher01",
    "password": "teacher123",
    "device_fp": "a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890a1b2c3d4e5f67890"
  }'
```

---

### 2.2 用户登出

登出当前会话。

| 项目 | 说明 |
|------|------|
| 方法 | `POST` |
| 路径 | `/api/auth/logout` |
| 认证 | 需要 JWT |
| 角色 | 任意 |

**请求体：** 无

**成功响应（200 OK）：**

```json
{
  "status": "ok",
  "message": "已登出"
}
```

> 注意：当前 MVP 版本登出接口直接返回成功，不进行 JWT 吊销。令牌将在过期后自动失效（默认 24 小时）。后续版本将实现服务端令牌吊销。

**示例（curl）：**

```bash
curl -X POST http://localhost:8080/api/auth/logout \
  -H "Authorization: Bearer <token>"
```

---

### 2.3 获取当前用户信息

获取当前登录用户的详细信息。

| 项目 | 说明 |
|------|------|
| 方法 | `GET` |
| 路径 | `/api/auth/me` |
| 认证 | 需要 JWT |
| 角色 | 任意 |

**请求体：** 无

**成功响应（200 OK）：**

```json
{
  "id": "550e8400-e29b-41d4-a716-446655440000",
  "username": "student01",
  "display_name": "李同学",
  "role": "student",
  "class_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string (UUID) | 用户唯一 ID |
| `username` | string | 用户名 |
| `display_name` | string | 显示名称 |
| `role` | string | 角色（`admin` / `teacher` / `student`） |
| `class_id` | string (UUID) \| null | 所属班级 ID（学生才有值） |

**错误响应：**

| 状态码 | error | 说明 |
|--------|-------|------|
| 401 | `unauthorized` | 缺少 Authorization 头或 JWT 验证失败 |

**示例（curl）：**

```bash
curl -X GET http://localhost:8080/api/auth/me \
  -H "Authorization: Bearer <token>"
```

---

## 3. 作业接口

### 3.1 创建作业

老师为自己负责的班级创建作业。

| 项目 | 说明 |
|------|------|
| 方法 | `POST` |
| 路径 | `/api/homework/create` |
| 认证 | 需要 JWT |
| 角色 | `teacher` 或 `admin` |

**请求体：**

```json
{
  "title": "第三单元数学练习",
  "description": "完成课本第45-47页习题，注意书写规范",
  "class_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
  "content": "SGVsbG8gV29ybGQ=",
  "deadline": "2024-03-20T23:59:59Z"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `title` | string | 是 | 作业标题 |
| `description` | string | 是 | 作业描述/要求 |
| `class_id` | string (UUID) | 是 | 班级 ID（必须是当前老师负责的班级） |
| `content` | string | 是 | 作业内容，Base64 编码的二进制数据 |
| `deadline` | string | 是 | 截止时间，ISO 8601 格式 |

**成功响应（200 OK）：**

```json
{
  "id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "title": "第三单元数学练习",
  "status": "published"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string (UUID) | 作业唯一 ID |
| `title` | string | 作业标题 |
| `status` | string | 作业状态（创建后为 `published`） |

**错误响应：**

| 状态码 | error | 说明 |
|--------|-------|------|
| 400 | `bad_request` | Base64 解码失败或截止时间格式错误 |
| 403 | `forbidden` | 需要老师权限，或不是该班级的任课老师 |
| 404 | `not_found` | 班级不存在 |

**示例（curl）：**

```bash
curl -X POST http://localhost:8080/api/homework/create \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "title": "第三单元数学练习",
    "description": "完成课本第45-47页习题",
    "class_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
    "content": "SGVsbG8gV29ybGQ=",
    "deadline": "2024-03-20T23:59:59Z"
  }'
```

---

### 3.2 获取作业列表

获取当前用户可见的作业列表。老师看到自己布置的作业，学生看到所在班级的作业。

| 项目 | 说明 |
|------|------|
| 方法 | `GET` |
| 路径 | `/api/homework/list` |
| 认证 | 需要 JWT |
| 角色 | 任意 |

**请求体：** 无

**成功响应（200 OK）：**

```json
[
  {
    "id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
    "title": "第三单元数学练习",
    "description": "完成课本第45-47页习题",
    "class_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
    "deadline": "2024-03-20T23:59:59Z",
    "status": "published"
  },
  {
    "id": "a1b2c3d4-e5f6-7890-abcd-ef1234567890",
    "title": "第四单元语文阅读",
    "description": "阅读课文并回答问题",
    "class_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
    "deadline": "2024-03-25T23:59:59Z",
    "status": "published"
  }
]
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `[].id` | string (UUID) | 作业唯一 ID |
| `[].title` | string | 作业标题 |
| `[].description` | string | 作业描述 |
| `[].class_id` | string (UUID) | 班级 ID |
| `[].deadline` | string | 截止时间（ISO 8601） |
| `[].status` | string | 作业状态（`draft` / `published` / `closed` / `archived`） |

**权限说明：**

| 角色 | 可见范围 |
|------|---------|
| 老师 / 管理员 | 自己布置的所有作业 |
| 学生 | 所在班级的所有已发布作业 |

**错误响应：**

| 状态码 | error | 说明 |
|--------|-------|------|
| 400 | `bad_request` | 学生未关联班级 |
| 401 | `unauthorized` | JWT 验证失败 |

**示例（curl）：**

```bash
curl -X GET http://localhost:8080/api/homework/list \
  -H "Authorization: Bearer <token>"
```

---

### 3.3 获取作业详情

根据作业 ID 获取单个作业的详细信息，包括作业内容。

| 项目 | 说明 |
|------|------|
| 方法 | `GET` |
| 路径 | `/api/homework/:id` |
| 认证 | 需要 JWT |
| 角色 | 任意（受班级权限限制） |

**路径参数：**

| 参数 | 类型 | 说明 |
|------|------|------|
| `id` | string (UUID) | 作业唯一 ID |

**成功响应（200 OK）：**

```json
{
  "id": "f47ac10b-58cc-4372-a567-0e02b2c3d479",
  "title": "第三单元数学练习",
  "description": "完成课本第45-47页习题，注意书写规范",
  "teacher_id": "550e8400-e29b-41d4-a716-446655440000",
  "class_id": "6ba7b810-9dad-11d1-80b4-00c04fd430c8",
  "content": "SGVsbG8gV29ybGQ=",
  "created_at": "2024-03-15T10:30:00Z",
  "deadline": "2024-03-20T23:59:59Z",
  "status": "published"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `id` | string (UUID) | 作业唯一 ID |
| `title` | string | 作业标题 |
| `description` | string | 作业描述 |
| `teacher_id` | string (UUID) | 布置老师 ID |
| `class_id` | string (UUID) | 班级 ID |
| `content` | string | 作业内容，Base64 编码 |
| `created_at` | string | 创建时间（ISO 8601） |
| `deadline` | string | 截止时间（ISO 8601） |
| `status` | string | 作业状态 |

**权限说明：**

| 角色 | 访问限制 |
|------|---------|
| 学生 | 仅能访问自己所在班级的作业 |
| 老师 | 仅能访问自己布置的作业 |
| 管理员 | 可访问所有作业 |

**错误响应：**

| 状态码 | error | 说明 |
|--------|-------|------|
| 403 | `forbidden` | 学生不属于该班级，或老师不是该作业的布置者 |
| 404 | `not_found` | 作业不存在 |

**示例（curl）：**

```bash
curl -X GET http://localhost:8080/api/homework/f47ac10b-58cc-4372-a567-0e02b2c3d479 \
  -H "Authorization: Bearer <token>"
```

---

### 3.4 提交作业

学生提交 drftx 格式的作业文件。系统会验证文件的 Ed25519 签名和 CRC32 完整性。

| 项目 | 说明 |
|------|------|
| 方法 | `POST` |
| 路径 | `/api/homework/submit` |
| 认证 | 需要 JWT |
| 角色 | `student` |
| Content-Type | `multipart/form-data` |

**请求体（multipart 表单）：**

| 字段名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `homework_id` | text | 是 | 作业 ID（UUID 格式） |
| `file` | binary | 是 | drftx 作业文件（二进制数据） |

**成功响应（200 OK）：**

```json
{
  "submission_id": "d4a3b2c1-1234-5678-9abc-def012345678",
  "status": "submitted"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `submission_id` | string (UUID) | 提交记录唯一 ID |
| `status` | string | 提交状态（`submitted`） |

**drftx 文件验证流程：**

1. 解析文件头（魔数 `DRFT`、版本号）
2. 校验 CRC32 完整性（文件尾部 4 字节）
3. 验证快照层 SHA-256 内容哈希
4. 验证 Ed25519 数字签名（使用学生公钥验证快照哈希）
5. 校验文件中的 `homework_id` 和 `student_id` 与请求一致

**错误响应：**

| 状态码 | error | 说明 |
|--------|-------|------|
| 400 | `bad_request` | 缺少字段、drftx 文件验证失败、ID 不匹配 |
| 403 | `forbidden` | 需要学生权限，或学生不属于该作业的班级 |
| 404 | `not_found` | 作业不存在 |

**示例（curl）：**

```bash
curl -X POST http://localhost:8080/api/homework/submit \
  -H "Authorization: Bearer <token>" \
  -F "homework_id=f47ac10b-58cc-4372-a567-0e02b2c3d479" \
  -F "file=@/path/to/homework.drftx"
```

---

### 3.5 批改作业

老师批改学生提交的作业，写入评分和评语。批注信息写入 drftx 文件的批注层，不影响原始快照层。

| 项目 | 说明 |
|------|------|
| 方法 | `POST` |
| 路径 | `/api/homework/grade` |
| 认证 | 需要 JWT |
| 角色 | `teacher` 或 `admin` |

**请求体：**

```json
{
  "submission_id": "d4a3b2c1-1234-5678-9abc-def012345678",
  "score": 95.0,
  "comments": "做得很好！计算过程清晰，书写规范。第3题注意单位换算。"
}
```

| 字段 | 类型 | 必填 | 说明 |
|------|------|------|------|
| `submission_id` | string (UUID) | 是 | 提交记录 ID |
| `score` | number (float) | 是 | 分数（0-100） |
| `comments` | string | 是 | 评语 |

**成功响应（200 OK）：**

```json
{
  "submission_id": "d4a3b2c1-1234-5678-9abc-def012345678",
  "status": "graded",
  "score": 95.0
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `submission_id` | string (UUID) | 提交记录 ID |
| `status` | string | 提交状态（`graded`） |
| `score` | number (float) | 分数 |

**错误响应：**

| 状态码 | error | 说明 |
|--------|-------|------|
| 403 | `forbidden` | 需要老师权限，或不是该班级的任课老师 |
| 404 | `not_found` | 提交记录或作业不存在 |

**示例（curl）：**

```bash
curl -X POST http://localhost:8080/api/homework/grade \
  -H "Authorization: Bearer <token>" \
  -H "Content-Type: application/json" \
  -d '{
    "submission_id": "d4a3b2c1-1234-5678-9abc-def012345678",
    "score": 95.0,
    "comments": "做得很好！计算过程清晰。"
  }'
```

---

## 4. 资源接口

### 4.1 上传资源

上传文件到服务器，返回存储路径。可用于上传课件、图片等教学资源。

| 项目 | 说明 |
|------|------|
| 方法 | `POST` |
| 路径 | `/api/resource/upload` |
| 认证 | 需要 JWT |
| 角色 | 任意 |
| Content-Type | `multipart/form-data` |
| 最大请求体 | 50 MB（后端限制）/ 10 MB（网关限制） |

**请求体（multipart 表单）：**

| 字段名 | 类型 | 必填 | 说明 |
|--------|------|------|------|
| `file` | binary | 是 | 要上传的文件（二进制数据） |

**成功响应（200 OK）：**

```json
{
  "path": "resources/a1b2c3d4-e5f6-7890-abcd-ef1234567890/courseware.pptx",
  "size": 1048576
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `path` | string | 文件存储路径（用于后续下载） |
| `size` | number | 文件大小（字节） |

**错误响应：**

| 状态码 | error | 说明 |
|--------|-------|------|
| 400 | `bad_request` | 缺少 file 字段或文件解析失败 |
| 401 | `unauthorized` | JWT 验证失败 |

**示例（curl）：**

```bash
curl -X POST http://localhost:8080/api/resource/upload \
  -H "Authorization: Bearer <token>" \
  -F "file=@/path/to/courseware.pptx"
```

---

### 4.2 下载资源

根据存储路径下载资源文件。

| 项目 | 说明 |
|------|------|
| 方法 | `GET` |
| 路径 | `/api/resource/*path` |
| 认证 | 需要 JWT |
| 角色 | 任意 |

**路径参数：**

路径为 catch-all 模式，`*path` 捕获 `/api/resource/` 之后的所有内容（含子路径）。

例如：`/api/resource/resources/a1b2c3d4-e5f6-7890-abcd-ef1234567890/courseware.pptx`

**成功响应（200 OK）：**

响应体为文件二进制数据，Content-Type 为 `application/octet-stream`。

**错误响应：**

| 状态码 | error | 说明 |
|--------|-------|------|
| 404 | `not_found` | 文件不存在 |
| 401 | `unauthorized` | JWT 验证失败 |

**示例（curl）：**

```bash
curl -X GET http://localhost:8080/api/resource/resources/a1b2c3d4-e5f6-7890-abcd-ef1234567890/courseware.pptx \
  -H "Authorization: Bearer <token>" \
  -o courseware.pptx
```

---

## 5. 健康检查

### 5.1 健康检查接口

用于检测后端服务是否正常运行。Docker 容器健康检查和负载均衡器探针使用此接口。

| 项目 | 说明 |
|------|------|
| 方法 | `GET` |
| 路径 | `/api/health` |
| 认证 | 不需要 |
| 角色 | 任意 |

**请求体：** 无

**成功响应（200 OK）：**

```json
{
  "status": "ok"
}
```

| 字段 | 类型 | 说明 |
|------|------|------|
| `status` | string | 服务状态（`ok` 表示正常） |

**示例（curl）：**

```bash
curl http://localhost:8080/api/health
```

---

## 6. 网关接口

公网网关是后端的安全代理，仅暴露部分接口。通过网关访问时，除登录外所有接口均需同时提供 JWT 令牌和设备指纹。

### 6.1 网关暴露的接口

| 方法 | 路径 | 认证要求 | 说明 |
|------|------|---------|------|
| POST | `/api/auth/login` | 无 | 转发到后端登录接口 |
| GET | `/api/homework/:id` | JWT + 设备指纹 | 获取作业详情 |
| POST | `/api/homework/submit` | JWT + 设备指纹 | 提交作业 |
| GET | `/api/homework/result/:id` | JWT + 设备指纹 | 获取作业结果 |

### 6.2 网关请求头要求

通过网关访问受保护接口时，必须同时携带以下请求头：

```
Authorization: Bearer <token>
X-Device-FP: <设备指纹>
```

设备指纹必须与登录时提交的 `device_fp` 完全一致。网关会验证 JWT 中的 `device_fp` 声明与 `X-Device-FP` 头是否匹配，不一致则返回 401。

### 6.3 网关安全防护

网关在转发请求前依次执行以下安全检查：

| 顺序 | 安全层 | 说明 |
|------|--------|------|
| 1 | 请求体大小限制 | 超过 `DRAFTTINK_MAX_REQUEST_SIZE`（默认 10MB）的请求被拒绝 |
| 2 | 速率限制 | 单个 IP 每分钟请求超过 `DRAFTTINK_RATE_LIMIT_PER_MINUTE`（默认 60 次）返回 429 |
| 3 | WAF 检查 | 检测 SQL 注入、XSS、路径穿越等恶意模式，命中则返回 403 |
| 4 | JWT + 设备指纹验证 | 验证令牌签名、过期时间和设备指纹绑定 |

### 6.4 网关审计日志

网关不存储任何业务数据。所有经过网关的请求会被记录审计日志并转发到后端存储。审计日志包含：

| 字段 | 说明 |
|------|------|
| 用户 ID | 从 JWT 中提取 |
| 设备指纹 | 从 `X-Device-FP` 头提取 |
| 客户端 IP | 从 TCP 连接或 `X-Forwarded-For` 头提取 |
| 请求路径 | 如 `/api/homework/submit` |
| 请求方法 | 如 `POST` |
| 时间戳 | UTC 时间 |

---

## 7. 错误码参考

### 7.1 后端错误响应格式

所有后端错误响应均使用统一的 JSON 格式：

```json
{
  "error": "error_kind",
  "message": "具体错误信息"
}
```

| HTTP 状态码 | error 值 | 说明 |
|------------|----------|------|
| 400 | `bad_request` | 请求参数错误（格式不对、缺少必填字段等） |
| 401 | `unauthorized` | 未认证（缺少 Authorization 头、JWT 无效或过期） |
| 403 | `forbidden` | 权限不足（角色不符或无权访问该资源） |
| 404 | `not_found` | 资源不存在 |
| 500 | `internal` | 服务器内部错误 |

### 7.2 网关错误响应

网关错误使用纯文本响应体：

| HTTP 状态码 | 响应体 | 说明 |
|------------|--------|------|
| 401 | `Unauthorized: <原因>` | JWT 验证失败或设备指纹不匹配 |
| 403 | `Blocked by WAF: <原因>` | 请求被 WAF 规则拦截 |
| 429 | `Rate limit exceeded` | 请求频率超过限制 |
| 502 | `Bad gateway: <原因>` | 后端不可达或响应超时（30 秒） |

### 7.3 作业状态值

| 状态值 | 说明 |
|--------|------|
| `draft` | 草稿（未发布） |
| `published` | 已发布，学生可提交 |
| `closed` | 已截止，不再接受提交 |
| `archived` | 已归档 |

### 7.4 提交状态值

| 状态值 | 说明 |
|--------|------|
| `not_submitted` | 未提交 |
| `submitted` | 已提交（等待批改） |
| `graded` | 已批改 |
| `returned` | 已退回（需重做） |

---

## 附录：接口速查表

| 方法 | 路径 | 认证 | 角色 | 说明 |
|------|------|------|------|------|
| GET | `/api/health` | 无 | 任意 | 健康检查 |
| POST | `/api/auth/login` | 无 | 任意 | 用户登录 |
| POST | `/api/auth/logout` | JWT | 任意 | 用户登出 |
| GET | `/api/auth/me` | JWT | 任意 | 获取当前用户信息 |
| POST | `/api/homework/create` | JWT | teacher/admin | 创建作业 |
| GET | `/api/homework/list` | JWT | 任意 | 获取作业列表 |
| GET | `/api/homework/:id` | JWT | 任意（受限） | 获取作业详情 |
| POST | `/api/homework/submit` | JWT | student | 提交作业 |
| POST | `/api/homework/grade` | JWT | teacher/admin | 批改作业 |
| POST | `/api/resource/upload` | JWT | 任意 | 上传资源 |
| GET | `/api/resource/*path` | JWT | 任意 | 下载资源 |
