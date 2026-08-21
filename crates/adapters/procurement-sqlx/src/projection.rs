use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraftV2, procurement};
use erpx_procurement::{PurchaseOrderStatus, PurchaseOrderView};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, PROJECTION_NAME, SNAPSHOT_EVENT_TYPE,
    purchase_order_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcurementSnapshot {
    pub order: PurchaseOrderView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ProcurementOutboxPayload {
    Integration(procurement::Event),
    Snapshot(ProcurementSnapshot),
    Directory(ItemDescriptorDraftV2),
}

#[derive(Debug, Clone)]
pub struct ProcurementProjector {
    event_types: Vec<String>,
}

impl ProcurementProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl Default for ProcurementProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for ProcurementProjector {
    fn name(&self) -> &str {
        PROJECTION_NAME
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
        let snapshot = validate_event(context, event)?;
        let view = snapshot.order;
        let scope_key = scope_key(&view.scope)?;
        let state = serde_json::to_value(&view).map_err(failure)?;
        let lines = serde_json::to_value(&view.lines).map_err(failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_procurement_read.purchase_order_projection
                (projection_name,generation,partition_key,purchase_order_id,
                 source_aggregate_version,source_global_sequence,active_scope,scope_key,
                 order_code,supplier_id,currency,order_status,lines,state,
                 aggregate_created_at,aggregate_updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,now())
             ON CONFLICT (holding_id,projection_name,generation,partition_key,purchase_order_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                source_global_sequence=EXCLUDED.source_global_sequence,
                active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,
                order_code=EXCLUDED.order_code,supplier_id=EXCLUDED.supplier_id,
                currency=EXCLUDED.currency,order_status=EXCLUDED.order_status,
                lines=EXCLUDED.lines,state=EXCLUDED.state,
                aggregate_created_at=EXCLUDED.aggregate_created_at,
                aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_procurement_read.purchase_order_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.purchase_order_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(serde_json::to_value(&view.scope).map_err(failure)?)
        .bind(scope_key)
        .bind(&view.order_code)
        .bind(view.supplier_id.as_uuid())
        .bind(&view.currency)
        .bind(status_name(view.status))
        .bind(lines)
        .bind(state)
        .bind(view.created_at)
        .bind(view.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "procurement snapshot version did not advance",
            ));
        }
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query(
            "DELETE FROM erpx_procurement_read.purchase_order_projection
             WHERE projection_name=$1 AND generation=$2",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        Ok(())
    }
}

fn validate_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<ProcurementSnapshot, ProjectionFailure> {
    if context.projection != PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.aggregate_type != AGGREGATE_TYPE
        || event.event_type != SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "procurement projector received an incompatible event",
        ));
    }
    let payload: ProcurementOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let ProcurementOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "procurement snapshot carried an integration payload",
        ));
    };
    let view = &snapshot.order;
    if view.version != event.aggregate_version
        || view.purchase_order_id.as_uuid() != event.aggregate_id.as_uuid()
        || view.scope != event.active_scope
        || event.partition != purchase_order_partition(view.purchase_order_id)
        || scope_key(&view.scope).is_err()
    {
        return Err(ProjectionFailure::new(
            "procurement snapshot metadata mismatch",
        ));
    }
    Ok(snapshot)
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
            "procurement requires an exact holding scope",
        )),
    }
}

pub(crate) const fn status_name(status: PurchaseOrderStatus) -> &'static str {
    match status {
        PurchaseOrderStatus::Draft => "draft",
        PurchaseOrderStatus::Released => "released",
        PurchaseOrderStatus::Cancelled => "cancelled",
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
    fn projector_consumes_only_private_snapshots() {
        let projector = ProcurementProjector::new();
        assert_eq!(projector.name(), PROJECTION_NAME);
        assert_eq!(projector.event_types(), &[SNAPSHOT_EVENT_TYPE.to_owned()]);
        assert_eq!(status_name(PurchaseOrderStatus::Released), "released");
    }
}
