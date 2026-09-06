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

//! PostgreSQL Persistence Adapter（M10，方案 §21.1）。
//! 与 SQLite 实现同一组 Repository Port；feature = "postgres" 启用。
//! 本地开发/CI 通过 TEST_DATABASE_URL 提供实例跑同一契约测试。

use aihub_domain::cost::Pricing;
use aihub_domain::entities::*;
use aihub_domain::error::DomainError;
use aihub_domain::repos::*;
use aihub_domain::DomainResource;
use async_trait::async_trait;
use chrono::{DateTime, Utc};
use serde_json::Value;
use sqlx::postgres::{PgConnectOptions, PgPoolOptions};
use sqlx::{PgPool, Row};

type Result<T> = std::result::Result<T, DomainError>;

pub async fn open_postgres(url: &str) -> std::result::Result<PgPool, crate::PersistenceError> {
    let options: PgConnectOptions = url.parse()?;
    let pool = PgPoolOptions::new()
        .max_connections(10)
        .acquire_timeout(std::time::Duration::from_secs(10))
        .connect_with(options)
        .await?;
    sqlx::migrate!("migrations/postgres").run(&pool).await?;
    Ok(pool)
}

pub(crate) fn db_error(resource: DomainResource, err: sqlx::Error) -> DomainError {
    match &err {
        sqlx::Error::RowNotFound => DomainError::not_found(resource, "row"),
        sqlx::Error::Database(db) if db.is_unique_violation() => {
            DomainError::duplicate(resource, "key")
        }
        other => DomainError::internal(resource, format!("db error: {other}")),
    }
}

fn ts(value: DateTime<Utc>) -> DateTime<Utc> {
    value
}

// ================= Provider =================

pub struct PgProviderRepository {
    pool: PgPool,
}

impl PgProviderRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn provider_from_row(row: &sqlx::postgres::PgRow) -> Provider {
    Provider {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        kind: ProviderKind::parse(&row.get::<String, _>("kind"))
            .unwrap_or(ProviderKind::OpenAICompatible),
        base_url: row.get("base_url"),
        credential_ref: row.get("credential_ref"),
        credential_configured: row.get("credential_configured"),
        proxy_url: row.get("proxy_url"),
        timeout_ms: row.get("timeout_ms"),
        max_retries: row.get::<i32, _>("max_retries"),
        enabled: row.get("enabled"),
        status: row.get("status"),
        health: row.get("health"),
        last_health_check_at: row.get("last_health_check_at"),
        config: row.get::<Value, _>("config_json"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[async_trait]
impl ProviderRepository for PgProviderRepository {
    async fn create(&self, p: NewProvider) -> Result<Provider> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let credential_ref = format!("provider/{id}/api_key");
        sqlx::query("INSERT INTO providers (id, key, name, kind, base_url, credential_ref, credential_configured, proxy_url, timeout_ms, max_retries, enabled, status, health, config_json, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,FALSE,$7,$8,$9,$10,$11,'unknown',$12,$13,$14)")
            .bind(&id).bind(&p.key).bind(&p.name).bind(p.kind.as_str())
            .bind(&p.base_url).bind(&credential_ref).bind(&p.proxy_url)
            .bind(p.timeout_ms).bind(p.max_retries).bind(p.enabled)
            .bind(&p.status).bind(&p.config).bind(ts(now)).bind(ts(now))
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::Provider, e))?;
        self.get(&id).await
    }

    async fn get(&self, id: &str) -> Result<Provider> {
        let row = sqlx::query("SELECT * FROM providers WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Provider, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::Provider, id))?;
        Ok(provider_from_row(&row))
    }

    async fn get_by_key(&self, key: &str) -> Result<Provider> {
        let row = sqlx::query("SELECT * FROM providers WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Provider, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::Provider, key))?;
        Ok(provider_from_row(&row))
    }

    async fn list(&self, enabled_only: bool) -> Result<Vec<Provider>> {
        let sql = if enabled_only {
            "SELECT * FROM providers WHERE enabled = TRUE ORDER BY created_at"
        } else {
            "SELECT * FROM providers ORDER BY created_at"
        };
        let rows = sqlx::query(sql)
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Provider, e))?;
        Ok(rows.iter().map(provider_from_row).collect())
    }

    async fn update(&self, id: &str, u: ProviderUpdate) -> Result<Provider> {
        let e = self.get(id).await?;
        sqlx::query("UPDATE providers SET name=$1, base_url=$2, credential_configured=$3, proxy_url=$4, timeout_ms=$5, max_retries=$6, enabled=$7, status=$8, config_json=$9, credential_ref=$10, updated_at=$11 WHERE id=$12")
            .bind(u.name.unwrap_or(e.name))
            .bind(u.base_url.unwrap_or(e.base_url))
            .bind(u.credential_configured.unwrap_or(e.credential_configured))
            .bind(u.proxy_url.unwrap_or(e.proxy_url))
            .bind(u.timeout_ms.unwrap_or(e.timeout_ms))
            .bind(u.max_retries.unwrap_or(e.max_retries))
            .bind(u.enabled.unwrap_or(e.enabled))
            .bind(u.status.unwrap_or(e.status))
            .bind(u.config.unwrap_or(e.config))
            .bind(u.credential_ref.unwrap_or_else(|| e.credential_ref.unwrap_or_else(|| format!("provider/{id}/api_key"))))
            .bind(ts(Utc::now()))
            .bind(id)
            .execute(&self.pool).await.map_err(|err| db_error(DomainResource::Provider, err))?;
        self.get(id).await
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM providers WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Provider, e))?;
        Ok(())
    }

    async fn set_health(&self, id: &str, health: &str, checked_at: DateTime<Utc>) -> Result<()> {
        sqlx::query(
            "UPDATE providers SET health=$1, last_health_check_at=$2, updated_at=$3 WHERE id=$4",
        )
        .bind(health)
        .bind(ts(checked_at))
        .bind(ts(Utc::now()))
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Provider, e))?;
        Ok(())
    }
}

pub struct PgProviderHealthRepository {
    pool: PgPool,
}

impl PgProviderHealthRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

#[async_trait]
impl ProviderHealthRepository for PgProviderHealthRepository {
    async fn insert_sample(&self, s: NewHealthSample) -> Result<()> {
        sqlx::query("INSERT INTO provider_health_samples (id, provider_id, model_id, status, latency_ms, http_status, error_category, checked_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(uuid::Uuid::new_v4().to_string()).bind(&s.provider_id).bind(&s.model_id)
            .bind(&s.status).bind(s.latency_ms).bind(s.http_status)
            .bind(s.error_category.map(|c| c.to_string())).bind(ts(Utc::now()))
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::Provider, e))?;
        Ok(())
    }

    async fn latest_per_provider(&self) -> Result<Vec<ProviderHealthSample>> {
        let rows = sqlx::query(
            "SELECT DISTINCT ON (provider_id) * FROM provider_health_samples ORDER BY provider_id, checked_at DESC",
        ).fetch_all(&self.pool).await.map_err(|e| db_error(DomainResource::Provider, e))?;
        Ok(rows
            .iter()
            .map(|row| ProviderHealthSample {
                id: row.get("id"),
                provider_id: row.get("provider_id"),
                model_id: row.get("model_id"),
                status: row.get("status"),
                latency_ms: row.get("latency_ms"),
                http_status: row.get("http_status"),
                error_category: row.get("error_category"),
                checked_at: row.get("checked_at"),
            })
            .collect())
    }
}

// ================= Model =================

pub struct PgModelRepository {
    pool: PgPool,
}

impl PgModelRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn model_from_row(row: &sqlx::postgres::PgRow) -> Model {
    Model {
        id: row.get("id"),
        provider_id: row.get("provider_id"),
        model_key: row.get("model_key"),
        display_name: row.get("display_name"),
        model_type: ModelType::parse(&row.get::<String, _>("model_type"))
            .unwrap_or(ModelType::Chat),
        context_window: row.get("context_window"),
        max_output_tokens: row.get("max_output_tokens"),
        capabilities: row.get::<Value, _>("capabilities_json"),
        pricing: Pricing::from_json(&row.get::<Value, _>("pricing_json")),
        enabled: row.get("enabled"),
        discovered: row.get("discovered"),
        metadata: row.get::<Value, _>("metadata_json"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[async_trait]
impl ModelRepository for PgModelRepository {
    async fn create(&self, m: NewModel) -> Result<Model> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        let pricing = serde_json::to_value(&m.pricing).unwrap_or_default();
        let inserted = sqlx::query("INSERT INTO models (id, provider_id, model_key, display_name, model_type, context_window, max_output_tokens, capabilities_json, pricing_json, enabled, discovered, metadata_json, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14) ON CONFLICT (provider_id, model_key) DO NOTHING RETURNING id")
            .bind(&id).bind(&m.provider_id).bind(&m.model_key).bind(&m.display_name)
            .bind(m.model_type.as_str()).bind(m.context_window).bind(m.max_output_tokens)
            .bind(&m.capabilities).bind(&pricing).bind(m.enabled).bind(m.discovered)
            .bind(&m.metadata).bind(ts(now)).bind(ts(now))
            .fetch_optional(&self.pool).await.map_err(|e| db_error(DomainResource::Model, e))?;
        if inserted.is_none() {
            return Err(DomainError::duplicate(DomainResource::Model, &m.model_key));
        }
        self.get(&id).await
    }

    async fn upsert_discovered(&self, m: NewModel) -> Result<Model> {
        match self.create(m.clone()).await {
            Ok(model) => Ok(model),
            Err(_) => {
                let row =
                    sqlx::query("SELECT id FROM models WHERE provider_id = $1 AND model_key = $2")
                        .bind(&m.provider_id)
                        .bind(&m.model_key)
                        .fetch_one(&self.pool)
                        .await
                        .map_err(|e| db_error(DomainResource::Model, e))?;
                let id: String = row.get("id");
                sqlx::query(
                    "UPDATE models SET model_type=$1, discovered=TRUE, updated_at=$2 WHERE id=$3",
                )
                .bind(m.model_type.as_str())
                .bind(ts(Utc::now()))
                .bind(&id)
                .execute(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::Model, e))?;
                self.get(&id).await
            }
        }
    }

    async fn get(&self, id: &str) -> Result<Model> {
        let row = sqlx::query("SELECT * FROM models WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Model, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::Model, id))?;
        Ok(model_from_row(&row))
    }

    async fn list(&self, f: &ModelFilter) -> Result<Vec<Model>> {
        let mut clauses: Vec<String> = Vec::new();
        if f.provider_id.is_some() {
            clauses.push("provider_id = $1".into());
        }
        if f.model_type.is_some() {
            clauses.push(format!("model_type = ${}", clauses.len() + 1));
        }
        if let Some(enabled) = f.enabled {
            clauses.push(format!("enabled = ${}", clauses.len() + 1));
            let _ = enabled;
        }
        let mut sql = "SELECT * FROM models".to_string();
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY created_at");
        let mut q = sqlx::query(&sql);
        if let Some(p) = &f.provider_id {
            q = q.bind(p);
        }
        if let Some(t) = &f.model_type {
            q = q.bind(t);
        }
        if f.enabled.is_some() {
            q = q.bind(f.enabled);
        }
        let rows = q
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Model, e))?;
        Ok(rows.iter().map(model_from_row).collect())
    }

    async fn update(&self, id: &str, u: ModelUpdate) -> Result<Model> {
        let e = self.get(id).await?;
        let pricing = match u.pricing {
            Some(p) => serde_json::to_value(&p).unwrap_or_default(),
            None => serde_json::to_value(&e.pricing).unwrap_or_default(),
        };
        sqlx::query("UPDATE models SET display_name=$1, model_type=$2, context_window=$3, max_output_tokens=$4, capabilities_json=$5, pricing_json=$6, enabled=$7, metadata_json=$8, updated_at=$9 WHERE id=$10")
            .bind(u.display_name.unwrap_or(e.display_name))
            .bind(u.model_type.unwrap_or(e.model_type).as_str())
            .bind(u.context_window.unwrap_or(e.context_window))
            .bind(u.max_output_tokens.unwrap_or(e.max_output_tokens))
            .bind(u.capabilities.unwrap_or(e.capabilities))
            .bind(&pricing)
            .bind(u.enabled.unwrap_or(e.enabled))
            .bind(u.metadata.unwrap_or(e.metadata))
            .bind(ts(Utc::now()))
            .bind(id)
            .execute(&self.pool).await.map_err(|err| db_error(DomainResource::Model, err))?;
        self.get(id).await
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM models WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Model, e))?;
        Ok(())
    }

    async fn count_by_provider(&self, provider_id: &str) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) AS c FROM models WHERE provider_id = $1")
            .bind(provider_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Model, e))?;
        Ok(row.get::<i64, _>("c"))
    }

    async fn count_enabled(&self) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) AS c FROM models WHERE enabled = TRUE")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Model, e))?;
        Ok(row.get::<i64, _>("c"))
    }
}

// ================= Virtual Model =================

pub struct PgVirtualModelRepository {
    pool: PgPool,
}

impl PgVirtualModelRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn vm_from_row(row: &sqlx::postgres::PgRow) -> VirtualModel {
    VirtualModel {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        description: row.get("description"),
        routing_strategy: RoutingStrategy::parse(&row.get::<String, _>("routing_strategy"))
            .unwrap_or(RoutingStrategy::PriorityFailover),
        enabled: row.get("enabled"),
        config: row.get::<Value, _>("config_json"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

fn target_from_row(row: &sqlx::postgres::PgRow) -> VirtualModelTarget {
    VirtualModelTarget {
        id: row.get("id"),
        virtual_model_id: row.get("virtual_model_id"),
        model_id: row.get("model_id"),
        priority: row.get("priority"),
        weight: row.get("weight"),
        enabled: row.get("enabled"),
        condition: row.get::<Value, _>("condition_json"),
        overrides: row.get::<Value, _>("overrides_json"),
    }
}

#[async_trait]
impl VirtualModelRepository for PgVirtualModelRepository {
    async fn create(&self, vm: NewVirtualModel, targets: Vec<NewTarget>) -> Result<VirtualModel> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        sqlx::query("INSERT INTO virtual_models (id, key, name, description, routing_strategy, enabled, config_json, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9)")
            .bind(&id).bind(&vm.key).bind(&vm.name).bind(&vm.description)
            .bind(vm.routing_strategy.as_str()).bind(vm.enabled).bind(&vm.config)
            .bind(ts(now)).bind(ts(now))
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        self.replace_targets(&id, targets).await?;
        self.get(&id).await
    }

    async fn get(&self, id: &str) -> Result<VirtualModel> {
        let row = sqlx::query("SELECT * FROM virtual_models WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::VirtualModel, id))?;
        Ok(vm_from_row(&row))
    }

    async fn get_by_key(&self, key: &str) -> Result<VirtualModel> {
        let row = sqlx::query("SELECT * FROM virtual_models WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::VirtualModel, key))?;
        Ok(vm_from_row(&row))
    }

    async fn list(&self) -> Result<Vec<VirtualModel>> {
        let rows = sqlx::query("SELECT * FROM virtual_models ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        Ok(rows.iter().map(vm_from_row).collect())
    }

    async fn update(&self, id: &str, u: VirtualModelUpdate) -> Result<VirtualModel> {
        let e = self.get(id).await?;
        sqlx::query("UPDATE virtual_models SET name=$1, description=$2, routing_strategy=$3, enabled=$4, config_json=$5, updated_at=$6 WHERE id=$7")
            .bind(u.name.unwrap_or(e.name))
            .bind(u.description.unwrap_or(e.description))
            .bind(u.routing_strategy.unwrap_or(e.routing_strategy).as_str())
            .bind(u.enabled.unwrap_or(e.enabled))
            .bind(u.config.unwrap_or(e.config))
            .bind(ts(Utc::now()))
            .bind(id)
            .execute(&self.pool).await.map_err(|err| db_error(DomainResource::VirtualModel, err))?;
        self.get(id).await
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM virtual_models WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        Ok(())
    }

    async fn replace_targets(
        &self,
        vm_id: &str,
        targets: Vec<NewTarget>,
    ) -> Result<Vec<VirtualModelTarget>> {
        sqlx::query("DELETE FROM virtual_model_targets WHERE virtual_model_id = $1")
            .bind(vm_id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        for t in targets {
            sqlx::query("INSERT INTO virtual_model_targets (id, virtual_model_id, model_id, priority, weight, enabled, condition_json, overrides_json) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT (virtual_model_id, model_id) DO UPDATE SET priority=EXCLUDED.priority, weight=EXCLUDED.weight, enabled=EXCLUDED.enabled")
                .bind(uuid::Uuid::new_v4().to_string()).bind(vm_id).bind(&t.model_id)
                .bind(t.priority).bind(t.weight).bind(t.enabled)
                .bind(&t.condition).bind(&t.overrides)
                .execute(&self.pool).await.map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        }
        self.targets_for(vm_id).await
    }

    async fn targets_for(&self, vm_id: &str) -> Result<Vec<VirtualModelTarget>> {
        let rows = sqlx::query(
            "SELECT * FROM virtual_model_targets WHERE virtual_model_id = $1 ORDER BY priority",
        )
        .bind(vm_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        Ok(rows.iter().map(target_from_row).collect())
    }

    async fn list_all_targets(&self) -> Result<Vec<VirtualModelTarget>> {
        let rows = sqlx::query("SELECT * FROM virtual_model_targets ORDER BY priority")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        Ok(rows.iter().map(target_from_row).collect())
    }
}

// ================= Application / ApiKey / Quota =================

pub struct PgApplicationRepository {
    pool: PgPool,
}

impl PgApplicationRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn app_from_row(row: &sqlx::postgres::PgRow) -> Application {
    Application {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        status: row.get("status"),
        allowed_virtual_models: row
            .get::<Value, _>("allowed_virtual_models_json")
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        allow_direct_models: row.get("allow_direct_models"),
        monthly_budget_microunits: row.get("monthly_budget_microunits"),
        metadata: row.get::<Value, _>("metadata_json"),
        created_at: row.get("created_at"),
        updated_at: row.get("updated_at"),
    }
}

#[async_trait]
impl ApplicationRepository for PgApplicationRepository {
    async fn create(&self, a: NewApplication) -> Result<Application> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = Utc::now();
        sqlx::query("INSERT INTO applications (id, key, name, status, allowed_virtual_models_json, allow_direct_models, monthly_budget_microunits, metadata_json, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)")
            .bind(&id).bind(&a.key).bind(&a.name).bind(&a.status)
            .bind(serde_json::to_value(&a.allowed_virtual_models).unwrap_or_default())
            .bind(a.allow_direct_models).bind(a.monthly_budget_microunits)
            .bind(&a.metadata).bind(ts(now)).bind(ts(now))
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::Application, e))?;
        self.get(&id).await
    }

    async fn get(&self, id: &str) -> Result<Application> {
        let row = sqlx::query("SELECT * FROM applications WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Application, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::Application, id))?;
        Ok(app_from_row(&row))
    }

    async fn get_by_key(&self, key: &str) -> Result<Application> {
        let row = sqlx::query("SELECT * FROM applications WHERE key = $1")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Application, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::Application, key))?;
        Ok(app_from_row(&row))
    }

    async fn list(&self) -> Result<Vec<Application>> {
        let rows = sqlx::query("SELECT * FROM applications ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Application, e))?;
        Ok(rows.iter().map(app_from_row).collect())
    }

    async fn update(&self, id: &str, u: ApplicationUpdate) -> Result<Application> {
        let e = self.get(id).await?;
        sqlx::query("UPDATE applications SET name=$1, status=$2, allowed_virtual_models_json=$3, allow_direct_models=$4, monthly_budget_microunits=$5, metadata_json=$6, updated_at=$7 WHERE id=$8")
            .bind(u.name.unwrap_or(e.name))
            .bind(u.status.unwrap_or(e.status))
            .bind(serde_json::to_value(u.allowed_virtual_models.unwrap_or(e.allowed_virtual_models)).unwrap_or_default())
            .bind(u.allow_direct_models.unwrap_or(e.allow_direct_models))
            .bind(u.monthly_budget_microunits.unwrap_or(e.monthly_budget_microunits))
            .bind(u.metadata.unwrap_or(e.metadata))
            .bind(ts(Utc::now()))
            .bind(id)
            .execute(&self.pool).await.map_err(|err| db_error(DomainResource::Application, err))?;
        self.get(id).await
    }

    async fn delete(&self, id: &str) -> Result<()> {
        sqlx::query("DELETE FROM applications WHERE id = $1")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Application, e))?;
        Ok(())
    }
}

pub struct PgApiKeyRepository {
    pool: PgPool,
}

impl PgApiKeyRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn key_from_row(row: &sqlx::postgres::PgRow) -> ApiKey {
    ApiKey {
        id: row.get("id"),
        application_id: row.get("application_id"),
        name: row.get("name"),
        prefix: row.get("prefix"),
        secret_hash: row.get("secret_hash"),
        scopes: row
            .get::<Value, _>("scopes_json")
            .as_array()
            .map(|a| {
                a.iter()
                    .filter_map(|v| v.as_str().map(String::from))
                    .collect()
            })
            .unwrap_or_default(),
        expires_at: row.get("expires_at"),
        last_used_at: row.get("last_used_at"),
        revoked_at: row.get("revoked_at"),
        created_at: row.get("created_at"),
    }
}

#[async_trait]
impl ApiKeyRepository for PgApiKeyRepository {
    async fn create(&self, k: NewApiKey) -> Result<ApiKey> {
        let id = uuid::Uuid::new_v4().to_string();
        sqlx::query("INSERT INTO api_keys (id, application_id, name, prefix, secret_hash, scopes_json, expires_at, created_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(&id).bind(&k.application_id).bind(&k.name).bind(&k.prefix)
            .bind(&k.secret_hash)
            .bind(serde_json::to_value(&k.scopes).unwrap_or_default())
            .bind(k.expires_at).bind(ts(Utc::now()))
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::ApiKey, e))?;
        self.get(&id).await
    }

    async fn get(&self, id: &str) -> Result<ApiKey> {
        let row = sqlx::query("SELECT * FROM api_keys WHERE id = $1")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::ApiKey, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::ApiKey, id))?;
        Ok(key_from_row(&row))
    }

    async fn get_by_prefix(&self, prefix: &str) -> Result<Option<ApiKey>> {
        Ok(
            sqlx::query("SELECT * FROM api_keys WHERE prefix = $1 AND revoked_at IS NULL")
                .bind(prefix)
                .fetch_optional(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::ApiKey, e))?
                .map(|r| key_from_row(&r)),
        )
    }

    async fn list_by_application(&self, application_id: &str) -> Result<Vec<ApiKey>> {
        let rows = sqlx::query(
            "SELECT * FROM api_keys WHERE application_id = $1 ORDER BY created_at DESC",
        )
        .bind(application_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::ApiKey, e))?;
        Ok(rows.iter().map(key_from_row).collect())
    }

    async fn revoke(&self, id: &str, at: DateTime<Utc>) -> Result<()> {
        sqlx::query("UPDATE api_keys SET revoked_at = $1 WHERE id = $2")
            .bind(ts(at))
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::ApiKey, e))?;
        Ok(())
    }

    async fn touch_last_used(&self, id: &str, at: DateTime<Utc>) -> Result<()> {
        sqlx::query("UPDATE api_keys SET last_used_at = $1 WHERE id = $2")
            .bind(ts(at))
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::ApiKey, e))?;
        Ok(())
    }

    async fn count_by_application(&self, application_id: &str) -> Result<i64> {
        let row = sqlx::query("SELECT COUNT(*) AS c FROM api_keys WHERE application_id = $1")
            .bind(application_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::ApiKey, e))?;
        Ok(row.get::<i64, _>("c"))
    }
}

pub struct PgQuotaRepository {
    pool: PgPool,
}

impl PgQuotaRepository {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }
}

fn quota_from_row(row: &sqlx::postgres::PgRow) -> QuotaPolicy {
    QuotaPolicy {
        id: row.get("id"),
        subject_type: row.get("subject_type"),
        subject_id: row.get("subject_id"),
        rpm: row.get("rpm"),
        tpm: row.get("tpm"),
        daily_requests: row.get("daily_requests"),
        monthly_tokens: row.get("monthly_tokens"),
        monthly_cost_microunits: row.get("monthly_cost_microunits"),
        exceed_action: row.get("exceed_action"),
        fallback_virtual_model_id: row.get("fallback_virtual_model_id"),
        enabled: row.get("enabled"),
    }
}

#[async_trait]
impl QuotaRepository for PgQuotaRepository {
    async fn upsert_for_subject(
        &self,
        subject_type: &str,
        subject_id: &str,
        v: QuotaValues,
    ) -> Result<QuotaPolicy> {
        sqlx::query("INSERT INTO quota_policies (id, subject_type, subject_id, rpm, tpm, daily_requests, monthly_tokens, monthly_cost_microunits, exceed_action, enabled, created_at, updated_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,TRUE,$10,$10) ON CONFLICT (subject_type, subject_id) DO UPDATE SET rpm=EXCLUDED.rpm, tpm=EXCLUDED.tpm, daily_requests=EXCLUDED.daily_requests, monthly_tokens=EXCLUDED.monthly_tokens, monthly_cost_microunits=EXCLUDED.monthly_cost_microunits, exceed_action=EXCLUDED.exceed_action, updated_at=EXCLUDED.updated_at")
            .bind(uuid::Uuid::new_v4().to_string()).bind(subject_type).bind(subject_id)
            .bind(v.rpm).bind(v.tpm).bind(v.daily_requests)
            .bind(v.monthly_tokens).bind(v.monthly_cost_microunits)
            .bind(&v.exceed_action).bind(ts(Utc::now()))
            .execute(&self.pool).await.map_err(|e| db_error(DomainResource::Application, e))?;
        self.get_for_subject(subject_type, subject_id)
            .await?
            .ok_or_else(|| {
                DomainError::internal(DomainResource::Application, "quota policy vanished")
            })
    }

    async fn get_for_subject(
        &self,
        subject_type: &str,
        subject_id: &str,
    ) -> Result<Option<QuotaPolicy>> {
        Ok(sqlx::query("SELECT * FROM quota_policies WHERE subject_type = $1 AND subject_id = $2 AND enabled = TRUE")
            .bind(subject_type).bind(subject_id)
            .fetch_optional(&self.pool).await.map_err(|e| db_error(DomainResource::Application, e))?
            .map(|r| quota_from_row(&r)))
    }
}
