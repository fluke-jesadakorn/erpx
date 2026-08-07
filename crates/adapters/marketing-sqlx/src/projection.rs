use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::marketing;
use erpx_marketing::{LeadCandidateStatus, LeadCandidateView};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, PROJECTION_NAME, SNAPSHOT_EVENT_TYPE,
    command::scope_key, lead_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MarketingSnapshot {
    pub candidate: LeadCandidateView,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MarketingOutboxPayload {
    Integration(marketing::Event),
    Snapshot(MarketingSnapshot),
}

#[derive(Debug, Clone)]
pub struct MarketingLeadProjector {
    event_types: Vec<String>,
}
impl MarketingLeadProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}
impl Default for MarketingLeadProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for MarketingLeadProjector {
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
        let view = snapshot.candidate;
        let state = serde_json::to_value(&view).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_marketing_read.lead_candidate_projection (projection_name,generation,partition_key,lead_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,campaign_id,audience_member_id,source,existing_customer_id,score_basis_points,lead_status,engagement_count,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,now()) ON CONFLICT (projection_name,generation,partition_key,lead_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,campaign_id=EXCLUDED.campaign_id,audience_member_id=EXCLUDED.audience_member_id,source=EXCLUDED.source,existing_customer_id=EXCLUDED.existing_customer_id,score_basis_points=EXCLUDED.score_basis_points,lead_status=EXCLUDED.lead_status,engagement_count=EXCLUDED.engagement_count,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_marketing_read.lead_candidate_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(PROJECTION_NAME).bind(context.generation).bind(&event.partition).bind(view.id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?).bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?).bind(view.campaign_id.as_uuid()).bind(view.audience_member_id.as_uuid()).bind(&view.source).bind(view.existing_customer_id).bind(view.score_basis_points.map(i32::from)).bind(status_name(view.status)).bind(to_i64(view.engagement_count)?).bind(state).bind(view.created_at).bind(view.updated_at).execute(&mut **transaction).await.map_err(failure)?;
        Ok(())
    }
    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query("DELETE FROM erpx_marketing_read.lead_candidate_projection WHERE projection_name=$1 AND generation=$2").bind(PROJECTION_NAME).bind(context.generation).execute(&mut **transaction).await.map_err(failure)?;
        Ok(())
    }
}

fn validate_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
) -> Result<MarketingSnapshot, ProjectionFailure> {
    if context.projection != PROJECTION_NAME
        || event.context != CONTEXT_NAME
        || event.aggregate_type != AGGREGATE_TYPE
        || event.event_type != SNAPSHOT_EVENT_TYPE
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "Marketing projector received incompatible event",
        ));
    }
    let payload: MarketingOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let MarketingOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "Marketing snapshot event carried integration payload",
        ));
    };
    let view = &snapshot.candidate;
    if view.version != event.aggregate_version
        || view.id.as_uuid() != event.aggregate_id.as_uuid()
        || view.scope != event.tenant_scope
        || event.partition != lead_partition(view.id)
        || scope_key(&view.scope).is_err()
    {
        return Err(ProjectionFailure::new(
            "Marketing snapshot metadata mismatch",
        ));
    }
    Ok(snapshot)
}
pub(crate) const fn status_name(value: LeadCandidateStatus) -> &'static str {
    match value {
        LeadCandidateStatus::Captured => "open",
        LeadCandidateStatus::Qualified => "qualified",
        LeadCandidateStatus::Rejected => "rejected",
    }
}
fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| ProjectionFailure::new("Marketing numeric value is too large"))
}
fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projector_is_private_snapshot_only() {
        let projector = MarketingLeadProjector::new();
        assert_eq!(projector.name(), PROJECTION_NAME);
        assert_eq!(projector.event_types(), &[SNAPSHOT_EVENT_TYPE.to_owned()]);
    }
}
