-- M11 Runtime Jobs（附录 A.7）：文档解析/Embedding 等 Job 模型（§21.6 Tokio task + job table）
CREATE TABLE IF NOT EXISTS runtime_jobs (
    id TEXT PRIMARY KEY,
    job_type TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'pending',   -- pending / running / succeeded / failed
    resource_type TEXT,
    resource_id TEXT,
    payload_json TEXT NOT NULL DEFAULT '{}',
    attempt INTEGER NOT NULL DEFAULT 0,
    max_attempts INTEGER NOT NULL DEFAULT 3,
    available_at TEXT NOT NULL,
    started_at TEXT,
    completed_at TEXT,
    last_error TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_runtime_jobs_due ON runtime_jobs(status, available_at);
