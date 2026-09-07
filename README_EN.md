<div align="center">

# Enterprise AI Hub

### Unified Enterprise AI Infrastructure · AI Control Plane

**Every model, one gateway · Every app, one entry point · Every route, governed · Every token, metered · Every action, audited**

English · [简体中文](README.md)

[![CI](https://github.com/TuSiV/ai-hub/actions/workflows/ci.yml/badge.svg)](https://github.com/TuSiV/ai-hub/actions/workflows/ci.yml)
[![License](https://img.shields.io/badge/license-Apache--2.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-1.94-DEA584?logo=rust)](https://www.rust-lang.org)
[![Tests](https://img.shields.io/badge/tests-62%20passing-16a34a)](#testing--quality-gates)
[![PostgreSQL](https://img.shields.io/badge/PostgreSQL-16-336791?logo=postgresql)](https://www.postgresql.org)
[![Desktop](https://img.shields.io/badge/Desktop-Tauri%202-FFC131)](https://tauri.app)

*One product across Desktop / Server / Connected Desktop — the Rust Core is the product; Tauri is just a desktop host.*

</div>

---

## Why

Enterprises don't need "yet another chatbot". They face fragmented model access, costs that can't be attributed, routing that can't be governed, and AI behavior that can't be audited.

**Enterprise AI Hub treats AI as unified infrastructure** — any OpenAI SDK app changes one line of `base_url` and gets:

| Capability | What you get |
|:---:|---|
| 🔌 **Unified Access** | Provider native protocols (OpenAI / Anthropic / Gemini / Ollama…) + connection testing + automatic model discovery |
| 🧭 **Smart Routing** | Virtual Model abstraction (swap backends, zero client changes), priority failover, circuit breakers, retries |
| 🔑 **Unified Invocation** | Application / API Key lifecycle, model allow-lists, RPM / daily / monthly-cost quotas |
| 📊 **Unified Metering** | Token & cost (integer microunits + pricing snapshots), P95/TTFT, per-model / per-app / per-user rollups |
| 👤 **User Attribution** | End-user `user_id` full-chain tracking (OpenAI `user` field / `X-AiHub-User` header), per-user usage & cost aggregation |
| 🛡️ **Unified Governance** | Full audit trail, RBAC, data-classification → routing matrix, DLP redaction, SSRF guard |
| 📚 **Knowledge** | KB upload → parse (PDF/DOCX) → chunk → embed → hybrid retrieval → cited answers |
| 🤖 **Agents / MCP** | Versioned agent loops, tool allow-lists with full audit, MCP over stdio / HTTP |
| 🧪 **Evaluation** | Datasets → multi-candidate runs → rule scoring + LLM Judge → cost/latency regression |

<details>
<summary><b>Design boundaries — what we deliberately don't do (anti-pattern constraints)</b></summary>

- ❌ Chat-first (a chat UI is a client, not the platform core)
- ❌ Provider if/else inside the Gateway (providers go through Adapters)
- ❌ Desktop requiring Docker / PostgreSQL / Redis
- ❌ Agent permissions via prompts (enforced by Core-level permission intersection)
- ❌ Retrieve-then-filter RAG (authorization must be part of recall)
- ❌ Microservices upfront (modular monolith + a Runtime boundary)

</details>

---

## When you need it

<table>
<tr>
<td width="33%" valign="top">

### 🏢 Multiple teams on shared AI
Engineering, support, and operations each bring their own models — costs, quotas, and behavior scattered everywhere with no unified dashboard for finance or security.

</td>
<td width="33%" valign="top">

### 🔀 Multi-model routing governance
You want the best model per task but also vendor-flexibility, cost control, and availability — failover, circuit breaking, scenario-based routing.

</td>
<td width="33%" valign="top">

### 🛡️ Security & compliance
Finance, healthcare, and government scenarios need full audit, classification-based routing, DLP redaction, SSRF protection — enforced by the system, not conventions.

</td>
</tr>
<tr>
<td width="33%" valign="top">

### 📚 Enterprise knowledge Q&A
Turn internal documents (PDF/DOCX…) into cited, traceable answers instead of pasting them into prompts.

</td>
<td width="33%" valign="top">

### 🤖 Agents & automation
Build versioned, auditable agent workflows where tool permissions are enforced by the system — not by prompt wording.

</td>
<td width="33%" valign="top">

### 🧪 Model evaluation & selection
Benchmark new models on shared datasets with cost and latency regression, instead of switching on gut feeling.

</td>
</tr>
</table>

---

## Comparison

|  | Direct vendor APIs | Home-grown gateway script | **Enterprise AI Hub** |
|---|:---:|:---:|:---:|
| Unified multi-model access | ❌ integrate one by one | ⚠️ you maintain it | ✅ Provider Adapters built in |
| Smart routing / failover / circuit breaking | ❌ | ⚠️ build it yourself | ✅ Virtual Model + retry + breakers |
| Unified cost & usage metering | ❌ | ⚠️ build it yourself | ✅ tokens / cost / P95 / TTFT, full dimensions |
| Built-in provider & pricing presets | ❌ | ❌ | ✅ 14 built-in providers + 40+ model prices (OpenAI / Anthropic / Gemini / DeepSeek / GLM / MiniMax…), one-click fill in console |
| Full audit + RBAC + DLP | ❌ | ⚠️ build it yourself | ✅ built in |
| Knowledge RAG (with citations) | ❌ | ❌ | ✅ parse / embed / hybrid retrieval |
| Enforced agent permission isolation | — | ⚠️ by convention | ✅ enforced intersection in Core, not prompts |
| Deployment shapes | — | varies | ✅ Desktop / Server / Connected |
| Desktop external dependencies | — | varies | ✅ zero Docker / PostgreSQL / Redis |

> Positioning: not another chat client, but the **unified infrastructure layer** for model access, routing, metering, and audit — business systems only change one line of `base_url`.

---

## Architecture

```
        ┌─────────────┐   ┌──────────────┐   ┌──────────────────┐
        │ Tauri Desktop│   │  Web Console │   │ OpenAI SDK clients│
        └──────┬──────┘   └──────┬───────┘   └────────┬─────────┘
               │    Tauri IPC    │  Admin API + Web   │ /v1/*
               └────────┬────────┴────────────────────┘
                        ▼
        ┌───────────────────────────────────────────┐
        │              Rust Core (16 crates)         │
        │                                            │
        │  auth → quota → resolve → route → retry/   │
        │  breaker → Usage/Cost → user_id → audit    │
        ├────────────┬─────────────┬─────────────────┤
        │ Gateway    │ Application │ Provider Adapter │
        │ /v1/* + SSE│ RBAC + Jobs │ OpenAI (core)    │
        │            │             │ Anthropic (native)│
        │            │             │ Gemini (native)   │
        │            │             │ Ollama / others   │
        ├────────────┴──────┬──────┴─────────────────┤
        ▼                   ▼                         ▼
  SQLite / PostgreSQL   Secret Store         Python Runtime Sidecar
  (dual-dialect contract tests) Keychain/env/mem parse · PDF/DOCX · handshake/self-heal
```

---

## Quick Start

### 🖥️ Desktop

```bash
# 0. Build (one-time)
cd web && npm install && npm run build && cd ..
cargo build

# 1. Launch the desktop app (banner prints the Admin Token)
cargo run -p aihub-desktop
```

> Re-running opens the already-running instance (single-instance semantics); if port 8787 is busy it automatically falls back to the next port.

> First-time setup: when adding a provider, pick a built-in provider preset (OpenAI / Anthropic / Gemini / DeepSeek / GLM and 14 in total — base URL auto-filled, just enter the API key); when registering a model, one-click fill pricing from the preset library (40+ mainstream models).

<details>
<summary><b>Headless (browser only)</b></summary>

```bash
AIHUB_SECRET_BACKEND=memory ./target/debug/aihub-server --mode desktop --print-admin-token
# Open http://127.0.0.1:8787 and sign in with the token from stderr
```

</details>

<details>
<summary><b>60-second end-to-end demo (mock upstream, no real API key)</b></summary>

```bash
./target/debug/aihub-mock-openai &      # local mock upstream :9901
./scripts/smoke.sh                       # provider → model → routing → call → metering, verified
```

</details>

### 🌐 Server deployment

**Docker Compose (recommended, includes PostgreSQL)**

```bash
docker compose up -d --build
# Console http://<host>:8787 · Token: docker exec <container> cat /data/admin_token
```

**Bare metal + systemd**

```bash
sudo tee /etc/aihub.env <<'EOF'
AIHUB_MODE=server
AIHUB_DATA_DIR=/var/lib/aihub
AIHUB_DATABASE_URL=postgres://aihub:password@127.0.0.1:5432/aihub
AIHUB_SECRET_BACKEND=env
AIHUB_ADMIN_TOKEN=<strong random string>
EOF
sudo systemctl enable --now aihub   # migrations run automatically
```

<details>
<summary><b>systemd unit template</b></summary>

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

**Verify**

```bash
curl http://127.0.0.1:8787/health/live    # → ok
curl http://127.0.0.1:8787/health/ready   # → ready (DB up)
```

### 🔌 Integrate business systems (OpenAI SDK compatible)

```python
from openai import OpenAI

client = OpenAI(
    base_url="http://127.0.0.1:8787/v1",
    api_key="aih_live_<application key>",   # Console → Applications & Keys → create
)

client.chat.completions.create(
    model="general-smart",                   # Virtual Model: swap backends freely
    messages=[{"role": "user", "content": "hello"}],
)
```

<details>
<summary><b>Equivalent cURL (streaming)</b></summary>

```bash
curl http://127.0.0.1:8787/v1/chat/completions \
  -H "Authorization: Bearer aih_live_..." \
  -H "Content-Type: application/json" \
  -d '{"model":"general-smart","messages":[{"role":"user","content":"hello"}],"stream":true}'
```

Every call carries `X-AIH-Request-ID` / `X-AIH-Resolved-Model` / `X-AIH-Provider` response headers,
with full Trace / Usage / Cost visible on the console "Requests" page.

</details>

---

## Testing & quality gates

```bash
cargo fmt --check                                       # formatting
cargo clippy --workspace --all-targets -- -D warnings   # zero warnings
cargo test                                              # 62 tests: unit/contract/e2e
TEST_DATABASE_URL=... cargo test -p aihub-persistence \
    --test pg_contract -- --ignored                     # PostgreSQL contract (verified on real PG 18)
cargo test -p aihub-application --features s3-test \
    --test s3_storage                                   # MinIO integration (verified on real S3)
cd web && npm run build                                 # TS type check + build
./scripts/smoke.sh                                      # process-level e2e smoke
```

Coverage matrix: **Provider contract (11 cases)** · gateway integration (auth / failover / streaming /
rate limit / key revocation) · platform e2e (Prompt / KB / Agent / Eval / DLP / OIDC verification) ·
RBAC multi-user & anti-forgery · concurrency & cancellation storm · SQLite/PostgreSQL dual-dialect contracts.

---

## Configuration

Priority: **CLI > env vars > config.toml > defaults**

| Env var | Description | Default |
|---|---|:---|
| `AIHUB_MODE` | `desktop` / `server` | `desktop` |
| `AIHUB_DATA_DIR` | Data dir (DB, token, documents) | macOS: `~/Library/Application Support/Enterprise AI Hub` |
| `AIHUB_GATEWAY_PORT` | Gateway port (desktop auto-fallback) | `8787` |
| `AIHUB_ADMIN_TOKEN` | Fixed admin token | generated on first start into data dir |
| `AIHUB_SECRET_BACKEND` | `keyring` / `memory` / `env` | `keyring` |
| `AIHUB_DATABASE_URL` | SQLite path or PostgreSQL URL | `<data>/aihub.db` |
| `AIHUB_RUNTIME_ENABLED` | Enable the Python runtime sidecar | `false` |
| `AIHUB_OIDC_JWKS_URL` / `_ISSUER` / `_AUDIENCE` | all three required to enable OIDC login | disabled |
| `AIHUB_LOG_LEVEL` | tracing log level | `info` |

CLI: `aihub-server --config <toml> --mode <desktop|server> --port <n> --print-admin-token`

---

## Project layout

```
├── crates/                     # Rust Core (domain has zero reverse dependencies)
│   ├── domain                  # entities / canonical protocol / repository ports / cost engine / pricing presets
│   ├── application             # services + execution pipeline + RBAC + RAG + Agent + Eval
│   ├── gateway                 # /v1 protocol adapter + SSE
│   ├── persistence             # SQLite/PostgreSQL adapters + migrations
│   ├── provider-openai-compatible  # OpenAI compatible core adapter
│   ├── provider-anthropic          # Anthropic Messages API native adapter
│   ├── provider-gemini             # Gemini generateContent native adapter
│   ├── provider-openai-responses   # OpenAI Responses API adapter
│   ├── secrets / config / …    # infrastructure ports
│   └── runtime-client          # sidecar supervision & protocol
├── apps/
│   ├── server                  # aihub-server: Admin API + Gateway + web hosting
│   ├── desktop                 # Tauri 2 desktop shell (single instance + signed updates)
│   └── mock-openai             # local mock upstream (fault injection)
├── web/                        # React + TS console + Playwright E2E
├── runtime/                    # Python sidecar (parse handshake protocol)
├── migrations/                 # SQLite / PostgreSQL dual dialect
└── scripts/smoke.sh            # end-to-end smoke
```

---

## License

[Apache-2.0](LICENSE)

### Security configuration and upgrade notes

Admin routes enforce the existing system-role permissions. `end_user` has no management permissions. Password and OIDC login endpoints do not require an Admin Token. MCP server configuration requires `security.manage` because stdio commands execute with the server's OS privileges. Keep the full-access Admin Token restricted to administrators.

New passwords use Argon2id; legacy SHA-256 records upgrade on successful login without a schema migration. User sessions expire after 8 hours and can be revoked through `POST /api/v1/auth/logout`. Restarting still invalidates all sessions. Login endpoints share a per-process limit of 60 requests per minute; public deployments should also enforce per-client limits at the reverse proxy.

Browser tokens now live only in page memory. Old localStorage tokens are removed on upgrade, so refreshing requires login again. Desktop automatic login remains available. CSP covers both the HTTP console and the desktop configuration.

Cross-origin browser access is disabled by default. Set `AIHUB_CORS_ALLOWED_ORIGINS` on the target server to comma-separated exact origins, for example `http://localhost:5173,https://console.example.com`. Wildcards, paths and trailing slashes are not accepted. Same-origin consoles need no setting.

Enable `auth.trusted_header_user` only behind a trusted proxy that removes incoming `X-AIH-User-ID` and injects the authenticated identity. Restrict direct backend access. Newly provisioned users receive `end_user` and still require explicit authorization.

Before running Docker Compose, set `POSTGRES_PASSWORD` and `AIHUB_DATABASE_URL`, for example `postgres://postgres:<URL-encoded-password>@postgres:5432/aihub`. Credentials must match. An untracked `.env` may supply these settings; never commit real credentials. Compose explicitly selects PostgreSQL, and the image provides a readiness health check.

Restore accepts only regular `aihub.db` and `manifest.json` files, rejecting links, unexpected or duplicate entries. Limits are 8 GiB for the database and 64 KiB for the manifest. The database is published only after integrity validation, without overwriting an existing destination.
