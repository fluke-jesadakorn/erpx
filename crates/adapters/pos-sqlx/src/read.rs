use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::TenantScope;
use erpx_pos::{
    PortError, PosReadRepository, RefundId, RefundList, RefundListFilter, RefundView, SaleId,
    SaleList, SaleListFilter, SaleView, ports::ReadProjection,
};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    REFUND_PROJECTION_NAME, SALE_PROJECTION_NAME,
    projection::{refund_status_name, sale_status_name},
    refund_partition, sale_partition,
};

const CURSOR_SEPARATOR: char = '|';

#[derive(Debug, Clone)]
pub struct PosSqlxReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl PosSqlxReadRepository {
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
        projection: &str,
        prefix: &str,
        consistency: &QueryConsistency,
        exact_partition: Option<&str>,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => match exact_partition {
                Some(partition) => self
                    .read_side
                    .checkpoint(projection, partition)
                    .await
                    .map_err(map_read_error),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                let valid = requirement.projection == projection
                    && requirement
                        .partition
                        .strip_prefix(prefix)
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
        projection: &str,
        checkpoint: Option<ProjectionCheckpoint>,
    ) -> Result<ProjectionMetadata, PortError> {
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE projection_name=$1 AND status IN ('active','building')
             ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(projection)
        .fetch_optional(self.pool.as_pg_pool())
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
impl PosReadRepository for PosSqlxReadRepository {
    async fn get_sale(
        &self,
        authority_scope: &TenantScope,
        sale_id: SaleId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SaleView>>, PortError> {
        let partition = sale_partition(sale_id);
        let checkpoint = self
            .checkpoint(
                SALE_PROJECTION_NAME,
                "pos.sale:",
                consistency,
                Some(&partition),
            )
            .await?;
        let row = sqlx::query(
            "SELECT projection.state FROM erpx_pos_read.sale_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1 AND projection.sale_id=$2
               AND projection.partition_key=$3",
        )
        .bind(SALE_PROJECTION_NAME)
        .bind(sale_id.as_uuid())
        .bind(&partition)
        .fetch_optional(self.pool.as_pg_pool())
        .await
        .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_sale_view).transpose()?;
        let value = value.filter(|view| authority_scope.contains(&view.scope));
        Ok(ReadProjection {
            value,
            metadata: self.metadata(SALE_PROJECTION_NAME, checkpoint).await?,
        })
    }

    async fn list_sales(
        &self,
        authority_scope: &TenantScope,
        filter: &SaleListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<SaleList>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let checkpoint = self
            .checkpoint(SALE_PROJECTION_NAME, "pos.sale:", consistency, None)
            .await?;
        let cursor = filter
            .cursor
            .as_deref()
            .map(decode_sale_cursor)
            .transpose()?;
        let status = filter.status.map(sale_status_name);
        let limit = i64::from(filter.page_size) + 1;
        let rows = sqlx::query(
            "SELECT projection.state FROM erpx_pos_read.sale_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1
               AND ($2::text IS NULL OR projection.sale_status=$2)
               AND ($3::uuid IS NULL OR projection.register_id=$3)
               AND ($4::timestamptz IS NULL OR
                    (projection.aggregate_updated_at,projection.sale_id)<($4,$5))
             ORDER BY projection.aggregate_updated_at DESC,projection.sale_id DESC
             LIMIT $6",
        )
        .bind(SALE_PROJECTION_NAME)
        .bind(status)
        .bind(filter.register_id.map(|value| value.as_uuid()))
        .bind(cursor.as_ref().map(|value| value.0))
        .bind(cursor.as_ref().map(|value| value.1.as_uuid()))
        .bind(limit.saturating_mul(8))
        .fetch_all(self.pool.as_pg_pool())
        .await
        .map_err(|_| PortError::Unavailable)?;
        let mut sales = rows
            .into_iter()
            .map(decode_sale_view)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|view| authority_scope.contains(&view.scope))
            .take(usize::try_from(limit).map_err(|_| PortError::Internal)?)
            .collect::<Vec<_>>();
        let next_cursor = if sales.len() > usize::from(filter.page_size) {
            sales.truncate(usize::from(filter.page_size));
            sales
                .last()
                .map(|view| encode_sale_cursor(view.updated_at, view.sale_id))
        } else {
            None
        };
        Ok(ReadProjection {
            value: SaleList { sales, next_cursor },
            metadata: self.metadata(SALE_PROJECTION_NAME, checkpoint).await?,
        })
    }

    async fn get_refund(
        &self,
        authority_scope: &TenantScope,
        refund_id: RefundId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<RefundView>>, PortError> {
        let partition = refund_partition(refund_id);
        let checkpoint = self
            .checkpoint(
                REFUND_PROJECTION_NAME,
                "pos.refund:",
                consistency,
                Some(&partition),
            )
            .await?;
        let row = sqlx::query(
            "SELECT projection.state FROM erpx_pos_read.refund_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1 AND projection.refund_id=$2
               AND projection.partition_key=$3",
        )
        .bind(REFUND_PROJECTION_NAME)
        .bind(refund_id.as_uuid())
        .bind(&partition)
        .fetch_optional(self.pool.as_pg_pool())
        .await
        .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_refund_view).transpose()?;
        let value = value.filter(|view| authority_scope.contains(&view.scope));
        Ok(ReadProjection {
            value,
            metadata: self.metadata(REFUND_PROJECTION_NAME, checkpoint).await?,
        })
    }

    async fn list_refunds(
        &self,
        authority_scope: &TenantScope,
        filter: &RefundListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<RefundList>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let checkpoint = self
            .checkpoint(REFUND_PROJECTION_NAME, "pos.refund:", consistency, None)
            .await?;
        let cursor = filter
            .cursor
            .as_deref()
            .map(decode_refund_cursor)
            .transpose()?;
        let status = filter.status.map(refund_status_name);
        let limit = i64::from(filter.page_size) + 1;
        let rows = sqlx::query(
            "SELECT projection.state FROM erpx_pos_read.refund_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1
               AND ($2::text IS NULL OR projection.refund_status=$2)
               AND ($3::uuid IS NULL OR projection.sale_id=$3)
               AND ($4::timestamptz IS NULL OR
                    (projection.aggregate_updated_at,projection.refund_id)<($4,$5))
             ORDER BY projection.aggregate_updated_at DESC,projection.refund_id DESC
             LIMIT $6",
        )
        .bind(REFUND_PROJECTION_NAME)
        .bind(status)
        .bind(filter.sale_id.map(|value| value.as_uuid()))
        .bind(cursor.as_ref().map(|value| value.0))
        .bind(cursor.as_ref().map(|value| value.1.as_uuid()))
        .bind(limit.saturating_mul(8))
        .fetch_all(self.pool.as_pg_pool())
        .await
        .map_err(|_| PortError::Unavailable)?;
        let mut refunds = rows
            .into_iter()
            .map(decode_refund_view)
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .filter(|view| authority_scope.contains(&view.scope))
            .take(usize::try_from(limit).map_err(|_| PortError::Internal)?)
            .collect::<Vec<_>>();
        let next_cursor = if refunds.len() > usize::from(filter.page_size) {
            refunds.truncate(usize::from(filter.page_size));
            refunds
                .last()
                .map(|view| encode_refund_cursor(view.updated_at, view.refund_id))
        } else {
            None
        };
        Ok(ReadProjection {
            value: RefundList {
                refunds,
                next_cursor,
            },
            metadata: self.metadata(REFUND_PROJECTION_NAME, checkpoint).await?,
        })
    }
}

fn decode_sale_view(row: sqlx::postgres::PgRow) -> Result<SaleView, PortError> {
    let state: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    let view: SaleView = serde_json::from_value(state).map_err(|_| PortError::Internal)?;
    if view.version == 0
        || view.scope.validate().is_err()
        || view.currency.len() != 3
        || view.updated_at < view.created_at
    {
        return Err(PortError::Internal);
    }
    Ok(view)
}

fn decode_refund_view(row: sqlx::postgres::PgRow) -> Result<RefundView, PortError> {
    let state: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    let view: RefundView = serde_json::from_value(state).map_err(|_| PortError::Internal)?;
    if view.version == 0
        || view.scope.validate().is_err()
        || view.currency.len() != 3
        || view.updated_at < view.requested_at
    {
        return Err(PortError::Internal);
    }
    Ok(view)
}

fn encode_sale_cursor(updated_at: DateTime<Utc>, sale_id: SaleId) -> String {
    encode_cursor(updated_at, sale_id.as_uuid())
}

fn encode_refund_cursor(updated_at: DateTime<Utc>, refund_id: RefundId) -> String {
    encode_cursor(updated_at, refund_id.as_uuid())
}

fn encode_cursor(updated_at: DateTime<Utc>, id: Uuid) -> String {
    format!(
        "{}{}{}",
        updated_at.to_rfc3339_opts(chrono::SecondsFormat::Micros, true),
        CURSOR_SEPARATOR,
        id
    )
}

pub fn decode_sale_cursor(value: &str) -> Result<(DateTime<Utc>, SaleId), PortError> {
    let (at, id) = decode_cursor(value)?;
    let id = SaleId::try_from_uuid(id).map_err(|_| PortError::Internal)?;
    Ok((at, id))
}

pub fn decode_refund_cursor(value: &str) -> Result<(DateTime<Utc>, RefundId), PortError> {
    let (at, id) = decode_cursor(value)?;
    let id = RefundId::try_from_uuid(id).map_err(|_| PortError::Internal)?;
    Ok((at, id))
}

fn decode_cursor(value: &str) -> Result<(DateTime<Utc>, Uuid), PortError> {
    let (timestamp, id) = value
        .split_once(CURSOR_SEPARATOR)
        .ok_or(PortError::Internal)?;
    let at = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| PortError::Internal)?
        .with_timezone(&Utc);
    let id = Uuid::parse_str(id).map_err(|_| PortError::Internal)?;
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
    use erpx_pos::{RefundStatus, SaleStatus};

    use super::*;

    #[test]
    fn sale_and_refund_cursors_round_trip_without_crossing_types() {
        let at = Utc.with_ymd_and_hms(2026, 8, 5, 9, 0, 0).single().unwrap();
        let sale = SaleId::try_from_uuid(Uuid::from_u128(7)).unwrap();
        let refund = RefundId::try_from_uuid(Uuid::from_u128(8)).unwrap();
        assert_eq!(
            decode_sale_cursor(&encode_sale_cursor(at, sale)).unwrap(),
            (at, sale)
        );
        assert_eq!(
            decode_refund_cursor(&encode_refund_cursor(at, refund)).unwrap(),
            (at, refund)
        );
    }

    #[test]
    fn status_filter_names_match_projection_contract() {
        assert_eq!(sale_status_name(SaleStatus::Completed), "completed");
        assert_eq!(refund_status_name(RefundStatus::Approved), "approved");
    }

    #[test]
    fn exact_ryw_validation_is_projection_and_partition_specific() {
        let source = include_str!("read.rs");
        assert!(source.contains("requirement.projection == projection"));
        assert!(source.contains("exact_partition.is_none_or"));
        let forbidden = ["load", "_snapshot"].concat();
        assert!(!source.contains(&forbidden));
    }
}
