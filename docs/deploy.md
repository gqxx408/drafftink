# 校本教学套件部署指南

本指南面向学校信息技术教师，详细介绍如何在校内服务器上部署"校本教学套件"（seewo-class-mvp）。

系统采用前后端分离架构，包含两个核心服务：

- **后端服务（backend）**：部署在学校内网，负责数据存储、作业管理、用户认证等全部业务逻辑。所有学生作业、课件资源均存储在内网服务器本地，不外传公网。
- **网关服务（gateway）**：部署在公网入口，作为反向代理转发请求到内网后端。网关不存储任何业务数据，仅负责安全防护（WAF、限流、JWT 验证、设备指纹绑定）。

---

## 目录

1. [系统要求](#1-系统要求)
2. [快速部署](#2-快速部署)
3. [配置说明](#3-配置说明)
4. [首次使用](#4-首次使用)
5. [日常维护](#5-日常维护)
6. [故障排除](#6-故障排除)
7. [网络配置](#7-网络配置)

---

## 1. 系统要求

### 1.1 硬件要求

| 项目 | 最低配置 | 推荐配置 |
|------|---------|---------|
| CPU | 2 核 | 4 核 |
| 内存 | 2 GB | 4 GB |
| 磁盘 | 20 GB | 50 GB（SSD） |
| 网络 | 内网千兆 | 内网千兆 + 公网带宽 10Mbps |

> 磁盘空间主要用于存储学生提交的 drftx 作业文件和每日自动备份。按每班 50 人、每人每周 5 次作业估算，一个学期约需 5-10 GB。

### 1.2 软件要求

| 软件 | 最低版本 | 说明 |
|------|---------|------|
| 操作系统 | Linux（推荐 Ubuntu 22.04 LTS / Debian 12） | 也可使用 Windows Server 2019+ 或 macOS |
| Docker Engine | 24.0+ | 容器运行时 |
| Docker Compose | v2.20+ | 容器编排工具 |
| curl | 任意版本 | 用于健康检查 |

### 1.3 安装 Docker（如尚未安装）

**Ubuntu / Debian 系统：**

```bash
# 更新包管理器
sudo apt-get update

# 安装 Docker
sudo apt-get install -y docker.io docker-compose-plugin

# 启动 Docker 服务并设置开机自启
sudo systemctl enable --now docker

# 验证安装
docker --version
docker compose version
```

**Windows 系统：**

下载并安装 [Docker Desktop for Windows](https://www.docker.com/products/docker-desktop/)，安装后启动 Docker Desktop 即可。

### 1.4 端口要求

| 端口 | 服务 | 用途 | 开放范围 |
|------|------|------|---------|
| 8080 | backend | 内网后端 API | 仅内网 |
| 80 | gateway | 公网 HTTP 入口 | 公网（可选） |
| 443 | gateway | 公网 HTTPS 入口 | 公网（推荐） |

---

## 2. 快速部署

### 2.1 获取项目代码

将项目代码复制到服务器上。假设项目放在 `/opt/seewo-class-mvp` 目录：

```bash
# 如果使用 Git
cd /opt
git clone <仓库地址> seewo-class-mvp

# 或直接将代码包解压到 /opt/seewo-class-mvp
```

### 2.2 修改默认密钥（重要！）

在部署前，**必须**修改默认的 JWT 密钥。编辑 `docker/docker-compose.yml` 文件，将 `DRAFTTINK_JWT_SECRET` 的值从 `change-me-in-production` 改为一个足够复杂的随机字符串。

> **安全警告**：默认密钥 `change-me-in-production` 仅用于测试。生产环境若不修改，攻击者可伪造 JWT 令牌冒充任意用户登录系统。请使用至少 32 位以上的随机字符串，例如：
> ```
> DRAFTTINK_JWT_SECRET=Sch00l_2024_r4nd0m_s3cr3t_k3y_x9f2k7
> ```

**生成随机密钥的方法：**

```bash
# Linux / macOS
openssl rand -hex 32

# 或使用 Python
python3 -c "import secrets; print(secrets.token_hex(32))"
```

将生成的字符串同时填入 backend 和 gateway 的 `DRAFTTINK_JWT_SECRET` 环境变量中，**两个服务的密钥必须完全一致**，否则网关无法验证后端签发的 JWT 令牌。

### 2.3 构建并启动服务

```bash
# 进入项目目录
cd /opt/seewo-class-mvp

# 构建镜像（首次构建需要较长时间，约 10-30 分钟）
docker compose -f docker/docker-compose.yml build

# 启动服务
docker compose -f docker/docker-compose.yml up -d
```

### 2.4 验证部署

等待服务启动后（约 10-30 秒），执行以下命令验证：

```bash
# 检查容器运行状态
docker compose -f docker/docker-compose.yml ps

# 检查后端健康状态
curl http://localhost:8080/api/health

# 预期输出：
# {"status":"ok"}
```

如果健康检查返回 `{"status":"ok"}`，说明后端已成功启动。

### 2.5 查看服务日志

```bash
# 查看所有服务日志
docker compose -f docker/docker-compose.yml logs

# 仅查看后端日志
docker compose -f docker/docker-compose.yml logs backend

# 实时跟踪日志
docker compose -f docker/docker-compose.yml logs -f
```

---

## 3. 配置说明

所有配置通过环境变量在 `docker/docker-compose.yml` 中设置。以下逐一说明各配置项。

### 3.1 后端服务（backend）配置

| 环境变量 | 默认值 | 说明 |
|---------|--------|------|
| `DRAFTTINK_LISTEN_ADDR` | `0.0.0.0:8080` | 后端监听地址和端口。容器内一般不需要修改。 |
| `DRAFTTINK_DB_PATH` | `/app/data/db` | sled 数据库存储路径。对应 Docker volume `backend-data`。 |
| `DRAFTTINK_STORAGE_PATH` | `/app/data/storage` | 学生作业文件、课件资源存储路径。对应 Docker volume `backend-data`。 |
| `DRAFTTINK_BACKUP_PATH` | `/app/data/backup` | 自动备份存储路径。对应 Docker volume `backend-data`。 |
| `DRAFTTINK_JWT_SECRET` | `change-me-in-production` | JWT 令牌签名密钥。**生产环境必须修改**，且需与网关一致。 |
| `DRAFTTINK_BACKUP_HOUR` | `2` | 每日自动备份执行时间（24 小时制，0-23）。默认凌晨 2 点。 |

### 3.2 网关服务（gateway）配置

| 环境变量 | 默认值 | 说明 |
|---------|--------|------|
| `DRAFTTINK_LISTEN_ADDR` | `0.0.0.0:80` | 网关监听地址和端口。公网入口通常使用 80 或 443。 |
| `DRAFTTINK_BACKEND_URL` | `http://backend:8080` | 后端服务地址。使用 Docker 内部网络时填 `http://backend:8080`。 |
| `DRAFTTINK_RATE_LIMIT_PER_MINUTE` | `60` | 每个 IP 每分钟最大请求数。超出将返回 429 错误。 |
| `DRAFTTINK_MAX_REQUEST_SIZE` | `10485760` | 单个请求体最大字节数（默认 10MB）。作业文件较大时可适当调大。 |
| `DRAFTTINK_JWT_SECRET` | `change-me-in-production` | JWT 验证密钥。**必须与后端完全一致**。 |
| `GATEWAY_TLS_CERT_PATH` | 空（不启用） | TLS 证书（PEM）路径。网关监听 **443** 时**必须**同时提供 `GATEWAY_TLS_KEY_PATH`，否则启动直接拒绝。 |
| `GATEWAY_TLS_KEY_PATH` | 空（不启用） | TLS 私钥（PEM）路径。与 `GATEWAY_TLS_CERT_PATH` 必须成对出现。 |

### 3.3 数据持久化

后端数据通过 Docker volume `backend-data` 持久化，映射到容器内 `/app/data` 目录。该目录包含三个子目录：

```
/app/data/
├── db/         # sled 数据库（用户、班级、作业、提交记录等元数据）
├── storage/    # 文件存储（学生提交的 drftx 作业、上传的课件资源）
└── backup/     # 自动备份（每日备份，保留最近 7 份）
```

即使容器被删除重建，只要 volume 存在，数据就不会丢失。

### 3.4 配置示例

以下是一个生产环境的 `docker-compose.yml` 配置示例（仅展示 environment 部分）：

```yaml
services:
  backend:
    environment:
      - DRAFTTINK_LISTEN_ADDR=0.0.0.0:8080
      - DRAFTTINK_DB_PATH=/app/data/db
      - DRAFTTINK_STORAGE_PATH=/app/data/storage
      - DRAFTTINK_BACKUP_PATH=/app/data/backup
      - DRAFTTINK_JWT_SECRET=Sch00l_2024_r4nd0m_s3cr3t_k3y_x9f2k7
      - DRAFTTINK_BACKUP_HOUR=3
    # ... 其他配置

  gateway:
    environment:
      - DRAFTTINK_LISTEN_ADDR=0.0.0.0:80
      - DRAFTTINK_BACKEND_URL=http://backend:8080
      - DRAFTTINK_RATE_LIMIT_PER_MINUTE=120
      - DRAFTTINK_MAX_REQUEST_SIZE=52428800
      - DRAFTTINK_JWT_SECRET=Sch00l_2024_r4nd0m_s3cr3t_k3y_x9f2k7
    # ... 其他配置
```

---

## 4. 首次使用

### 4.1 默认账号

系统首次启动时会自动创建以下演示账号和班级：

| 角色 | 用户名 | 密码 | 说明 |
|------|--------|------|------|
| 管理员（admin） | `admin` | `admin123` | 系统管理员，拥有全部权限 |
| 老师（teacher） | `teacher01` | `teacher123` | 示例老师账号，关联"三年二班" |
| 学生（student） | `student01` | `student123` | 示例学生账号，属于"三年二班" |

默认班级：**三年二班**（三年级），班主任为 teacher01。

> **安全警告**：默认密码仅用于初次登录测试。正式使用前，请务必修改所有默认账号密码。当前 MVP 版本密码为明文存储，后续版本将升级为 Argon2 哈希存储。

### 4.2 验证登录

使用 curl 测试登录接口：

```bash
# 管理员登录
curl -X POST http://localhost:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123","device_fp":"test-device-001"}'

# 预期返回：
# {
#   "token": "eyJ...",
#   "user": {
#     "id": "550e8400-...",
#     "username": "admin",
#     "display_name": "系统管理员",
#     "role": "admin",
#     "class_id": null
#   }
# }
```

返回的 `token` 字段是 JWT 令牌，后续请求需在 HTTP 头中携带：

```
Authorization: Bearer <token>
```

### 4.3 配置班级和用户

当前 MVP 版本通过后端 API 管理班级和用户。以下是创建新班级和新用户的示例：

```bash
# 1. 管理员登录获取 token
TOKEN=$(curl -s -X POST http://localhost:8080/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username":"admin","password":"admin123","device_fp":"test-device-001"}' \
  | python3 -c "import sys,json; print(json.load(sys.stdin)['token'])")

# 2. 创建老师账号（通过资源上传或数据库直接操作，MVP 阶段需管理员操作）
# 3. 创建班级并关联老师
# 4. 创建学生账号并关联班级
```

> 注意：当前 MVP 版本暂未提供独立的管理员用户管理 API。班级和用户的创建主要通过系统初始化时的种子数据完成。后续版本将提供完整的管理接口。

### 4.4 客户端连接

教师端和学生端客户端需配置服务器地址：

- **内网访问**：直接连接后端地址 `http://<内网IP>:8080`
- **公网访问**（家校互通场景）：通过网关地址 `http://<公网域名或IP>` 或 `https://<公网域名>`

---

## 5. 日常维护

### 5.1 备份验证

系统每日凌晨自动备份数据库和文件存储，保留最近 7 份备份。

**检查备份是否正常生成：**

```bash
# 进入后端容器查看备份目录
docker compose -f docker/docker-compose.yml exec backend ls -la /app/data/backup/

# 预期看到类似以下目录：
# backup_20240315_020000
# backup_20240316_020000
# backup_20240317_020000
# ...
```

**手动触发备份（可选）：**

当前版本备份由后端定时任务自动执行，暂不支持手动触发。如需立即备份，可手动复制数据目录：

```bash
# 停止服务（确保数据一致性）
docker compose -f docker/docker-compose.yml stop backend

# 复制数据卷（具体路径可通过 docker volume inspect 查看）
docker run --rm -v backend-data:/data -v $(pwd):/backup alpine \
  cp -r /data /backup/manual_backup_$(date +%Y%m%d)

# 重新启动服务
docker compose -f docker/docker-compose.yml start backend
```

**备份恢复：**

```bash
# 停止后端服务
docker compose -f docker/docker-compose.yml stop backend

# 用备份数据替换当前数据
docker run --rm -v backend-data:/data -v $(pwd):/backup alpine \
  sh -c "rm -rf /data/* && cp -r /backup/manual_backup_20240315/data/* /data/"

# 重新启动
docker compose -f docker/docker-compose.yml start backend
```

### 5.2 日志检查

**查看容器日志：**

```bash
# 查看最近 100 行后端日志
docker compose -f docker/docker-compose.yml logs --tail 100 backend

# 查看最近 100 行网关日志
docker compose -f docker/docker-compose.yml logs --tail 100 gateway

# 查看指定时间后的日志
docker compose -f docker/docker-compose.yml logs --since 24h backend
```

**关键日志关键字：**

| 关键字 | 含义 | 处理方式 |
|--------|------|---------|
| `服务器启动` | 后端正常启动 | 无需处理 |
| `备份完成` | 自动备份成功 | 无需处理 |
| `备份失败` | 自动备份出错 | 检查磁盘空间 |
| `Rate limit exceeded` | 网关限流触发 | 检查是否有异常请求 |
| `Blocked by WAF` | WAF 拦截恶意请求 | 检查是否有攻击行为 |
| `JWT 验证失败` | 令牌验证失败 | 可能是密钥不匹配或令牌过期 |
| `ERROR` / `panic` | 系统错误 | 查看完整日志，联系开发人员 |

### 5.3 更新程序

```bash
# 1. 进入项目目录
cd /opt/seewo-class-mvp

# 2. 拉取最新代码
git pull origin main

# 3. 重新构建镜像
docker compose -f docker/docker-compose.yml build

# 4. 重启服务（数据不会丢失）
docker compose -f docker/docker-compose.yml up -d

# 5. 验证服务正常
curl http://localhost:8080/api/health
```

> 更新前建议先手动备份一次数据（参见 5.1 节），以防万一。

### 5.4 磁盘空间管理

定期检查磁盘空间，避免因备份文件或日志占满磁盘：

```bash
# 查看磁盘使用情况
df -h

# 查看 Docker 数据卷大小
docker system df

# 清理未使用的 Docker 镜像（不影响运行中的容器）
docker image prune -a
```

系统自动保留最近 7 份备份，更早的备份会被自动删除。如需调整保留数量，需修改后端源码中 `backup.rs` 的 `keep_count` 常量。

---

## 6. 故障排除

### 6.1 服务无法启动

**现象**：`docker compose up -d` 后容器立即退出。

**排查步骤**：

```bash
# 查看容器退出状态
docker compose -f docker/docker-compose.yml ps -a

# 查看退出日志
docker compose -f docker/docker-compose.yml logs backend
```

**常见原因**：

| 原因 | 解决方案 |
|------|---------|
| 端口 8080 被占用 | 修改 `docker-compose.yml` 中端口映射，如 `"9090:8080"` |
| 端口 80/443 被占用 | 关闭占用端口的程序，或修改网关端口映射 |
| 数据目录权限不足 | `sudo chown -R 1000:1000 /var/lib/docker/volumes/backend-data/` |
| 内存不足 | 增加服务器内存或添加 swap |

### 6.2 健康检查失败

**现象**：`curl http://localhost:8080/api/health` 无响应或返回错误。

**排查步骤**：

```bash
# 1. 确认容器正在运行
docker compose -f docker/docker-compose.yml ps

# 2. 查看后端日志
docker compose -f docker/docker-compose.yml logs backend

# 3. 确认端口映射正确
docker port $(docker compose -f docker/docker-compose.yml ps -q backend)
```

### 6.3 网关无法连接后端

**现象**：通过网关访问返回 502 Bad Gateway。

**排查步骤**：

```bash
# 1. 确认后端正常运行
curl http://localhost:8080/api/health

# 2. 确认 Docker 内部网络正常
docker compose -f docker/docker-compose.yml exec gateway curl http://backend:8080/api/health

# 3. 检查 docker-compose.yml 中 backend_url 配置是否为 http://backend:8080
```

### 6.4 登录返回 401

**现象**：登录接口返回 `{"error":"unauthorized","message":"用户名或密码错误"}`。

**排查步骤**：

- 确认用户名和密码拼写正确（默认账号见 4.1 节）。
- 确认请求体中包含 `device_fp` 字段（设备指纹，任意非空字符串即可）。
- 确认 Content-Type 为 `application/json`。

### 6.5 网关返回 429

**现象**：通过网关访问返回 `429 Too Many Requests`。

**原因**：单个 IP 每分钟请求次数超过 `DRAFTTINK_RATE_LIMIT_PER_MINUTE`（默认 60 次）。

**解决方案**：

- 等待一分钟后重试。
- 如确实需要更高频率（如全班同时提交作业），在 `docker-compose.yml` 中调大 `DRAFTTINK_RATE_LIMIT_PER_MINUTE` 值，例如设为 `300`。
- 修改后需重启网关：`docker compose -f docker/docker-compose.yml restart gateway`

### 6.6 网关返回 403（WAF 拦截）

**现象**：请求被 WAF 拦截，返回 `403 Forbidden: Blocked by WAF`。

**原因**：请求中包含了被 WAF 识别为恶意的内容（SQL 注入、XSS、路径穿越等特征）。

**排查步骤**：

- 检查请求内容是否包含特殊字符（如 `' OR`、`<script>`、`../` 等）。
- 查看网关日志确认拦截原因：`docker compose -f docker/docker-compose.yml logs gateway | grep WAF`
- 确认为正常请求后，调整请求内容避免触发规则。

### 6.7 数据丢失

**现象**：重启容器后用户数据、作业记录消失。

**原因**：Docker volume 被意外删除，或未正确配置 volume 挂载。

**排查步骤**：

```bash
# 确认 volume 存在
docker volume ls | grep backend-data

# 查看 volume 详情
docker volume inspect backend-data
```

如果 volume 不存在，说明配置有误。请确认 `docker-compose.yml` 中 `volumes` 部分正确配置了 `backend-data:/app/data`。

---

## 7. 网络配置

### 7.1 网络架构

```
                     ┌─────────────────────────────────────┐
                     │           学校内网                    │
                     │                                     │
  公网用户 ────────► │  ┌──────────┐    ┌───────────────┐  │
 (教师/学生在家)     │  │ gateway  │───►│   backend     │  │
                     │  │ :80/:443 │    │   :8080       │  │
                     │  └──────────┘    └───────┬───────┘  │
                     │                          │          │
                     │                  ┌───────▼───────┐  │
  内网用户 ──────────────────────────────│   backend     │  │
 (教师/学生在校)                          │   :8080       │  │
                     │                  └───────────────┘  │
                     └─────────────────────────────────────┘
```

- **内网用户**（在校教师和学生）：直接访问后端 `http://<内网IP>:8080`，无需经过网关。
- **公网用户**（在家访问）：通过网关 `http://<公网IP>` 或 `https://<域名>` 访问，网关将请求安全转发到内网后端。

### 7.2 防火墙规则

**后端服务器防火墙配置：**

```bash
# 仅允许内网网段访问后端 8080 端口
# 假设学校内网网段为 192.168.1.0/24

# Ubuntu / Debian (ufw)
sudo ufw allow from 192.168.1.0/24 to any port 8080
sudo ufw deny 8080

# 允许 Docker 内部通信
sudo ufw allow from 172.16.0.0/12 to any port 8080
```

**网关服务器防火墙配置：**

```bash
# 开放公网 HTTP/HTTPS 端口
sudo ufw allow 80/tcp
sudo ufw allow 443/tcp

# 确保后端 8080 端口不对公网开放
sudo ufw deny 8080
```

### 7.3 端口转发（NAT）

如果学校使用 NAT 路由器（内网服务器无独立公网 IP），需要在路由器上配置端口转发：

| 外部端口 | 内部 IP | 内部端口 | 协议 | 说明 |
|---------|---------|---------|------|------|
| 80 | <网关服务器内网IP> | 80 | TCP | HTTP |
| 443 | <网关服务器内网IP> | 443 | TCP | HTTPS |

> **安全提示**：仅转发网关端口（80/443），切勿直接转发后端 8080 端口到公网。后端必须仅在内网可访问，所有公网流量必须经过网关的安全检查。

### 7.4 域名解析（可选）

如果学校有自己的域名，可以将域名指向网关服务器公网 IP：

```
hw.yourschool.edu.cn  →  <网关公网IP>
```

配置 DNS A 记录后，用户可通过 `http://hw.yourschool.edu.cn` 访问系统。

### 7.5 HTTPS / TLS 配置

当前 MVP 版本网关以 HTTP 模式运行，但已内置 **TLS 端口强制校验**（fail-closed）：

> **安全强化**：若网关监听**标准 TLS 端口（443）**却未提供证书，启动时会直接拒绝并退出；仅配置 `GATEWAY_TLS_CERT_PATH` / `GATEWAY_TLS_KEY_PATH` 之一也会因配置不完整被拒绝。可选方案：
> - **在 443 上启用 TLS**：同时设置 `GATEWAY_TLS_CERT_PATH` 与 `GATEWAY_TLS_KEY_PATH`（PEM 文件路径）；
> - **保持明文 HTTP**：将 `GATEWAY_LISTEN_ADDR` 设为非 TLS 端口（如 `0.0.0.0:80`），由前置 Nginx/Caddy 终结 TLS（推荐，见方案一）。

生产环境推荐使用 HTTPS，有以下两种方案：

**方案一：使用反向代理（推荐）**

在网关前部署 Nginx/Caddy 作为 TLS 终结点：

```nginx
# /etc/nginx/sites-available/seewo-class
server {
    listen 443 ssl http2;
    server_name hw.yourschool.edu.cn;

    ssl_certificate     /etc/ssl/certs/yourschool.crt;
    ssl_certificate_key /etc/ssl/private/yourschool.key;
    ssl_protocols       TLSv1.3;
    ssl_ciphers         HIGH:!aNULL:!MD5;

    client_max_body_size 50m;

    location / {
        proxy_pass http://127.0.0.1:80;
        proxy_set_header Host $host;
        proxy_set_header X-Real-IP $remote_addr;
        proxy_set_header X-Forwarded-For $proxy_add_x_forwarded_for;
        proxy_set_header X-Forwarded-Proto $scheme;
    }
}

# HTTP 重定向到 HTTPS
server {
    listen 80;
    server_name hw.yourschool.edu.cn;
    return 301 https://$host$request_uri;
}
```

**方案二：使用 Let's Encrypt 自动证书**

```bash
# 安装 Certbot
sudo apt-get install -y certbot python3-certbot-nginx

# 自动获取并配置证书
sudo certbot --nginx -d hw.yourschool.edu.cn
```

> 无论使用哪种方案，都应强制使用 TLS 1.3，禁用 TLS 1.2 及以下版本。

---

## 附录：常用命令速查

| 操作 | 命令 |
|------|------|
| 启动服务 | `docker compose -f docker/docker-compose.yml up -d` |
| 停止服务 | `docker compose -f docker/docker-compose.yml down` |
| 重启服务 | `docker compose -f docker/docker-compose.yml restart` |
| 查看状态 | `docker compose -f docker/docker-compose.yml ps` |
| 查看日志 | `docker compose -f docker/docker-compose.yml logs -f` |
| 健康检查 | `curl http://localhost:8080/api/health` |
| 重新构建 | `docker compose -f docker/docker-compose.yml build` |
| 进入后端容器 | `docker compose -f docker/docker-compose.yml exec backend bash` |
| 查看数据卷 | `docker volume inspect backend-data` |
