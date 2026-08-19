use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::{ItemDescriptorDraft, quality};
use erpx_quality::{
    application::InspectionView,
    domain::{InspectionKind, InspectionStatus},
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, PROJECTION_NAME, SNAPSHOT_EVENT_TYPE,
    inspection_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualitySnapshot {
    pub inspection: InspectionView,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum QualityOutboxPayload {
    Integration(quality::Event),
    Snapshot(QualitySnapshot),
    Directory(ItemDescriptorDraft),
}

#[derive(Debug, Clone)]
pub struct QualityInspectionProjector {
    event_types: Vec<String>,
}

impl QualityInspectionProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl Default for QualityInspectionProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for QualityInspectionProjector {
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
        let view = snapshot.inspection;
        let (holding_id, company_id, branch_id) = scope_ids(&view)?;
        let state = serde_json::to_value(&view).map_err(projection_failure)?;
        let result = sqlx::query(
            "INSERT INTO erpx_quality_read.inspection_projection
                (projection_name, generation, partition_key, inspection_id,
                 source_aggregate_version, source_global_sequence, reference,
                 active_scope, holding_id, company_id, branch_id, inspection_kind,
                 product_id, lot_reference, quantity, passed_quantity, failed_quantity,
                 inspection_status, disposition_note, aggregate_created_at,
                 aggregate_updated_at, state, projected_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12,
                     $13, $14, CAST($15 AS NUMERIC), CAST($16 AS NUMERIC),
                     CAST($17 AS NUMERIC), $18, $19, $20, $21, $22, now())
             ON CONFLICT (holding_id,projection_name, generation, partition_key, inspection_id)
             DO UPDATE SET source_aggregate_version = EXCLUDED.source_aggregate_version,
                           source_global_sequence = EXCLUDED.source_global_sequence,
                           reference = EXCLUDED.reference,
                           active_scope = EXCLUDED.active_scope,
                           holding_id = EXCLUDED.holding_id,
                           company_id = EXCLUDED.company_id,
                           branch_id = EXCLUDED.branch_id,
                           inspection_kind = EXCLUDED.inspection_kind,
                           product_id = EXCLUDED.product_id,
                           lot_reference = EXCLUDED.lot_reference,
                           quantity = EXCLUDED.quantity,
                           passed_quantity = EXCLUDED.passed_quantity,
                           failed_quantity = EXCLUDED.failed_quantity,
                           inspection_status = EXCLUDED.inspection_status,
                           disposition_note = EXCLUDED.disposition_note,
                           aggregate_created_at = EXCLUDED.aggregate_created_at,
                           aggregate_updated_at = EXCLUDED.aggregate_updated_at,
                           state = EXCLUDED.state,
                           projected_at = now()
             WHERE erpx_quality_read.inspection_projection.source_aggregate_version
                   < EXCLUDED.source_aggregate_version",
        )
        .bind(PROJECTION_NAME)
        .bind(context.generation)
        .bind(&event.partition)
        .bind(view.inspection_id.as_uuid())
        .bind(to_i64(view.version)?)
        .bind(to_i64(event.global_sequence)?)
        .bind(&view.reference)
        .bind(serde_json::to_value(&view.scope).map_err(projection_failure)?)
        .bind(holding_id)
        .bind(company_id)
        .bind(branch_id)
        .bind(kind_name(view.kind))
        .bind(view.product_id)
        .bind(&view.lot_reference)
        .bind(view.quantity.to_string())
        .bind(view.passed_quantity.to_string())
        .bind(view.failed_quantity.to_string())
        .bind(status_name(view.status))
        .bind(&view.disposition_note)
        .bind(view.created_at)
        .bind(view.updated_at)
        .bind(state)
        .execute(&mut **transaction)
        .await
        .map_err(projection_failure)?;
        if result.rows_affected() != 1 {
            return Err(ProjectionFailure::new(
                "quality snapshot aggregate version did not advance",
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
            "DELETE FROM erpx_quality_read.inspection_projection
             WHERE projection_name = $1 AND generation = $2",
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
) -> Result<QualitySnapshot, ProjectionFailure> {
    if context.projection != PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.aggregate_type != AGGREGATE_TYPE
        || event.event_type != SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "quality projector received an incompatible event contract",
        ));
    }
    let payload: QualityOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(projection_failure)?;
    let QualityOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "quality snapshot event carried an integration payload",
        ));
    };
    let view = &snapshot.inspection;
    if view.version != event.aggregate_version
        || view.inspection_id.as_uuid() != event.aggregate_id.as_uuid()
        || view.scope != event.active_scope
        || event.partition != inspection_partition(view.inspection_id)
        || scope_ids(view).is_err()
    {
        return Err(ProjectionFailure::new(
            "quality snapshot metadata does not match its outbox envelope",
        ));
    }
    Ok(snapshot)
}

fn scope_ids(
    view: &InspectionView,
) -> Result<(uuid::Uuid, uuid::Uuid, Option<uuid::Uuid>), ProjectionFailure> {
    match &view.scope {
        erpx_kernel::AuthorityScope::Company {
            holding_id,
            company_id,
        } => Ok((*holding_id, *company_id, None)),
        erpx_kernel::AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok((*holding_id, *company_id, Some(*branch_id))),
        _ => Err(ProjectionFailure::new(
            "quality snapshot uses a non-exact holding scope",
        )),
    }
}

const fn kind_name(kind: InspectionKind) -> &'static str {
    match kind {
        InspectionKind::Incoming => "incoming",
        InspectionKind::InProcess => "in_process",
        InspectionKind::Final => "final",
    }
}

pub(crate) const fn status_name(status: InspectionStatus) -> &'static str {
    match status {
        InspectionStatus::Draft => "draft",
        InspectionStatus::InProgress => "in_progress",
        InspectionStatus::Passed => "passed",
        InspectionStatus::Failed => "failed",
        InspectionStatus::Released => "released",
        InspectionStatus::Cancelled => "cancelled",
    }
}

fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| ProjectionFailure::new("quality numeric value is too large"))
}

fn projection_failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn projector_consumes_only_private_snapshot_events() {
        let projector = QualityInspectionProjector::new();
        assert_eq!(projector.name(), PROJECTION_NAME);
        assert_eq!(projector.event_types(), &[SNAPSHOT_EVENT_TYPE.to_owned()]);
        assert!(
            !projector
                .event_types()
                .iter()
                .any(|event| event == "quality.inspection_completed")
        );
    }

    #[test]
    fn database_enum_names_are_stable() {
        assert_eq!(kind_name(InspectionKind::InProcess), "in_process");
        assert_eq!(status_name(InspectionStatus::Released), "released");
    }
}
