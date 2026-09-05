# Enterprise AI Hub 企业智能能力平台——完整开发方案

> 文档版本：V1.0  
> 目标形态：Desktop / Server / Connected Desktop 三模式统一产品  
> 核心技术：Rust Core + Tauri + React/TypeScript + Python AI Runtime  
> 文档用途：产品设计、架构设计、开发实施、测试验收、后续演进的统一基线

---

## 0. 文档结论与设计基线

[KNOWN] 本项目不定位为“聊天机器人”或“单一知识库”，而定位为企业统一 AI 基础设施与 AI Control Plane。

[KNOWN] 同一套核心代码必须同时支持以下三种运行模式：

1. **Desktop Mode**：完整能力在单机本地运行。
2. **Server Mode**：部署在服务器，为多人、多系统统一提供 AI 能力。
3. **Connected Desktop Mode**：桌面客户端作为企业 Server 的高级客户端，本地保留必要能力，但主要模型、知识、权限和审计由服务端控制。

[KNOWN] Rust Core 是产品本体；Tauri 只是 Desktop Host/Adapter；Browser + HTTP 是 Server Host/Adapter。

[KNOWN] Python 不作为系统主后端，只作为 AI Runtime，承担 RAG、文档解析、Embedding、Rerank、Agent、Tool/MCP、Eval 等更依赖 Python AI 生态的能力。

[KNOWN] Docker、Kubernetes、Nginx/Caddy、MinIO 等均属于可选部署与运维能力，不得成为核心运行前提。

[INFERRED] 架构必须优先保证“本地零运维可运行”，再通过 Adapter 扩展到服务端，而不是反过来把服务端微服务架构强行塞进桌面应用。置信度：HIGH。

---

# 1. 产品定义

## 1.1 产品名称

**Enterprise AI Hub / 企业智能能力平台**

## 1.2 产品目标

[INFERRED] 平台解决的核心问题不是“如何让员工使用某个大模型”，而是“企业如何统一接入、路由、治理、审计、计量和复用所有 AI 能力”。置信度：HIGH。

核心能力包括：

- [COMMON] 模型 Provider 统一接入。
- [COMMON] 模型注册与能力描述。
- [INFERRED] Virtual Model 抽象，避免业务系统绑定具体模型。置信度：HIGH。
- [COMMON] OpenAI-compatible Gateway。
- [COMMON] 统一 API Key、Quota、Rate Limit。
- [COMMON] 智能路由、Failover、Retry。
- [COMMON] Token、成本、缓存和性能统计。
- [COMMON] Prompt 版本管理。
- [COMMON] Playground。
- [COMMON] Knowledge Base、RAG、Enterprise Search。
- [COMMON] Agent、Tool、MCP。
- [COMMON] Evaluation。
- [COMMON] AI Audit 与 Telemetry。
- [COMMON] 敏感信息、数据分级、模型使用策略。

## 1.3 非目标

[INFERRED] V1 不建设以下能力，避免把产品做成“大而全 AI 平台”后无法完成核心闭环。置信度：HIGH。

- 模型训练平台。
- GPU 集群调度平台。
- Fine-tuning 全流程平台。
- Kubernetes 管理平台。
- 复杂 BPMN 式 Agent Workflow Builder。
- 大规模知识图谱平台。
- 完整企业 IAM 替代品。
- 通用 OA 审批平台。

---

# 2. 用户角色与核心场景

## 2.1 系统角色

| 角色 | 主要权限 |
|---|---|
| Super Admin | 系统级配置、存储、认证、Provider、审计策略 |
| AI Admin | Provider、Model、Virtual Model、路由、Prompt、Agent、知识库管理 |
| Security Auditor | 只读查看审计、敏感数据命中、策略执行记录 |
| App Owner | 管理所属 Application、API Key、Quota、Usage |
| Knowledge Owner | 管理所属知识库、文档、索引策略 |
| Developer | Playground、API 调试、模型调用、查看自身 Application Usage |
| End User | 对话、文件分析、企业知识检索、Agent 使用 |

[INFERRED] Desktop Mode 可将 Super Admin、AI Admin、App Owner 合并为本地所有者角色；Server Mode 再启用完整角色模型。置信度：HIGH。

## 2.2 核心场景

### 场景 A：统一模型 API

```text
OpenCode / IDE / 法治平台 / Python Script
                 ↓
        AI Hub Gateway
                 ↓
         Virtual Model
                 ↓
      Provider / Local Model
```

### 场景 B：个人本地 AI Hub

```text
AI Hub Desktop
├─ SQLite
├─ Local Files
├─ Keychain
├─ Local Gateway
├─ Python Runtime
└─ Ollama / Cloud APIs
```

### 场景 C：企业 Server

```text
Users / Business Apps
         ↓
     HTTPS Gateway
         ↓
      Rust Core
   ┌─────┼─────┐
   ↓     ↓     ↓
Postgres S3  Runtime
```

### 场景 D：Connected Desktop

```text
AI Hub Desktop
      ↓ OIDC / Token
Enterprise AI Hub Server
      ↓
Company Models / KB / Agents / Policies
```

---

# 3. 功能架构

## 3.1 一级模块

```text
Enterprise AI Hub
│
├── Dashboard
├── Providers
├── Models
├── Virtual Models
├── Applications
├── API Keys
├── Gateway
├── Router & Policies
├── Usage & Cost
├── Audit & Telemetry
├── Prompts
├── Playground
├── Knowledge
│   ├── Knowledge Bases
│   ├── Documents
│   ├── Retrieval
│   └── Enterprise Search
├── Agents
├── Tools / MCP
├── Evaluation
├── Security
├── Settings
└── Runtime
```

## 3.2 模块边界原则

[COMMON] UI 不直接访问数据库。

[KNOWN] Tauri Command 不承载领域业务逻辑，只负责把 Desktop UI 请求适配到 Rust Core。

[COMMON] Gateway 不负责管理后台 CRUD，它只读取已发布的模型、路由、配额和安全策略快照。

[INFERRED] Python Runtime 不直接拥有 Provider、Application、Quota、Cost 等控制面数据；其职责是执行 AI 任务并返回标准化运行结果。置信度：HIGH。

---

# 4. 系统总体架构

```text
                         ┌─────────────────────┐
                         │     React UI        │
                         └─────────┬───────────┘
                                   │
                    ┌──────────────┴──────────────┐
                    │                             │
             Desktop Host                    Web Host
               Tauri                         Browser
                    │                             │
                    └──────────────┬──────────────┘
                                   ↓
                    ┌─────────────────────────────┐
                    │          Rust Core          │
                    │                             │
                    │ Domain / Application        │
                    │ Control Plane               │
                    │ AI Gateway                  │
                    │ Routing / Policy            │
                    │ Usage / Cost / Audit        │
                    └──────────────┬──────────────┘
                                   │ Ports
              ┌────────────────────┼────────────────────┐
              ↓                    ↓                    ↓
          Storage              Secret Store         Runtime
              ↓                    ↓                    ↓
       SQLite/Postgres       Keychain/Vault      Python Sidecar
              │                                         │
              ├──────── Vector Store ──────────────────┤
              │                                         │
              ↓                                         ↓
        Local/S3 Files                           RAG/Agent/Eval
                                   │
                                   ↓
                       Cloud / Local Model APIs
```

---

# 5. Desktop / Server / Connected Desktop 差异矩阵

| 能力 | Desktop Mode | Server Mode | Connected Desktop |
|---|---|---|---|
| UI Host | Tauri | Browser | Tauri |
| Rust Core | 本机 | 服务器 | 主要在服务器，本地保留客户端 Core |
| DB | SQLite | PostgreSQL | 服务端 PostgreSQL + 可选本地缓存 |
| 文件存储 | Local Filesystem | S3-compatible / Local | 服务端为主 |
| Secret | OS Keychain | Env/Vault/Secret Adapter | 本地 Token + 服务端 Secret |
| Vector | SQLite Vector/LanceDB | pgvector/Vector Adapter | 服务端为主 |
| Auth | Local Owner | OIDC/SSO/JWT/API Key | OIDC/Token |
| 多用户 | 否/极简 | 是 | 是 |
| RBAC/ABAC | 简化 | 完整 | 服务端执行 |
| Gateway Bind | 127.0.0.1 | 0.0.0.0/内网 | 可选本地代理到 Server |
| Python Runtime | Managed Sidecar | Local/Remote Runtime | Server Runtime 为主 |
| 自动更新 | Desktop Updater | 运维升级 | Desktop Updater |
| 单实例锁 | 必须 | 不适用 | 必须 |
| 审计 | 本机 | 企业级集中审计 | 服务端集中审计 |

[INFERRED] 业务域层不允许到处判断 `if desktop` / `if server`，差异必须通过 Adapter 和配置注入处理。置信度：HIGH。

---

# 6. 技术选型

| 层 | 首选技术 | 说明 |
|---|---|---|
| Desktop | Tauri 2 | [INFERRED] 轻量、Rust 原生集成，适合本地 Gateway 与系统能力。置信度：HIGH |
| Frontend | React + TypeScript | [COMMON] 成熟组件生态与状态管理生态 |
| Build | Vite | [COMMON] 桌面前端构建简洁 |
| Core | Rust | [INFERRED] 同时覆盖 Desktop 和 Server，适合 Gateway、并发、流式传输和本地系统能力。置信度：HIGH |
| HTTP Server | Axum | [INFERRED] 与 Tokio/Tower 生态匹配，适合 API 和 SSE。置信度：HIGH |
| Async Runtime | Tokio | [COMMON] Rust 主流异步运行时 |
| HTTP Client | reqwest | [COMMON] Provider Adapter 统一客户端基础 |
| Serialization | serde | [COMMON] Rust JSON/配置序列化 |
| DB | SQLite / PostgreSQL | [KNOWN] Desktop / Server 分别采用嵌入式和服务端数据库 |
| DB Access | SQLx | [INFERRED] 同时支持 SQLite/Postgres，减少双 Adapter 重复实现。置信度：HIGH |
| Desktop Secret | keyring/OS native | [COMMON] 系统凭据存储 |
| Runtime | Python 3.x + FastAPI/内部 RPC | [INFERRED] AI 生态隔离为 Sidecar。置信度：HIGH |
| Vector | SQLite Vector/LanceDB + pgvector | [INFERRED] 通过 VectorStore Port 抽象。置信度：HIGH |
| Observability | tracing + OpenTelemetry | [COMMON] 分布式 Trace 与结构化日志 |
| Testing | cargo test / pytest / Playwright | [COMMON] 分层测试 |

---

# 7. Monorepo / Rust Workspace 目录结构

```text
enterprise-ai-hub/
│
├── apps/
│   ├── desktop/
│   │   ├── src-tauri/
│   │   └── tauri.conf.json
│   └── server/
│       └── src/main.rs
│
├── web/
│   ├── src/
│   ├── components/
│   ├── pages/
│   ├── features/
│   └── api/
│
├── crates/
│   ├── domain/
│   ├── application/
│   ├── gateway/
│   ├── provider-core/
│   ├── provider-openai/
│   ├── provider-anthropic/
│   ├── provider-gemini/
│   ├── provider-openai-compatible/
│   ├── persistence/
│   ├── storage/
│   ├── vector-store/
│   ├── secrets/
│   ├── auth/
│   ├── policy/
│   ├── audit/
│   ├── telemetry/
│   ├── runtime-client/
│   └── api-types/
│
├── runtime/
│   ├── app/
│   ├── rag/
│   ├── parsing/
│   ├── agents/
│   ├── tools/
│   ├── eval/
│   └── tests/
│
├── migrations/
│   ├── sqlite/
│   └── postgres/
│
├── docs/
├── scripts/
├── tests/
│   ├── contracts/
│   ├── integration/
│   └── e2e/
│
├── Cargo.toml
└── README.md
```

## 7.1 依赖方向

```text
api-types
   ↑
domain
   ↑
application
   ↑
┌─────────────┬───────────────┬───────────────┐
│ gateway     │ desktop host  │ server host   │
└─────────────┴───────────────┴───────────────┘

Adapters → implement Ports defined by domain/application
```

[COMMON] Domain 不得依赖 SQLx、Axum、Tauri、reqwest、具体 Provider SDK。

[COMMON] Application 层可以依赖 Domain，但不得直接依赖具体数据库实现。

[INFERRED] Provider-specific crate 只实现 Provider Adapter，不得侵入 Gateway 路由逻辑。置信度：HIGH。

---

# 8. Domain Model 与核心聚合

## 8.1 核心实体

```text
Provider
Model
VirtualModel
VirtualModelTarget
Application
ApiKey
QuotaPolicy
RoutingPolicy
SecurityPolicy
AIRequest
UsageRecord
CostRecord
AuditEvent
Prompt
PromptVersion
KnowledgeBase
Document
DocumentChunk
Agent
AgentVersion
Tool
McpServer
EvalDataset
EvalCase
EvalRun
```

## 8.2 聚合建议

| Aggregate | Aggregate Root | 主要子实体 |
|---|---|---|
| Provider Aggregate | Provider | ProviderCredentialRef, ProviderHealth |
| Model Aggregate | Model | ModelCapability, ModelPrice |
| Virtual Model Aggregate | VirtualModel | VirtualModelTarget |
| Application Aggregate | Application | ApiKey, QuotaPolicy |
| Prompt Aggregate | Prompt | PromptVersion |
| Knowledge Aggregate | KnowledgeBase | Document, DocumentChunk |
| Agent Aggregate | Agent | AgentVersion, AgentToolBinding |
| Evaluation Aggregate | EvalDataset | EvalCase, EvalRun |

[INFERRED] Request/Usage/Audit 属于高频 append-only 数据，不应被设计成复杂聚合关系。置信度：HIGH。


---

# 9. 数据库设计

## 9.1 通用设计规则

[COMMON] 所有主键使用 UUID/ULID 字符串，避免 Desktop 与 Server 间迁移时发生自增 ID 冲突。

[COMMON] 所有时间点统一存储 UTC；前端根据本地时区展示。

[INFERRED] Desktop 与 Server 使用同一逻辑 Schema，但迁移脚本分别维护 SQLite/PostgreSQL 方言。置信度：HIGH。

类型映射：

| 逻辑类型 | SQLite | PostgreSQL |
|---|---|---|
| ID | TEXT | UUID / TEXT |
| Bool | INTEGER | BOOLEAN |
| Timestamp | TEXT/INTEGER | TIMESTAMPTZ |
| JSON | TEXT | JSONB |
| Decimal Cost | INTEGER micro-unit | BIGINT micro-unit |
| Enum | TEXT + CHECK | TEXT/CHECK 或 enum |

[INFERRED] 金额不使用浮点数，统一存最小计费单位整数，例如 `cost_microunits`。置信度：HIGH。

## 9.2 providers

| 字段 | 类型 | 约束 | 说明 |
|---|---|---|---|
| id | UUID | PK | Provider ID |
| key | string | UNIQUE NOT NULL | 稳定程序标识 |
| name | string | NOT NULL | 展示名称 |
| kind | string | NOT NULL | openai / anthropic / gemini / openai_compatible / ollama 等 |
| base_url | string | NOT NULL | API Base URL |
| credential_ref | string | NULL | SecretStore 引用 |
| proxy_url | string | NULL | 可选代理 |
| timeout_ms | int | NOT NULL | 默认超时 |
| max_retries | int | NOT NULL | Provider 默认重试上限 |
| enabled | bool | NOT NULL | 是否启用 |
| config_json | json | NOT NULL | Provider 特有配置 |
| created_at | timestamp | NOT NULL | |
| updated_at | timestamp | NOT NULL | |

索引：`key` 唯一索引、`enabled` 普通索引。

## 9.3 models

| 字段 | 类型 | 约束 | 说明 |
|---|---|---|---|
| id | UUID | PK | |
| provider_id | UUID | FK NOT NULL | providers.id |
| model_key | string | NOT NULL | Provider 实际模型名 |
| display_name | string | NOT NULL | |
| model_type | string | NOT NULL | chat/reasoning/embedding/rerank/image/multimodal |
| context_window | int | NULL | |
| max_output_tokens | int | NULL | |
| capabilities_json | json | NOT NULL | vision/tools/json/stream/reasoning/cache 等 |
| pricing_json | json | NOT NULL | 标准化价格表 |
| enabled | bool | NOT NULL | |
| discovered | bool | NOT NULL | 自动发现或手工创建 |
| metadata_json | json | NOT NULL | |
| created_at | timestamp | NOT NULL | |
| updated_at | timestamp | NOT NULL | |

唯一约束：`(provider_id, model_key)`。

## 9.4 virtual_models

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| key | string | UNIQUE NOT NULL |
| name | string | NOT NULL |
| description | string | NULL |
| routing_strategy | string | NOT NULL |
| enabled | bool | NOT NULL |
| config_json | json | NOT NULL |
| created_at | timestamp | NOT NULL |
| updated_at | timestamp | NOT NULL |

## 9.5 virtual_model_targets

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| virtual_model_id | UUID | FK NOT NULL |
| model_id | UUID | FK NOT NULL |
| priority | int | NOT NULL |
| weight | int | NOT NULL DEFAULT 100 |
| enabled | bool | NOT NULL |
| condition_json | json | NOT NULL |
| overrides_json | json | NOT NULL |

唯一约束：`(virtual_model_id, model_id)`。

## 9.6 applications

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| key | string | UNIQUE NOT NULL |
| name | string | NOT NULL |
| owner_user_id | UUID | NULL |
| owner_department_id | UUID | NULL |
| status | string | NOT NULL |
| allowed_models_json | json | NOT NULL |
| allowed_tools_json | json | NOT NULL |
| allowed_kb_json | json | NOT NULL |
| monthly_budget_microunits | bigint | NULL |
| metadata_json | json | NOT NULL |
| created_at | timestamp | NOT NULL |
| updated_at | timestamp | NOT NULL |

## 9.7 api_keys

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| application_id | UUID | FK NOT NULL |
| name | string | NOT NULL |
| prefix | string | NOT NULL |
| secret_hash | string | NOT NULL |
| scopes_json | json | NOT NULL |
| expires_at | timestamp | NULL |
| last_used_at | timestamp | NULL |
| revoked_at | timestamp | NULL |
| created_at | timestamp | NOT NULL |

[COMMON] 明文 Application API Key 仅创建时返回一次，数据库只保存不可逆 hash 与可识别 prefix。

## 9.8 quota_policies

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| subject_type | string | NOT NULL |
| subject_id | UUID/string | NOT NULL |
| rpm | int | NULL |
| tpm | bigint | NULL |
| daily_requests | bigint | NULL |
| monthly_tokens | bigint | NULL |
| monthly_cost_microunits | bigint | NULL |
| exceed_action | string | NOT NULL |
| fallback_virtual_model_id | UUID | NULL |
| enabled | bool | NOT NULL |

唯一约束：`(subject_type, subject_id)`。

## 9.9 ai_requests

[INFERRED] `ai_requests` 只保存请求元数据与可选内容引用，是否保存完整 Prompt/Response 由审计策略决定。置信度：HIGH。

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| trace_id | string | INDEX |
| application_id | UUID | NULL/INDEX |
| user_id | UUID | NULL/INDEX |
| api_key_id | UUID | NULL |
| endpoint | string | NOT NULL |
| requested_model | string | NOT NULL |
| resolved_model_id | UUID | NULL |
| provider_id | UUID | NULL |
| status | string | NOT NULL |
| http_status | int | NULL |
| started_at | timestamp | NOT NULL |
| completed_at | timestamp | NULL |
| ttft_ms | int | NULL |
| latency_ms | int | NULL |
| retry_count | int | NOT NULL DEFAULT 0 |
| cache_status | string | NULL |
| error_code | string | NULL |
| error_message_safe | string | NULL |
| metadata_json | json | NOT NULL |

索引：`started_at`、`application_id + started_at`、`provider_id + started_at`、`status + started_at`。

## 9.10 usage_records

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| request_id | UUID | FK UNIQUE NOT NULL |
| input_tokens | bigint | NOT NULL DEFAULT 0 |
| output_tokens | bigint | NOT NULL DEFAULT 0 |
| cached_input_tokens | bigint | NOT NULL DEFAULT 0 |
| reasoning_tokens | bigint | NOT NULL DEFAULT 0 |
| total_tokens | bigint | NOT NULL DEFAULT 0 |
| usage_source | string | NOT NULL | provider/estimated |
| raw_usage_json | json | NOT NULL |
| created_at | timestamp | NOT NULL |

## 9.11 cost_records

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| request_id | UUID | FK UNIQUE NOT NULL |
| currency | string | NOT NULL |
| input_cost_microunits | bigint | NOT NULL |
| output_cost_microunits | bigint | NOT NULL |
| cache_cost_microunits | bigint | NOT NULL |
| reasoning_cost_microunits | bigint | NOT NULL |
| total_cost_microunits | bigint | NOT NULL |
| pricing_snapshot_json | json | NOT NULL |
| created_at | timestamp | NOT NULL |

[INFERRED] 每条 CostRecord 保存计费快照，避免模型价格修改后历史账单被重新解释。置信度：HIGH。

## 9.12 audit_events

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| trace_id | string | INDEX |
| actor_type | string | NOT NULL |
| actor_id | string | NULL |
| event_type | string | NOT NULL |
| resource_type | string | NULL |
| resource_id | string | NULL |
| decision | string | NULL |
| payload_ref | string | NULL |
| metadata_json | json | NOT NULL |
| created_at | timestamp | NOT NULL INDEX |

## 9.13 prompts / prompt_versions

`prompts`：

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| key | string | UNIQUE NOT NULL |
| name | string | NOT NULL |
| description | string | NULL |
| owner_id | UUID | NULL |
| created_at | timestamp | NOT NULL |
| updated_at | timestamp | NOT NULL |

`prompt_versions`：

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| prompt_id | UUID | FK NOT NULL |
| version | int | NOT NULL |
| status | string | NOT NULL |
| system_template | text | NULL |
| user_template | text | NULL |
| variables_schema_json | json | NOT NULL |
| model_config_json | json | NOT NULL |
| output_schema_json | json | NULL |
| created_by | UUID | NULL |
| created_at | timestamp | NOT NULL |

唯一约束：`(prompt_id, version)`。

## 9.14 knowledge_bases

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| key | string | UNIQUE NOT NULL |
| name | string | NOT NULL |
| visibility | string | NOT NULL |
| owner_user_id | UUID | NULL |
| owner_department_id | UUID | NULL |
| retrieval_config_json | json | NOT NULL |
| embedding_model_id | UUID | NULL |
| rerank_model_id | UUID | NULL |
| status | string | NOT NULL |
| created_at | timestamp | NOT NULL |
| updated_at | timestamp | NOT NULL |

## 9.15 documents

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| knowledge_base_id | UUID | FK NOT NULL |
| filename | string | NOT NULL |
| mime_type | string | NOT NULL |
| size_bytes | bigint | NOT NULL |
| file_hash | string | NOT NULL |
| storage_key | string | NOT NULL |
| parse_status | string | NOT NULL |
| index_status | string | NOT NULL |
| parser_version | string | NULL |
| metadata_json | json | NOT NULL |
| created_by | UUID | NULL |
| created_at | timestamp | NOT NULL |
| updated_at | timestamp | NOT NULL |

索引：`knowledge_base_id`、`file_hash`。

## 9.16 document_chunks

| 字段 | 类型 | 约束 |
|---|---|---|
| id | UUID | PK |
| document_id | UUID | FK NOT NULL |
| chunk_index | int | NOT NULL |
| content | text | NOT NULL |
| content_hash | string | NOT NULL |
| page_no | int | NULL |
| section_path | string | NULL |
| token_count | int | NULL |
| vector_ref | string | NULL |
| metadata_json | json | NOT NULL |

唯一约束：`(document_id, chunk_index)`。

## 9.17 agents / agent_versions

`agents`：`id, key, name, description, owner_id, status, created_at, updated_at`。

`agent_versions`：

```text
id
agent_id
version
status
model_ref
prompt_version_id
max_steps
timeout_ms
memory_config_json
knowledge_bindings_json
tool_bindings_json
policy_json
created_at
```

唯一约束：`(agent_id, version)`。

## 9.18 tools / mcp_servers

`tools`：

```text
id
key
name
description
kind              # http / builtin / mcp
input_schema_json
output_schema_json
permission_json
timeout_ms
enabled
created_at
updated_at
```

`mcp_servers`：

```text
id
key
name
transport         # stdio / streamable-http 等
endpoint_or_command
credential_ref
config_json
enabled
created_at
updated_at
```

## 9.19 eval_* 表

```text
eval_datasets
eval_cases
eval_runs
eval_results
```

`eval_results` 至少保存：

```text
run_id
case_id
candidate_ref
response_ref
latency_ms
cost_microunits
score_json
judge_json
created_at
```

---

# 10. Provider Adapter 统一协议

## 10.1 设计目标

[INFERRED] Gateway 内部必须只处理统一 `CanonicalRequest/CanonicalResponse`，Provider 特有协议只能存在于 Adapter 内。置信度：HIGH。

## 10.2 Provider trait

```rust
#[async_trait]
pub trait ModelProvider: Send + Sync {
    fn kind(&self) -> ProviderKind;

    async fn list_models(&self) -> Result<Vec<DiscoveredModel>, ProviderError>;

    async fn chat(
        &self,
        request: CanonicalChatRequest,
    ) -> Result<CanonicalChatResponse, ProviderError>;

    async fn chat_stream(
        &self,
        request: CanonicalChatRequest,
    ) -> Result<ProviderStream, ProviderError>;

    async fn embeddings(
        &self,
        request: CanonicalEmbeddingRequest,
    ) -> Result<CanonicalEmbeddingResponse, ProviderError>;

    async fn health_check(&self) -> Result<ProviderHealth, ProviderError>;
}
```

## 10.3 CanonicalChatRequest

```rust
pub struct CanonicalChatRequest {
    pub model: String,
    pub messages: Vec<Message>,
    pub tools: Vec<ToolDefinition>,
    pub tool_choice: Option<ToolChoice>,
    pub temperature: Option<f32>,
    pub top_p: Option<f32>,
    pub max_output_tokens: Option<u32>,
    pub response_format: Option<ResponseFormat>,
    pub reasoning: Option<ReasoningConfig>,
    pub stream: bool,
    pub metadata: Map<String, Value>,
}
```

## 10.4 CanonicalUsage

```rust
pub struct CanonicalUsage {
    pub input_tokens: u64,
    pub output_tokens: u64,
    pub cached_input_tokens: u64,
    pub reasoning_tokens: u64,
    pub total_tokens: u64,
    pub source: UsageSource,
}
```

## 10.5 StreamEvent

```text
ResponseStarted
ContentDelta
ReasoningDelta
ToolCallStarted
ToolCallArgumentsDelta
ToolCallCompleted
UsageUpdated
ResponseCompleted
ProviderError
```

[INFERRED] Provider Adapter 必须将各厂商 SSE/event 结构映射成上述统一事件；Gateway 再根据客户端协议转换成 OpenAI-compatible 输出。置信度：HIGH。

## 10.6 Provider Error 分类

```text
Authentication
PermissionDenied
RateLimited
QuotaExceeded
InvalidRequest
ModelNotFound
ContextLengthExceeded
Timeout
Connection
Provider5xx
MalformedResponse
ContentFiltered
Unknown
```

[COMMON] Gateway 的 retry/failover 决策基于统一错误类别，而不是 Provider 原始 HTTP 状态码字符串。

---

# 11. OpenAI-Compatible Gateway 规范

## 11.1 V1 必须支持

```text
GET  /v1/models
POST /v1/chat/completions
POST /v1/embeddings
```

[INFERRED] `/v1/responses` 应在 V1.x 尽早加入，但不应阻塞最初 Gateway 可用版本。置信度：HIGH。

后续：

```text
POST /v1/responses
POST /v1/rerank              # 扩展接口
```

## 11.2 模型解析

客户端：

```json
{
  "model": "general-smart"
}
```

解析顺序：

```text
VirtualModel key 命中
    ↓ no
Physical Model alias 命中
    ↓ no
允许 direct model 时检查 provider/model
    ↓ no
MODEL_NOT_FOUND
```

[INFERRED] Server 默认推荐业务应用只允许 Virtual Model；管理员 Playground 可允许直接指定 Physical Model。置信度：HIGH。

## 11.3 Header

支持：

```text
Authorization: Bearer <application-key>
X-AIH-Request-ID: optional client request id
X-AIH-Task: optional task type
X-AIH-User-ID: only trusted upstream mode
```

[INFERRED] 普通客户端不得通过自定义 Header 任意伪造用户身份；`X-AIH-User-ID` 仅对显式注册的 trusted application 生效。置信度：HIGH。

## 11.4 Gateway 返回扩展 Header

```text
X-AIH-Request-ID
X-AIH-Resolved-Model
X-AIH-Provider
X-AIH-Cache
X-AIH-Retry-Count
```

[INFERRED] 成本等敏感信息是否返回由 Application Policy 控制。置信度：HIGH。

---

# 12. Gateway 请求执行流水线

## 12.1 固定执行顺序

```text
1. Accept Request
2. Generate/Validate Request ID + Trace ID
3. Parse Client Protocol
4. Authenticate
5. Resolve Application/User
6. Authorize Endpoint/Model
7. Validate Request
8. Check Quota
9. Rate Limit
10. Apply Security/DLP Policy
11. Resolve Virtual Model
12. Build Candidate Route
13. Select Provider Target
14. Execute Provider Adapter
15. Retry/Failover if eligible
16. Normalize Stream/Response
17. Collect Usage
18. Calculate Cost
19. Persist Request/Usage/Cost
20. Emit Audit/Telemetry
21. Return/Close Stream
```

## 12.2 Retry 策略

允许自动 retry：

- [COMMON] 连接失败。
- [COMMON] 建连超时。
- [COMMON] 明确可重试的 429。
- [COMMON] 临时性 5xx。

默认不 retry：

- [COMMON] 401/403。
- [COMMON] 参数错误。
- [COMMON] Context Length Exceeded。
- [COMMON] Content Policy 拒绝。
- [INFERRED] 已经向客户端输出有效 token 后，不得静默切换模型重放同一流，否则可能产生重复内容。置信度：HIGH。

## 12.3 Failover

Failover 触发条件：

```text
Provider unavailable
RateLimited and policy allows
Timeout before first content
Retry exhausted
Circuit breaker open
```

候选排序支持：

```text
priority
weighted
lowest_cost
lowest_latency
policy_rule
```

[INFERRED] V1 实际实现先支持 `priority + failover`，Weighted/Cost/Latency Router 后续增加，避免路由算法先于可靠性基础设施。置信度：HIGH。

## 12.4 Streaming

[COMMON] Gateway 必须支持客户端断开检测。

客户端取消：

```text
client disconnect
   ↓
cancel Rust task
   ↓
abort provider stream when possible
   ↓
record status=client_cancelled
   ↓
persist available usage
```

## 12.5 Provider 不返回 Usage

处理优先级：

```text
1 Provider explicit usage
2 Provider final event usage
3 Local tokenizer estimate
4 unknown
```

[INFERRED] 估算值必须标记 `usage_source=estimated`，不得与厂商原始 usage 混同。置信度：HIGH。

## 12.6 Circuit Breaker

状态：

```text
CLOSED → OPEN → HALF_OPEN → CLOSED
```

建议 Provider Target 维度维护，而不是整个 Provider 维度；同一 Provider 的某个模型故障不必使其他模型全部退出候选。

---

# 13. API 设计

## 13.1 Admin API

### Provider

```text
GET    /api/admin/providers
POST   /api/admin/providers
GET    /api/admin/providers/{id}
PATCH  /api/admin/providers/{id}
DELETE /api/admin/providers/{id}
POST   /api/admin/providers/{id}/test
POST   /api/admin/providers/{id}/discover-models
```

### Model

```text
GET    /api/admin/models
POST   /api/admin/models
GET    /api/admin/models/{id}
PATCH  /api/admin/models/{id}
POST   /api/admin/models/{id}/enable
POST   /api/admin/models/{id}/disable
```

### Virtual Model

```text
GET    /api/admin/virtual-models
POST   /api/admin/virtual-models
GET    /api/admin/virtual-models/{id}
PATCH  /api/admin/virtual-models/{id}
PUT    /api/admin/virtual-models/{id}/targets
POST   /api/admin/virtual-models/{id}/simulate-route
```

### Application/API Key

```text
GET    /api/admin/applications
POST   /api/admin/applications
PATCH  /api/admin/applications/{id}
POST   /api/admin/applications/{id}/keys
GET    /api/admin/applications/{id}/keys
DELETE /api/admin/applications/{id}/keys/{keyId}
```

### Usage / Audit

```text
GET /api/admin/usage/summary
GET /api/admin/usage/timeseries
GET /api/admin/usage/by-model
GET /api/admin/usage/by-application
GET /api/admin/cost/summary
GET /api/admin/requests
GET /api/admin/requests/{id}
GET /api/admin/audit
```

### Prompt/Knowledge/Agent/Eval

```text
/api/admin/prompts/**
/api/admin/knowledge-bases/**
/api/admin/documents/**
/api/admin/agents/**
/api/admin/tools/**
/api/admin/mcp-servers/**
/api/admin/evals/**
```

## 13.2 Runtime API

[INFERRED] Runtime API 默认只监听 loopback 或受信内部网络，不对普通终端用户暴露。置信度：HIGH。

```text
GET  /internal/health
POST /internal/documents/parse
POST /internal/embeddings
POST /internal/rerank
POST /internal/retrieval/query
POST /internal/agents/run
POST /internal/evals/run
```

请求必须携带 Runtime Session Token。

## 13.3 统一响应格式（Admin API）

成功：

```json
{
  "data": {},
  "meta": {
    "requestId": "..."
  }
}
```

错误：

```json
{
  "error": {
    "code": "PROVIDER_AUTH_FAILED",
    "message": "Provider authentication failed",
    "requestId": "...",
    "details": {}
  }
}
```

分页：

```json
{
  "data": [],
  "meta": {
    "page": 1,
    "pageSize": 50,
    "total": 1250
  }
}
```

---

# 14. Authentication / Authorization / Application / API Key

## 14.1 Desktop Auth

默认：本地 Owner Identity。

可选：应用启动锁、系统生物识别/系统认证由 Desktop Adapter 实现，不侵入 Domain。

## 14.2 Server Auth

接口：

```rust
trait IdentityProvider {
    async fn authenticate(&self, credential: Credential) -> Result<Identity>;
}
```

实现：

```text
LocalIdentityProvider
OidcIdentityProvider
TrustedHeaderIdentityProvider
ApiKeyIdentityProvider
```

## 14.3 RBAC + ABAC

RBAC 解决“角色能做什么”；ABAC 解决“对哪一个资源能做”。

有效权限：

```text
Role Permission
∩ Resource Scope
∩ Application Policy
∩ Security Policy
```

Agent/Tool 进一步：

```text
Effective Agent Permission
=
User Permission
∩ Application Permission
∩ Agent Permission
∩ Tool Permission
∩ Data Policy
```

## 14.4 API Key 格式

建议：

```text
aih_live_<public-prefix>_<secret>
aih_test_<public-prefix>_<secret>
```

[COMMON] Secret 使用密码学安全随机源生成。

## 14.5 Key 生命周期

```text
Created
  ↓
Active
  ↓
Revoked / Expired
```

支持双 Key 滚动：创建新 Key → 更新应用 → 验证 → 撤销旧 Key。

---

# 15. Quota / Rate Limit

## 15.1 限制维度

```text
Global
Application
API Key
User
Virtual Model
Physical Model
Provider
```

## 15.2 V1 算法

[INFERRED] Desktop 使用内存 Token Bucket + 持久化月度 Usage；Server 单实例同样使用本地 limiter，多实例时再通过共享 Adapter 引入 Redis 等分布式限流。置信度：HIGH。

## 15.3 超限动作

```text
BLOCK
WARN_ONLY
FALLBACK_MODEL
```

响应：

```text
HTTP 429
error.code = AIH_RATE_LIMITED / AIH_QUOTA_EXCEEDED
```

---

# 16. Provider / Model / Virtual Model / Router

## 16.1 Provider 生命周期

```text
Draft
 ↓ test success
Active
 ↓
Disabled
```

健康状态独立：

```text
Unknown / Healthy / Degraded / Unavailable
```

## 16.2 Model Capability

统一 Capability：

```json
{
  "chat": true,
  "streaming": true,
  "vision": true,
  "tools": true,
  "parallelTools": false,
  "jsonMode": true,
  "structuredOutput": false,
  "reasoning": true,
  "promptCache": true,
  "embeddings": false,
  "rerank": false
}
```

## 16.3 Virtual Model

建议预置：

```text
general-fast
general-smart
reasoning
coding
private
embedding-default
rerank-default
```

[INFERRED] 预置只是初始模板，真正企业环境应允许管理员重新映射而不修改客户端。置信度：HIGH。

## 16.4 RoutingPolicy 示例

```json
{
  "strategy": "priority_failover",
  "targets": [
    {"modelId": "A", "priority": 10},
    {"modelId": "B", "priority": 20}
  ],
  "retry": {
    "maxAttemptsPerTarget": 2,
    "retry429": true,
    "retry5xx": true
  }
}
```

## 16.5 路由模拟器

Admin UI 输入：

```text
Application
Virtual Model
Task
Data Classification
Estimated Tokens
```

返回：

```text
Matched Policies
Candidate Targets
Excluded Targets + reason
Selected Target
Fallback Order
```

[INFERRED] 该功能对调试复杂企业策略很有价值，应在 Server 完整策略体系形成时提供。置信度：HIGH。


---

# 17. Usage / Cost / Audit / Telemetry

## 17.1 Usage 采集

统一 Usage 维度：

```text
requestId
application
user
apiKey
provider
physicalModel
virtualModel
task
inputTokens
outputTokens
cachedInputTokens
reasoningTokens
totalTokens
usageSource
```

[INFERRED] Usage 入库与客户端响应不得强耦合；非流式请求可同步写入，流式请求应在结束时快速持久化，失败时允许通过本地异步任务补写。置信度：HIGH。

## 17.2 Cost Engine

价格配置：

```json
{
  "currency": "USD",
  "unitTokens": 1000000,
  "input": 1.0,
  "output": 4.0,
  "cachedInput": 0.2,
  "reasoning": null
}
```

计算：

```text
inputCost = inputTokens / unitTokens × inputPrice
outputCost = outputTokens / unitTokens × outputPrice
cacheCost = cachedInputTokens / unitTokens × cachedInputPrice
```

[INFERRED] Provider 存在按请求、图片、音频、工具或搜索次数计费时，通过 `pricing_json` 扩展，不强行塞入纯 token 模型。置信度：HIGH。

## 17.3 Dashboard 指标

```text
Requests Today
Requests This Month
Tokens
Cost
Success Rate
P50/P95 Latency
TTFT
Cache Hit Rate
Top Applications
Top Models
Top Providers
Error Distribution
```

## 17.4 Audit 保存级别

```text
NONE
METADATA_ONLY
REDACTED_CONTENT
FULL_CONTENT
```

[INFERRED] 默认企业模式推荐 `METADATA_ONLY` 或 `REDACTED_CONTENT`，完整 Prompt/Response 应由显式安全策略开启，而不是默认永久保存。置信度：HIGH。

## 17.5 Trace

统一：

```text
trace_id    一次业务调用链
request_id  一次 Gateway 请求
span_id     某个内部操作
run_id      Agent/Eval 等高级 Runtime 执行
```

---

# 18. Prompt Center 与 Playground

## 18.1 Prompt 生命周期

```text
Draft
 ↓
Testing
 ↓
Published
 ↓
Deprecated
```

[INFERRED] Published 版本不可原地修改；修改必须创建新版本，以保证生产调用可追溯。置信度：HIGH。

## 18.2 Prompt Variable

示例：

```json
{
  "type": "object",
  "required": ["document"],
  "properties": {
    "document": {"type": "string"},
    "language": {"type": "string", "default": "zh-CN"}
  }
}
```

## 18.3 Playground 页面

功能：

- Physical / Virtual Model 切换。
- System/User Message 编辑。
- 多轮对话。
- Temperature、Top P、Max Tokens。
- Reasoning 配置。
- JSON/Structured Output。
- Tool Definitions。
- Streaming 开关。
- 结果并排比较。
- Token、TTFT、Latency、Cost 实时展示。
- Raw Request/Response 查看。
- 一键保存为 Prompt。
- 从 Prompt Version 加载。

## 18.4 Compare Mode

```text
Same Input
├─ Candidate A
├─ Candidate B
└─ Candidate C
```

结果表：

| Candidate | Result | TTFT | Latency | Tokens | Cost |
|---|---|---:|---:|---:|---:|

---

# 19. Knowledge Base / RAG / Enterprise Search

## 19.1 文档摄取流程

```text
Upload
 ↓
Hash / Deduplicate
 ↓
Store Original
 ↓
Parse
 ↓
Normalize
 ↓
Structure Extraction
 ↓
Chunk
 ↓
Embedding
 ↓
Vector Store
 ↓
Index Ready
```

状态：

```text
UPLOADED
PARSING
PARSE_FAILED
CHUNKING
EMBEDDING
READY
INDEX_FAILED
```

## 19.2 Parser 接口

```python
class DocumentParser(Protocol):
    async def parse(self, file: FileRef) -> ParsedDocument: ...
```

`ParsedDocument`：

```text
text
pages
sections
tables
images metadata
source mappings
```

[INFERRED] Parser 输出必须保留 page/section/source mapping，否则 RAG 无法提供可靠引用定位。置信度：HIGH。

## 19.3 Chunking

V1 支持：

```text
fixed_token
recursive_text
section_aware
```

默认配置：

```json
{
  "strategy": "section_aware",
  "targetTokens": 700,
  "overlapTokens": 100,
  "minTokens": 100
}
```

[INFERRED] 不将固定 chunk size 写死在代码中，Knowledge Base 必须保存自己的 indexing profile。置信度：HIGH。

## 19.4 Retrieval Pipeline

```text
User Query
 ↓
Access Scope Filter
 ↓
Query Normalize
 ↓
Optional Query Rewrite
 ↓
Keyword Search ─┐
                ├→ Fusion → Candidate Set
Vector Search ──┘
 ↓
Metadata Filter
 ↓
Rerank
 ↓
Top K
 ↓
Context Builder
 ↓
LLM
```

## 19.5 Retrieval Result

```json
{
  "chunkId": "...",
  "documentId": "...",
  "score": 0.91,
  "content": "...",
  "citation": {
    "filename": "...",
    "page": 12,
    "section": "..."
  }
}
```

## 19.6 Enterprise Search

[INFERRED] Enterprise Search 与 Knowledge Base 问答分离：前者目标是“找到资源”，后者目标是“生成答案”。置信度：HIGH。

Search Provider Port：

```rust
trait EnterpriseSearchSource {
    async fn search(&self, query: SearchQuery) -> Result<Vec<SearchHit>>;
}
```

后续可接：

```text
AI Hub Knowledge
法治平台
OA
合同系统
项目系统
采购系统
文件系统
```

## 19.7 权限前置

[INFERRED] 检索权限必须在召回阶段过滤，不能先召回无权数据后依赖 LLM“不说出来”。置信度：HIGH。

---

# 20. Agent / Tool / MCP / Eval

## 20.1 Agent Runtime

执行结构：

```text
AgentRun
 ↓
Load Published AgentVersion
 ↓
Resolve User/Application Permission
 ↓
Load Prompt / Model / KB / Tools
 ↓
Step 1 LLM
 ↓
Tool Calls?
 ├─ no → Complete
 └─ yes
      ↓
   Authorize Tool
      ↓
   Execute Tool
      ↓
   Append Result
      ↓
   Next Step
```

## 20.2 AgentRun 状态

```text
QUEUED
RUNNING
WAITING_TOOL
COMPLETED
FAILED
CANCELLED
TIMEOUT
```

## 20.3 Agent Guardrails

必须限制：

```text
maxSteps
maxToolCalls
maxRuntimeMs
maxCost
allowedModels
allowedTools
allowedKnowledgeBases
```

[INFERRED] V1 Agent 不允许任意 shell/文件系统写入工具作为默认能力；高风险 Tool 必须显式授权。置信度：HIGH。

## 20.4 Tool 执行

统一接口：

```python
class ToolExecutor(Protocol):
    async def execute(
        self,
        tool: ToolDefinition,
        arguments: dict,
        context: ExecutionContext,
    ) -> ToolResult: ...
```

ToolResult：

```text
status
content
structured_data
artifacts
error
latency
```

## 20.5 MCP

MCP Server Registry 保存连接配置与权限。

[INFERRED] MCP Tool 被发现后应映射为平台内部标准 ToolDefinition，再进入同一权限、审计、超时和 ToolCall 流程，而不是绕开 Tool Center。置信度：HIGH。

## 20.6 Evaluation

Eval Dataset：

```text
Dataset
 └─ Case
     ├─ input
     ├─ expectedOutput optional
     ├─ referenceContext optional
     └─ metadata
```

Eval Candidate：

```text
Physical Model
Virtual Model
Prompt Version
Agent Version
RAG Pipeline Version
```

指标：

```text
Exact/Rule Score
LLM Judge
Human Score
Latency
TTFT
Tokens
Cost
Groundedness
Citation Accuracy
Tool Success Rate
```

[INFERRED] 自动 Judge 只是一种评估信号，不应被当成绝对真值；关键生产评估应允许人工评分。置信度：HIGH。

---

# 21. Storage / Secret / Cache / Queue / Vector Adapter

## 21.1 Persistence Port

核心 Repository：

```text
ProviderRepository
ModelRepository
VirtualModelRepository
ApplicationRepository
RequestRepository
UsageRepository
PromptRepository
KnowledgeRepository
AgentRepository
AuditRepository
```

实现：

```text
SqliteRepositories
PostgresRepositories
```

[INFERRED] 使用 SQLx 时可以共享相当部分 query/domain mapping，但不要为了追求 100% SQL 共用而牺牲数据库正确性。置信度：HIGH。

## 21.2 ObjectStorage Port

```rust
#[async_trait]
pub trait ObjectStorage {
    async fn put(&self, key: &str, body: ByteStream) -> Result<ObjectMeta>;
    async fn get(&self, key: &str) -> Result<ByteStream>;
    async fn delete(&self, key: &str) -> Result<()>;
    async fn exists(&self, key: &str) -> Result<bool>;
}
```

实现：

```text
LocalObjectStorage
S3ObjectStorage
```

## 21.3 SecretStore Port

```rust
trait SecretStore {
    async fn get(&self, key: &SecretRef) -> Result<SecretValue>;
    async fn set(&self, key: &SecretRef, value: SecretValue) -> Result<()>;
    async fn delete(&self, key: &SecretRef) -> Result<()>;
}
```

Desktop：OS Keychain。  
Server：Environment / File Secret / Vault Adapter。

## 21.4 VectorStore Port

```text
upsert(collection, vectors)
query(collection, vector, filter, topK)
deleteByDocument(documentId)
createCollection(config)
```

Desktop 实现优先：SQLite vector extension 或 LanceDB。  
Server 实现优先：pgvector。

## 21.5 Cache

V1：

```text
Rust in-memory cache
SQLite persistent semantic/exact cache optional
```

Cache Key 至少包含：

```text
model/version
prompt hash
parameters hash
application scope
security scope
knowledge version
```

[INFERRED] 默认关闭跨用户 Semantic Cache，除非安全域完全一致。置信度：HIGH。

## 21.6 Queue

Desktop：Tokio task + SQLite job table。  
Server 单实例：同上。  
Server 多实例：后续实现 QueuePort，接 Redis Streams / NATS / 其他队列。

[INFERRED] 文档解析和 Embedding 适合 Job 模型；聊天 Gateway 不应通过通用消息队列绕一圈。置信度：HIGH。

---

# 22. 前端信息架构与页面规格

## 22.1 全局布局

```text
Top Bar
├─ Workspace / Server Selector
├─ Search
├─ Runtime Status
└─ User / Settings

Side Navigation
├─ Overview
├─ Models
├─ Applications
├─ Playground
├─ Prompts
├─ Knowledge
├─ Agents
├─ Evaluation
├─ Observability
├─ Audit
└─ Settings
```

## 22.2 Overview

组件：

```text
Requests
Tokens
Cost
Success Rate
P95
Provider Health
Cost Trend
Usage by Model
Usage by Application
Recent Errors
Runtime Status
```

Desktop 首次启动空状态必须提供：

```text
1 Add Provider
2 Test Connection
3 Discover/Add Model
4 Create Virtual Model
5 Open Playground
```

## 22.3 Providers 页面

列表字段：

```text
Name
Kind
Base URL
Models
Health
Last Check
Enabled
```

操作：

```text
Add
Edit
Test
Discover Models
Enable/Disable
Rotate Credential
Delete
```

Provider Detail：

```text
General
Credential
Models
Health
Requests
Errors
Cost
Raw Configuration
```

## 22.4 Models 页面

筛选：Provider、Type、Capability、Enabled。

详情：

```text
Capabilities
Context
Pricing
Provider
Aliases
Recent Performance
Usage
```

## 22.5 Virtual Models 页面

列表：

```text
key
strategy
target count
primary target
status
```

编辑页：

```text
Targets drag/order
Priority
Weight
Conditions
Retry Policy
Fallback
Simulation
```

## 22.6 Applications 页面

详情 Tabs：

```text
Overview
API Keys
Models
Quota
Knowledge
Tools
Usage
Audit
```

创建 Key 后仅一次展示完整 Secret，并提供复制按钮与明确不可再次查看提示。

## 22.7 Playground

布局建议：

```text
Left: Request Builder
Center: Conversation/Response
Right: Model Config + Metrics
Bottom Drawer: Raw / Trace / Usage / Cost
```

## 22.8 Prompts

```text
Prompt List
Prompt Detail
Version History
Diff
Test in Playground
Publish
Deprecate
```

## 22.9 Knowledge

Knowledge Base List：文档数、Chunks、索引状态、Embedding Model、Owner。

Document List：文件名、类型、大小、Parse Status、Index Status、上传时间。

Document Detail：

```text
Original metadata
Parsed preview
Chunk preview
Index status
Errors
Reindex
Delete
```

## 22.10 Agents

```text
Agent List
Builder
Versions
Runs
Tool Calls
Knowledge Bindings
Permissions
```

[INFERRED] V1 Builder 使用表单式配置，不实现节点画布式 Workflow Designer。置信度：HIGH。

## 22.11 Evaluation

```text
Datasets
Cases
Runs
Comparison
Scores
Regression History
```

## 22.12 Observability

```text
Requests
Latency
TTFT
Tokens/sec
Error Rate
Provider Health
Circuit Breakers
Runtime Jobs
```

## 22.13 Audit

筛选：时间、用户、Application、Event Type、Model、Resource、Decision。

Audit Detail 必须明确区分：

```text
Metadata
Policy Decisions
Tool Calls
RAG Sources
Content Snapshot (if policy allows)
```

## 22.14 Settings

```text
General
Gateway
Database
Runtime
Storage
Security
Telemetry
Update
Backup
Server Connection
Advanced
```

---

# 23. Desktop Mode 设计

## 23.1 启动顺序

```text
1 OS Single Instance Lock
2 Resolve App Data Directory
3 Load bootstrap config
4 Initialize SecretStore
5 Open SQLite
6 Run migrations
7 Initialize Core Services
8 Start Local Gateway
9 Start/Attach Python Runtime if enabled
10 Health Check Runtime
11 Load React UI
12 Background Provider Health Check
```

[INFERRED] 数据库迁移失败时不得继续进入正常 UI，应进入 Recovery 页面，提供备份、日志和重试。置信度：HIGH。

## 23.2 单实例

第二个进程启动：

```text
Detect existing instance
 ↓
Send activate/open-deeplink message
 ↓
Exit second instance
```

## 23.3 Local Gateway

默认：

```text
127.0.0.1:8787
```

支持端口冲突自动选择，但应在 UI 中明确展示当前 endpoint。

安全：

- 仅绑定 loopback。
- API Key 鉴权仍然开启。
- 管理 API 不直接暴露给任意本机应用；Desktop UI 优先使用 Tauri IPC 或受保护 Admin API。

## 23.4 Runtime Sidecar 生命周期

```text
Desktop Core
  ↓ spawn
aihub-runtime --port 0 --session-token <ephemeral>
  ↓ stdout handshake
PORT=54321
  ↓
Core health check
```

异常：

```text
Runtime crash
→ mark unavailable
→ restart with bounded backoff
→ UI notification
→ after threshold require manual restart
```

[INFERRED] 不允许无限快速重启形成 crash loop。置信度：HIGH。

## 23.5 Local Data Directory

逻辑结构：

```text
AIHub/
├── aihub.db
├── config.toml
├── documents/
├── vectors/
├── cache/
├── backups/
├── logs/
└── runtime/
```

## 23.6 Desktop 自动更新

更新流程：

```text
Check
↓
Download signed package
↓
Verify signature
↓
Backup DB when migration required
↓
Install
↓
Run migration
```

[COMMON] 自动更新包必须进行签名验证。

---

# 24. Server Mode 设计

## 24.1 启动

```text
aihub-server --config /etc/aihub/config.toml
```

启动顺序：

```text
Load config
Validate required secrets
Connect PostgreSQL
Run/verify migrations
Connect ObjectStorage
Connect Runtime
Initialize Core
Start Admin API
Start Gateway API
Start health/readiness endpoints
```

## 24.2 Health Endpoints

```text
GET /health/live
GET /health/ready
```

`live`：进程活着。  
`ready`：DB/Core 已可服务。

## 24.3 Reverse Proxy

可选：

```text
Caddy / Nginx / LB
        ↓ HTTPS
AI Hub Server
```

[KNOWN] Reverse Proxy 不是 AI Hub 启动必要条件；Server 可自行监听 HTTP/TLS，生产环境可根据运维体系增加代理。

## 24.4 横向扩展

无状态部分：

```text
Admin API
Gateway
```

共享状态：

```text
PostgreSQL
Object Storage
Optional distributed RateLimit/Cache Adapter
Runtime Cluster
```

[INFERRED] 在真正启用多实例前，需要把内存 RateLimit、Circuit Breaker 和 Queue 的一致性要求逐项评估，不应仅因为“能启动两个实例”就宣称完成横向扩展。置信度：HIGH。

---

# 25. Connected Desktop Mode

## 25.1 Workspace

桌面客户端支持：

```text
Local Workspace
Server Workspace A
Server Workspace B
```

每个 Workspace 保存：

```text
name
serverUrl
authMethod
localCachePolicy
lastConnectedAt
```

Server Token 存 OS Keychain。

## 25.2 行为

连接 Server Workspace 后：

- Provider/Model 读取服务器。
- 企业 Knowledge/Agent 读取服务器。
- Usage/Cost/Audit 在服务器记录。
- 本地 UI 设置保留本机。
- 可选 Local Tool 通过受控 Desktop Bridge 暴露给 Server Agent。

[INFERRED] Desktop Bridge 属于后续高风险高级功能，MVP Connected Mode 不实现“服务器 Agent 任意操作本机”。置信度：HIGH。

---

# 26. 配置规范

## 26.1 config.toml 示例

```toml
mode = "desktop"

[gateway]
host = "127.0.0.1"
port = 8787
request_timeout_ms = 120000

[database]
driver = "sqlite"
url = "./data/aihub.db"

[storage]
driver = "local"
path = "./data/documents"

[vector]
driver = "local"
path = "./data/vectors"

[runtime]
mode = "managed"
enabled = true

[telemetry]
log_level = "info"
otel_enabled = false
```

Server：

```toml
mode = "server"

[gateway]
host = "0.0.0.0"
port = 8787

[database]
driver = "postgres"
url_env = "AIHUB_DATABASE_URL"

[storage]
driver = "s3"
endpoint_env = "AIHUB_S3_ENDPOINT"
bucket = "aihub"

[runtime]
mode = "remote"
url = "http://127.0.0.1:9000"
```

## 26.2 环境变量优先级

```text
CLI Argument
>
Environment Variable
>
Config File
>
Default
```

## 26.3 环境变量命名

统一前缀：

```text
AIHUB_MODE
AIHUB_DATABASE_URL
AIHUB_GATEWAY_HOST
AIHUB_GATEWAY_PORT
AIHUB_RUNTIME_URL
AIHUB_LOG_LEVEL
```

[INFERRED] API Provider Secret 不建议大量塞入普通 config.toml；统一通过 SecretStore 管理。置信度：HIGH。

---

# 27. 错误码规范

## 27.1 格式

```text
DOMAIN_REASON
```

## 27.2 Gateway

```text
AIH_INVALID_REQUEST
AIH_UNAUTHORIZED
AIH_FORBIDDEN
AIH_RATE_LIMITED
AIH_QUOTA_EXCEEDED
AIH_MODEL_NOT_FOUND
AIH_MODEL_NOT_ALLOWED
AIH_ROUTE_NOT_FOUND
AIH_PROVIDER_UNAVAILABLE
AIH_PROVIDER_AUTH_FAILED
AIH_PROVIDER_RATE_LIMITED
AIH_PROVIDER_TIMEOUT
AIH_PROVIDER_ERROR
AIH_CONTEXT_TOO_LONG
AIH_CONTENT_FILTERED
AIH_STREAM_INTERRUPTED
```

## 27.3 Knowledge

```text
KB_NOT_FOUND
DOCUMENT_TOO_LARGE
DOCUMENT_TYPE_UNSUPPORTED
DOCUMENT_PARSE_FAILED
DOCUMENT_INDEX_FAILED
RETRIEVAL_FAILED
```

## 27.4 Runtime

```text
RUNTIME_UNAVAILABLE
RUNTIME_TIMEOUT
RUNTIME_PROTOCOL_ERROR
AGENT_MAX_STEPS
AGENT_TOOL_DENIED
TOOL_EXECUTION_FAILED
MCP_UNAVAILABLE
EVAL_FAILED
```

## 27.5 Admin

```text
PROVIDER_DUPLICATE
MODEL_DUPLICATE
VIRTUAL_MODEL_DUPLICATE
APPLICATION_DUPLICATE
API_KEY_REVOKED
PROMPT_VERSION_CONFLICT
RESOURCE_IN_USE
VALIDATION_FAILED
```

[COMMON] 内部异常日志可保留完整错误链，但外部响应不得泄漏 Secret、SQL、文件路径等敏感实现信息。

---

# 28. 数据迁移、备份与恢复

## 28.1 Migration

每次发布：

```text
App Version
Schema Version
Runtime Protocol Version
```

必须分别记录。

[INFERRED] Desktop 启动时允许自动执行向前兼容 migration；破坏性 migration 前必须自动备份。置信度：HIGH。

## 28.2 Desktop Backup

备份包：

```text
manifest.json
aihub.db
config-export.json
documents/ optional
vectors/ optional
```

Secret 默认不进入普通备份包。

## 28.3 Server Backup

由产品提供导出命令：

```text
aihub-server export --output backup.tar
```

但数据库底层生产备份可继续使用 PostgreSQL 原生运维工具。

## 28.4 Desktop → Server Migration

导出：

```text
Providers metadata
Models
Virtual Models
Prompts
Knowledge metadata/documents
Agents
```

Secret：要求在目标 Server 重新配置或通过显式加密迁移机制处理。

[INFERRED] Usage/Audit 历史默认可选迁移，避免个人历史无必要进入企业环境。置信度：HIGH。

---

# 29. 安全模型

## 29.1 威胁面

```text
Provider Key 泄漏
Application Key 泄漏
恶意本机程序访问 localhost Gateway
Prompt Injection
Tool Abuse
RAG Unauthorized Retrieval
Sensitive Data Exfiltration
Malicious MCP Server
Document Parser Exploit
Audit Data Exposure
Server SSRF
```

## 29.2 基础控制

- Secret 独立存储。
- API Key hash 存储。
- localhost Gateway 仍鉴权。
- Server 强制 HTTPS 或受控反向代理。
- Provider Base URL 配置权限受限。
- Tool 按权限白名单。
- RAG 召回前权限过滤。
- MCP Server 显式注册。
- 文档解析 Runtime 隔离。
- 日志默认脱敏。
- Audit 内容保存策略可配置。

## 29.3 SSRF

[INFERRED] 自定义 Provider、HTTP Tool、MCP HTTP endpoint 都可能形成 SSRF 入口；Server Mode 应提供网络目标策略，例如阻止环回、链路本地、云 metadata 地址和未授权内网网段。置信度：HIGH。

## 29.4 Prompt Injection

[COMMON] Prompt Injection 不能仅通过 System Prompt 从根本消除。

控制：

```text
Tool permission independent from model text
Data authorization independent from model instruction
Untrusted retrieved content marked as data
High-risk tool requires explicit policy
```

## 29.5 数据分级

```text
PUBLIC
INTERNAL
CONFIDENTIAL
STRICT
```

Model Policy 示例：

```text
PUBLIC       → any approved provider
INTERNAL     → approved enterprise providers
CONFIDENTIAL → restricted provider set
STRICT       → local/private provider only
```

## 29.6 DLP

策略链：

```text
Detect
 ↓
Classify
 ↓
ALLOW / MASK / BLOCK / REQUIRE_CONFIRMATION
```

[INFERRED] V1 可先实现规则/正则与自定义关键词 DLP，不必先建设复杂分类模型。置信度：HIGH。

---

# 30. 可观测性

## 30.1 Structured Logging

字段：

```text
timestamp
level
service
request_id
trace_id
application_id
provider_id
model_id
event
latency_ms
error_code
```

不得默认输出：

```text
Provider Secret
Application Secret
完整 Authorization Header
完整 Prompt/Response
```

## 30.2 Metrics

```text
aihub_requests_total
aihub_request_duration_ms
aihub_ttft_ms
aihub_tokens_total
aihub_cost_total
aihub_provider_errors_total
aihub_active_streams
aihub_runtime_jobs
aihub_document_parse_duration
aihub_retrieval_duration
aihub_tool_calls_total
```

## 30.3 OpenTelemetry

Server 可配置 OTLP Exporter。Desktop 默认本地 tracing，不强制外发 Telemetry。

[INFERRED] 用户可观测性和产品遥测分开配置；不能因为启用本地日志就默认把使用数据上传外部服务。置信度：HIGH。

---

# 31. 测试方案

## 31.1 Unit Test

重点：

```text
Routing
Quota
Cost Calculation
Policy Evaluation
Error Mapping
Canonical Request Conversion
Permission Intersection
Chunking
Citation Mapping
```

目标：纯 Domain/Application 逻辑尽可能无网络测试。

## 31.2 Repository Contract Test

同一测试集同时运行：

```text
SQLite implementation
PostgreSQL implementation
```

验证：

```text
CRUD
unique constraints
transactions
pagination
timestamp semantics
JSON roundtrip
migration
```

## 31.3 Provider Contract Test

每个 Provider Adapter 必须通过统一用例：

| 用例 | 必测 |
|---|---|
| 普通 Chat | 是 |
| Streaming | 是 |
| System Message | 是 |
| Tool Call | 能力支持时 |
| JSON Mode | 能力支持时 |
| Vision | 能力支持时 |
| Reasoning | 能力支持时 |
| Usage 解析 | 是 |
| 401 | 是 |
| 429 | 是 |
| 5xx | 是 |
| Timeout | 是 |
| Client Cancel | 是 |
| Malformed Provider Response | 是 |
| Context Too Long | 是 |

[INFERRED] 不通过 Contract Test 的 Adapter 不标记为“Supported”，最多标记 Experimental。置信度：HIGH。

## 31.4 Gateway Integration Test

```text
API Key auth
Virtual Model resolution
Failover
Retry
Quota
Rate Limit
Streaming
Usage
Cost
Audit
Client cancellation
Concurrent requests
```

## 31.5 Runtime Test

```text
PDF parse
DOCX parse
Chunk mapping
Embedding
Vector retrieval
Rerank
Agent step limit
Tool timeout
MCP failure
Eval run
```

## 31.6 Desktop E2E

Playwright/Tauri 测试：

```text
first launch
add provider
test provider
create model
create virtual model
playground call
restart persistence
sidecar crash/restart
port conflict
single instance
backup/restore
```

## 31.7 Server E2E

```text
OIDC/mock auth
multi-user permission
application keys
postgres migrations
S3 adapter
runtime connection
reverse proxy headers
health/readiness
```

## 31.8 性能测试

Gateway 重点：

```text
concurrent non-streaming
concurrent streaming
large context proxy
slow provider
client cancellation storm
provider 429 burst
```

[INFERRED] 性能验收重点应是“Gateway 自身额外开销”和稳定性，而不是把下游模型延迟算成自己的性能问题。置信度：HIGH。

---

# 32. 开发计划与工程任务拆解

## M0：工程骨架

任务：

```text
Rust workspace
React/Vite
Tauri app
Server binary
CI
format/lint/test
shared api-types
config loader
tracing
```

依赖：无。

验收：Desktop 和 Server 都能启动并返回 Core version/health。

## M1：Persistence + Settings

任务：

```text
SQLite adapter
Postgres adapter
migration framework
config.toml
SecretStore abstraction
Keychain adapter
basic settings UI
```

验收：同一 Repository Contract Test 在 SQLite/Postgres 通过。

## M2：Provider + Model Registry

任务：

```text
Provider CRUD
Secret binding
connection test
OpenAI-compatible adapter
model CRUD
model discovery
capability/pricing config
```

验收：Desktop 可配置一个真实 OpenAI-compatible Provider 并发现/手工建立模型。

## M3：Gateway Foundation

任务：

```text
Axum Gateway
/v1/models
/v1/chat/completions non-stream
API Key
canonical protocol
provider error mapping
request record
```

验收：OpenAI SDK 修改 baseURL 后可调用 AI Hub。

## M4：Streaming + Reliability

任务：

```text
SSE
client cancel
retry
failover
provider health
circuit breaker
TTFT
```

验收：模拟 Provider 5xx/429/timeout 可按策略 failover；流取消正确终止。

## M5：Virtual Model + Router

任务：

```text
virtual model CRUD
targets
priority router
policy validation
route trace
```

验收：客户端固定调用 `general-smart`，切换后端模型无需修改客户端。

## M6：Usage + Cost + Dashboard

任务：

```text
usage normalization
pricing snapshot
cost engine
usage aggregation
dashboard charts
request explorer
```

验收：每次请求可追踪到模型、tokens、费用、延迟和状态。

## M7：Application + Quota + Audit

任务：

```text
application
key lifecycle
quota
rate limit
audit event
audit UI
```

验收：两个 Application 可配置不同模型权限和额度。

## M8：Playground + Prompt

任务：

```text
playground
compare mode
prompt CRUD
prompt versions
publish/deprecate
load/save playground
```

验收：Prompt 版本可稳定复现调用配置。

## M9：Desktop Productization

任务：

```text
single instance
app data directory
local gateway settings
keychain
backup/restore
auto update integration
recovery page
```

验收：普通用户安装后无需 Docker/PostgreSQL/Python 手工配置即可使用基础 Gateway。

## M10：Server Productization

任务：

```text
server auth
RBAC
Postgres production path
S3 adapter
health/readiness
server config
connected desktop login
```

验收：另一台机器可通过 HTTPS/API Key 调用 Server；Desktop 可连接 Server Workspace。

## M11：Python Runtime Foundation

任务：

```text
runtime protocol
managed sidecar
remote runtime
health
job execution
crash restart
version negotiation
```

验收：Desktop 可自动启动 runtime，Server 可连接 remote runtime。

## M12：Knowledge Base

任务：

```text
upload
object storage
parse
chunk
embedding
vector adapter
retrieval
citation
knowledge UI
```

验收：上传 PDF/DOCX 后可以检索并返回页码/章节定位。

## M13：Enterprise RAG

任务：

```text
hybrid search
rerank
query rewrite
access filter
context builder
RAG response citations
```

验收：不同权限用户无法检索不属于其权限的数据。

## M14：Agent + Tool

任务：

```text
agent/version
tool registry
agent run
tool auth
timeout/max steps
run trace
```

验收：Agent 可以安全调用一个只读 Tool，并完整记录 ToolCall。

## M15：MCP

任务：

```text
MCP registry
transport adapter
tool discovery
schema mapping
permission integration
audit
```

验收：MCP Tool 与原生 Tool 使用同一权限和审计机制。

## M16：Evaluation

任务：

```text
dataset
cases
runs
candidate comparison
rule metrics
LLM judge
human score
regression view
```

验收：可对两个 Model/Prompt 组合运行同一数据集并比较质量、成本和延迟。

## M17：Security Governance

任务：

```text
data classification
model policy
DLP rules
SSRF policy
content audit levels
security dashboard
```

验收：STRICT 数据分类请求只能路由到允许模型；违规请求可解释地被拒绝。

---

# 33. 里程碑依赖图

```text
M0
 ↓
M1
 ↓
M2
 ↓
M3
 ↓
M4 ─→ M5 ─→ M6 ─→ M7 ─→ M8
                         │
                ┌────────┴────────┐
                ↓                 ↓
               M9                M10
                │                 │
                └────────┬────────┘
                         ↓
                        M11
                         ↓
                        M12
                         ↓
                        M13
                         ↓
                        M14
                         ↓
                        M15
                         ↓
                        M16
                         ↓
                        M17
```

[INFERRED] M9/M10 可在 M8 后并行推进；Knowledge/Agent 必须建立在 Runtime 和基础权限/审计之后。置信度：HIGH。

---

# 34. MVP 定义

## 34.1 Desktop MVP

必须：

```text
Tauri + React
Rust Core
SQLite
Keychain
Provider
Model
Virtual Model
OpenAI-compatible Gateway
Application/API Key
Priority Failover
Streaming
Usage
Cost
Audit Metadata
Playground
Basic Dashboard
Backup
```

MVP 不需要：

```text
Python Runtime
Knowledge Base
Agent
MCP
Eval
S3
Postgres if only validating Desktop
OIDC
Distributed Cache
Docker
Kubernetes
```

## 34.2 Server MVP

在 Desktop Core 基础增加：

```text
Server Host
PostgreSQL
Multi-user Auth
RBAC
Network Gateway
Admin Web
Server Config
```

## 34.3 产品可用性判定

[INFERRED] 当用户能够将 OpenCode/任意 OpenAI SDK 应用的 Base URL 指向 AI Hub，并稳定获得路由、Failover、Usage、Cost、Audit 时，平台核心价值已经成立。置信度：HIGH。

---

# 35. 明确不做项与反模式

## 35.1 不做“聊天页面优先”

[INFERRED] Chat UI 是客户端，不是平台核心。置信度：HIGH。

## 35.2 不做“每个 Provider 写进 Gateway if/else”

必须 Provider Adapter。

## 35.3 不做“Desktop 强依赖 Docker”

Desktop 安装后应直接运行。

## 35.4 不做“Desktop 强依赖 PostgreSQL/Redis/MinIO”

本地使用 SQLite、内存、本地文件系统和 OS Secret。

## 35.5 不做“Tauri Command = Service Layer”

Tauri 仅 Adapter。

## 35.6 不做“Python 管所有业务”

Python 仅 AI Runtime。

## 35.7 不做“Agent 权限靠 Prompt”

权限由 Core Policy 强制执行。

## 35.8 不做“RAG 先检索后权限过滤”

权限必须进入检索条件。

## 35.9 不做“模型名称硬编码进业务系统”

业务系统优先使用 Virtual Model。

## 35.10 不做“为了未来规模先上微服务”

[INFERRED] 先保持模块化单体 Core + Runtime 边界，只有明确独立扩容/故障域需求时再拆服务。置信度：HIGH。

---

# 36. 发布物

## 36.1 Desktop

```text
AIHub.dmg
AIHub.exe
AIHub.AppImage / package optional
```

包含：

```text
React Assets
Rust Core
SQLite Migrations
Optional Runtime Sidecar
```

## 36.2 Server

```text
aihub-server
```

原则：Rust Server 核心为单二进制，可直接 systemd/命令行运行。

## 36.3 Runtime

```text
aihub-runtime
```

Desktop 可随安装包提供；Server 可独立部署。

## 36.4 可选部署附件

```text
Dockerfile
docker-compose.yml
Helm chart later
systemd unit
Caddy/Nginx sample
```

[KNOWN] 这些属于附加交付，不是产品运行逻辑的必要组成。

---

# 37. 版本兼容策略

必须维护：

```text
Core API Version
Gateway Compatibility Version
Database Schema Version
Runtime Protocol Version
Desktop Version
Server Version
```

Runtime handshake：

```json
{
  "runtimeVersion": "1.2.0",
  "protocolVersion": "1",
  "capabilities": ["parse", "embedding", "rerank", "agent"]
}
```

[INFERRED] Core 发现不兼容 Runtime Protocol 时应明确拒绝高级功能，而不是继续调用产生不可预测错误。置信度：HIGH。

---

# 38. API Versioning

Gateway：保持 `/v1/*` 对 OpenAI-compatible 客户端兼容。

平台管理 API：

```text
/api/v1/admin/...
```

Runtime：

```text
/internal/v1/...
```

[INFERRED] 内部 Runtime API 仍需要版本号，因为 Desktop Core 和 Runtime Sidecar 可能存在升级时间差。置信度：HIGH。

---

# 39. 核心状态机

## 39.1 Provider

```text
DRAFT → ACTIVE ↔ DISABLED
          │
          └ health: HEALTHY/DEGRADED/UNAVAILABLE
```

## 39.2 Document

```text
UPLOADED
  ↓
PARSING → PARSE_FAILED
  ↓
CHUNKING
  ↓
EMBEDDING → INDEX_FAILED
  ↓
READY
```

## 39.3 Prompt Version

```text
DRAFT → TESTING → PUBLISHED → DEPRECATED
```

## 39.4 Agent Version

```text
DRAFT → PUBLISHED → DEPRECATED
```

## 39.5 Request

```text
ACCEPTED
 ↓
ROUTING
 ↓
RUNNING
 ├→ COMPLETED
 ├→ FAILED
 ├→ CLIENT_CANCELLED
 └→ TIMEOUT
```

---

# 40. 关键接口契约示例

## 40.1 创建 Provider

```http
POST /api/v1/admin/providers
Content-Type: application/json
```

```json
{
  "key": "deepseek-primary",
  "name": "DeepSeek Primary",
  "kind": "openai_compatible",
  "baseUrl": "https://example-provider/v1",
  "credential": {
    "type": "api_key",
    "value": "***"
  },
  "timeoutMs": 120000,
  "enabled": true
}
```

响应不回显 Secret：

```json
{
  "data": {
    "id": "...",
    "key": "deepseek-primary",
    "credentialConfigured": true
  }
}
```

## 40.2 创建 Virtual Model

```json
{
  "key": "general-smart",
  "name": "General Smart",
  "routingStrategy": "priority_failover",
  "targets": [
    {"modelId": "model-a", "priority": 10},
    {"modelId": "model-b", "priority": 20}
  ]
}
```

## 40.3 OpenAI-compatible 调用

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer aih_live_xxx" \
  -H "Content-Type: application/json" \
  -d '{
    "model":"general-smart",
    "messages":[{"role":"user","content":"hello"}],
    "stream":true
  }'
```

---

# 41. Coding Rules

## 41.1 Rust

- Domain Error 使用明确 enum，不用字符串判断。
- 禁止在 Domain 中直接 `reqwest`。
- 禁止 Provider Adapter 直接访问 Application Repository。
- 所有 Secret 类型实现防止 Debug 明文输出。
- 请求取消必须向下游传播 cancellation。
- Streaming 使用 bounded channel/backpressure，避免无限缓存。
- 数据库事务边界由 Application Service 明确控制。

## 41.2 TypeScript

- API 类型从 OpenAPI/共享 schema 生成或集中维护。
- 页面不直接拼接后端 URL。
- Server/Local Workspace 通过统一 client abstraction。
- Server state 使用 Query Cache；复杂表单状态独立管理。

## 41.3 Python Runtime

- Runtime 不读取主数据库凭据。
- Runtime 所需任务数据由 Core 传入或通过限定 StorageRef 获取。
- 每类任务明确 timeout/cancel。
- Parser、Embedder、Reranker、AgentExecutor 均接口化。
- Runtime stdout 不输出敏感正文；结构化日志写 stderr/日志通道。

---

# 42. CI/CD 与质量门禁

每次 PR：

```text
cargo fmt --check
cargo clippy -- -D warnings
cargo test
frontend lint/typecheck/test
python lint/typecheck/test
contract tests with mocks
migration validation
```

Release：

```text
Desktop build/sign
Server build
Runtime package
SBOM
Checksums
Release notes
Migration notes
```

[COMMON] Desktop 可执行文件和自动更新包应使用平台签名机制。

---

# 43. 验收标准

## 43.1 Core/Gateway

必须达到：

- OpenAI SDK 可通过替换 Base URL/API Key 直接使用。
- Streaming 正常。
- 客户端取消可向下游传播。
- Provider 429/5xx/timeout 按策略处理。
- Virtual Model 可无客户端改动切换物理模型。
- 每次请求产生 Trace/Usage/Cost/Audit metadata。
- Key 撤销后立即禁止新请求。

## 43.2 Desktop

必须达到：

- 安装后无需 Docker。
- 安装后无需用户安装 PostgreSQL。
- 基础 Gateway 无需 Python 即可运行。
- Runtime 功能启用时由应用管理 Sidecar。
- SQLite 自动迁移。
- Keychain 正常保存 Provider Credential。
- 支持备份与恢复。
- 本地 Gateway 默认仅监听 loopback。

## 43.3 Server

必须达到：

- 单二进制 Core 可启动。
- PostgreSQL 模式通过 Repository Contract Test。
- 多用户鉴权与 RBAC 可用。
- Application API Key 可供外部系统调用。
- Runtime 可独立部署。
- Health/Readiness 可供运维系统探测。

## 43.4 Knowledge

必须达到：

- 文档上传、解析、Chunk、Embedding、Retrieval 闭环。
- Retrieval Result 保留文档/page/section 映射。
- 权限过滤在召回前执行。
- 删除文档可删除对应向量。

## 43.5 Agent

必须达到：

- Published Agent Version 不可原地修改。
- maxSteps/maxRuntime 生效。
- Tool 调用必须通过权限检查。
- ToolCall 全量审计。
- Client cancel 能停止 AgentRun。

---

# 44. 后续演进路线

## Stage A：AI Gateway Product

```text
Provider
Model
Virtual Model
Gateway
Application
Usage
Cost
Audit
Playground
```

## Stage B：Enterprise Control Plane

```text
Server
SSO
RBAC/ABAC
Security Policy
Quota
Connected Desktop
```

## Stage C：Knowledge Platform

```text
Document AI
RAG
Hybrid Search
Enterprise Search
Citation
```

## Stage D：Agent Runtime

```text
Agents
Tools
MCP
Agent Evaluation
```

## Stage E：Governance

```text
DLP
Model Governance
Eval Regression
Budget Governance
Policy Simulation
Compliance Audit
```

## Stage F：Scale Adapters

仅在有实际需求时增加：

```text
Distributed Rate Limit
Redis Cache
Queue Service
Runtime Cluster
Dedicated Vector DB
Kubernetes
Multi-region
```

[INFERRED] 这一路线保持核心 Domain/API 稳定，把“规模化组件”延后到真正出现规模需求之后。置信度：HIGH。

---

# 45. 最终架构图

```text
                                Clients
          ┌───────────────────────┼────────────────────────┐
          ↓                       ↓                        ↓
   AI Hub Desktop          Business Systems          Browser Admin
          │                       │                        │
          │ Tauri IPC             │ OpenAI-compatible     │ Admin API
          └───────────────────────┼────────────────────────┘
                                  ↓
                     ┌─────────────────────────┐
                     │       Rust Core         │
                     │                         │
                     │ Domain/Application      │
                     │ Control Plane           │
                     │ Gateway                 │
                     │ Routing/Policy          │
                     │ Usage/Cost/Audit        │
                     └───────────┬─────────────┘
                                 │ Ports
       ┌─────────────────────────┼─────────────────────────────┐
       ↓                         ↓                             ↓
Persistence                 Object/Vector                  Secrets
       ↓                         ↓                             ↓
SQLite/Postgres        Local/S3 + Local/pgvector       Keychain/Vault
                                 │
                                 ↓
                       ┌──────────────────┐
                       │ Python Runtime   │
                       │                  │
                       │ Parse / RAG      │
                       │ Embed / Rerank   │
                       │ Agent / Tool     │
                       │ MCP / Eval       │
                       └────────┬─────────┘
                                │
                   ┌────────────┼──────────────┐
                   ↓            ↓              ↓
               Cloud LLM    Local LLM      External Tools
```

---

# 46. 最终工程决策清单

| 决策 | 结论 |
|---|---|
| 产品本体 | Rust Core |
| Desktop 壳 | Tauri |
| 前端 | React + TypeScript |
| Server | 同一 Rust Core 的 Server Host |
| Gateway | Rust/Axum |
| AI Runtime | Python Sidecar/Remote Runtime |
| Desktop DB | SQLite |
| Server DB | PostgreSQL |
| Desktop File | Local Filesystem |
| Server File | S3-compatible 或 Local Adapter |
| Desktop Secret | OS Keychain |
| Server Secret | SecretStore Adapter |
| Desktop Vector | SQLite Vector / LanceDB |
| Server Vector | pgvector，后续可替换 |
| Docker | 可选部署附件 |
| Kubernetes | 后续规模化附件 |
| Provider 接入 | Adapter |
| Client 模型引用 | Virtual Model 优先 |
| Agent 权限 | Core 强制交集权限 |
| RAG 权限 | Retrieval 前置过滤 |
| Prompt | Versioned / Published immutable |
| Agent | Versioned / Published immutable |
| Usage | 标准化并标记来源 |
| Cost | 保存 pricing snapshot |
| Audit | 分级保存，默认非全文 |

---

# 47. 开发启动顺序（可直接执行）

[INFERRED] 如果从零开始编码，建议严格按以下顺序推进，避免提前进入 Agent/RAG 导致基础设施返工。置信度：HIGH。

```text
01 初始化 monorepo/workspace
02 建 domain/application/api-types
03 建 config + tracing
04 建 SQLite/Postgres Repository contracts
05 建 Provider/Model domain
06 实现 OpenAI-compatible Provider Adapter
07 实现 Provider 管理与 Connection Test
08 实现 Gateway 非流式 Chat
09 实现 Application/API Key
10 实现 Streaming + Cancel
11 实现 Virtual Model + Priority Failover
12 实现 Usage/Cost/Audit
13 实现 Dashboard/Request Explorer
14 完成 Desktop Productization
15 完成 Server Host + Auth/RBAC
16 完成 Connected Desktop
17 引入 Python Runtime Protocol
18 做 Document Parsing/KB
19 做 Retrieval/RAG
20 做 Agent/Tool
21 做 MCP
22 做 Eval
23 做 DLP/Policy Governance
24 最后再决定是否引入分布式基础设施
```

---

# 48. Definition of Done

一个模块只有同时满足以下条件才算完成：

```text
Domain Rule Defined
API Contract Defined
Persistence Implemented
Authorization Implemented
Audit Implemented where applicable
Error Codes Defined
Unit Tests Passed
Integration/Contract Tests Passed
UI Empty/Error/Loading States Implemented
Migration Added
Documentation Updated
```

[INFERRED] “页面能点、接口能返回”不作为模块完成标准。置信度：HIGH。

---

# 49. 项目一句话定义

> **Enterprise AI Hub 是一套以 Rust Core 为核心、同时支持本地桌面与企业服务器部署的统一 AI Control Plane：所有模型统一接入、所有应用统一调用、所有路由统一治理、所有使用统一计量、所有 AI 行为统一审计，并通过独立 Python Runtime 扩展知识、Agent、MCP 与 Evaluation 能力。**

---

# 50. 开发优先级总结

```text
P0
Rust Core
Provider
Model
Virtual Model
Gateway
Application/API Key
Streaming
Failover
Usage/Cost/Audit
Desktop

P1
Server
Auth/RBAC
Prompt
Playground
Connected Desktop

P2
Python Runtime
Knowledge/RAG

P3
Agent/Tool/MCP/Eval

P4
DLP/高级治理/分布式规模化
```

[INFERRED] P0 完成后，该项目已经是一款成立的独立产品；P2/P3 是在稳定 AI 基础设施之上增加高级 AI 能力，而不是反过来定义产品。置信度：HIGH。


---

# 附录 A：辅助数据库表

## A.1 users

Server Mode 内置用户或外部 Identity 映射：

```text
id                  UUID PK
external_subject    string NULL
identity_provider   string NOT NULL
username            string NULL
email               string NULL
display_name        string NOT NULL
status              string NOT NULL
metadata_json       json NOT NULL
created_at          timestamp NOT NULL
updated_at          timestamp NOT NULL
```

唯一约束：`(identity_provider, external_subject)`（external_subject 非空时）。

## A.2 roles / permissions / user_roles

`roles`：

```text
id
key UNIQUE
name
description
system_role bool
created_at
```

`permissions`：

```text
id
key UNIQUE             # provider.read / provider.write / audit.read ...
description
```

`role_permissions`：`role_id, permission_id` 联合主键。

`user_roles`：

```text
user_id
role_id
scope_type             # global / department / application / knowledge_base
scope_id nullable
created_at
```

## A.3 routing_policies

```text
id
key UNIQUE
name
priority
match_json
 action_json
enabled
created_at
updated_at
```

`match_json` 示例：

```json
{
  "applications": ["app-x"],
  "tasks": ["legal_analysis"],
  "dataClassifications": ["CONFIDENTIAL"]
}
```

`action_json` 示例：

```json
{
  "virtualModel": "private-smart",
  "allowedProviderKinds": ["local"]
}
```

## A.4 security_policies

```text
id
key UNIQUE
name
policy_type            # data_classification / dlp / provider_access / tool
priority
rule_json
action_json
enabled
created_at
updated_at
```

## A.5 tool_calls

```text
id UUID PK
trace_id INDEX
agent_run_id NULL INDEX
request_id NULL INDEX
tool_id NOT NULL
actor_user_id NULL
application_id NULL
status NOT NULL
arguments_ref NULL
result_ref NULL
started_at NOT NULL
completed_at NULL
latency_ms NULL
error_code NULL
metadata_json NOT NULL
```

## A.6 agent_runs

```text
id UUID PK
agent_id NOT NULL
agent_version_id NOT NULL
trace_id INDEX
user_id NULL
application_id NULL
status NOT NULL
current_step int NOT NULL
max_steps int NOT NULL
started_at NOT NULL
completed_at NULL
input_ref NULL
output_ref NULL
error_code NULL
usage_json NOT NULL
cost_microunits bigint NOT NULL DEFAULT 0
metadata_json NOT NULL
```

## A.7 runtime_jobs

```text
id UUID PK
job_type NOT NULL
status NOT NULL
resource_type NULL
resource_id NULL
payload_ref NULL
attempt int NOT NULL
max_attempts int NOT NULL
available_at NOT NULL
started_at NULL
completed_at NULL
last_error NULL
created_at NOT NULL
```

索引：`(status, available_at)`。

## A.8 provider_health_samples

```text
id UUID PK
provider_id NOT NULL
model_id NULL
status NOT NULL
latency_ms NULL
http_status NULL
error_category NULL
checked_at NOT NULL
```

[INFERRED] 高频 health sample 应配置保留期，避免长期无限膨胀。置信度：HIGH。

## A.9 content_blobs

[INFERRED] 对 Prompt/Response/Audit 全文建议通过独立 Blob 表或 ObjectStorage 引用保存，避免 `ai_requests` 主表被大字段拖慢。置信度：HIGH。

```text
id UUID PK
kind NOT NULL
storage_mode NOT NULL       # inline / object
content_text NULL
storage_key NULL
content_hash NOT NULL
redaction_level NOT NULL
created_at NOT NULL
expires_at NULL
```

---

# 附录 B：DDL 示例

以下仅展示逻辑实现形式；SQLite/PostgreSQL migration 分别维护实际方言。

```sql
CREATE TABLE virtual_models (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    routing_strategy TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    config_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE virtual_model_targets (
    id TEXT PRIMARY KEY,
    virtual_model_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    priority INTEGER NOT NULL,
    weight INTEGER NOT NULL DEFAULT 100,
    enabled INTEGER NOT NULL DEFAULT 1,
    condition_json TEXT NOT NULL DEFAULT '{}',
    overrides_json TEXT NOT NULL DEFAULT '{}',
    UNIQUE (virtual_model_id, model_id),
    FOREIGN KEY (virtual_model_id) REFERENCES virtual_models(id),
    FOREIGN KEY (model_id) REFERENCES models(id)
);

CREATE INDEX idx_vmt_virtual_priority
ON virtual_model_targets(virtual_model_id, enabled, priority);
```

PostgreSQL `usage_records` 示例：

```sql
CREATE TABLE usage_records (
    id UUID PRIMARY KEY,
    request_id UUID NOT NULL UNIQUE REFERENCES ai_requests(id),
    input_tokens BIGINT NOT NULL DEFAULT 0,
    output_tokens BIGINT NOT NULL DEFAULT 0,
    cached_input_tokens BIGINT NOT NULL DEFAULT 0,
    reasoning_tokens BIGINT NOT NULL DEFAULT 0,
    total_tokens BIGINT NOT NULL DEFAULT 0,
    usage_source TEXT NOT NULL,
    raw_usage_json JSONB NOT NULL DEFAULT '{}'::jsonb,
    created_at TIMESTAMPTZ NOT NULL
);
```

---

# 附录 C：权限矩阵

| Resource | Super Admin | AI Admin | Auditor | App Owner | Developer | End User |
|---|---:|---:|---:|---:|---:|---:|
| Provider Read | ✓ | ✓ | ✓ | - | - | - |
| Provider Write | ✓ | ✓ | - | - | - | - |
| Model Read | ✓ | ✓ | ✓ | ✓ | ✓ | limited |
| Model Write | ✓ | ✓ | - | - | - | - |
| Virtual Model Write | ✓ | ✓ | - | - | - | - |
| Application Read | ✓ | ✓ | ✓ | own | assigned | - |
| Application Write | ✓ | ✓ | - | own | - | - |
| API Key Create | ✓ | ✓ | - | own | scoped | - |
| Usage Read | ✓ | ✓ | ✓ | own | own | own optional |
| Audit Read | ✓ | configurable | ✓ | own metadata | - | - |
| Prompt Write | ✓ | ✓ | - | scoped | scoped | - |
| KB Write | ✓ | ✓ | - | scoped | scoped | - |
| Agent Write | ✓ | ✓ | - | scoped | scoped | - |
| Tool Register | ✓ | ✓ | - | - | - | - |
| Eval Run | ✓ | ✓ | read | scoped | scoped | - |

[INFERRED] 实际 Server Mode 应在角色权限基础上叠加 Resource Scope，不能只依靠上表的全局角色。置信度：HIGH。

---

# 附录 D：Provider Adapter 支持矩阵模板

每个 Provider 的能力不硬编码成“厂商天然支持”，以实际 Adapter Contract Test 结果为准。

| Provider Adapter | Chat | Stream | Tools | Vision | JSON | Reasoning | Embedding | Usage | Status |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|
| OpenAI-compatible Generic | required | required | tested capability | tested capability | tested capability | tested capability | optional | required | Core |
| OpenAI | contract test | contract test | contract test | contract test | contract test | contract test | contract test | contract test | Adapter |
| Anthropic | contract test | contract test | contract test | contract test | mapped | contract test | n/a/adapter | contract test | Adapter |
| Gemini | contract test | contract test | contract test | contract test | contract test | contract test | optional | contract test | Adapter |
| Ollama | contract test | contract test | capability-based | capability-based | capability-based | capability-based | capability-based | adapter | Adapter |

[COMMON] 当 Provider API 行为发生变化时，应以 Contract Test 和官方协议为准更新 Adapter，而不是修改统一 Domain 以迁就单个异常实现。

---

# 附录 E：目录与本地数据规范

## E.1 macOS

逻辑位置使用系统 Application Support 目录，不在代码中硬编码用户名路径。

```text
Application Support/Enterprise AI Hub/
├── config.toml
├── aihub.db
├── documents/
├── vectors/
├── cache/
├── logs/
└── backups/
```

## E.2 Windows

使用系统 AppData API 解析数据目录。

## E.3 临时文件

Runtime 解析临时文件必须放 OS temp/app temp；任务结束或崩溃恢复时清理。

[INFERRED] 原始知识文档不应依赖临时路径，上传后必须进入受管理 ObjectStorage。置信度：HIGH。

---

# 附录 F：开发验收 Checklist

## Core

- [ ] Domain 无基础设施反向依赖
- [ ] SQLite/Postgres Repository Contract Test
- [ ] Provider Adapter Contract Test
- [ ] Secret 不写普通日志
- [ ] Error mapping 统一

## Gateway

- [ ] `/v1/models`
- [ ] `/v1/chat/completions`
- [ ] streaming
- [ ] cancellation
- [ ] API Key
- [ ] quota
- [ ] failover
- [ ] usage
- [ ] cost
- [ ] audit

## Desktop

- [ ] single instance
- [ ] localhost only by default
- [ ] SQLite migration
- [ ] Keychain
- [ ] backup
- [ ] recovery
- [ ] runtime lifecycle
- [ ] updater signing

## Server

- [ ] PostgreSQL
- [ ] Auth
- [ ] RBAC/Scope
- [ ] S3 adapter
- [ ] health/readiness
- [ ] external API Key
- [ ] Connected Desktop

## Runtime

- [ ] version handshake
- [ ] health
- [ ] cancel
- [ ] parse
- [ ] embedding
- [ ] retrieval
- [ ] rerank
- [ ] agent
- [ ] tool
- [ ] MCP
- [ ] eval

## Security

- [ ] localhost auth
- [ ] secret redaction
- [ ] retrieval ACL
- [ ] tool authorization
- [ ] SSRF policy
- [ ] audit content policy
- [ ] DLP policy

---

# 附录 G：建议的首个开发 Sprint Backlog

[INFERRED] 如果由一个人从零启动，首个 Sprint 不碰 RAG/Agent，先验证架构主干。置信度：HIGH。

```text
S1-01 Cargo workspace + Tauri + React
S1-02 domain/application crate
S1-03 config + tracing
S1-04 SQLite migrations
S1-05 ProviderRepository + ModelRepository
S1-06 SecretStore + Keychain
S1-07 Generic OpenAI-compatible Provider Adapter
S1-08 Provider CRUD UI
S1-09 Provider connection test
S1-10 Model Registry UI
S1-11 Axum localhost Gateway
S1-12 /v1/models
S1-13 /v1/chat/completions non-stream
S1-14 minimal Application/API Key
S1-15 OpenAI SDK smoke test
```

Sprint 完成判定：

> OpenAI SDK 将 `base_url` 改为本机 AI Hub 后，可以使用 AI Hub Application Key 通过 Rust Gateway 调用已配置 Provider；Provider Secret 存在 OS Keychain；请求元数据进入 SQLite。

---

# 附录 H：架构变更准入规则

[INFERRED] 后续增加技术组件前先回答以下问题，任一答案不成立则默认不增加。置信度：HIGH。

1. 当前已有 Adapter/单进程方案是否真实遇到容量或故障域问题？
2. 新组件是否解决可测量问题，而不是仅“行业常见”？
3. Desktop Mode 是否仍保持零外部基础设施依赖？
4. 新组件是否能封装在 Port/Adapter 后，不污染 Domain？
5. 能否提供故障、备份、升级和测试方案？

示例：

```text
Redis
→ 只有 Server 多实例 RateLimit/Cache 一致性出现实际需求后引入。

Kafka/NATS
→ 只有 Runtime Job 多节点吞吐/可靠投递确有需要后引入。

Qdrant/Milvus
→ 只有 pgvector/本地 VectorStore 在规模或检索能力上成为瓶颈后引入。

Kubernetes
→ 只有部署规模需要编排、自愈、弹性和多实例治理时引入。
```

---

**文档结束。**
