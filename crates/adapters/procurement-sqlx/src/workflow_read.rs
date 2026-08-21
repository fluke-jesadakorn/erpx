use async_trait::async_trait;
use chrono::Utc;
use erpx_cqrs::{
    FreshnessState, ProjectionCheckpoint, ProjectionMetadata, ProjectionNotReady, QueryConsistency,
};
use erpx_cqrs_sqlx::{
    CqrsSqlxError, ReadPool, ReadSideStore, SqlxProjectionWaiter, WaitError, WaiterConfig,
};
use erpx_kernel::AuthorityScope;
use erpx_procurement::{
    GoodsReceiptId, GoodsReceiptView, PortError, ProcurementWorkflowReadRepository,
    PurchaseRequisitionId, PurchaseRequisitionView, ReadProjection,
};
use sqlx::Row;

use crate::workflow_projection::{
    WORKFLOW_PROJECTION_NAME, goods_receipt_partition, requisition_partition,
};

#[derive(Debug, Clone)]
pub struct ProcurementWorkflowReadAdapter {
    pool: ReadPool,
    read_side: ReadSideStore,
    waiter: SqlxProjectionWaiter,
}

impl ProcurementWorkflowReadAdapter {
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

    pub fn new_read(pool: ReadPool) -> Result<Self, CqrsSqlxError> {
        Self::new(pool)
    }

    async fn checkpoint(
        &self,
        consistency: &QueryConsistency,
        partition: &str,
    ) -> Result<Option<ProjectionCheckpoint>, PortError> {
        match consistency {
            QueryConsistency::Eventual => self
                .read_side
                .checkpoint(WORKFLOW_PROJECTION_NAME, partition)
                .await
                .map_err(map_read_error),
            QueryConsistency::ReadYourWrites(requirement) => {
                if requirement.projection != WORKFLOW_PROJECTION_NAME
                    || requirement.partition != partition
                    || !valid_partition(partition)
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
        let mut transaction = self.pool.begin().await.map_err(map_read_error)?;
        let status: Option<String> = sqlx::query_scalar(
            "SELECT status FROM erpx_cqrs_read.projection_generations
             WHERE holding_id=erpx_cqrs_read.current_holding_id()
               AND projection_name=$1 AND status IN ('active','building')
             ORDER BY CASE status WHEN 'active' THEN 0 ELSE 1 END LIMIT 1",
        )
        .bind(WORKFLOW_PROJECTION_NAME)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
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
impl ProcurementWorkflowReadRepository for ProcurementWorkflowReadAdapter {
    async fn get_requisition(
        &self,
        authority_scope: &AuthorityScope,
        requisition_id: PurchaseRequisitionId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<PurchaseRequisitionView>>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let partition = requisition_partition(requisition_id);
        let checkpoint = self.checkpoint(consistency, &partition).await?;
        let mut transaction = self.pool.begin().await.map_err(map_read_error)?;
        let row = sqlx::query(
            "SELECT projection.state
             FROM erpx_procurement_read.purchase_requisition_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.holding_id=projection.holding_id
              AND generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.holding_id=erpx_procurement_read.current_holding_id()
               AND projection.projection_name=$1 AND projection.partition_key=$2
               AND projection.requisition_id=$3",
        )
        .bind(WORKFLOW_PROJECTION_NAME)
        .bind(&partition)
        .bind(requisition_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let value = row
            .map(|row| decode::<PurchaseRequisitionView>(&row))
            .transpose()?;
        if value
            .as_ref()
            .is_some_and(|record| !authority_scope.contains(&record.scope))
        {
            return Err(PortError::Denied);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(checkpoint).await?,
        })
    }

    async fn get_goods_receipt(
        &self,
        authority_scope: &AuthorityScope,
        goods_receipt_id: GoodsReceiptId,
        consistency: &QueryConsistency,
    ) -> Result<ReadProjection<Option<GoodsReceiptView>>, PortError> {
        authority_scope.validate().map_err(|_| PortError::Denied)?;
        let partition = goods_receipt_partition(goods_receipt_id);
        let checkpoint = self.checkpoint(consistency, &partition).await?;
        let mut transaction = self.pool.begin().await.map_err(map_read_error)?;
        let row = sqlx::query(
            "SELECT projection.state
             FROM erpx_procurement_read.goods_receipt_projection projection
             JOIN erpx_cqrs_read.projection_generations generation
               ON generation.holding_id=projection.holding_id
              AND generation.projection_name=projection.projection_name
              AND generation.generation=projection.generation AND generation.status='active'
             WHERE projection.holding_id=erpx_procurement_read.current_holding_id()
               AND projection.projection_name=$1 AND projection.partition_key=$2
               AND projection.goods_receipt_id=$3",
        )
        .bind(WORKFLOW_PROJECTION_NAME)
        .bind(&partition)
        .bind(goods_receipt_id.as_uuid())
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let value = row
            .map(|row| decode::<GoodsReceiptView>(&row))
            .transpose()?;
        if value
            .as_ref()
            .is_some_and(|record| !authority_scope.contains(&record.scope))
        {
            return Err(PortError::Denied);
        }
        Ok(ReadProjection {
            value,
            metadata: self.metadata(checkpoint).await?,
        })
    }
}

fn decode<T: serde::de::DeserializeOwned>(row: &sqlx::postgres::PgRow) -> Result<T, PortError> {
    let value: serde_json::Value = row.try_get("state").map_err(|_| PortError::Internal)?;
    serde_json::from_value(value).map_err(|_| PortError::Internal)
}

fn valid_partition(partition: &str) -> bool {
    ["procurement.requisition:", "procurement.goods_receipt:"]
        .iter()
        .any(|prefix| {
            partition.strip_prefix(prefix).is_some_and(|value| {
                value.len() == 32 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
            })
        })
}

fn map_read_error(_: CqrsSqlxError) -> PortError {
    PortError::Unavailable
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn workflow_read_your_writes_partitions_fail_closed() {
        assert!(valid_partition(
            "procurement.requisition:00000000000000000000000000000001"
        ));
        assert!(valid_partition(
            "procurement.goods_receipt:00000000000000000000000000000002"
        ));
        assert!(!valid_partition("procurement.purchase_order:01"));
        assert!(!valid_partition("procurement.goods_receipt:not-a-uuid"));
    }
}
