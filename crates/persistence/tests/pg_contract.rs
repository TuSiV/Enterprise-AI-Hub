// Copyright 2026 YONGZHE CHEN
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

//! PostgreSQL Repository Contract Test（M1/M10 验收：同一测试集在 SQLite/Postgres 通过）。
//! 默认 #[ignore]：需要 TEST_DATABASE_URL（如 postgres://postgres:postgres@localhost/aihub_test）。
//! CI 中由 postgres service 提供；本地 `docker run postgres` 后运行：
//!   TEST_DATABASE_URL=... cargo test -p aihub-persistence --test pg_contract -- --ignored --test-threads=1

#![cfg(feature = "postgres")]

use aihub_domain::cost::Pricing;
use aihub_domain::entities::*;
use aihub_domain::platform::NewUser;
use aihub_domain::platform::UserRepository;
use aihub_domain::prompt::PromptRepository;
use aihub_domain::repos::*;
use aihub_domain::DomainError;
use aihub_domain::DomainErrorCode;

async fn pool() -> sqlx::PgPool {
    let url = std::env::var("TEST_DATABASE_URL")
        .expect("TEST_DATABASE_URL must be set for pg contract tests");
    let pool = aihub_persistence::open_postgres(&url)
        .await
        .expect("open postgres");
    // 清理既有数据，保证幂等
    for table in [
        "agent_runs",
        "tool_calls",
        "eval_results",
        "eval_runs",
        "eval_cases",
        "eval_datasets",
        "document_chunks",
        "documents",
        "knowledge_bases",
        "agent_versions",
        "agents",
        "mcp_servers",
        "tools",
        "user_roles",
        "role_permissions",
        "permissions",
        "roles",
        "users",
        "prompt_versions",
        "prompts",
        "security_policies",
        "routing_policies",
        "usage_records",
        "cost_records",
        "ai_requests",
        "api_keys",
        "quota_policies",
        "virtual_model_targets",
        "virtual_models",
        "models",
        "provider_health_samples",
        "providers",
        "applications",
    ] {
        sqlx::query(&format!("DELETE FROM {table}"))
            .execute(&pool)
            .await
            .expect("cleanup");
    }
    pool
}

#[tokio::test]
#[ignore]
async fn pg_provider_model_vm_app_key_quota() {
    let pool = pool().await;
    let providers = aihub_persistence::pg_core::PgProviderRepository::new(pool.clone());
    let models = aihub_persistence::pg_core::PgModelRepository::new(pool.clone());
    let vms = aihub_persistence::pg_core::PgVirtualModelRepository::new(pool.clone());
    let apps = aihub_persistence::pg_core::PgApplicationRepository::new(pool.clone());
    let keys = aihub_persistence::pg_core::PgApiKeyRepository::new(pool.clone());
    let quotas = aihub_persistence::pg_core::PgQuotaRepository::new(pool.clone());

    let provider = providers
        .create(NewProvider {
            key: "pg-p1".into(),
            name: "PG Provider".into(),
            kind: ProviderKind::OpenAICompatible,
            base_url: "http://localhost:9901/v1".into(),
            proxy_url: None,
            timeout_ms: 30_000,
            max_retries: 2,
            enabled: true,
            status: "active".into(),
            config: serde_json::json!({"a": 1}),
        })
        .await
        .unwrap();
    assert_eq!(
        providers.get(&provider.id).await.unwrap().config["a"],
        serde_json::json!(1)
    );

    let dup = providers
        .create(NewProvider {
            key: "pg-p1".into(),
            name: "dup".into(),
            kind: ProviderKind::OpenAICompatible,
            base_url: "x".into(),
            proxy_url: None,
            timeout_ms: 1,
            max_retries: 0,
            enabled: true,
            status: "active".into(),
            config: serde_json::json!({}),
        })
        .await;
    assert!(matches!(
        dup,
        Err(DomainError {
            code: DomainErrorCode::Duplicate,
            ..
        })
    ));

    let model = models
        .create(NewModel {
            provider_id: provider.id.clone(),
            model_key: "m1".into(),
            display_name: "M1".into(),
            model_type: ModelType::Chat,
            context_window: Some(8192),
            max_output_tokens: None,
            capabilities: serde_json::json!({"streaming": true}),
            pricing: Pricing {
                input: Some(1.5),
                output: Some(2.0),
                ..Default::default()
            },
            enabled: true,
            discovered: true,
            metadata: serde_json::json!({}),
        })
        .await
        .unwrap();
    assert_eq!(
        models.get(&model.id).await.unwrap().pricing.input,
        Some(1.5)
    );

    let vm = vms
        .create(
            NewVirtualModel {
                key: "pg-smart".into(),
                name: "PG Smart".into(),
                description: None,
                routing_strategy: RoutingStrategy::PriorityFailover,
                enabled: true,
                config: serde_json::json!({"retry": {"maxAttemptsPerTarget": 2}}),
            },
            vec![NewTarget {
                model_id: model.id.clone(),
                priority: 10,
                weight: 100,
                enabled: true,
                condition: serde_json::json!({}),
                overrides: serde_json::json!({}),
            }],
        )
        .await
        .unwrap();
    assert_eq!(vms.targets_for(&vm.id).await.unwrap().len(), 1);

    let app = apps
        .create(NewApplication {
            key: "pg-app".into(),
            name: "PG App".into(),
            status: "active".into(),
            allowed_virtual_models: vec!["pg-smart".into()],
            allow_direct_models: false,
            monthly_budget_microunits: Some(1_000_000),
            metadata: serde_json::json!({}),
        })
        .await
        .unwrap();
    let key = keys
        .create(NewApiKey {
            application_id: app.id.clone(),
            name: "default".into(),
            prefix: "pgprefix1".into(),
            secret_hash: "hash".into(),
            scopes: vec![],
            expires_at: None,
        })
        .await
        .unwrap();
    assert!(key.is_active(chrono::Utc::now()));
    keys.revoke(&key.id, chrono::Utc::now()).await.unwrap();
    assert!(keys.get_by_prefix("pgprefix1").await.unwrap().is_none());

    quotas
        .upsert_for_subject(
            "application",
            &app.id,
            QuotaValues {
                rpm: Some(30),
                monthly_cost_microunits: Some(2_000_000),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert_eq!(
        quotas
            .get_for_subject("application", &app.id)
            .await
            .unwrap()
            .unwrap()
            .rpm,
        Some(30)
    );
}

#[tokio::test]
#[ignore]
async fn pg_request_usage_cost_aggregation() {
    let pool = pool().await;
    let apps = aihub_persistence::pg_core::PgApplicationRepository::new(pool.clone());
    let requests = aihub_persistence::pg_platform::PgRequestRepository::new(pool.clone());
    let usage_repo = aihub_persistence::pg_platform::PgUsageRepository::new(pool.clone());

    let app = apps
        .create(NewApplication {
            key: "pg-agg".into(),
            name: "Agg".into(),
            status: "active".into(),
            allowed_virtual_models: vec![],
            allow_direct_models: false,
            monthly_budget_microunits: None,
            metadata: serde_json::json!({}),
        })
        .await
        .unwrap();

    for i in 0..3 {
        let request_id = uuid::Uuid::new_v4().to_string();
        requests
            .create(NewAiRequest {
                id: request_id.clone(),
                trace_id: format!("t{i}"),
                application_id: Some(app.id.clone()),
                user_id: None,
                api_key_id: None,
                endpoint: "/v1/chat/completions".into(),
                requested_model: "general-smart".into(),
                metadata: serde_json::json!({}),
                started_at: chrono::Utc::now(),
            })
            .await
            .unwrap();
        requests
            .finish(
                &request_id,
                RequestFinish {
                    status: RequestStatus::Completed,
                    http_status: Some(200),
                    completed_at: chrono::Utc::now(),
                    ttft_ms: Some(50),
                    latency_ms: Some(1000 + i * 1000),
                    retry_count: 0,
                    error_code: None,
                    error_message_safe: None,
                    resolved_model_id: None,
                    resolved_model_key: Some("m1".into()),
                    provider_id: None,
                },
            )
            .await
            .unwrap();
        requests
            .insert_usage(UsageRecord {
                id: uuid::Uuid::new_v4().to_string(),
                request_id: request_id.clone(),
                input_tokens: 100,
                output_tokens: 50,
                cached_input_tokens: 0,
                reasoning_tokens: 0,
                total_tokens: 150,
                usage_source: "provider".into(),
                raw_usage: serde_json::json!({}),
                created_at: chrono::Utc::now(),
            })
            .await
            .unwrap();
        requests
            .insert_cost(CostRecord {
                id: uuid::Uuid::new_v4().to_string(),
                request_id: request_id.clone(),
                currency: "USD".into(),
                input_cost_microunits: 0,
                output_cost_microunits: 0,
                cache_cost_microunits: 0,
                reasoning_cost_microunits: 0,
                total_cost_microunits: 500,
                pricing_snapshot: serde_json::json!({}),
                created_at: chrono::Utc::now(),
            })
            .await
            .unwrap();
    }

    let (page, total) = requests
        .list(&RequestFilter {
            application_id: Some(app.id.clone()),
            page: 1,
            page_size: 2,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(total, 3);
    assert_eq!(page.len(), 2);

    let summary = usage_repo.summary(&UsageQuery::default()).await.unwrap();
    assert_eq!(summary.requests, 3);
    assert_eq!(summary.total_tokens, 450);
    assert_eq!(summary.cost_microunits, 1500);
    assert!(summary.p95_latency_ms.is_some());

    // usage 一对一：重复插入冲突
    let dup = requests
        .insert_usage(UsageRecord {
            id: uuid::Uuid::new_v4().to_string(),
            request_id: "missing-request".into(),
            input_tokens: 0,
            output_tokens: 0,
            cached_input_tokens: 0,
            reasoning_tokens: 0,
            total_tokens: 0,
            usage_source: "provider".into(),
            raw_usage: serde_json::json!({}),
            created_at: chrono::Utc::now(),
        })
        .await;
    assert!(dup.is_err());
}

#[tokio::test]
#[ignore]
async fn pg_prompt_and_user() {
    let pool = pool().await;
    let prompts = aihub_persistence::pg_platform::PgPromptRepository::new(pool.clone());
    let users = aihub_persistence::pg_platform::PgUserRepository::new(pool.clone());

    let prompt = prompts
        .create(aihub_domain::prompt::NewPrompt {
            key: "pg-legal".into(),
            name: "Legal".into(),
            description: None,
            owner_id: None,
        })
        .await
        .unwrap();
    let v1 = prompts
        .create_version(aihub_domain::prompt::NewPromptVersion {
            prompt_id: prompt.id.clone(),
            system_template: Some("sys".into()),
            user_template: Some("user {{x}}".into()),
            variables_schema: serde_json::json!({}),
            model_config: serde_json::json!({}),
            output_schema: None,
            created_by: None,
        })
        .await
        .unwrap();
    assert_eq!(v1.version, 1);
    let published = prompts
        .set_version_status(&v1.id, aihub_domain::prompt::PromptVersionStatus::Published)
        .await
        .unwrap();
    assert_eq!(
        published.status,
        aihub_domain::prompt::PromptVersionStatus::Published
    );
    assert!(prompts
        .published_version(&prompt.id)
        .await
        .unwrap()
        .is_some());

    let user = users
        .create(NewUser {
            identity_provider: "local".into(),
            external_subject: None,
            username: Some("pg-alice".into()),
            email: None,
            display_name: "Alice".into(),
            password_hash: Some("s$h".into()),
        })
        .await
        .unwrap();
    users
        .ensure_role("pg_dev", "Dev", "dev", &["usage.read"])
        .await
        .unwrap();
    users.assign_role(&user.id, "pg_dev").await.unwrap();
    assert_eq!(users.roles_of(&user.id).await.unwrap().len(), 1);
    assert!(users.get_by_username("pg-alice").await.unwrap().is_some());
}
