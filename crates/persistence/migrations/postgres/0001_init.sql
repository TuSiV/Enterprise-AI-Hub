-- Copyright 2026 YONGZHE CHEN
--
-- Licensed under the Apache License, Version 2.0 (the "License");
-- you may not use this file except in compliance with the License.
-- You may obtain a copy of the License at
--
--     http://www.apache.org/licenses/LICENSE-2.0
--
-- Unless required by applicable law or agreed to in writing, software
-- distributed under the License is distributed on an "AS IS" BASIS,
-- WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
-- See the License for the specific language governing permissions and
-- limitations under the License.

-- Enterprise AI Hub 核心 Schema（PostgreSQL 方言，方案 §9 / 附录 B）
-- ID 保持 TEXT（与 Desktop SQLite 一致，避免迁移冲突）；Bool=BOOLEAN；时间=TIMESTAMPTZ；JSON=JSONB。

CREATE TABLE IF NOT EXISTS providers (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    base_url TEXT NOT NULL,
    credential_ref TEXT,
    credential_configured BOOLEAN NOT NULL DEFAULT FALSE,
    proxy_url TEXT,
    timeout_ms BIGINT NOT NULL DEFAULT 120000,
    max_retries INTEGER NOT NULL DEFAULT 2,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    status TEXT NOT NULL DEFAULT 'active',
    health TEXT NOT NULL DEFAULT 'unknown',
    last_health_check_at TIMESTAMPTZ,
    config_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_providers_enabled ON providers(enabled);

CREATE TABLE IF NOT EXISTS models (
    id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    model_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    model_type TEXT NOT NULL,
    context_window BIGINT,
    max_output_tokens BIGINT,
    capabilities_json JSONB NOT NULL DEFAULT '{}',
    pricing_json JSONB NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    discovered BOOLEAN NOT NULL DEFAULT FALSE,
    metadata_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (provider_id, model_key)
);
CREATE INDEX IF NOT EXISTS idx_models_provider ON models(provider_id);

CREATE TABLE IF NOT EXISTS virtual_models (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    routing_strategy TEXT NOT NULL DEFAULT 'priority_failover',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    config_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS virtual_model_targets (
    id TEXT PRIMARY KEY,
    virtual_model_id TEXT NOT NULL REFERENCES virtual_models(id) ON DELETE CASCADE,
    model_id TEXT NOT NULL REFERENCES models(id),
    priority INTEGER NOT NULL,
    weight INTEGER NOT NULL DEFAULT 100,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    condition_json JSONB NOT NULL DEFAULT '{}',
    overrides_json JSONB NOT NULL DEFAULT '{}',
    UNIQUE (virtual_model_id, model_id)
);
CREATE INDEX IF NOT EXISTS idx_vmt_virtual_priority ON virtual_model_targets(virtual_model_id, enabled, priority);

CREATE TABLE IF NOT EXISTS applications (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    allowed_virtual_models_json JSONB NOT NULL DEFAULT '[]',
    allow_direct_models BOOLEAN NOT NULL DEFAULT FALSE,
    monthly_budget_microunits BIGINT,
    metadata_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    prefix TEXT NOT NULL,
    secret_hash TEXT NOT NULL,
    scopes_json JSONB NOT NULL DEFAULT '[]',
    expires_at TIMESTAMPTZ,
    last_used_at TIMESTAMPTZ,
    revoked_at TIMESTAMPTZ,
    created_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(prefix);

CREATE TABLE IF NOT EXISTS quota_policies (
    id TEXT PRIMARY KEY,
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    rpm BIGINT,
    tpm BIGINT,
    daily_requests BIGINT,
    monthly_tokens BIGINT,
    monthly_cost_microunits BIGINT,
    exceed_action TEXT NOT NULL DEFAULT 'block',
    fallback_virtual_model_id TEXT,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL,
    UNIQUE (subject_type, subject_id)
);

CREATE TABLE IF NOT EXISTS ai_requests (
    id TEXT PRIMARY KEY,
    trace_id TEXT NOT NULL,
    application_id TEXT,
    user_id TEXT,
    api_key_id TEXT,
    endpoint TEXT NOT NULL,
    requested_model TEXT NOT NULL,
    resolved_model_id TEXT,
    resolved_model_key TEXT,
    provider_id TEXT,
    status TEXT NOT NULL DEFAULT 'accepted',
    http_status INTEGER,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    ttft_ms BIGINT,
    latency_ms BIGINT,
    retry_count INTEGER NOT NULL DEFAULT 0,
    cache_status TEXT,
    error_code TEXT,
    error_message_safe TEXT,
    metadata_json JSONB NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_requests_started ON ai_requests(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_requests_app ON ai_requests(application_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_requests_status ON ai_requests(status, started_at DESC);

CREATE TABLE IF NOT EXISTS usage_records (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE REFERENCES ai_requests(id),
    input_tokens BIGINT NOT NULL DEFAULT 0,
    output_tokens BIGINT NOT NULL DEFAULT 0,
    cached_input_tokens BIGINT NOT NULL DEFAULT 0,
    reasoning_tokens BIGINT NOT NULL DEFAULT 0,
    total_tokens BIGINT NOT NULL DEFAULT 0,
    usage_source TEXT NOT NULL DEFAULT 'provider',
    raw_usage_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS cost_records (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE REFERENCES ai_requests(id),
    currency TEXT NOT NULL DEFAULT 'USD',
    input_cost_microunits BIGINT NOT NULL DEFAULT 0,
    output_cost_microunits BIGINT NOT NULL DEFAULT 0,
    cache_cost_microunits BIGINT NOT NULL DEFAULT 0,
    reasoning_cost_microunits BIGINT NOT NULL DEFAULT 0,
    total_cost_microunits BIGINT NOT NULL DEFAULT 0,
    pricing_snapshot_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL
);

CREATE TABLE IF NOT EXISTS audit_events (
    id TEXT PRIMARY KEY,
    trace_id TEXT,
    actor_type TEXT NOT NULL,
    actor_id TEXT,
    event_type TEXT NOT NULL,
    resource_type TEXT,
    resource_id TEXT,
    decision TEXT,
    payload_ref TEXT,
    metadata_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_events(created_at DESC);

CREATE TABLE IF NOT EXISTS provider_health_samples (
    id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL,
    model_id TEXT,
    status TEXT NOT NULL,
    latency_ms BIGINT,
    http_status INTEGER,
    error_category TEXT,
    checked_at TIMESTAMPTZ NOT NULL
);

-- M8+ 平台表（0003 对应）
CREATE TABLE IF NOT EXISTS prompts (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    owner_id TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS prompt_versions (
    id TEXT PRIMARY KEY,
    prompt_id TEXT NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    system_template TEXT,
    user_template TEXT,
    variables_schema_json JSONB NOT NULL DEFAULT '{}',
    model_config_json JSONB NOT NULL DEFAULT '{}',
    output_schema_json JSONB,
    created_by TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE (prompt_id, version)
);
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    external_subject TEXT,
    identity_provider TEXT NOT NULL DEFAULT 'local',
    username TEXT,
    email TEXT,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    password_hash TEXT,
    metadata_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_provider_subject ON users(identity_provider, external_subject) WHERE external_subject IS NOT NULL;
CREATE TABLE IF NOT EXISTS roles (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    system_role BOOLEAN NOT NULL DEFAULT FALSE,
    created_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS permissions (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    description TEXT
);
CREATE TABLE IF NOT EXISTS role_permissions (
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    permission_id TEXT NOT NULL REFERENCES permissions(id) ON DELETE CASCADE,
    PRIMARY KEY (role_id, permission_id)
);
CREATE TABLE IF NOT EXISTS user_roles (
    user_id TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    role_id TEXT NOT NULL REFERENCES roles(id) ON DELETE CASCADE,
    scope_type TEXT NOT NULL DEFAULT 'global',
    scope_id TEXT,
    created_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS knowledge_bases (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',
    owner_user_id TEXT,
    owner_department_id TEXT,
    retrieval_config_json JSONB NOT NULL DEFAULT '{}',
    embedding_model_id TEXT,
    rerank_model_id TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY,
    knowledge_base_id TEXT NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes BIGINT NOT NULL,
    file_hash TEXT NOT NULL,
    storage_key TEXT NOT NULL,
    parse_status TEXT NOT NULL DEFAULT 'uploaded',
    index_status TEXT NOT NULL DEFAULT 'pending',
    parser_version TEXT,
    error_message TEXT,
    metadata_json JSONB NOT NULL DEFAULT '{}',
    created_by TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS document_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    page_no INTEGER,
    section_path TEXT,
    token_count INTEGER,
    embedding_json JSONB,
    metadata_json JSONB NOT NULL DEFAULT '{}',
    UNIQUE (document_id, chunk_index)
);
CREATE TABLE IF NOT EXISTS tools (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    kind TEXT NOT NULL,
    input_schema_json JSONB NOT NULL DEFAULT '{}',
    output_schema_json JSONB NOT NULL DEFAULT '{}',
    permission_json JSONB NOT NULL DEFAULT '{}',
    config_json JSONB NOT NULL DEFAULT '{}',
    timeout_ms BIGINT NOT NULL DEFAULT 30000,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS mcp_servers (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    transport TEXT NOT NULL,
    endpoint_or_command TEXT NOT NULL,
    credential_ref TEXT,
    config_json JSONB NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    owner_id TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS agent_versions (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',
    model_ref TEXT NOT NULL,
    prompt_version_id TEXT,
    system_prompt TEXT,
    max_steps INTEGER NOT NULL DEFAULT 8,
    max_tool_calls INTEGER NOT NULL DEFAULT 16,
    timeout_ms BIGINT NOT NULL DEFAULT 120000,
    max_cost_microunits BIGINT,
    allowed_tools_json JSONB NOT NULL DEFAULT '[]',
    knowledge_bindings_json JSONB NOT NULL DEFAULT '[]',
    policy_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL,
    UNIQUE (agent_id, version)
);
CREATE TABLE IF NOT EXISTS agent_runs (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    agent_version_id TEXT NOT NULL,
    trace_id TEXT,
    user_id TEXT,
    application_id TEXT,
    status TEXT NOT NULL,
    current_step INTEGER NOT NULL DEFAULT 0,
    max_steps INTEGER NOT NULL,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    input_ref TEXT,
    output_ref TEXT,
    error_code TEXT,
    error_message TEXT,
    usage_json JSONB NOT NULL DEFAULT '{}',
    cost_microunits BIGINT NOT NULL DEFAULT 0,
    metadata_json JSONB NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS tool_calls (
    id TEXT PRIMARY KEY,
    trace_id TEXT,
    agent_run_id TEXT,
    request_id TEXT,
    tool_id TEXT NOT NULL,
    tool_key TEXT NOT NULL,
    actor_user_id TEXT,
    application_id TEXT,
    status TEXT NOT NULL,
    arguments_json JSONB NOT NULL DEFAULT '{}',
    result_ref TEXT,
    error_code TEXT,
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    latency_ms BIGINT,
    metadata_json JSONB NOT NULL DEFAULT '{}'
);
CREATE TABLE IF NOT EXISTS eval_datasets (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS eval_cases (
    id TEXT PRIMARY KEY,
    dataset_id TEXT NOT NULL REFERENCES eval_datasets(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    input_json JSONB NOT NULL,
    expected_output TEXT,
    reference_context TEXT,
    metadata_json JSONB NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS eval_runs (
    id TEXT PRIMARY KEY,
    dataset_id TEXT NOT NULL REFERENCES eval_datasets(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    candidate_json JSONB NOT NULL,
    judge_config_json JSONB NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'running',
    started_at TIMESTAMPTZ NOT NULL,
    completed_at TIMESTAMPTZ,
    summary_json JSONB NOT NULL DEFAULT '{}',
    error TEXT
);
CREATE TABLE IF NOT EXISTS eval_results (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES eval_runs(id) ON DELETE CASCADE,
    case_id TEXT NOT NULL,
    response_text TEXT,
    latency_ms BIGINT,
    input_tokens BIGINT NOT NULL DEFAULT 0,
    output_tokens BIGINT NOT NULL DEFAULT 0,
    cost_microunits BIGINT NOT NULL DEFAULT 0,
    score_json JSONB NOT NULL DEFAULT '{}',
    judge_json JSONB,
    created_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS routing_policies (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 100,
    match_json JSONB NOT NULL DEFAULT '{}',
    action_json JSONB NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
CREATE TABLE IF NOT EXISTS security_policies (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    policy_type TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 100,
    rule_json JSONB NOT NULL DEFAULT '{}',
    action_json JSONB NOT NULL DEFAULT '{}',
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL,
    updated_at TIMESTAMPTZ NOT NULL
);
