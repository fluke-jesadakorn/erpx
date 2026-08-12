use async_trait::async_trait;
use chrono::{DateTime, Utc};
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::TenantScope;
use erpx_sales::{
    PortError, QuoteId, QuoteView, ReadRepository, RevenueArrangementId, RevenueArrangementView,
    SalesOrderId, SalesOrderList, SalesOrderListFilter, SalesOrderView, ports::ReadProjection,
};
use sqlx::Row;
use uuid::Uuid;

use crate::{
    ARRANGEMENT_PROJECTION, ORDER_PROJECTION, QUOTE_PROJECTION, arrangement_partition,
    order_partition, projection::order_status_name, quote_partition,
};

const CURSOR_SEPARATOR: char = '|';

#[derive(Debug, Clone)]
pub struct SalesReadRepository {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl SalesReadRepository {
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
        exact: Option<&str>,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => match exact {
                Some(partition) => self
                    .read_side
                    .checkpoint(projection, partition)
                    .await
                    .map_err(map_read_error),
                None => Ok(None),
            },
            QueryConsistency::ReadYourWrites(requirement) => {
                let suffix = requirement.partition.strip_prefix(prefix);
                let valid_partition = suffix.is_some_and(|value| {
                    value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
                });
                if requirement.projection != projection
                    || !valid_partition
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
impl ReadRepository for SalesReadRepository {
    async fn get_quote(
        &self,
        authority_scope: &TenantScope,
        quote_id: QuoteId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<QuoteView>>, PortError> {
        let partition = quote_partition(quote_id);
        let checkpoint = self
            .checkpoint(
                QUOTE_PROJECTION,
                "sales.quote:",
                consistency,
                Some(&partition),
            )
            .await?;
        let row = sqlx::query(
            "SELECT projection.state FROM erpx_sales_read.quote_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1 AND projection.quote_id=$2
               AND projection.partition_key=$3",
        )
        .bind(QUOTE_PROJECTION)
        .bind(quote_id.as_uuid())
        .bind(&partition)
        .fetch_optional(self.pool.as_pg_pool())
        .await
        .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_state).transpose()?;
        if value
            .as_ref()
            .is_some_and(|view: &QuoteView| !authority_scope.contains(&view.scope))
        {
            return Err(PortError::Denied);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(QUOTE_PROJECTION, checkpoint).await?,
        })
    }

    async fn get_order(
        &self,
        authority_scope: &TenantScope,
        sales_order_id: SalesOrderId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<SalesOrderView>>, PortError> {
        let partition = order_partition(sales_order_id);
        let checkpoint = self
            .checkpoint(
                ORDER_PROJECTION,
                "sales.order:",
                consistency,
                Some(&partition),
            )
            .await?;
        let row = sqlx::query(
            "SELECT projection.state FROM erpx_sales_read.sales_order_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1 AND projection.sales_order_id=$2
               AND projection.partition_key=$3",
        )
        .bind(ORDER_PROJECTION)
        .bind(sales_order_id.as_uuid())
        .bind(&partition)
        .fetch_optional(self.pool.as_pg_pool())
        .await
        .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_state).transpose()?;
        if value
            .as_ref()
            .is_some_and(|view: &SalesOrderView| !authority_scope.contains(&view.scope))
        {
            return Err(PortError::Denied);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(ORDER_PROJECTION, checkpoint).await?,
        })
    }

    async fn get_revenue_arrangement(
        &self,
        authority_scope: &TenantScope,
        arrangement_id: RevenueArrangementId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<RevenueArrangementView>>, PortError> {
        let partition = arrangement_partition(arrangement_id);
        let checkpoint = self
            .checkpoint(
                ARRANGEMENT_PROJECTION,
                "sales.arrangement:",
                consistency,
                Some(&partition),
            )
            .await?;
        let row = sqlx::query(
            "SELECT projection.state
             FROM erpx_sales_read.revenue_arrangement_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1 AND projection.arrangement_id=$2
               AND projection.partition_key=$3",
        )
        .bind(ARRANGEMENT_PROJECTION)
        .bind(arrangement_id.as_uuid())
        .bind(&partition)
        .fetch_optional(self.pool.as_pg_pool())
        .await
        .map_err(|_| PortError::Unavailable)?;
        let value = row.map(decode_state).transpose()?;
        if value
            .as_ref()
            .is_some_and(|view: &RevenueArrangementView| !authority_scope.contains(&view.scope))
        {
            return Err(PortError::Denied);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(ARRANGEMENT_PROJECTION, checkpoint).await?,
        })
    }

    async fn list_orders(
        &self,
        authority_scope: &TenantScope,
        filter: &SalesOrderListFilter,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<SalesOrderList>, PortError> {
        let checkpoint = self
            .checkpoint(ORDER_PROJECTION, "sales.order:", consistency, None)
            .await?;
        let cursor = filter.cursor.as_deref().map(decode_cursor).transpose()?;
        let cursor_at = cursor.as_ref().map(|value| value.0);
        let cursor_id = cursor.as_ref().map(|value| value.1.as_uuid());
        let status = filter.status.map(order_status_name);
        let scan_limit = i64::from(filter.page_size)
            .saturating_mul(8)
            .saturating_add(8);
        let rows = sqlx::query(
            "SELECT projection.state FROM erpx_sales_read.sales_order_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.projection_name=$1
               AND ($2::text IS NULL OR projection.order_status=$2)
               AND ($3::uuid IS NULL OR projection.customer_id=$3)
               AND ($4::timestamptz IS NULL OR
                    (projection.aggregate_updated_at,projection.sales_order_id)<($4,$5))
             ORDER BY projection.aggregate_updated_at DESC,projection.sales_order_id DESC
             LIMIT $6",
        )
        .bind(ORDER_PROJECTION)
        .bind(status)
        .bind(filter.customer_id)
        .bind(cursor_at)
        .bind(cursor_id)
        .bind(scan_limit)
        .fetch_all(self.pool.as_pg_pool())
        .await
        .map_err(|_| PortError::Unavailable)?;
        let mut orders = rows
            .into_iter()
            .map(decode_state)
            .collect::<Result<Vec<SalesOrderView>, _>>()?;
        orders.retain(|value| authority_scope.contains(&value.scope));
        let page_size = usize::from(filter.page_size);
        let next_cursor = if orders.len() > page_size {
            orders.truncate(page_size);
            orders.last().map(encode_cursor)
        } else {
            None
        };
        Ok(ReadProjection {
            value: SalesOrderList {
                orders,
                next_cursor,
            },
            metadata: self.metadata(ORDER_PROJECTION, checkpoint).await?,
        })
    }
}

fn decode_state<T: serde::de::DeserializeOwned>(
    row: sqlx::postgres::PgRow,
) -> Result<T, PortError> {
    let value: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    serde_json::from_value(value).map_err(|_| PortError::Internal)
}

pub fn encode_cursor(value: &SalesOrderView) -> String {
    format!(
        "{}{}{}",
        value.updated_at.to_rfc3339(),
        CURSOR_SEPARATOR,
        value.sales_order_id.as_uuid()
    )
}

pub fn decode_cursor(value: &str) -> Result<(DateTime<Utc>, SalesOrderId), PortError> {
    let (timestamp, id) = value
        .split_once(CURSOR_SEPARATOR)
        .ok_or(PortError::Internal)?;
    let timestamp = DateTime::parse_from_rfc3339(timestamp)
        .map_err(|_| PortError::Internal)?
        .with_timezone(&Utc);
    let id = SalesOrderId::try_from_uuid(Uuid::parse_str(id).map_err(|_| PortError::Internal)?)
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

    #[test]
    fn read_your_writes_partition_names_are_exact() {
        let quote = QuoteId::try_from_uuid(Uuid::from_u128(1)).unwrap();
        let order = SalesOrderId::try_from_uuid(Uuid::from_u128(2)).unwrap();
        assert_eq!(quote_partition(quote).len(), "sales.quote:".len() + 32);
        assert_eq!(order_partition(order).len(), "sales.order:".len() + 32);
    }
}
