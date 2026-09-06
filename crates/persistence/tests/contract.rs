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

//! SQLite Repository Contract Test（方案 §31.2）。
//! PostgreSQL Adapter 在 M10 落地时复用同一套断言。

use aihub_domain::cost::Pricing;
use aihub_domain::entities::*;
use aihub_domain::repos::*;
use aihub_domain::{DomainError, DomainErrorCode};
use aihub_persistence::*;
use sqlx::SqlitePool;

async fn pool() -> SqlitePool {
    let dir = std::env::temp_dir().join(format!("aihub-contract-{}", uuid::Uuid::new_v4()));
    open_sqlite(&dir.join("test.db"))
        .await
        .expect("open sqlite")
}

#[tokio::test]
async fn provider_crud_and_unique_key() {
    let pool = pool().await;
    let repo = SqliteProviderRepository::new(pool);

    let created = repo
        .create(NewProvider {
            key: "p1".into(),
            name: "Provider One".into(),
            kind: ProviderKind::OpenAICompatible,
            base_url: "http://localhost:9901/v1".into(),
            proxy_url: None,
            timeout_ms: 30_000,
            max_retries: 2,
            enabled: true,
            status: "active".into(),
            config: serde_json::json!({"sendStreamOptions": true}),
        })
        .await
        .expect("create provider");
    assert_eq!(created.key, "p1");
    assert!(created.credential_ref.is_some());

    // 唯一 key 冲突 → DUPLICATE
    let dup = repo
        .create(NewProvider {
            key: "p1".into(),
            ..same_provider()
        })
        .await;
    assert!(matches!(
        dup,
        Err(DomainError {
            code: DomainErrorCode::Duplicate,
            ..
        })
    ));

    let fetched = repo.get(&created.id).await.unwrap();
    assert_eq!(fetched.config["sendStreamOptions"], serde_json::json!(true));

    let updated = repo
        .update(
            &created.id,
            ProviderUpdate {
                enabled: Some(false),
                status: Some("disabled".into()),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(!updated.enabled);
    assert_eq!(updated.status, "disabled");

    repo.set_health(&created.id, "healthy", chrono::Utc::now())
        .await
        .unwrap();
    assert_eq!(repo.get(&created.id).await.unwrap().health, "healthy");

    repo.delete(&created.id).await.unwrap();
    assert!(repo.get(&created.id).await.is_err());
}

fn same_provider() -> NewProvider {
    NewProvider {
        key: String::new(),
        name: "Provider One".into(),
        kind: ProviderKind::OpenAICompatible,
        base_url: "http://localhost:9901/v1".into(),
        proxy_url: None,
        timeout_ms: 30_000,
        max_retries: 2,
        enabled: true,
        status: "active".into(),
        config: serde_json::json!({}),
    }
}

#[tokio::test]
async fn model_unique_per_provider_and_filters() {
    let pool = pool().await;
    let providers = SqliteProviderRepository::new(pool.clone());
    let models = SqliteModelRepository::new(pool);

    let provider = providers.create(same_provider()).await.unwrap();

    let model = models
        .create(NewModel {
            provider_id: provider.id.clone(),
            model_key: "mock-mini".into(),
            display_name: "Mock Mini".into(),
            model_type: ModelType::Chat,
            context_window: Some(8192),
            max_output_tokens: None,
            capabilities: serde_json::json!({"streaming": true}),
            pricing: Pricing {
                input: Some(1.0),
                output: Some(2.0),
                ..Default::default()
            },
            enabled: true,
            discovered: true,
            metadata: serde_json::json!({}),
        })
        .await
        .unwrap();

    // (provider_id, model_key) 唯一
    let dup = models
        .create(NewModel {
            provider_id: provider.id.clone(),
            model_key: "mock-mini".into(),
            display_name: "Dup".into(),
            model_type: ModelType::Chat,
            context_window: None,
            max_output_tokens: None,
            capabilities: serde_json::json!({}),
            pricing: Pricing::default(),
            enabled: true,
            discovered: false,
            metadata: serde_json::json!({}),
        })
        .await;
    assert!(matches!(
        dup,
        Err(DomainError {
            code: DomainErrorCode::Duplicate,
            ..
        })
    ));

    let listed = models
        .list(&ModelFilter {
            provider_id: Some(provider.id.clone()),
            enabled: Some(true),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(listed.len(), 1);
    assert_eq!(listed[0].pricing.input, Some(1.0));

    let updated = models
        .update(
            &model.id,
            ModelUpdate {
                enabled: Some(false),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    assert!(!updated.enabled);
    assert_eq!(models.count_by_provider(&provider.id).await.unwrap(), 1);
}

#[tokio::test]
async fn virtual_model_targets_replace() {
    let pool = pool().await;
    let providers = SqliteProviderRepository::new(pool.clone());
    let models = SqliteModelRepository::new(pool.clone());
    let vms = SqliteVirtualModelRepository::new(pool);

    let provider = providers.create(same_provider()).await.unwrap();
    let model = models
        .create(NewModel {
            provider_id: provider.id.clone(),
            model_key: "m1".into(),
            display_name: "M1".into(),
            model_type: ModelType::Chat,
            context_window: None,
            max_output_tokens: None,
            capabilities: serde_json::json!({}),
            pricing: Pricing::default(),
            enabled: true,
            discovered: false,
            metadata: serde_json::json!({}),
        })
        .await
        .unwrap();

    let vm = vms
        .create(
            NewVirtualModel {
                key: "general-smart".into(),
                name: "General Smart".into(),
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

    let by_key = vms.get_by_key("general-smart").await.unwrap();
    assert_eq!(by_key.id, vm.id);
    assert_eq!(
        by_key.config["retry"]["maxAttemptsPerTarget"],
        serde_json::json!(2)
    );

    let targets = vms.targets_for(&vm.id).await.unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].priority, 10);

    // 替换 targets：删除旧 + 插入新
    vms.replace_targets(
        &vm.id,
        vec![NewTarget {
            model_id: model.id.clone(),
            priority: 1,
            weight: 50,
            enabled: false,
            condition: serde_json::json!({}),
            overrides: serde_json::json!({}),
        }],
    )
    .await
    .unwrap();
    let targets = vms.targets_for(&vm.id).await.unwrap();
    assert_eq!(targets.len(), 1);
    assert_eq!(targets[0].priority, 1);
    assert!(!targets[0].enabled);
}

#[tokio::test]
async fn application_keys_and_quota() {
    let pool = pool().await;
    let apps = SqliteApplicationRepository::new(pool.clone());
    let keys = SqliteApiKeyRepository::new(pool.clone());
    let quotas = SqliteQuotaRepository::new(pool);

    let app = apps
        .create(NewApplication {
            key: "app-1".into(),
            name: "App One".into(),
            status: "active".into(),
            allowed_virtual_models: vec!["general-smart".into()],
            allow_direct_models: false,
            monthly_budget_microunits: Some(5_000_000),
            metadata: serde_json::json!({}),
        })
        .await
        .unwrap();

    let key = keys
        .create(NewApiKey {
            application_id: app.id.clone(),
            name: "default".into(),
            prefix: "abcd1234".into(),
            secret_hash: "hash".into(),
            scopes: vec![],
            expires_at: None,
        })
        .await
        .unwrap();

    assert!(key.is_active(chrono::Utc::now()));
    assert_eq!(
        keys.get_by_prefix("abcd1234").await.unwrap().unwrap().id,
        key.id
    );
    assert!(keys.get_by_prefix("missing").await.unwrap().is_none());

    keys.revoke(&key.id, chrono::Utc::now()).await.unwrap();
    let revoked = keys.get(&key.id).await.unwrap();
    assert!(!revoked.is_active(chrono::Utc::now()));
    // 已撤销的 key 不再参与前缀查找
    assert!(keys.get_by_prefix("abcd1234").await.unwrap().is_none());

    quotas
        .upsert_for_subject(
            "application",
            &app.id,
            QuotaValues {
                rpm: Some(10),
                monthly_cost_microunits: Some(1_000_000),
                ..Default::default()
            },
        )
        .await
        .unwrap();
    let quota = quotas
        .get_for_subject("application", &app.id)
        .await
        .unwrap()
        .unwrap();
    assert_eq!(quota.rpm, Some(10));
    // upsert 更新
    quotas
        .upsert_for_subject(
            "application",
            &app.id,
            QuotaValues {
                rpm: Some(20),
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
        Some(20)
    );
}

#[tokio::test]
async fn request_usage_cost_lifecycle_and_aggregations() {
    let pool = pool().await;
    let apps = SqliteApplicationRepository::new(pool.clone());
    let requests = SqliteRequestRepository::new(pool.clone());
    let usage_repo = SqliteUsageRepository::new(pool);

    let app = apps
        .create(NewApplication {
            key: "app-agg".into(),
            name: "App".into(),
            status: "active".into(),
            allowed_virtual_models: vec![],
            allow_direct_models: false,
            monthly_budget_microunits: None,
            metadata: serde_json::json!({}),
        })
        .await
        .unwrap();

    let mut request_ids = Vec::new();
    for i in 0..4 {
        let request_id = uuid::Uuid::new_v4().to_string();
        requests
            .create(NewAiRequest {
                id: request_id.clone(),
                trace_id: format!("trace-{i}"),
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
                    ttft_ms: Some(100 + i),
                    latency_ms: Some(1000 + i * 1000),
                    retry_count: 0,
                    error_code: None,
                    error_message_safe: None,
                    resolved_model_id: None,
                    resolved_model_key: Some("mock-pro".into()),
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
                total_cost_microunits: 1000,
                pricing_snapshot: serde_json::json!({}),
                created_at: chrono::Utc::now(),
            })
            .await
            .unwrap();
        request_ids.push(request_id);
    }

    let (page, total) = requests
        .list(&RequestFilter {
            page: 1,
            page_size: 2,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(total, 4);
    assert_eq!(page.len(), 2);

    let summary = usage_repo.summary(&UsageQuery::default()).await.unwrap();
    assert_eq!(summary.requests, 4);
    assert_eq!(summary.total_tokens, 600);
    assert_eq!(summary.cost_microunits, 4000);
    assert_eq!(summary.success_requests, 4);
    assert!(summary.p50_latency_ms.is_some());
    assert!(summary.p95_latency_ms.is_some());

    let by_model = usage_repo.by_model(&UsageQuery::default()).await.unwrap();
    assert_eq!(by_model.len(), 1);
    assert_eq!(by_model[0].0, "mock-pro");

    let by_app = usage_repo
        .by_application(&UsageQuery::default())
        .await
        .unwrap();
    assert_eq!(by_app.len(), 1);

    let monthly = usage_repo
        .monthly_cost_for_application(&app.id, month_start())
        .await
        .unwrap();
    assert_eq!(monthly, 4000);

    // usage/cost 与 request 一对一：重复插入应失败
    let dup = requests.insert_usage(UsageRecord {
        id: uuid::Uuid::new_v4().to_string(),
        request_id: request_ids[0].clone(),
        input_tokens: 0,
        output_tokens: 0,
        cached_input_tokens: 0,
        reasoning_tokens: 0,
        total_tokens: 0,
        usage_source: "provider".into(),
        raw_usage: serde_json::json!({}),
        created_at: chrono::Utc::now(),
    });
    assert!(dup.await.is_err());
}

fn month_start() -> chrono::DateTime<chrono::Utc> {
    use chrono::{Datelike, TimeZone};
    let now = chrono::Utc::now();
    chrono::Utc
        .with_ymd_and_hms(now.year(), now.month(), 1, 0, 0, 0)
        .single()
        .unwrap()
}

#[tokio::test]
async fn audit_insert_and_filter() {
    let pool = pool().await;
    let audit = SqliteAuditRepository::new(pool);

    for i in 0..3 {
        audit
            .insert(AuditEvent {
                id: uuid::Uuid::new_v4().to_string(),
                trace_id: None,
                actor_type: "admin".into(),
                actor_id: None,
                event_type: if i == 0 {
                    "provider.created"
                } else {
                    "provider.updated"
                }
                .into(),
                resource_type: Some("provider".into()),
                resource_id: Some("p1".into()),
                decision: None,
                payload_ref: None,
                metadata: serde_json::json!({"index": i}),
                created_at: chrono::Utc::now(),
            })
            .await
            .unwrap();
    }

    let (events, total) = audit
        .list(&AuditFilter {
            event_type: Some("provider.created".into()),
            page: 1,
            page_size: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(total, 1);
    assert_eq!(events[0].metadata["index"], serde_json::json!(0));

    let (_, total) = audit
        .list(&AuditFilter {
            page: 1,
            page_size: 10,
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(total, 3);
}
