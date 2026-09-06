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

-- Enterprise AI Hub 初始 Schema（方案 §9 / 附录 B，SQLite 方言）
-- 时间统一 UTC RFC3339 文本；Bool=INTEGER；JSON=TEXT；金额=INTEGER microunits。

CREATE TABLE IF NOT EXISTS providers (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    kind TEXT NOT NULL,
    base_url TEXT NOT NULL,
    credential_ref TEXT,
    proxy_url TEXT,
    timeout_ms INTEGER NOT NULL DEFAULT 120000,
    max_retries INTEGER NOT NULL DEFAULT 2,
    enabled INTEGER NOT NULL DEFAULT 1,
    status TEXT NOT NULL DEFAULT 'active',
    health TEXT NOT NULL DEFAULT 'unknown',
    last_health_check_at TEXT,
    config_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_providers_enabled ON providers(enabled);

CREATE TABLE IF NOT EXISTS models (
    id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL REFERENCES providers(id) ON DELETE CASCADE,
    model_key TEXT NOT NULL,
    display_name TEXT NOT NULL,
    model_type TEXT NOT NULL,
    context_window INTEGER,
    max_output_tokens INTEGER,
    capabilities_json TEXT NOT NULL DEFAULT '{}',
    pricing_json TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1,
    discovered INTEGER NOT NULL DEFAULT 0,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    UNIQUE (provider_id, model_key)
);
CREATE INDEX IF NOT EXISTS idx_models_provider ON models(provider_id);
CREATE INDEX IF NOT EXISTS idx_models_enabled_type ON models(enabled, model_type);

CREATE TABLE IF NOT EXISTS virtual_models (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    routing_strategy TEXT NOT NULL DEFAULT 'priority_failover',
    enabled INTEGER NOT NULL DEFAULT 1,
    config_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS virtual_model_targets (
    id TEXT PRIMARY KEY,
    virtual_model_id TEXT NOT NULL REFERENCES virtual_models(id) ON DELETE CASCADE,
    model_id TEXT NOT NULL REFERENCES models(id),
    priority INTEGER NOT NULL,
    weight INTEGER NOT NULL DEFAULT 100,
    enabled INTEGER NOT NULL DEFAULT 1,
    condition_json TEXT NOT NULL DEFAULT '{}',
    overrides_json TEXT NOT NULL DEFAULT '{}',
    UNIQUE (virtual_model_id, model_id)
);
CREATE INDEX IF NOT EXISTS idx_vmt_virtual_priority
    ON virtual_model_targets(virtual_model_id, enabled, priority);

CREATE TABLE IF NOT EXISTS applications (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    allowed_virtual_models_json TEXT NOT NULL DEFAULT '[]',
    allow_direct_models INTEGER NOT NULL DEFAULT 0,
    monthly_budget_microunits INTEGER,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS api_keys (
    id TEXT PRIMARY KEY,
    application_id TEXT NOT NULL REFERENCES applications(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    prefix TEXT NOT NULL,
    secret_hash TEXT NOT NULL,
    scopes_json TEXT NOT NULL DEFAULT '[]',
    expires_at TEXT,
    last_used_at TEXT,
    revoked_at TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_api_keys_prefix ON api_keys(prefix);
CREATE INDEX IF NOT EXISTS idx_api_keys_application ON api_keys(application_id);

CREATE TABLE IF NOT EXISTS quota_policies (
    id TEXT PRIMARY KEY,
    subject_type TEXT NOT NULL,
    subject_id TEXT NOT NULL,
    rpm INTEGER,
    tpm INTEGER,
    daily_requests INTEGER,
    monthly_tokens INTEGER,
    monthly_cost_microunits INTEGER,
    exceed_action TEXT NOT NULL DEFAULT 'block',
    fallback_virtual_model_id TEXT,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
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
    started_at TEXT NOT NULL,
    completed_at TEXT,
    ttft_ms INTEGER,
    latency_ms INTEGER,
    retry_count INTEGER NOT NULL DEFAULT 0,
    cache_status TEXT,
    error_code TEXT,
    error_message_safe TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_requests_started ON ai_requests(started_at DESC);
CREATE INDEX IF NOT EXISTS idx_requests_app_started ON ai_requests(application_id, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_requests_status ON ai_requests(status, started_at DESC);
CREATE INDEX IF NOT EXISTS idx_requests_provider ON ai_requests(provider_id, started_at DESC);

CREATE TABLE IF NOT EXISTS usage_records (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE REFERENCES ai_requests(id),
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cached_input_tokens INTEGER NOT NULL DEFAULT 0,
    reasoning_tokens INTEGER NOT NULL DEFAULT 0,
    total_tokens INTEGER NOT NULL DEFAULT 0,
    usage_source TEXT NOT NULL DEFAULT 'provider',
    raw_usage_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS cost_records (
    id TEXT PRIMARY KEY,
    request_id TEXT NOT NULL UNIQUE REFERENCES ai_requests(id),
    currency TEXT NOT NULL DEFAULT 'USD',
    input_cost_microunits INTEGER NOT NULL DEFAULT 0,
    output_cost_microunits INTEGER NOT NULL DEFAULT 0,
    cache_cost_microunits INTEGER NOT NULL DEFAULT 0,
    reasoning_cost_microunits INTEGER NOT NULL DEFAULT 0,
    total_cost_microunits INTEGER NOT NULL DEFAULT 0,
    pricing_snapshot_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
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
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_events(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_audit_type ON audit_events(event_type, created_at DESC);

CREATE TABLE IF NOT EXISTS provider_health_samples (
    id TEXT PRIMARY KEY,
    provider_id TEXT NOT NULL,
    model_id TEXT,
    status TEXT NOT NULL,
    latency_ms INTEGER,
    http_status INTEGER,
    error_category TEXT,
    checked_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_health_provider ON provider_health_samples(provider_id, checked_at DESC);
