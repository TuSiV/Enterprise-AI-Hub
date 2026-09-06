-- M8/M10/M12/M14-M17 扩展 Schema：Prompt / IAM / Knowledge / Agent+Tool+MCP / Eval / 治理策略
-- 方案 §9.13 / §9.17-9.19 / 附录 A.2-A.7

-- ---------- M8 Prompt Center（§9.13 / §39.3） ----------
CREATE TABLE IF NOT EXISTS prompts (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    owner_id TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS prompt_versions (
    id TEXT PRIMARY KEY,
    prompt_id TEXT NOT NULL REFERENCES prompts(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',   -- draft / testing / published / deprecated
    system_template TEXT,
    user_template TEXT,
    variables_schema_json TEXT NOT NULL DEFAULT '{}',
    model_config_json TEXT NOT NULL DEFAULT '{}',
    output_schema_json TEXT,
    created_by TEXT,
    created_at TEXT NOT NULL,
    UNIQUE (prompt_id, version)
);
CREATE INDEX IF NOT EXISTS idx_prompt_versions_prompt ON prompt_versions(prompt_id, version DESC);

-- ---------- M10 IAM（附录 A.1/A.2） ----------
CREATE TABLE IF NOT EXISTS users (
    id TEXT PRIMARY KEY,
    external_subject TEXT,
    identity_provider TEXT NOT NULL DEFAULT 'local',
    username TEXT,
    email TEXT,
    display_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'active',
    password_hash TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_provider_subject
    ON users(identity_provider, external_subject) WHERE external_subject IS NOT NULL;
CREATE UNIQUE INDEX IF NOT EXISTS idx_users_username ON users(username) WHERE username IS NOT NULL;

CREATE TABLE IF NOT EXISTS roles (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    system_role INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL
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
    scope_type TEXT NOT NULL DEFAULT 'global',   -- global / department / application / knowledge_base
    scope_id TEXT,
    created_at TEXT NOT NULL
);

-- ---------- M12 Knowledge（§9.14-9.16 / §19 / §39.2） ----------
CREATE TABLE IF NOT EXISTS knowledge_bases (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    visibility TEXT NOT NULL DEFAULT 'private',  -- private / department / public
    owner_user_id TEXT,
    owner_department_id TEXT,
    retrieval_config_json TEXT NOT NULL DEFAULT '{}',
    embedding_model_id TEXT,
    rerank_model_id TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS documents (
    id TEXT PRIMARY KEY,
    knowledge_base_id TEXT NOT NULL REFERENCES knowledge_bases(id) ON DELETE CASCADE,
    filename TEXT NOT NULL,
    mime_type TEXT NOT NULL,
    size_bytes INTEGER NOT NULL,
    file_hash TEXT NOT NULL,
    storage_key TEXT NOT NULL,
    parse_status TEXT NOT NULL DEFAULT 'uploaded',  -- uploaded/parsing/parse_failed/chunking/embedding/ready/index_failed
    index_status TEXT NOT NULL DEFAULT 'pending',
    parser_version TEXT,
    error_message TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_by TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_documents_kb ON documents(knowledge_base_id);
CREATE INDEX IF NOT EXISTS idx_documents_hash ON documents(file_hash);

CREATE TABLE IF NOT EXISTS document_chunks (
    id TEXT PRIMARY KEY,
    document_id TEXT NOT NULL REFERENCES documents(id) ON DELETE CASCADE,
    chunk_index INTEGER NOT NULL,
    content TEXT NOT NULL,
    content_hash TEXT NOT NULL,
    page_no INTEGER,
    section_path TEXT,
    token_count INTEGER,
    embedding_json TEXT,          -- V1 本地向量：JSON float array；pgvector 在 M10 Server 模式替换
    metadata_json TEXT NOT NULL DEFAULT '{}',
    UNIQUE (document_id, chunk_index)
);

-- ---------- M14/M15 Agent / Tool / MCP（§9.17/§9.18 / 附录 A.5/A.6） ----------
CREATE TABLE IF NOT EXISTS tools (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    kind TEXT NOT NULL,                     -- builtin / http / mcp
    input_schema_json TEXT NOT NULL DEFAULT '{}',
    output_schema_json TEXT NOT NULL DEFAULT '{}',
    permission_json TEXT NOT NULL DEFAULT '{}',
    config_json TEXT NOT NULL DEFAULT '{}',
    timeout_ms INTEGER NOT NULL DEFAULT 30000,
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS mcp_servers (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    transport TEXT NOT NULL,                -- stdio / streamable-http
    endpoint_or_command TEXT NOT NULL,
    credential_ref TEXT,
    config_json TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agents (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    owner_id TEXT,
    status TEXT NOT NULL DEFAULT 'active',
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS agent_versions (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL REFERENCES agents(id) ON DELETE CASCADE,
    version INTEGER NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft',   -- draft / published / deprecated
    model_ref TEXT NOT NULL,                -- virtual/physical model key
    prompt_version_id TEXT,
    system_prompt TEXT,
    max_steps INTEGER NOT NULL DEFAULT 8,
    max_tool_calls INTEGER NOT NULL DEFAULT 16,
    timeout_ms INTEGER NOT NULL DEFAULT 120000,
    max_cost_microunits INTEGER,
    allowed_tools_json TEXT NOT NULL DEFAULT '[]',   -- 空=全部 enabled tools
    knowledge_bindings_json TEXT NOT NULL DEFAULT '[]',
    policy_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL,
    UNIQUE (agent_id, version)
);

CREATE TABLE IF NOT EXISTS agent_runs (
    id TEXT PRIMARY KEY,
    agent_id TEXT NOT NULL,
    agent_version_id TEXT NOT NULL,
    trace_id TEXT,
    user_id TEXT,
    application_id TEXT,
    status TEXT NOT NULL,                   -- queued/running/waiting_tool/completed/failed/cancelled/timeout
    current_step INTEGER NOT NULL DEFAULT 0,
    max_steps INTEGER NOT NULL,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    input_ref TEXT,
    output_ref TEXT,
    error_code TEXT,
    error_message TEXT,
    usage_json TEXT NOT NULL DEFAULT '{}',
    cost_microunits INTEGER NOT NULL DEFAULT 0,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_agent_runs_agent ON agent_runs(agent_id, started_at DESC);

CREATE TABLE IF NOT EXISTS tool_calls (
    id TEXT PRIMARY KEY,
    trace_id TEXT,
    agent_run_id TEXT,
    request_id TEXT,
    tool_id TEXT NOT NULL,
    tool_key TEXT NOT NULL,
    actor_user_id TEXT,
    application_id TEXT,
    status TEXT NOT NULL,                   -- running/succeeded/failed/denied/timeout
    arguments_json TEXT NOT NULL DEFAULT '{}',
    result_ref TEXT,
    error_code TEXT,
    started_at TEXT NOT NULL,
    completed_at TEXT,
    latency_ms INTEGER,
    metadata_json TEXT NOT NULL DEFAULT '{}'
);
CREATE INDEX IF NOT EXISTS idx_tool_calls_run ON tool_calls(agent_run_id);

-- ---------- M16 Evaluation（§9.19） ----------
CREATE TABLE IF NOT EXISTS eval_datasets (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    description TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS eval_cases (
    id TEXT PRIMARY KEY,
    dataset_id TEXT NOT NULL REFERENCES eval_datasets(id) ON DELETE CASCADE,
    name TEXT NOT NULL,
    input_json TEXT NOT NULL,               -- {messages | system+user}
    expected_output TEXT,
    reference_context TEXT,
    metadata_json TEXT NOT NULL DEFAULT '{}',
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS eval_runs (
    id TEXT PRIMARY KEY,
    dataset_id TEXT NOT NULL REFERENCES eval_datasets(id) ON DELETE CASCADE,
    label TEXT NOT NULL,
    candidate_json TEXT NOT NULL,           -- {model, promptVersionId, system}
    judge_config_json TEXT NOT NULL DEFAULT '{}',
    status TEXT NOT NULL DEFAULT 'running', -- running/completed/failed
    started_at TEXT NOT NULL,
    completed_at TEXT,
    summary_json TEXT NOT NULL DEFAULT '{}',
    error TEXT
);

CREATE TABLE IF NOT EXISTS eval_results (
    id TEXT PRIMARY KEY,
    run_id TEXT NOT NULL REFERENCES eval_runs(id) ON DELETE CASCADE,
    case_id TEXT NOT NULL,
    response_text TEXT,
    latency_ms INTEGER,
    input_tokens INTEGER NOT NULL DEFAULT 0,
    output_tokens INTEGER NOT NULL DEFAULT 0,
    cost_microunits INTEGER NOT NULL DEFAULT 0,
    score_json TEXT NOT NULL DEFAULT '{}',
    judge_json TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_eval_results_run ON eval_results(run_id);

-- ---------- M5/M17 治理策略（§16.4 / 附录 A.3/A.4 / §29） ----------
CREATE TABLE IF NOT EXISTS routing_policies (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    priority INTEGER NOT NULL DEFAULT 100,
    match_json TEXT NOT NULL DEFAULT '{}',
    action_json TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS security_policies (
    id TEXT PRIMARY KEY,
    key TEXT NOT NULL UNIQUE,
    name TEXT NOT NULL,
    policy_type TEXT NOT NULL,              -- data_classification / dlp / provider_access / ssrf
    priority INTEGER NOT NULL DEFAULT 100,
    rule_json TEXT NOT NULL DEFAULT '{}',
    action_json TEXT NOT NULL DEFAULT '{}',
    enabled INTEGER NOT NULL DEFAULT 1,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
