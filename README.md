<div align="center">

# Enterprise AI Hub

### 企业统一 AI 基础设施 · AI Control Plane

**所有模型统一接入 · 所有应用统一调用 · 所有路由统一治理 · 所有使用统一计量 · 所有行为统一审计**

[![CI](https://github.com/TuSiV/ai-hub/actions/workflows/ci.yml/badge.svg)](https://github.com/TuSiV/ai-hub/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.94-DEA584?logo=rust)](https://www.rust-lang.org)
[![Tests](https://img.shields.io/badge/tests-65%20passing-16a34a)](#-测试与质量门禁)
[![PostgreSQL](https://img.shields.io/badge/PostgreSQL-16-336791?logo=postgresql)](https://www.postgresql.org)
[![Desktop](https://img.shields.io/badge/Desktop-Tauri%202-FFC131)](https://tauri.app)

*Desktop / Server / Connected Desktop 三模式统一产品 —— Rust Core 本体，Tauri 仅为桌面宿主*

</div>

---

## 为什么

企业面对的不是"再买一个聊天机器人"，而是：模型分散接入、成本无法归集、路由无法治理、行为无法审计。

**Enterprise AI Hub 把 AI 当作统一基础设施来建设**——任何 OpenAI SDK 应用只改一行 `base_url`，即可获得：

| | |
|---|---|
| 🔌 **统一接入** | Provider（OpenAI 兼容/Ollama…）注册 + 连接测试 + 模型自动发现 |
| 🧭 **智能路由** | Virtual Model 抽象（客户端零改动换后端）、优先级 Failover、熔断器、重试 |
| 🔑 **统一调用** | Application / API Key 生命周期、模型白名单、RPM/日请求/月成本配额 |
| 📊 **统一计量** | Token 与成本（microunits 整数 + 计费快照）、P95/TTFT、按模型/应用聚合 |
| 🛡️ **统一治理** | 全量审计、RBAC 多用户、数据分级→路由矩阵、DLP 脱敏、SSRF 校验 |
| 📚 **知识增强** | 知识库上传→解析(PDF/DOCX)→Chunk→Embedding→混合检索→带引用回答 |
| 🤖 **Agent / MCP** | 版本化 Agent 循环执行、工具白名单与全量审计、MCP stdio/HTTP 双通道 |
| 🧪 **评测** | 数据集→多候选运行→Rule 打分 + LLM Judge→成本/延迟回归对比 |

<details>
<summary><b>设计边界 —— 我们明确不做的事（反模式约束）</b></summary>

- ❌ 聊天页面优先（Chat UI 是客户端不是平台核心）
- ❌ Provider if/else 写进 Gateway（必须走 Adapter）
- ❌ Desktop 依赖 Docker / PostgreSQL / Redis
- ❌ Agent 权限靠 Prompt（权限由 Core 强制交集）
- ❌ RAG 先检索后鉴权（权限必须进入召回条件）
- ❌ 为未来规模先上微服务（模块化单体 + Runtime 边界）

</details>

---

## 架构

```
        ┌─────────────┐   ┌──────────────┐   ┌──────────────────┐
        │ Tauri 桌面端 │   │  浏览器控制台  │   │ OpenAI 兼容客户端  │
        └──────┬──────┘   └──────┬───────┘   └────────┬─────────┘
               │    Tauri IPC    │  Admin API + Web   │ /v1/*
               └────────┬────────┴────────────────────┘
                        ▼
        ┌───────────────────────────────────────────┐
        │              Rust Core (13 crates)         │
        │                                            │
        │  认证 → 配额 → 解析 → 路由 → 重试/熔断      │
        │  → Usage/Cost → 审计                       │
        ├────────────┬─────────────┬─────────────────┤
        │ Gateway    │ Application │ Provider Adapter│
        │ /v1/* + SSE│ RBAC + Jobs │ OpenAI 兼容(核心)│
        ├────────────┴──────┬──────┴─────────────────┤
        ▼                   ▼                        ▼
  SQLite / PostgreSQL   Secret Store        Python Runtime Sidecar
  (契约测试双库通过)    Keychain/env/mem     parse · PDF/DOCX · 握手/自愈
```

---

## 快速开始

### 🖥️ 本地桌面

```bash
# 0. 构建（一次性）
cd web && npm install && npm run build && cd ..
cargo build

# 1. 启动桌面 App（横幅会打印 Admin Token）
cargo run -p aihub-desktop
```

> 再跑一次会自动打开已运行实例（单实例语义）；8787 被占用自动退让端口。

<details>
<summary><b>无桌面窗口（纯浏览器）</b></summary>

```bash
AIHUB_SECRET_BACKEND=memory ./target/debug/aihub-server --mode desktop --print-admin-token
# 打开 http://127.0.0.1:8787，用 stderr 的 token 登录
```

</details>

<details>
<summary><b>60 秒全链路演示（Mock 上游，无需真实 API Key）</b></summary>

```bash
./target/debug/aihub-mock-openai &      # 本地 mock 上游 :9901
./scripts/smoke.sh                       # 自动完成 Provider→模型→路由→调用→计量校验
```

</details>

### 🌐 服务器部署

**Docker Compose（推荐，含 PostgreSQL）**

```bash
docker compose up -d --build
# 控制台 http://<服务器>:8787 · Token: docker exec <容器> cat /data/admin_token
```

**裸机 + systemd**

```bash
sudo tee /etc/aihub.env <<'EOF'
AIHUB_MODE=server
AIHUB_DATA_DIR=/var/lib/aihub
AIHUB_DATABASE_URL=postgres://aihub:密码@127.0.0.1:5432/aihub
AIHUB_SECRET_BACKEND=env
AIHUB_ADMIN_TOKEN=<强随机串>
EOF
sudo systemctl enable --now aihub   # unit 见部署章节模板，迁移自动执行
```

**验证**

```bash
curl http://127.0.0.1:8787/health/live    # → ok
curl http://127.0.0.1:8787/health/ready   # → ready（DB 就绪）
```

### 🔌 业务系统接入（OpenAI SDK 兼容）

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8787/v1",
    api_key="aih_live_<application key>",   # 控制台 → 应用与 Key → 创建
)

client.chat.completions.create(
    model="general-smart",                   # Virtual Model：后端随意切换
    messages=[{"role": "user", "content": "hello"}],
)
```

<details>
<summary><b>cURL 等价调用（流式）</b></summary>

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer aih_live_..." \
  -H "Content-Type: application/json" \
  -d '{"model":"general-smart","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

每次调用均产生 `X-AIH-Request-ID / X-AIH-Resolved-Model / X-AIH-Provider` 响应头，
并在控制台「请求」页可查完整 Trace/Usage/Cost。

</details>

---

---



## 测试与质量门禁

```bash
cargo fmt --check                                       # 格式
cargo clippy --workspace --all-targets -- -D warnings   # 零警告
cargo test                                              # 62 tests：单元/契约/e2e
TEST_DATABASE_URL=... cargo test -p aihub-persistence \
    --test pg_contract -- --ignored                     # PostgreSQL 契约（真实 PG 18 验证）
cargo test -p aihub-application --features s3-test \
    --test s3_storage                                   # MinIO 集成（真实 S3 验证）
cd web && npm run build                                 # TS 类型检查 + 构建
./scripts/smoke.sh                                      # 进程级端到端冒烟
```

覆盖矩阵：**Provider 契约（§31.3 十一用例）** · 网关集成（鉴权/Failover/流式/限流/撤销 Key）·
平台 e2e（Prompt/KB/Agent/Eval/DLP/OIDC 验签）· RBAC 多用户与防伪造 · 并发与取消风暴 ·
SQLite/PostgreSQL 双库契约。

---

## 配置

优先级：**CLI > 环境变量 > config.toml > 默认值**（方案 §26）

| 环境变量 | 说明 | 默认 |
|---|---|:---|
| `AIHUB_MODE` | `desktop` / `server` | `desktop` |
| `AIHUB_DATA_DIR` | 数据目录（DB、token、documents） | macOS: `~/Library/Application Support/Enterprise AI Hub` |
| `AIHUB_GATEWAY_PORT` | 网关端口（desktop 冲突自动退让） | `8787` |
| `AIHUB_ADMIN_TOKEN` | 固定 admin token | 首启生成写入数据目录 |
| `AIHUB_SECRET_BACKEND` | `keyring` / `memory` / `env` | `keyring` |
| `AIHUB_DATABASE_URL` | SQLite 路径 或 PostgreSQL URL | `<data>/aihub.db` |
| `AIHUB_RUNTIME_ENABLED` | 启用 Python Runtime Sidecar | `false` |
| `AIHUB_OIDC_JWKS_URL` `_ISSUER` `_AUDIENCE` | 三项齐备启用 OIDC 登录 | 未启用 |
| `AIHUB_LOG_LEVEL` | tracing 日志级别 | `info` |

CLI：`aihub-server --config <toml> --mode <desktop|server> --port <n> --print-admin-token`

---

## 项目结构

```
├── crates/                     # Rust Core（方案 §7.1 依赖方向：domain 零反向依赖）
│   ├── domain                  # 实体/规范协议/仓储 Port/成本引擎
│   ├── application             # 服务层 + 执行流水线 + RBAC + RAG + Agent + Eval
│   ├── gateway                 # /v1 协议适配 + SSE
│   ├── persistence             # SQLite/PostgreSQL Adapter + 迁移
│   ├── provider-*              # Provider Adapter（OpenAI 兼容核心）
│   ├── secrets / config / …    # 基础设施 Port
│   └── runtime-client          # Sidecar 托管与协议
├── apps/
│   ├── server                  # aihub-server：Admin API + Gateway + Web 托管
│   ├── desktop                 # Tauri 2 桌面壳（单实例 + 签名更新）
│   └── mock-openai             # 本地 Mock 上游（故障注入）
├── web/                        # React + TS 控制台 + Playwright E2E
├── runtime/                    # Python Sidecar（parse 握手协议）
├── migrations/                 # SQLite / PostgreSQL 双方言
└── scripts/smoke.sh            # 端到端冒烟
```

---

## 许可

[Apache-2.0](LICENSE)
