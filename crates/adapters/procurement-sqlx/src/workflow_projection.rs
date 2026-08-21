use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraftV2, procurement};
use erpx_procurement::{
    GoodsReceiptStatus, GoodsReceiptView, PurchaseRequisitionStatus, PurchaseRequisitionView,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{CONTEXT_NAME, EVENT_VERSION};

pub const WORKFLOW_PROJECTION_NAME: &str = "procurement.workflow-records";
pub const REQUISITION_AGGREGATE_TYPE: &str = "purchase_requisition";
pub const GOODS_RECEIPT_AGGREGATE_TYPE: &str = "goods_receipt";
pub const REQUISITION_SNAPSHOT_EVENT_TYPE: &str = "procurement.purchase_requisition_snapshot";
pub const GOODS_RECEIPT_SNAPSHOT_EVENT_TYPE: &str = "procurement.goods_receipt_snapshot";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "record_kind", content = "record", rename_all = "snake_case")]
pub enum ProcurementWorkflowSnapshot {
    Requisition(PurchaseRequisitionView),
    GoodsReceipt(GoodsReceiptView),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "payload_kind", content = "payload", rename_all = "snake_case")]
pub enum ProcurementWorkflowOutboxPayload {
    Integration(procurement::Event),
    Snapshot(ProcurementWorkflowSnapshot),
    Directory(ItemDescriptorDraftV2),
}

#[derive(Debug, Clone)]
pub struct ProcurementWorkflowProjector {
    event_types: Vec<String>,
}

impl ProcurementWorkflowProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![
                REQUISITION_SNAPSHOT_EVENT_TYPE.into(),
                GOODS_RECEIPT_SNAPSHOT_EVENT_TYPE.into(),
            ],
        }
    }
}

impl Default for ProcurementWorkflowProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for ProcurementWorkflowProjector {
    fn name(&self) -> &str {
        WORKFLOW_PROJECTION_NAME
    }

    fn event_types(&self) -> &[String] {
        &self.event_types
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        match validate_event(context, event)? {
            ProcurementWorkflowSnapshot::Requisition(view) => {
                project_requisition(transaction, context, event, view).await
            }
            ProcurementWorkflowSnapshot::GoodsReceipt(view) => {
                project_goods_receipt(transaction, context, event, view).await
            }
        }
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query(
            "DELETE FROM erpx_procurement_read.purchase_requisition_projection
             WHERE holding_id=erpx_procurement_read.current_holding_id()
               AND projection_name=$1 AND generation=$2",
        )
        .bind(WORKFLOW_PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        sqlx::query(
            "DELETE FROM erpx_procurement_read.goods_receipt_projection
             WHERE holding_id=erpx_procurement_read.current_holding_id()
               AND projection_name=$1 AND generation=$2",
        )
        .bind(WORKFLOW_PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        Ok(())
    }
}

async fn project_requisition(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    view: PurchaseRequisitionView,
) -> Result<(), ProjectionFailure> {
    let result = sqlx::query(
        "INSERT INTO erpx_procurement_read.purchase_requisition_projection
            (holding_id,projection_name,generation,partition_key,requisition_id,
             source_aggregate_version,source_global_sequence,active_scope,scope_key,
             requisition_code,requesting_department_code,budget_code,currency,
             requisition_status,state,aggregate_created_at,aggregate_updated_at,projected_at)
         VALUES (erpx_procurement_read.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,
                 $9,$10,$11,$12,$13,$14,$15,$16,now())
         ON CONFLICT (holding_id,projection_name,generation,partition_key,requisition_id)
         DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
            source_global_sequence=EXCLUDED.source_global_sequence,
            active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,
            requisition_code=EXCLUDED.requisition_code,
            requesting_department_code=EXCLUDED.requesting_department_code,
            budget_code=EXCLUDED.budget_code,currency=EXCLUDED.currency,
            requisition_status=EXCLUDED.requisition_status,state=EXCLUDED.state,
            aggregate_created_at=EXCLUDED.aggregate_created_at,
            aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
         WHERE erpx_procurement_read.purchase_requisition_projection.source_aggregate_version
               < EXCLUDED.source_aggregate_version",
    )
    .bind(WORKFLOW_PROJECTION_NAME)
    .bind(context.generation)
    .bind(&event.partition)
    .bind(view.requisition_id.as_uuid())
    .bind(to_i64(view.version)?)
    .bind(to_i64(event.global_sequence)?)
    .bind(serde_json::to_value(&view.scope).map_err(failure)?)
    .bind(scope_key(&view.scope)?)
    .bind(&view.requisition_code)
    .bind(&view.requesting_department_code)
    .bind(&view.budget_code)
    .bind(&view.currency)
    .bind(requisition_status_name(view.status))
    .bind(serde_json::to_value(&view).map_err(failure)?)
    .bind(view.created_at)
    .bind(view.updated_at)
    .execute(&mut **transaction)
    .await
    .map_err(failure)?;
    if result.rows_affected() != 1 {
        return Err(ProjectionFailure::new(
            "procurement requisition snapshot version did not advance",
        ));
    }
    Ok(())
}

async fn project_goods_receipt(
    transaction: &mut Transaction<'_, Postgres>,
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    view: GoodsReceiptView,
) -> Result<(), ProjectionFailure> {
    let result = sqlx::query(
        "INSERT INTO erpx_procurement_read.goods_receipt_projection
            (holding_id,projection_name,generation,partition_key,goods_receipt_id,
             source_aggregate_version,source_global_sequence,active_scope,scope_key,
             receipt_code,source_purchase_order,receipt_status,state,
             aggregate_created_at,aggregate_updated_at,projected_at)
         VALUES (erpx_procurement_read.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,
                 $9,$10,$11,$12,$13,$14,now())
         ON CONFLICT (holding_id,projection_name,generation,partition_key,goods_receipt_id)
         DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
            source_global_sequence=EXCLUDED.source_global_sequence,
            active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,
            receipt_code=EXCLUDED.receipt_code,
            source_purchase_order=EXCLUDED.source_purchase_order,
            receipt_status=EXCLUDED.receipt_status,state=EXCLUDED.state,
            aggregate_created_at=EXCLUDED.aggregate_created_at,
            aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
         WHERE erpx_procurement_read.goods_receipt_projection.source_aggregate_version
               < EXCLUDED.source_aggregate_version",
    )
    .bind(WORKFLOW_PROJECTION_NAME)
    .bind(context.generation)
    .bind(&event.partition)
    .bind(view.goods_receipt_id.as_uuid())
    .bind(to_i64(view.version)?)
    .bind(to_i64(event.global_sequence)?)
    .bind(serde_json::to_value(&view.scope).map_err(failure)?)
    .bind(scope_key(&view.scope)?)
    .bind(&view.receipt_code)
    .bind(serde_json::to_value(&view.source_purchase_order).map_err(failure)?)
    .bind(goods_receipt_status_name(view.status))
    .bind(serde_json::to_value(&view).map_err(failure)?)
    .bind(view.created_at)
    .bind(view.updated_at)
    .execute(&mut **transaction)
    .await
    .map_err(failure)?;
    if result.rows_affected() != 1 {
        return Err(ProjectionFailure::new(
            "procurement goods-receipt snapshot version did not advance",
        ));
    }
    Ok(())
}

fn validate_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<ProcurementWorkflowSnapshot, ProjectionFailure> {
    if context.projection != WORKFLOW_PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "procurement workflow projector received incompatible metadata",
        ));
    }
    let payload: ProcurementWorkflowOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let ProcurementWorkflowOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "procurement workflow snapshot carried a non-snapshot payload",
        ));
    };
    let valid = match &snapshot {
        ProcurementWorkflowSnapshot::Requisition(view) => {
            event.aggregate_type == REQUISITION_AGGREGATE_TYPE
                && event.event_type == REQUISITION_SNAPSHOT_EVENT_TYPE
                && view.version == event.aggregate_version
                && view.requisition_id.as_uuid() == event.aggregate_id.as_uuid()
                && view.scope == event.active_scope
                && event.partition == requisition_partition(view.requisition_id)
        }
        ProcurementWorkflowSnapshot::GoodsReceipt(view) => {
            event.aggregate_type == GOODS_RECEIPT_AGGREGATE_TYPE
                && event.event_type == GOODS_RECEIPT_SNAPSHOT_EVENT_TYPE
                && view.version == event.aggregate_version
                && view.goods_receipt_id.as_uuid() == event.aggregate_id.as_uuid()
                && view.scope == event.active_scope
                && event.partition == goods_receipt_partition(view.goods_receipt_id)
        }
    };
    if !valid {
        return Err(ProjectionFailure::new(
            "procurement workflow snapshot metadata mismatch",
        ));
    }
    Ok(snapshot)
}

pub fn requisition_partition(id: erpx_procurement::PurchaseRequisitionId) -> String {
    format!("procurement.requisition:{}", id.as_uuid().simple())
}

pub fn goods_receipt_partition(id: erpx_procurement::GoodsReceiptId) -> String {
    format!("procurement.goods_receipt:{}", id.as_uuid().simple())
}

pub(crate) fn scope_key(scope: &erpx_kernel::AuthorityScope) -> Result<String, ProjectionFailure> {
    match scope {
        erpx_kernel::AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("company:{holding_id}:{company_id}")),
        erpx_kernel::AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!("branch:{holding_id}:{company_id}:{branch_id}")),
        _ => Err(ProjectionFailure::new(
            "procurement workflow requires company or branch scope",
        )),
    }
}

pub(crate) const fn requisition_status_name(status: PurchaseRequisitionStatus) -> &'static str {
    match status {
        PurchaseRequisitionStatus::Draft => "draft",
        PurchaseRequisitionStatus::Submitted => "submitted",
        PurchaseRequisitionStatus::Approved => "approved",
        PurchaseRequisitionStatus::Rejected => "rejected",
        PurchaseRequisitionStatus::Cancelled => "cancelled",
    }
}

pub(crate) const fn goods_receipt_status_name(status: GoodsReceiptStatus) -> &'static str {
    match status {
        GoodsReceiptStatus::PendingAcceptance => "pending_acceptance",
        GoodsReceiptStatus::Accepted => "accepted",
        GoodsReceiptStatus::Rejected => "rejected",
        GoodsReceiptStatus::Cancelled => "cancelled",
    }
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| ProjectionFailure::new("numeric value is too large"))
}

fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projector_contract_and_partitions_are_stable() {
        let projector = ProcurementWorkflowProjector::new();
        assert_eq!(projector.name(), WORKFLOW_PROJECTION_NAME);
        assert_eq!(
            projector.event_types(),
            &[
                REQUISITION_SNAPSHOT_EVENT_TYPE.to_owned(),
                GOODS_RECEIPT_SNAPSHOT_EVENT_TYPE.to_owned(),
            ]
        );
        let requisition =
            erpx_procurement::PurchaseRequisitionId::try_from_uuid(uuid::Uuid::from_u128(1))
                .unwrap();
        assert_eq!(
            requisition_partition(requisition),
            "procurement.requisition:00000000000000000000000000000001"
        );
        assert_eq!(
            goods_receipt_status_name(GoodsReceiptStatus::Accepted),
            "accepted"
        );
    }
}
