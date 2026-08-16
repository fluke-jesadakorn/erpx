use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::TenantScope;
use erpx_maintenance::{
    PortError, ReadProjection, ReadRepository, WorkOrderId, WorkOrderList, WorkOrderListFilter,
    WorkOrderView,
};
use sqlx::Row;
use uuid::Uuid;

use crate::{PROJECTION_NAME, projection::status_name, work_order_partition};

const CURSOR_SEPARATOR: char = '|';

#[derive(Debug, Clone)]
pub struct MaintenanceReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl MaintenanceReadRepository {
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
        exact_partition: Option<&str>,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => match exact_partition {
                Some(partition) => self
                    .read_side
                    .checkpoint(PROJECTION_NAME, partition)
                    .await
                    .map_err(map_read_error),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                let valid = requirement.projection == PROJECTION_NAME
                    && requirement
                        .partition
                        .strip_prefix("maintenance.work_order:")
                        .is_some_and(|value| Uuid::parse_str(value).is_ok())
                    && exact_partition.is_none_or(|value| value == requirement.partition);
                if !valid {
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
        Ok(ProjectionMetadata {
            lag_millis: checkpoint.as_ref().and_then(|value| {
                Utc::now()
                    .signed_duration_since(value.updated_at)
                    .to_std()
                    .ok()
                    .and_then(|duration| u64::try_from(duration.as_millis()).ok())
            }),
            checkpoint,
            freshness,
        })
    }
}

#[async_trait]
impl ReadRepository for MaintenanceReadRepository {
    async fn get(
        &self,
        authority_scope: &TenantScope,
        id: WorkOrderId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<WorkOrderView>>, PortError> {
        let partition = work_order_partition(id);
        let checkpoint = self.checkpoint(consistency, Some(&partition)).await?;
        let row = sqlx::query(
            "SELECT projection.state FROM erpx_maintenance_read.work_order_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1 AND projection.work_order_id=$2
               AND projection.partition_key=$3",
        )
        .bind(PROJECTION_NAME)
        .bind(id.as_uuid())
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
        let value = row
            .map(decode_view)
            .transpose()?
            .filter(|view| authority_scope.contains(&view.scope));
        Ok(ReadProjection {
            value,
            metadata: self.metadata(checkpoint).await?,
        })
    }

    async fn list(
        &self,
        authority_scope: &TenantScope,
        filter: &WorkOrderListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<WorkOrderList>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let checkpoint = self.checkpoint(consistency, None).await?;
        let cursor = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let status = filter.status.map(status_name);
        let asset_id = filter
            .asset_id
            .map(|value| {
                if value.is_nil() {
                    Err(PortError::Internal)
                } else {
                    Ok(value)
                }
            })
            .transpose()?;
        let limit = i64::from(filter.page_size) + 1;
        let rows = sqlx::query(
            "SELECT projection.state FROM erpx_maintenance_read.work_order_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1
               AND ($2::text IS NULL OR projection.work_order_status=$2)
               AND ($3::uuid IS NULL OR projection.asset_id=$3)
               AND ($4::timestamptz IS NULL OR
                    (projection.aggregate_updated_at,projection.work_order_id)<($4,$5))
             ORDER BY projection.aggregate_updated_at DESC,projection.work_order_id DESC
             LIMIT $6",
        )
        .bind(PROJECTION_NAME)
        .bind(status)
        .bind(asset_id)
        .bind(cursor.as_ref().map(|value| value.0))
        .bind(cursor.as_ref().map(|value| value.1.as_uuid()))
        .bind(limit.saturating_mul(8))
        .fetch_all(
            &mut *self
                .pool
                .begin()
                .await
                .map_err(|_| PortError::Unavailable)?,
        )
        .await
        .map_err(|_| PortError::Unavailable)?;
        let mut work_orders = rows
            .into_iter()
            .map(decode_view)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|view| authority_scope.contains(&view.scope))
            .take(usize::try_from(limit).map_err(|_| PortError::Internal)?)
            .collect::<Vec<_>>();
        let next_cursor = if work_orders.len() > usize::from(filter.page_size) {
            work_orders.truncate(usize::from(filter.page_size));
            work_orders
                .last()
                .map(|view| encode_cursor(view.updated_at, view.work_order_id))
        } else {
            None
        };
        Ok(ReadProjection {
            value: WorkOrderList {
                work_orders,
                next_cursor,
            },
            metadata: self.metadata(checkpoint).await?,
        })
    }
}

fn decode_view(row: sqlx::postgres::PgRow) -> Result<WorkOrderView, PortError> {
    let state: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    let view: WorkOrderView = serde_json::from_value(state).map_err(|_| PortError::Internal)?;
    if view.version == 0 || view.scope.validate().is_err() {
        return Err(PortError::Internal);
    }
    Ok(view)
}

pub fn encode_cursor(updated_at: DateTime<Utc>, id: WorkOrderId) -> String {
    format!(
        "{}{}{}",
        updated_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        CURSOR_SEPARATOR,
        id.as_uuid()
    )
}

pub fn decode_cursor(value: &str) -> Result<(DateTime<Utc>, WorkOrderId), PortError> {
    let (timestamp, id) = value
        .split_once(CURSOR_SEPARATOR)
        .ok_or(PortError::Internal)?;
    let at = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| PortError::Internal)?
        .with_timezone(&Utc);
    let id = WorkOrderId::try_from_uuid(Uuid::parse_str(id).map_err(|_| PortError::Internal)?)
        .map_err(|_| PortError::Internal)?;
    Ok((at, id))
}

fn map_read_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;
    use erpx_maintenance::WorkOrderStatus;

    use super::*;

    #[test]
    fn cursor_round_trips_exactly() {
        let at = Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap();
        let id = WorkOrderId::try_from_uuid(Uuid::from_u128(7)).unwrap();
        assert_eq!(decode_cursor(&encode_cursor(at, id)).unwrap(), (at, id));
    }

    #[test]
    fn status_filter_names_match_projection_contract() {
        assert_eq!(status_name(WorkOrderStatus::Blocked), "blocked");
    }
}
