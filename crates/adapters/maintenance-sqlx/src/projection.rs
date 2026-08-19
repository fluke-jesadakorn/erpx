use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraft, assets, maintenance};
use erpx_kernel::AuthorityScope;
use erpx_maintenance::{WorkOrderStatus, WorkOrderView};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, PROJECTION_NAME, SNAPSHOT_EVENT_TYPE,
    work_order_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceSnapshot {
    pub work_order: WorkOrderView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
// This public serialized adapter payload is also constructed by command writers;
// boxing one variant would ripple through that API for no wire-size reduction.
#[allow(clippy::large_enum_variant)]
pub enum MaintenanceOutboxPayload {
    Integration(maintenance::Event),
    Snapshot(MaintenanceSnapshot),
    Directory(ItemDescriptorDraft),
}

#[derive(Debug, Clone)]
pub struct WorkOrderProjector {
    event_types: Vec<String>,
}

impl WorkOrderProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl Default for WorkOrderProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for WorkOrderProjector {
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
        let view = snapshot.work_order;
        let state = serde_json::to_value(&view).map_err(projection_failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_maintenance_read.work_order_projection
                (projection_name,generation,partition_key,work_order_id,
                 source_aggregate_version,source_global_sequence,active_scope,
                 scope_key,work_order_code,asset_id,request_code,source_event_id,
                 source_asset_version,title,work_order_status,scheduled_for,
                 started_at,completed_at,state,aggregate_created_at,
                 aggregate_updated_at,projected_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,
                     $16,$17,$18,$19,$20,$21,now())
             ON CONFLICT (holding_id,projection_name,generation,partition_key,work_order_id)
             DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,
                source_global_sequence=EXCLUDED.source_global_sequence,
                active_scope=EXCLUDED.active_scope,scope_key=EXCLUDED.scope_key,
                work_order_code=EXCLUDED.work_order_code,asset_id=EXCLUDED.asset_id,
                request_code=EXCLUDED.request_code,source_event_id=EXCLUDED.source_event_id,
                source_asset_version=EXCLUDED.source_asset_version,title=EXCLUDED.title,
                work_order_status=EXCLUDED.work_order_status,
                scheduled_for=EXCLUDED.scheduled_for,started_at=EXCLUDED.started_at,
                completed_at=EXCLUDED.completed_at,state=EXCLUDED.state,
                aggregate_created_at=EXCLUDED.aggregate_created_at,
                aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now()
             WHERE erpx_maintenance_read.work_order_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.work_order_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(serde_json::to_value(&view.scope).map_err(projection_failure)?)
        .bind(scope_key(&view.scope)?)
        .bind(&view.code)
        .bind(view.asset_fact.asset.as_uuid())
        .bind(&view.request.request_code)
        .bind(view.request.source_event_id.as_uuid())
        .bind(to_i64(view.request.source_aggregate_version)?)
        .bind(&view.title)
        .bind(status_name(view.status))
        .bind(view.scheduled_for)
        .bind(view.started_at)
        .bind(view.completed_at)
        .bind(state)
        .bind(view.created_at)
        .bind(view.updated_at)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "Maintenance snapshot aggregate version did not advance",
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
            "DELETE FROM erpx_maintenance_read.work_order_projection
             WHERE projection_name=$1 AND generation=$2",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        Ok(())
    }
}

fn validate_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<MaintenanceSnapshot, ProjectionFailure> {
    if context.projection != PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.aggregate_type != AGGREGATE_TYPE
        || event.event_type != SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "Maintenance projector received an incompatible event contract",
        ));
    }
    let payload: MaintenanceOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(projection_failure)?;
    let MaintenanceOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "Maintenance snapshot event carried an integration payload",
        ));
    };
    let view = &snapshot.work_order;
    if view.version != event.aggregate_version
        || view.work_order_id.as_uuid() != event.aggregate_id.as_uuid()
        || view.scope != event.active_scope
        || event.partition != work_order_partition(view.work_order_id)
        || scope_key(&view.scope).is_err()
    {
        return Err(ProjectionFailure::new(
            "Maintenance snapshot metadata does not match its envelope",
        ));
    }
    Ok(snapshot)
}

pub(crate) fn scope_key(scope: &AuthorityScope) -> Result<String, ProjectionFailure> {
    match scope {
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok(format!("holding:{holding_id}/company:{company_id}")),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok(format!(
            "holding:{holding_id}/company:{company_id}/branch:{branch_id}"
        )),
        _ => Err(ProjectionFailure::new(
            "Maintenance snapshot uses a non-exact scope",
        )),
    }
}

pub(crate) const fn status_name(status: WorkOrderStatus) -> &'static str {
    match status {
        WorkOrderStatus::Planned => "planned",
        WorkOrderStatus::Scheduled => "scheduled",
        WorkOrderStatus::InProgress => "in_progress",
        WorkOrderStatus::Blocked => "blocked",
        WorkOrderStatus::Completed => "completed",
        WorkOrderStatus::Cancelled => "cancelled",
    }
}

pub(crate) const fn condition_name(condition: assets::AssetCondition) -> &'static str {
    match condition {
        assets::AssetCondition::Operational => "operational",
        assets::AssetCondition::Degraded => "degraded",
        assets::AssetCondition::OutOfService => "out_of_service",
        assets::AssetCondition::Retired => "retired",
    }
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value)
        .map_err(|_| ProjectionFailure::new("Maintenance numeric value is too large"))
}

fn projection_failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use erpx_cqrs_sqlx::Projector;

    use super::*;

    #[test]
    fn projector_consumes_private_snapshots_only() {
        let projector = WorkOrderProjector::new();
        assert_eq!(projector.name(), PROJECTION_NAME);
        assert_eq!(projector.event_types(), &[SNAPSHOT_EVENT_TYPE.to_owned()]);
    }

    #[test]
    fn frozen_status_names_are_stable() {
        assert_eq!(status_name(WorkOrderStatus::InProgress), "in_progress");
        assert_eq!(condition_name(assets::AssetCondition::Retired), "retired");
    }
}
