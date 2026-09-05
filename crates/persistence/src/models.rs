use aihub_domain::cost::Pricing;
use aihub_domain::entities::*;
use aihub_domain::error::DomainError;
use aihub_domain::repos::*;
use aihub_domain::DomainResource;
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

use crate::{db_error, json_string, now_rfc3339, parse_json, parse_ts};

pub struct SqliteModelRepository {
    pool: SqlitePool,
}

impl SqliteModelRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn model_from_row(row: &sqlx::sqlite::SqliteRow) -> Model {
    let capabilities = parse_json(Some(row.get("capabilities_json")));
    let pricing_value = parse_json(Some(row.get("pricing_json")));
    Model {
        id: row.get("id"),
        provider_id: row.get("provider_id"),
        model_key: row.get("model_key"),
        display_name: row.get("display_name"),
        model_type: ModelType::parse(&row.get::<String, _>("model_type"))
            .unwrap_or(ModelType::Chat),
        context_window: row.get("context_window"),
        max_output_tokens: row.get("max_output_tokens"),
        capabilities,
        pricing: Pricing::from_json(&pricing_value),
        enabled: row.get::<i64, _>("enabled") != 0,
        discovered: row.get::<i64, _>("discovered") != 0,
        metadata: parse_json(Some(row.get("metadata_json"))),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
        updated_at: parse_ts(Some(row.get("updated_at"))).unwrap_or_default(),
    }
}

fn insert_model_sql() -> &'static str {
    "INSERT INTO models (id, provider_id, model_key, display_name, model_type, context_window, max_output_tokens, capabilities_json, pricing_json, enabled, discovered, metadata_json, created_at, updated_at)
     VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)
     ON CONFLICT(provider_id, model_key) DO NOTHING"
}

async fn insert_model(pool: &SqlitePool, model: &NewModel) -> Result<String, DomainError> {
    let id = uuid::Uuid::new_v4().to_string();
    let now = now_rfc3339();
    let result = sqlx::query(insert_model_sql())
        .bind(&id)
        .bind(&model.provider_id)
        .bind(&model.model_key)
        .bind(&model.display_name)
        .bind(model.model_type.as_str())
        .bind(model.context_window)
        .bind(model.max_output_tokens)
        .bind(json_string(&model.capabilities))
        .bind(json_string(
            &serde_json::to_value(&model.pricing).unwrap_or_default(),
        ))
        .bind(model.enabled as i64)
        .bind(model.discovered as i64)
        .bind(json_string(&model.metadata))
        .bind(&now)
        .bind(&now)
        .execute(pool)
        .await
        .map_err(|e| db_error(DomainResource::Model, e))?;
    if result.rows_affected() == 0 {
        return Err(DomainError::duplicate(
            DomainResource::Model,
            &model.model_key,
        ));
    }
    Ok(id)
}

#[async_trait]
impl ModelRepository for SqliteModelRepository {
    async fn create(&self, model: NewModel) -> Result<Model, DomainError> {
        let id = insert_model(&self.pool, &model).await?;
        self.get(&id).await
    }

    async fn upsert_discovered(&self, model: NewModel) -> Result<Model, DomainError> {
        match insert_model(&self.pool, &model).await {
            Ok(id) => self.get(&id).await,
            Err(_) => {
                // 已存在则刷新发现元数据（保留人工编辑的 display/pricing）
                let existing =
                    sqlx::query("SELECT id FROM models WHERE provider_id = ? AND model_key = ?")
                        .bind(&model.provider_id)
                        .bind(&model.model_key)
                        .fetch_one(&self.pool)
                        .await
                        .map_err(|e| db_error(DomainResource::Model, e))?;
                let id: String = existing.get("id");
                let type_str = model.model_type.as_str();
                sqlx::query(
                    "UPDATE models SET model_type=?, discovered=1, updated_at=? WHERE id=?",
                )
                .bind(type_str)
                .bind(now_rfc3339())
                .bind(&id)
                .execute(&self.pool)
                .await
                .map_err(|e| db_error(DomainResource::Model, e))?;
                self.get(&id).await
            }
        }
    }

    async fn get(&self, id: &str) -> Result<Model, DomainError> {
        let row = sqlx::query("SELECT * FROM models WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Model, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::Model, id))?;
        Ok(model_from_row(&row))
    }

    async fn list(&self, filter: &ModelFilter) -> Result<Vec<Model>, DomainError> {
        let mut clauses: Vec<String> = Vec::new();
        let mut binds: Vec<(String, String)> = Vec::new();
        if let Some(provider_id) = &filter.provider_id {
            clauses.push("provider_id = ?".to_string());
            binds.push((String::new(), provider_id.clone()));
        }
        if let Some(model_type) = &filter.model_type {
            clauses.push("model_type = ?".to_string());
            binds.push((String::new(), model_type.clone()));
        }
        if let Some(enabled) = filter.enabled {
            clauses.push(format!("enabled = {enabled}"));
        }
        let mut sql = "SELECT * FROM models".to_string();
        if !clauses.is_empty() {
            sql.push_str(" WHERE ");
            sql.push_str(&clauses.join(" AND "));
        }
        sql.push_str(" ORDER BY created_at");
        let mut query = sqlx::query(&sql);
        for (_, value) in &binds {
            query = query.bind(value);
        }
        let rows = query
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Model, e))?;
        Ok(rows.iter().map(model_from_row).collect())
    }

    async fn update(&self, id: &str, update: ModelUpdate) -> Result<Model, DomainError> {
        let existing = self.get(id).await?;
        let display_name = update.display_name.unwrap_or(existing.display_name);
        let model_type = update.model_type.unwrap_or(existing.model_type);
        let context_window = update.context_window.unwrap_or(existing.context_window);
        let max_output_tokens = update
            .max_output_tokens
            .unwrap_or(existing.max_output_tokens);
        let capabilities = update.capabilities.unwrap_or(existing.capabilities);
        let pricing = update.pricing.unwrap_or(existing.pricing);
        let enabled = update.enabled.unwrap_or(existing.enabled);
        let metadata = update.metadata.unwrap_or(existing.metadata);
        sqlx::query(
            "UPDATE models SET display_name=?, model_type=?, context_window=?, max_output_tokens=?, capabilities_json=?, pricing_json=?, enabled=?, metadata_json=?, updated_at=? WHERE id=?",
        )
        .bind(&display_name)
        .bind(model_type.as_str())
        .bind(context_window)
        .bind(max_output_tokens)
        .bind(json_string(&capabilities))
        .bind(json_string(&serde_json::to_value(&pricing).unwrap_or_default()))
        .bind(enabled as i64)
        .bind(json_string(&metadata))
        .bind(now_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::Model, e))?;
        self.get(id).await
    }

    async fn delete(&self, id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM models WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Model, e))?;
        Ok(())
    }

    async fn count_by_provider(&self, provider_id: &str) -> Result<i64, DomainError> {
        let row = sqlx::query("SELECT COUNT(*) AS c FROM models WHERE provider_id = ?")
            .bind(provider_id)
            .fetch_one(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Model, e))?;
        Ok(row.get::<i64, _>("c"))
    }

    async fn count_enabled(&self) -> Result<i64, DomainError> {
        let row = sqlx::query("SELECT COUNT(*) AS c FROM models WHERE enabled = 1")
            .fetch_one(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::Model, e))?;
        Ok(row.get::<i64, _>("c"))
    }
}
