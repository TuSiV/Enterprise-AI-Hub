use aihub_domain::entities::*;
use aihub_domain::error::DomainError;
use aihub_domain::repos::*;
use aihub_domain::DomainResource;
use async_trait::async_trait;
use sqlx::{Row, SqlitePool};

use crate::{db_error, json_string, now_rfc3339, parse_json, parse_ts};

pub struct SqliteVirtualModelRepository {
    pool: SqlitePool,
}

impl SqliteVirtualModelRepository {
    pub fn new(pool: SqlitePool) -> Self {
        Self { pool }
    }
}

fn vm_from_row(row: &sqlx::sqlite::SqliteRow) -> VirtualModel {
    VirtualModel {
        id: row.get("id"),
        key: row.get("key"),
        name: row.get("name"),
        description: row.get("description"),
        routing_strategy: RoutingStrategy::parse(&row.get::<String, _>("routing_strategy"))
            .unwrap_or(RoutingStrategy::PriorityFailover),
        enabled: row.get::<i64, _>("enabled") != 0,
        config: parse_json(Some(row.get("config_json"))),
        created_at: parse_ts(Some(row.get("created_at"))).unwrap_or_default(),
        updated_at: parse_ts(Some(row.get("updated_at"))).unwrap_or_default(),
    }
}

fn target_from_row(row: &sqlx::sqlite::SqliteRow) -> VirtualModelTarget {
    VirtualModelTarget {
        id: row.get("id"),
        virtual_model_id: row.get("virtual_model_id"),
        model_id: row.get("model_id"),
        priority: row.get::<i64, _>("priority") as i32,
        weight: row.get::<i64, _>("weight") as i32,
        enabled: row.get::<i64, _>("enabled") != 0,
        condition: parse_json(Some(row.get("condition_json"))),
        overrides: parse_json(Some(row.get("overrides_json"))),
    }
}

async fn insert_targets(pool: &SqlitePool, vm_id: &str, targets: Vec<NewTarget>) -> Result<(), DomainError> {
    for target in targets {
        sqlx::query(
            "INSERT INTO virtual_model_targets (id, virtual_model_id, model_id, priority, weight, enabled, condition_json, overrides_json)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?)
             ON CONFLICT(virtual_model_id, model_id) DO UPDATE SET priority=excluded.priority, weight=excluded.weight, enabled=excluded.enabled, condition_json=excluded.condition_json, overrides_json=excluded.overrides_json",
        )
        .bind(uuid::Uuid::new_v4().to_string())
        .bind(vm_id)
        .bind(&target.model_id)
        .bind(target.priority as i64)
        .bind(target.weight as i64)
        .bind(target.enabled as i64)
        .bind(json_string(&target.condition))
        .bind(json_string(&target.overrides))
        .execute(pool)
        .await
        .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
    }
    Ok(())
}

#[async_trait]
impl VirtualModelRepository for SqliteVirtualModelRepository {
    async fn create(&self, vm: NewVirtualModel, targets: Vec<NewTarget>) -> Result<VirtualModel, DomainError> {
        let id = uuid::Uuid::new_v4().to_string();
        let now = now_rfc3339();
        sqlx::query(
            "INSERT INTO virtual_models (id, key, name, description, routing_strategy, enabled, config_json, created_at, updated_at)
             VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(&id)
        .bind(&vm.key)
        .bind(&vm.name)
        .bind(&vm.description)
        .bind(vm.routing_strategy.as_str())
        .bind(vm.enabled as i64)
        .bind(json_string(&vm.config))
        .bind(&now)
        .bind(&now)
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        insert_targets(&self.pool, &id, targets).await?;
        self.get(&id).await
    }

    async fn get(&self, id: &str) -> Result<VirtualModel, DomainError> {
        let row = sqlx::query("SELECT * FROM virtual_models WHERE id = ?")
            .bind(id)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::VirtualModel, id))?;
        Ok(vm_from_row(&row))
    }

    async fn get_by_key(&self, key: &str) -> Result<VirtualModel, DomainError> {
        let row = sqlx::query("SELECT * FROM virtual_models WHERE key = ?")
            .bind(key)
            .fetch_optional(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?
            .ok_or_else(|| DomainError::not_found(DomainResource::VirtualModel, key))?;
        Ok(vm_from_row(&row))
    }

    async fn list(&self) -> Result<Vec<VirtualModel>, DomainError> {
        let rows = sqlx::query("SELECT * FROM virtual_models ORDER BY created_at")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        Ok(rows.iter().map(vm_from_row).collect())
    }

    async fn update(&self, id: &str, update: VirtualModelUpdate) -> Result<VirtualModel, DomainError> {
        let existing = self.get(id).await?;
        let name = update.name.unwrap_or(existing.name);
        let description = update.description.unwrap_or(existing.description);
        let routing_strategy = update.routing_strategy.unwrap_or(existing.routing_strategy);
        let enabled = update.enabled.unwrap_or(existing.enabled);
        let config = update.config.unwrap_or(existing.config);
        sqlx::query(
            "UPDATE virtual_models SET name=?, description=?, routing_strategy=?, enabled=?, config_json=?, updated_at=? WHERE id=?",
        )
        .bind(&name)
        .bind(&description)
        .bind(routing_strategy.as_str())
        .bind(enabled as i64)
        .bind(json_string(&config))
        .bind(now_rfc3339())
        .bind(id)
        .execute(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        self.get(id).await
    }

    async fn delete(&self, id: &str) -> Result<(), DomainError> {
        sqlx::query("DELETE FROM virtual_models WHERE id = ?")
            .bind(id)
            .execute(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        Ok(())
    }

    async fn replace_targets(&self, virtual_model_id: &str, targets: Vec<NewTarget>) -> Result<Vec<VirtualModelTarget>, DomainError> {
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        sqlx::query("DELETE FROM virtual_model_targets WHERE virtual_model_id = ?")
            .bind(virtual_model_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        tx.commit()
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        insert_targets(&self.pool, virtual_model_id, targets).await?;
        self.targets_for(virtual_model_id).await
    }

    async fn targets_for(&self, virtual_model_id: &str) -> Result<Vec<VirtualModelTarget>, DomainError> {
        let rows = sqlx::query(
            "SELECT * FROM virtual_model_targets WHERE virtual_model_id = ? ORDER BY priority ASC",
        )
        .bind(virtual_model_id)
        .fetch_all(&self.pool)
        .await
        .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        Ok(rows.iter().map(target_from_row).collect())
    }

    async fn list_all_targets(&self) -> Result<Vec<VirtualModelTarget>, DomainError> {
        let rows = sqlx::query("SELECT * FROM virtual_model_targets ORDER BY priority ASC")
            .fetch_all(&self.pool)
            .await
            .map_err(|e| db_error(DomainResource::VirtualModel, e))?;
        Ok(rows.iter().map(target_from_row).collect())
    }
}
