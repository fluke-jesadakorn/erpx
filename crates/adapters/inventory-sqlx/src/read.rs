use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_inventory::{
    PortError, ReadProjection, ReadRepository, ReservationId, ReservationReadRepository,
    ReservationStatus, ReservationView, StockItemId, StockItemList, StockItemListFilter,
    StockItemView, WarehouseId,
};
use erpx_kernel::TenantScope;
use sqlx::Row;
use uuid::Uuid;

use crate::{PROJECTION_NAME, stock_item_partition};

const CURSOR_SEPARATOR: char = '|';

#[derive(Debug, Clone)]
pub struct InventoryReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl InventoryReadRepository {
    pub fn new(pool: ReadPool) -> Result<Self, CqrsSqlxError> {
        Self::with_waiter_config(pool, WaiterConfig::default())
    }
    pub fn with_waiter_config(pool: ReadPool, config: WaiterConfig) -> Result<Self, CqrsSqlxError> {
        Ok(Self {
            read_side: ReadSideStore::new(pool.clone()),
            waiter: SqlxProjectionWaiter::new(pool.clone(), config)?,
            pool,
        })
    }
    async fn checkpoint(
        &self,
        consistency: &QueryConsistency,
        exact: Option<&str>,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => match exact {
                Some(partition) => self
                    .read_side
                    .checkpoint(PROJECTION_NAME, partition)
                    .await
                    .map_err(|_| PortError::Unavailable),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                let valid = requirement
                    .partition
                    .strip_prefix("inventory.stock_item:")
                    .is_some_and(|value| {
                        value.len() == 32 && value.bytes().all(|b| b.is_ascii_hexdigit())
                    });
                if requirement.projection != PROJECTION_NAME
                    || !valid
                    || exact.is_some_and(|partition| partition != requirement.partition)
                {
                    return Err(PortError::NotReady(ProjectionNotReady {
                        projection: requirement.projection.clone(),
                        partition: requirement.partition.clone(),
                        target: requirement.target,
                        observed: None,
                    }));
                }
                self.waiter
                    .wait_until_checked(requirement)
                    .await
                    .map(Some)
                    .map_err(|error| match error {
                        WaitError::NotReady(value) => PortError::NotReady(value),
                        WaitError::Database(_) => PortError::Unavailable,
                    })
            }
        }
    }
    async fn metadata(
        &self,
        checkpoint: Option<ProjectionCheckpoint>,
    ) -> Result<ProjectionMetadata, PortError> {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE projection_name=$1 AND status IN ('active','building')
             ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(PROJECTION_NAME)
        .fetch_optional(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let freshness = match status.as_deref() {
            Some("active") => FreshnessState::Current,
            Some("building") => FreshnessState::Rebuilding,
            _ => FreshnessState::Unavailable,
        };
        let lag_millis = checkpoint.as_ref().and_then(|value| {
            Utc::now()
                .signed_duration_since(value.updated_at)
                .to_std()
                .ok()
                .and_then(|duration| u64::try_from(duration.as_millis()).ok())
        });
        Ok(ProjectionMetadata {
            checkpoint,
            freshness,
            lag_millis,
        })
    }
}

#[async_trait]
impl ReadRepository for InventoryReadRepository {
    async fn get(
        &self,
        authority_scope: &TenantScope,
        stock_item_id: StockItemId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<StockItemView>>, PortError> {
        let partition = stock_item_partition(stock_item_id);
        let checkpoint = self.checkpoint(consistency, Some(&partition)).await?;
        let row = sqlx::query(
            "SELECT projection.state FROM erpx_inventory_read.stock_item_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1 AND projection.stock_item_id=$2
               AND projection.partition_key=$3",
        )
        .bind(PROJECTION_NAME)
        .bind(stock_item_id.as_uuid())
        .bind(&partition)
        .fetch_optional(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let view = row.map(decode_view).transpose()?;
        if view
            .as_ref()
            .is_some_and(|value| !authority_scope.contains(&value.scope))
        {
            return Err(PortError::Denied);
        }
        Ok(ReadProjection {
            value: view,
            metadata: self.metadata(checkpoint).await?,
        })
    }

    async fn list(
        &self,
        authority_scope: &TenantScope,
        filter: &StockItemListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<StockItemList>, PortError> {
        let checkpoint = self.checkpoint(consistency, None).await?;
        let cursor = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let cursor_at = cursor.as_ref().map(|value| value.0);
        let cursor_id = cursor.as_ref().map(|value| value.1.as_uuid());
        let scan_limit = i64::from(filter.page_size)
            .saturating_mul(8)
            .saturating_add(8);
        let rows = sqlx::query(
            "SELECT projection.state FROM erpx_inventory_read.stock_item_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1
               AND ($2::uuid IS NULL OR projection.warehouse_id=$2)
               AND ($3::text IS NULL OR projection.item_code=$3)
               AND ($4::timestamptz IS NULL OR
                    (projection.aggregate_updated_at,projection.stock_item_id)<($4,$5))
             ORDER BY projection.aggregate_updated_at DESC,projection.stock_item_id DESC LIMIT $6",
        )
        .bind(PROJECTION_NAME)
        .bind(filter.warehouse_id.map(|value| value.as_uuid()))
        .bind(filter.item_code.as_deref())
        .bind(cursor_at)
        .bind(cursor_id)
        .bind(scan_limit)
        .fetch_all(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let mut items = rows
            .into_iter()
            .map(decode_view)
            .collect::<Result<Vec<_>, _>>()?;
        items.retain(|value| authority_scope.contains(&value.scope));
        let page_size = usize::from(filter.page_size);
        let next_cursor = if items.len() > page_size {
            items.truncate(page_size);
            items.last().map(encode_cursor)
        } else {
            None
        };
        Ok(ReadProjection {
            value: StockItemList { items, next_cursor },
            metadata: self.metadata(checkpoint).await?,
        })
    }
}

#[async_trait]
impl ReservationReadRepository for InventoryReadRepository {
    async fn get_reservation(
        &self,
        authority_scope: &TenantScope,
        reservation_id: ReservationId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<ReservationView>>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let checkpoint = self.checkpoint(consistency, None).await?;
        let required_partition = match consistency {
            QueryConsistency::Eventual => None,
            QueryConsistency::ReadYourWrites(requirement) => Some(requirement.partition.as_str()),
        };
        let mut tx = self
            .pool
            .begin()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let row = sqlx::query(
            "SELECT projection.stock_item_id,projection.source_aggregate_version,
                    projection.tenant_scope,projection.warehouse_id,projection.item_code,
                    projection.quantity,projection.demand_reference,
                    projection.reservation_status,projection.projected_at
             FROM erpx_inventory_read.reservation_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.tenant_id=projection.tenant_id
              AND generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.tenant_id=erpx_cqrs_read.current_tenant_id()
               AND projection.projection_name=$1 AND projection.reservation_id=$2
               AND ($3::text IS NULL OR projection.partition_key=$3)",
        )
        .bind(PROJECTION_NAME)
        .bind(reservation_id.as_uuid())
        .bind(required_partition)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|_| PortError::Unavailable)?;
        tx.commit().await.map_err(|_| PortError::Unavailable)?;
        let value = row
            .map(|row| decode_reservation(row, reservation_id))
            .transpose()?;
        if value
            .as_ref()
            .is_some_and(|reservation| !authority_scope.contains(&reservation.scope))
        {
            return Err(PortError::Denied);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(checkpoint).await?,
        })
    }
}

fn decode_reservation(
    row: sqlx::postgres::PgRow,
    reservation_id: ReservationId,
) -> Result<ReservationView, PortError> {
    let status = match row.get::<String, _>("reservation_status").as_str() {
        "active" => ReservationStatus::Active,
        "released" => ReservationStatus::Released,
        _ => return Err(PortError::Internal),
    };
    let scope: TenantScope =
        serde_json::from_value(row.get("tenant_scope")).map_err(|_| PortError::Internal)?;
    Ok(ReservationView {
        reservation_id,
        stock_item_id: StockItemId::try_from_uuid(row.get("stock_item_id"))
            .map_err(|_| PortError::Internal)?,
        source_item_version: u64::try_from(row.get::<i64, _>("source_aggregate_version"))
            .map_err(|_| PortError::Internal)?,
        scope,
        warehouse_id: WarehouseId::try_from_uuid(row.get("warehouse_id"))
            .map_err(|_| PortError::Internal)?,
        item_code: row.get("item_code"),
        quantity: row.get("quantity"),
        demand_reference: row.get("demand_reference"),
        status,
        updated_at: row.get("projected_at"),
    })
}

fn decode_view(row: sqlx::postgres::PgRow) -> Result<StockItemView, PortError> {
    let value: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    serde_json::from_value(value).map_err(|_| PortError::Internal)
}
pub fn encode_cursor(value: &StockItemView) -> String {
    format!(
        "{}{}{}",
        value.updated_at.to_rfc3339(),
        CURSOR_SEPARATOR,
        value.stock_item_id.as_uuid()
    )
}
pub fn decode_cursor(value: &str) -> Result<(DateTime<Utc>, StockItemId), PortError> {
    let (timestamp, id) = value
        .split_once(CURSOR_SEPARATOR)
        .ok_or(PortError::Internal)?;
    let timestamp = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| PortError::Internal)?
        .with_timezone(&Utc);
    let id = StockItemId::try_from_uuid(Uuid::parse_str(id).map_err(|_| PortError::Internal)?)
        .map_err(|_| PortError::Internal)?;
    Ok((timestamp, id))
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_cursor_fails_closed() {
        assert_eq!(decode_cursor("bad"), Err(PortError::Internal));
    }
}
