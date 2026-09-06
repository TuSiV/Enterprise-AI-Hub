# Enterprise AI Hub / 企业智能能力平台

> 以 **Rust Core** 为核心、同时支持本地桌面与企业服务器部署的统一 AI Control Plane：
> 所有模型统一接入、所有应用统一调用、所有路由统一治理、所有使用统一计量、所有 AI 行为统一审计。
>
> 设计基线：[docs/enterprise-ai-hub-development-plan.md](docs/enterprise-ai-hub-development-plan.md)

## 当前实现范围（P0 / Desktop MVP 核心，方案 §34.1）

| 能力 | 状态 | 说明 |
|---|---|---|
| Rust Workspace（10 crates） | ✅ | api-types / config / telemetry / secrets / domain / provider-core / provider-openai-compatible / persistence / application / gateway |
| Provider 管理 | ✅ | CRUD、凭据入 SecretStore、连接测试、模型发现 |
| Model 注册表 | ✅ | 能力/定价/上下文配置，定价驱动成本计算（microunits 整数） |
| Virtual Model + 路由 | ✅ | priority_failover、按 target 重试、熔断器（provider×model 维度）、路由模拟器 |
| OpenAI 兼容 Gateway | ✅ | `/v1/models`、`/v1/chat/completions`（流式/非流式）、`/v1/embeddings`，`X-AIH-*` 扩展头 |
| Application / API Key | ✅ | `aih_live_<prefix>_<secret>` 格式、仅存 hash、撤销即时生效、RPM/日请求/月成本配额 |
| Usage / Cost / Audit | ✅ | provider usage 优先、缺失时估算并标记 `estimated`；计费快照；审计事件（metadata only） |
| SQLite Persistence | ✅ | SQLx + 自动迁移；Repository Port 与 PostgreSQL 方言隔离（M10 接入） |
| Web 控制台 | ✅ | React + TS：总览 / Providers / 模型 / Virtual Models / 应用与 Key / Playground（流式）/ 请求 / 审计 / 设置 |
| Desktop 壳（Tauri 2） | ✅ | 同一 Rust Core 嵌入，loopback Gateway + 自动登录桥接 |
| Mock 上游 | ✅ | `aihub-mock-openai`：本地 OpenAI 兼容 mock，支持故障注入（x-mock-behavior） |
| Prompt Center（M8） | ✅ | 版本化 draft→published→deprecated，发布不可变、唯一 published 可追溯 |
| Desktop 产品化（M9） | ✅ | 单实例锁、backup/restore（VACUUM INTO + tar）、端口退让 |
| Server 数据面（M10） | ✅ | PostgreSQL Adapter（16 仓储）+ 双方言迁移 + 与 SQLite 同一套契约测试（本机 PG 18 验证） |
| IAM/RBAC（M10） | ✅ | 本地用户 + 5 系统角色权限矩阵、登录 session、RBAC 鉴权接入 Admin API |
| Python Runtime（M11） | ✅ | runtime-client + managed sidecar（PORT 握手/崩溃有界重启/手动重启）+ remote 模式 + parse 端点（txt/md/pdf/docx） |
| Knowledge/RAG（M12/13） | ✅ | 上传去重→本地对象存储→chunk（页码保留）→embedding→余弦检索→引用；PDF/DOCX 走 runtime |
| Agent/Tool/MCP（M14/15） | ✅ | 版本化 Agent + 循环执行（maxSteps/maxToolCalls/cost guard）+ builtin/http(只读)/mcp(streamable-http) 工具 + ToolCall 全量审计 |
| Evaluation（M16） | ✅ | dataset/cases/runs、rule 打分 + 可选 LLM Judge、成本/延迟回归对比 |
| Security（M17） | ✅ | 数据分级→provider 矩阵、DLP（手机号/密钥脱敏+自定义规则）、SSRF 目标校验、routing/security 策略表 |

| Runtime Jobs（M11） | ✅ | runtime_jobs 持久化队列（重试/退避/requeue）+ 后台 worker + 管理 API |
| 对象存储（M10/M12） | ✅ | ObjectStorage Port：Local（默认）/ S3-compatible（SigV4 最小实现，兼容 MinIO） |
| Identity Provider（M10） | ✅ | local / trusted_header（§11.3 显式开启）/ **OIDC RS256 id_token JWKS 验签**（iss/aud 校验 + 篡改拒绝 + `POST /api/v1/auth/oidc` 登录端点，e2e + mock IdP 全链路实测） |
| 交付附件（§36.4） | ✅ | Dockerfile（多阶段：Rust+Web+Runtime）+ docker-compose（PostgreSQL+Server）；**Tauri 更新签名密钥已生成并实测 sign/verify 往返**（公钥入 conf，UPDATER.md）；Playwright web E2E spec（web/e2e） |
| S3 联调（M10） | ✅ | SigV4 adapter 对**真实 MinIO** 集成测试通过（put/get/delete/exists roundtrip；`--features s3-test`） |
| Connected Desktop（§25） | ✅ | 登录页支持 Server Workspace 地址切换，本地/远程统一 client |

未实现（明确标注）：Anthropic/Gemini 专属 Adapter、OIDC token 校验（JWKS，需部署 IdP）、分布式限流（Stage F 按需）、Desktop 自动更新签名（需发布证书）。

## 快速开始

### 1. 启动服务（本地开发）

```bash
# 构建
cargo build

# 可选：本地 mock 上游（不依赖真实 API Key）
./target/debug/aihub-mock-openai &          # http://127.0.0.1:9901

# 启动 AI Hub（SQLite + loopback 8787）
AIHUB_SECRET_BACKEND=memory ./target/debug/aihub-server &

# Web 控制台 + Admin API: http://127.0.0.1:8787
# OpenAI 兼容网关:        http://127.0.0.1:8787/v1
# Admin Token 首次启动生成于数据目录 admin_token 文件（stderr 也会提示路径）
```

> **`AIHUB_SECRET_BACKEND`**：`keyring`（默认，macOS Keychain / Win 凭据管理器）| `memory` | `env`。
> macOS 上**未签名**的开发二进制每次读取 Keychain 都会弹授权对话框，开发/CI 建议用 `memory`。

### 2. 全链路冒烟（自动创建 Provider→模型→Virtual Model→Key→调用→计量校验）

```bash
./scripts/smoke.sh
```

### 3. OpenAI SDK 接入

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8787/v1",
    api_key="aih_live_<application key>",   # Web 控制台 → 应用与 Key → 创建
)
client.chat.completions.create(
    model="general-smart",                   # Virtual Model key
    messages=[{"role": "user", "content": "hello"}],
)
```

### 4. Web 控制台开发模式

```bash
cd web && npm install && npm run dev   # http://localhost:5173，代理到 8787
npm run build                          # 产物由 aihub-server 静态托管
```

### 5. Desktop 壳（Tauri 2）

```bash
cargo run -p aihub-desktop
# 或 Tauri CLI 开发模式（前端热更新）：cargo install tauri-cli && cargo tauri dev
```

桌面壳内嵌同一 Rust Core：本地 SQLite + loopback Gateway，窗口加载内置 Web 控制台并通过
Tauri IPC 自动完成 Admin Token 登录（无需手动粘贴）。

### 6. Python Runtime（协议脚手架，M11+）

```bash
cd runtime && pip install -r requirements.txt
python -m app.main --port 0 --session-token <token>   # stdout 输出 PORT=<port> 握手
```

详见 [runtime/README.md](runtime/README.md)。

## 架构

```
React UI (web/) ── Tauri (apps/desktop) ─┐
浏览器 Admin ────────────────────────────┤
OpenAI 兼容客户端 ───────────────────────┴─▶ Rust Core (crates/)
                                              ├─ application: 执行流水线（认证→配额→解析→路由→重试/熔断→Usage/Cost/Audit）
                                              ├─ gateway: /v1 协议适配 + SSE
                                              ├─ provider-core/-openai-compatible: Provider Adapter（方案 §10）
                                              ├─ persistence: SQLx SQLite（迁移 0001/0002）
                                              └─ secrets: Keychain / memory / env（AIHUB_SECRET_BACKEND）
                                                    │
                                                    ▼
                                          runtime/ Python Sidecar（M11+：Parse/RAG/Agent/Eval）
```

依赖方向遵循方案 §7.1：Domain 不依赖 sqlx/axum/reqwest；Tauri 仅是 Desktop Adapter（§35.5）；Gateway 不承载管理 CRUD（§3.2）。

## 测试与质量门禁

```bash
cargo fmt --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test                                              # 65 tests（含 OIDC 验签 / Desktop 语义）：domain 单元 / SQLite 契约 / Provider Adapter 契约矩阵(§31.3) / Gateway+Platform e2e / RBAC / 并发与取消风暴(§31.8)
TEST_DATABASE_URL=... cargo test -p aihub-persistence --test pg_contract -- --ignored   # PostgreSQL 契约（本机 PG 18 验证）
cd web && npm run build                                 # tsc --noEmit + vite build
./scripts/smoke.sh                                      # 进程级端到端冒烟
```

CI：[.github/workflows/ci.yml](.github/workflows/ci.yml)（rust: fmt + clippy -D warnings + test；web: typecheck + build）。

## 配置

优先级：CLI > 环境变量 > config.toml > 默认值（方案 §26）。配置文件默认查找 `AIHUB_CONFIG`
环境变量指向的 TOML 文件，或启动时传 `--config /path/to/aihub.toml`。

| 环境变量 | 说明 | 默认 |
|---|---|---|
| `AIHUB_MODE` | desktop / server | desktop |
| `AIHUB_DATA_DIR` | 数据目录（SQLite、admin_token、documents） | macOS: `~/Library/Application Support/Enterprise AI Hub` |
| `AIHUB_GATEWAY_PORT` | 网关端口（desktop 模式冲突自动退让） | 8787 |
| `AIHUB_ADMIN_TOKEN` | 固定 admin token | 首次生成写入数据目录 |
| `AIHUB_SECRET_BACKEND` | keyring / memory / env | keyring |
| `AIHUB_DATABASE_URL` | SQLite 路径覆盖 | `<data>/aihub.db` |
| `AIHUB_LOG_LEVEL` | 日志级别（tracing EnvFilter） | info |

CLI：`aihub-server --config <path> --mode <desktop|server> --port <n> --print-admin-token`（启动横幅与 token 打印到 stderr）。


## 许可

Apache-2.0
