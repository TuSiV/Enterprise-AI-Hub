<div align="center">

# Enterprise AI Hub

### 企业统一 AI 基础设施 · AI Control Plane

**所有模型统一接入 · 所有应用统一调用 · 所有路由统一治理 · 所有使用统一计量 · 所有行为统一审计**

简体中文 · [English](README_EN.md)

[![CI](https://github.com/TuSiV/ai-hub/actions/workflows/ci.yml/badge.svg)](https://github.com/TuSiV/ai-hub/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.94-DEA584?logo=rust)](https://www.rust-lang.org)
[![Tests](https://img.shields.io/badge/tests-62%20passing-16a34a)](#测试与质量门禁)
[![PostgreSQL](https://img.shields.io/badge/PostgreSQL-16-336791?logo=postgresql)](https://www.postgresql.org)
[![Desktop](https://img.shields.io/badge/Desktop-Tauri%202-FFC131)](https://tauri.app)

*Desktop / Server / Connected Desktop 三模式统一产品 —— Rust Core 本体，Tauri 仅为桌面宿主*

</div>

---

## 为什么

企业面对的不是"再买一个聊天机器人"，而是模型分散接入、成本无法归集、路由无法治理、行为无法审计。

**Enterprise AI Hub 把 AI 当作统一基础设施来建设**——任何 OpenAI SDK 应用只需改一行 `base_url`，即可获得：

| 能力 | 说明 |
|:---:|---|
| 🔌 **统一接入** | Provider（OpenAI 兼容 / Ollama…）注册 + 连接测试 + 模型自动发现 |
| 🧭 **智能路由** | Virtual Model 抽象（客户端零改动换后端）、优先级 Failover、熔断器、重试 |
| 🔑 **统一调用** | Application / API Key 生命周期、模型白名单、RPM / 日请求 / 月成本配额 |
| 📊 **统一计量** | Token 与成本（microunits 整数 + 计费快照）、P95/TTFT、按模型 / 应用聚合 |
| 🛡️ **统一治理** | 全量审计、RBAC 多用户、数据分级 → 路由矩阵、DLP 脱敏、SSRF 校验 |
| 📚 **知识增强** | 知识库上传 → 解析（PDF/DOCX）→ Chunk → Embedding → 混合检索 → 带引用回答 |
| 🤖 **Agent / MCP** | 版本化 Agent 循环执行、工具白名单与全量审计、MCP stdio / HTTP 双通道 |
| 🧪 **评测** | 数据集 → 多候选运行 → Rule 打分 + LLM Judge → 成本 / 延迟回归对比 |

<details>
<summary><b>设计边界 —— 我们明确不做的事（反模式约束）</b></summary>

- ❌ 聊天页面优先（Chat UI 是客户端，不是平台核心）
- ❌ Provider if/else 写进 Gateway（必须走 Adapter）
- ❌ Desktop 依赖 Docker / PostgreSQL / Redis
- ❌ Agent 权限靠 Prompt（权限由 Core 强制交集）
- ❌ RAG 先检索后鉴权（权限必须进入召回条件）
- ❌ 为未来规模先上微服务（模块化单体 + Runtime 边界）

</details>

---

## 适用场景

<table>
<tr>
<td width="33%" valign="top">

### 🏢 多团队 / 多业务线共用 AI
研发、客服、运营各自接入不同模型，成本、配额、行为分散在各处，财务和安全都无法归集统一看板。

</td>
<td width="33%" valign="top">

### 🔀 多模型 / 多厂商路由治理
既想用最强模型，又要防单一厂商锁定、控成本、保可用性——需要故障转移、熔断、按场景路由。

</td>
<td width="33%" valign="top">

### 🛡️ 安全合规强约束
金融、医疗、政务等场景需要全量审计、数据分级路由、DLP 脱敏、SSRF 防护，且权限必须由系统强制而非约定。

</td>
</tr>
<tr>
<td width="33%" valign="top">

### 📚 企业知识库问答
需要把内部文档（PDF/DOCX 等）变成带引用、可追溯的问答能力，而不是简单粘贴进 Prompt。

</td>
<td width="33%" valign="top">

### 🤖 Agent / 自动化工作流
构建可版本化、可审计、工具权限受控的 Agent 流程，避免"权限全靠 Prompt 约定"的风险。

</td>
<td width="33%" valign="top">

### 🧪 模型评测与选型
新模型上线前，用统一数据集跑分、对比成本与延迟回归，而不是凭感觉切换。

</td>
</tr>
</table>

---

## 对比

|  | 直连各厂商 API | 自建网关脚本 | **Enterprise AI Hub** |
|---|:---:|:---:|:---:|
| 多模型统一接入 | ❌ 逐个对接 | ⚠️ 需自行维护 | ✅ Provider Adapter 内置 |
| 智能路由 / Failover / 熔断 | ❌ | ⚠️ 需自研 | ✅ Virtual Model + 熔断重试 |
| 成本与用量统一计量 | ❌ | ⚠️ 需自建 | ✅ Token/成本/P95/TTFT 全维度 |
| 全量审计 + RBAC + DLP | ❌ | ⚠️ 需自建 | ✅ 内置 |
| 知识库 RAG（带引用） | ❌ | ❌ | ✅ 解析/Embedding/混合检索 |
| Agent 权限强制隔离 | — | ⚠️ 靠约定 | ✅ Core 强制交集，非 Prompt 约定 |
| 部署形态 | — | 视实现而定 | ✅ Desktop / Server / Connected 三态 |
| 桌面端外部依赖 | — | 视实现而定 | ✅ 零 Docker / PostgreSQL / Redis |

> 定位：不是又一个聊天客户端，而是模型接入、路由、计量、审计的**统一基础设施层**——业务系统只需换一行 `base_url`。

### 对比 New API

[New API](https://github.com/QuantumNous/new-api)（原 One API 衍生，⭐ 46k+）是社区里非常成熟的开源大模型网关，尤其在**格式互转**（OpenAI/Claude/Gemini 互转）和**中转分发**场景做得很深。二者定位不同，供参考：

| | New API | **Enterprise AI Hub** |
|---|:---:|:---:|
| 多模型接入 / 格式互转 | ✅ OpenAI/Claude/Gemini 互转，覆盖 Midjourney、Suno、Rerank 等 | ✅ OpenAI 兼容 Adapter，聚焦企业业务系统接入 |
| 智能路由 | ✅ 渠道加权随机 + 失败重试 + 用户级限流 | ✅ Virtual Model 抽象 + 优先级 Failover + 熔断器 |
| 用量与成本核算 | ✅ 按次/按量/缓存命中计费，运营级数据看板 | ✅ Token/成本 microunits + P95/TTFT，按模型/应用聚合 |
| 权限与治理 | ⚠️ 令牌分组、模型限制、用户管理 | ✅ RBAC 多用户 + 数据分级路由矩阵 + DLP 脱敏 + SSRF 校验 |
| 全量行为审计 | ⚠️ 错误日志（`ERROR_LOG_ENABLED`），非面向合规的全量审计 | ✅ 全链路请求/Agent 行为审计 |
| 知识库 RAG（带引用） | ❌ | ✅ 解析 → Embedding → 混合检索 → 带引用回答 |
| Agent / MCP（权限强制隔离） | ❌ | ✅ 版本化循环执行 + 工具白名单，权限由 Core 强制交集 |
| 模型评测（Rule + LLM Judge） | ❌ | ✅ 数据集 → 多候选 → 成本/延迟回归对比 |
| 内置充值 / 分发转售 | ✅ 易支付、Stripe，面向个人/中转分发 | — （不做面向公众的转售计费，聚焦企业内部治理） |
| 部署形态 | Docker / Docker Compose / 宝塔面板（服务端） | Desktop（零 Docker/PG/Redis 依赖）/ Server / Connected 三态 |
| 开源协议 | AGPLv3（衍生使用需遵循开源义务或联系商用授权） | Apache-2.0 |

> 简单说：如果你要的是**个人/团队中转多家模型 API、按量转售或充值分发**，New API 的生态和格式覆盖非常成熟；如果你要的是**企业内部统一治理**——RBAC、数据分级、DLP、审计合规，外加知识库 RAG 和受控 Agent/MCP 执行——Enterprise AI Hub 是围绕这个目标设计的。两者并不完全互斥的竞品，更像是面向不同场景的工具。

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
        │ Gateway    │ Application │ Provider Adapter │
        │ /v1/* + SSE│ RBAC + Jobs │ OpenAI 兼容(核心) │
        ├────────────┴──────┬──────┴─────────────────┤
        ▼                   ▼                         ▼
  SQLite / PostgreSQL   Secret Store         Python Runtime Sidecar
  (契约测试双库通过)    Keychain/env/mem      parse · PDF/DOCX · 握手/自愈
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

> 再次运行会自动打开已运行实例（单实例语义）；8787 端口被占用时自动退让。

<details>
<summary><b>无桌面窗口（纯浏览器）</b></summary>

```bash
AIHUB_SECRET_BACKEND=memory ./target/debug/aihub-server --mode desktop --print-admin-token
# 打开 http://127.0.0.1:8787，用 stderr 输出的 token 登录
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
sudo systemctl enable --now aihub   # 迁移自动执行
```

<details>
<summary><b>systemd unit 模板</b></summary>

```ini
[Unit]
Description=Enterprise AI Hub
After=network.target postgresql.service

[Service]
EnvironmentFile=/etc/aihub.env
ExecStart=/usr/local/bin/aihub-server --mode server
Restart=on-failure
User=aihub

[Install]
WantedBy=multi-user.target
```

</details>

```bash
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

每次调用均产生 `X-AIH-Request-ID` / `X-AIH-Resolved-Model` / `X-AIH-Provider` 响应头，
并可在控制台「请求」页查看完整 Trace / Usage / Cost。

</details>

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

覆盖矩阵：**Provider Adapter 契约（11 个标准化用例）** · 网关集成（鉴权 / Failover / 流式 / 限流 / 撤销 Key）·
平台 e2e（Prompt / KB / Agent / Eval / DLP / OIDC 验签）· RBAC 多用户与防伪造 · 并发与取消风暴 ·
SQLite / PostgreSQL 双库契约。

---

## 配置

优先级：**CLI > 环境变量 > config.toml > 默认值**

| 环境变量 | 说明 | 默认 |
|---|---|:---|
| `AIHUB_MODE` | `desktop` / `server` | `desktop` |
| `AIHUB_DATA_DIR` | 数据目录（DB、token、documents） | macOS: `~/Library/Application Support/Enterprise AI Hub` |
| `AIHUB_GATEWAY_PORT` | 网关端口（desktop 冲突自动退让） | `8787` |
| `AIHUB_ADMIN_TOKEN` | 固定 admin token | 首次启动时生成并写入数据目录 |
| `AIHUB_SECRET_BACKEND` | `keyring` / `memory` / `env` | `keyring` |
| `AIHUB_DATABASE_URL` | SQLite 路径或 PostgreSQL URL | `<data>/aihub.db` |
| `AIHUB_RUNTIME_ENABLED` | 启用 Python Runtime Sidecar | `false` |
| `AIHUB_OIDC_JWKS_URL` / `_ISSUER` / `_AUDIENCE` | 三项齐备时启用 OIDC 登录 | 未启用 |
| `AIHUB_LOG_LEVEL` | tracing 日志级别 | `info` |

CLI：`aihub-server --config <toml> --mode <desktop|server> --port <n> --print-admin-token`

---

## 项目结构

```
├── crates/                     # Rust Core（domain 层零基础设施反向依赖）
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
