use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::TenantScope;
use erpx_procurement::{
    PortError, PurchaseOrderId, PurchaseOrderList, PurchaseOrderListFilter, PurchaseOrderView,
    ReadProjection, ReadRepository,
};
use sqlx::Row;
use uuid::Uuid;

use crate::{PROJECTION_NAME, projection::status_name, purchase_order_partition};

const CURSOR_SEPARATOR: char = '|';

#[derive(Debug, Clone)]
pub struct ProcurementReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl ProcurementReadRepository {
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
                    .map_err(map_read_error),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                let valid = requirement
                    .partition
                    .strip_prefix("procurement.purchase_order:")
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
impl ReadRepository for ProcurementReadRepository {
    async fn get(
        &self,
        authority_scope: &TenantScope,
        purchase_order_id: PurchaseOrderId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PurchaseOrderView>>, PortError> {
        let partition = purchase_order_partition(purchase_order_id);
        let checkpoint = self.checkpoint(consistency, Some(&partition)).await?;
        let row = sqlx::query(
            "SELECT projection.state FROM erpx_procurement_read.purchase_order_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1 AND projection.purchase_order_id=$2
               AND projection.partition_key=$3",
        )
        .bind(PROJECTION_NAME)
        .bind(purchase_order_id.as_uuid())
        .bind(&partition)
        .fetch_optional(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?)
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
        filter: &PurchaseOrderListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<PurchaseOrderList>, PortError> {
        let checkpoint = self.checkpoint(consistency, None).await?;
        let cursor = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let cursor_at = cursor.as_ref().map(|value| value.0);
        let cursor_id = cursor.as_ref().map(|value| value.1.as_uuid());
        let status = filter.status.map(status_name);
        let scan_limit = i64::from(filter.page_size)
            .saturating_mul(8)
            .saturating_add(8);
        let rows = sqlx::query(
            "SELECT projection.state FROM erpx_procurement_read.purchase_order_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1
               AND ($2::text IS NULL OR projection.order_status=$2)
               AND ($3::uuid IS NULL OR projection.supplier_id=$3)
               AND ($4::timestamptz IS NULL OR
                    (projection.aggregate_updated_at,projection.purchase_order_id)<($4,$5))
             ORDER BY projection.aggregate_updated_at DESC,projection.purchase_order_id DESC
             LIMIT $6",
        )
        .bind(PROJECTION_NAME)
        .bind(status)
        .bind(filter.supplier_id.map(|value| value.as_uuid()))
        .bind(cursor_at)
        .bind(cursor_id)
        .bind(scan_limit)
        .fetch_all(&mut *self.pool.begin().await.map_err(|_| PortError::Unavailable)?)
        .await
        .map_err(|_| PortError::Unavailable)?;
        let mut orders = rows
            .into_iter()
            .map(decode_view)
            .collect::<Result<Vec<_>, _>>()?;
        orders.retain(|value| authority_scope.contains(&value.scope));
        let page_size = usize::from(filter.page_size);
        let next_cursor = if orders.len() > page_size {
            orders.truncate(page_size);
            orders.last().map(encode_cursor)
        } else {
            None
        };
        Ok(ReadProjection {
            value: PurchaseOrderList {
                orders,
                next_cursor,
            },
            metadata: self.metadata(checkpoint).await?,
        })
    }
}

fn decode_view(row: sqlx::postgres::PgRow) -> Result<PurchaseOrderView, PortError> {
    let value: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    serde_json::from_value(value).map_err(|_| PortError::Internal)
}

pub fn encode_cursor(value: &PurchaseOrderView) -> String {
    format!(
        "{}{}{}",
        value.updated_at.to_rfc3339(),
        CURSOR_SEPARATOR,
        value.purchase_order_id.as_uuid()
    )
}

pub fn decode_cursor(value: &str) -> Result<(DateTime<Utc>, PurchaseOrderId), PortError> {
    let (timestamp, id) = value
        .split_once(CURSOR_SEPARATOR)
        .ok_or(PortError::Internal)?;
    let timestamp = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| PortError::Internal)?
        .with_timezone(&Utc);
    let id = PurchaseOrderId::try_from_uuid(Uuid::parse_str(id).map_err(|_| PortError::Internal)?)
        .map_err(|_| PortError::Internal)?;
    Ok((timestamp, id))
}

fn map_read_error(_: CqrsSqlxError) -> PortError {
    PortError::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn malformed_cursor_fails_closed() {
        assert_eq!(decode_cursor("bad"), Err(PortError::Internal));
    }
}
