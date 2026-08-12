use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_crm::{CrmAggregate, CrmLeadView, OpportunityStage, OpportunityView};
use erpx_integration_contracts::crm;
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    CONTEXT_NAME, EVENT_VERSION, LEAD_AGGREGATE_TYPE, LEAD_PROJECTION, LEAD_SNAPSHOT_EVENT,
    OPPORTUNITY_AGGREGATE_TYPE, OPPORTUNITY_PROJECTION, OPPORTUNITY_SNAPSHOT_EVENT,
    command::scope_key, lead_partition, opportunity_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "value", rename_all = "snake_case")]
pub enum CrmSnapshot {
    Lead(CrmLeadView),
    Opportunity(OpportunityView),
}
impl From<&CrmAggregate> for CrmSnapshot {
    fn from(value: &CrmAggregate) -> Self {
        match value {
            CrmAggregate::Lead(value) => Self::Lead(value.into()),
            CrmAggregate::Opportunity(value) => Self::Opportunity(value.into()),
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CrmOutboxPayload {
    Integration(crm::Event),
    Snapshot(CrmSnapshot),
}

#[derive(Debug, Clone)]
pub struct CrmLeadProjector {
    events: Vec<String>,
}
impl CrmLeadProjector {
    pub fn new() -> Self {
        Self {
            events: vec![LEAD_SNAPSHOT_EVENT.into()],
        }
    }
}
impl Default for CrmLeadProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for CrmLeadProjector {
    fn name(&self) -> &str {
        LEAD_PROJECTION
    }
    fn event_types(&self) -> &[String] {
        &self.events
    }
    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let snapshot = validate_event(
            context,
            event,
            LEAD_PROJECTION,
            LEAD_AGGREGATE_TYPE,
            LEAD_SNAPSHOT_EVENT,
        )?;
        let CrmSnapshot::Lead(view) = snapshot else {
            return Err(ProjectionFailure::new(
                "CRM lead projector received opportunity",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_crm_read.lead_projection (projection_name,generation,partition_key,lead_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,marketing_candidate_id,customer_id,source,qualification_score_basis_points,accepted_at,state,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,now()) ON CONFLICT (projection_name,generation,partition_key,lead_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,marketing_candidate_id=EXCLUDED.marketing_candidate_id,customer_id=EXCLUDED.customer_id,source=EXCLUDED.source,qualification_score_basis_points=EXCLUDED.qualification_score_basis_points,accepted_at=EXCLUDED.accepted_at,state=EXCLUDED.state,projected_at=now() WHERE erpx_crm_read.lead_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(LEAD_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?).bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?).bind(view.marketing_candidate_id).bind(view.customer_id).bind(&view.source).bind(i32::from(view.qualification_score_basis_points)).bind(view.accepted_at).bind(state).execute(&mut **transaction).await.map_err(failure)?;
        Ok(())
    }
    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query(
            "DELETE FROM erpx_crm_read.lead_projection WHERE projection_name=$1 AND generation=$2",
        )
        .bind(LEAD_PROJECTION)
        .bind(context.generation)
        .execute(&mut **transaction)
        .await
        .map_err(failure)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct CrmOpportunityProjector {
    events: Vec<String>,
}
impl CrmOpportunityProjector {
    pub fn new() -> Self {
        Self {
            events: vec![OPPORTUNITY_SNAPSHOT_EVENT.into()],
        }
    }
}
impl Default for CrmOpportunityProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for CrmOpportunityProjector {
    fn name(&self) -> &str {
        OPPORTUNITY_PROJECTION
    }
    fn event_types(&self) -> &[String] {
        &self.events
    }
    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
        event: &ProjectionEvent,
    ) -> Result<(), ProjectionFailure> {
        let snapshot = validate_event(
            context,
            event,
            OPPORTUNITY_PROJECTION,
            OPPORTUNITY_AGGREGATE_TYPE,
            OPPORTUNITY_SNAPSHOT_EVENT,
        )?;
        let CrmSnapshot::Opportunity(view) = snapshot else {
            return Err(ProjectionFailure::new(
                "CRM opportunity projector received lead",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_crm_read.opportunity_projection (projection_name,generation,partition_key,opportunity_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,accepted_lead_id,customer_id,name,expected_value_minor,currency,opportunity_stage,account_id,owner_ref,pipeline_category,probability_basis_points,expected_close_on,originating_campaign_id,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,now()) ON CONFLICT (projection_name,generation,partition_key,opportunity_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,accepted_lead_id=EXCLUDED.accepted_lead_id,customer_id=EXCLUDED.customer_id,name=EXCLUDED.name,expected_value_minor=EXCLUDED.expected_value_minor,currency=EXCLUDED.currency,opportunity_stage=EXCLUDED.opportunity_stage,account_id=EXCLUDED.account_id,owner_ref=EXCLUDED.owner_ref,pipeline_category=EXCLUDED.pipeline_category,probability_basis_points=EXCLUDED.probability_basis_points,expected_close_on=EXCLUDED.expected_close_on,originating_campaign_id=EXCLUDED.originating_campaign_id,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_crm_read.opportunity_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(OPPORTUNITY_PROJECTION).bind(context.generation).bind(&event.partition).bind(view.id.as_uuid()).bind(to_i64(view.version)?).bind(to_i64(event.global_sequence)?).bind(serde_json::to_value(&view.scope).map_err(failure)?).bind(scope_key(&view.scope).map_err(failure)?).bind(view.accepted_lead_id.as_uuid()).bind(view.customer_id).bind(&view.name).bind(view.expected_value_minor).bind(&view.currency).bind(stage_name(view.stage)).bind(view.account.map(|v|v.as_uuid())).bind(&view.owner).bind(view.pipeline_category.map(pipeline_category_name)).bind(view.probability_basis_points.map(i32::from)).bind(view.expected_close_on).bind(view.originating_campaign.map(|v|v.as_uuid())).bind(state).bind(view.created_at).bind(view.updated_at).execute(&mut **transaction).await.map_err(failure)?;
        Ok(())
    }
    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query("DELETE FROM erpx_crm_read.opportunity_projection WHERE projection_name=$1 AND generation=$2").bind(OPPORTUNITY_PROJECTION).bind(context.generation).execute(&mut **transaction).await.map_err(failure)?;
        Ok(())
    }
}

fn validate_event(
    context: &ProjectorContext<'_>,
    event: &ProjectionEvent,
    projection: &str,
    aggregate_type: &str,
    event_type: &str,
) -> Result<CrmSnapshot, ProjectionFailure> {
    if context.projection != projection
        || event.context != CONTEXT_NAME
        || event.aggregate_type != aggregate_type
        || event.event_type != event_type
        || event.event_version != EVENT_VERSION
    {
        return Err(ProjectionFailure::new(
            "CRM projector received incompatible event",
        ));
    }
    let payload: CrmOutboxPayload =
        serde_json::from_value(event.payload.clone()).map_err(failure)?;
    let CrmOutboxPayload::Snapshot(snapshot) = payload else {
        return Err(ProjectionFailure::new(
            "CRM snapshot event carried integration payload",
        ));
    };
    let matches = match &snapshot {
        CrmSnapshot::Lead(value) => {
            value.version == event.aggregate_version
                && value.id.as_uuid() == event.aggregate_id.as_uuid()
                && value.scope == event.tenant_scope
                && event.partition == lead_partition(value.id)
                && aggregate_type == LEAD_AGGREGATE_TYPE
        }
        CrmSnapshot::Opportunity(value) => {
            value.version == event.aggregate_version
                && value.id.as_uuid() == event.aggregate_id.as_uuid()
                && value.scope == event.tenant_scope
                && event.partition == opportunity_partition(value.id)
                && aggregate_type == OPPORTUNITY_AGGREGATE_TYPE
        }
    };
    if !matches {
        return Err(ProjectionFailure::new("CRM snapshot metadata mismatch"));
    }
    Ok(snapshot)
}
pub(crate) const fn stage_name(value: OpportunityStage) -> &'static str {
    match value {
        OpportunityStage::Qualified => "qualified",
        OpportunityStage::Discovery => "discovery",
        OpportunityStage::Proposal => "proposal",
        OpportunityStage::Negotiation => "negotiation",
        OpportunityStage::Won => "won",
        OpportunityStage::Lost => "lost",
    }
}
pub(crate) const fn pipeline_category_name(
    value: erpx_integration_contracts::crm::PipelineCategory,
) -> &'static str {
    match value {
        erpx_integration_contracts::crm::PipelineCategory::Pipeline => "pipeline",
        erpx_integration_contracts::crm::PipelineCategory::BestCase => "best_case",
        erpx_integration_contracts::crm::PipelineCategory::Commit => "commit",
        erpx_integration_contracts::crm::PipelineCategory::Closed => "closed",
    }
}
fn to_i64(value: u64) -> Result<i64, ProjectionFailure> {
    i64::try_from(value).map_err(|_| ProjectionFailure::new("CRM numeric value is too large"))
}
fn failure(error: impl std::fmt::Display) -> ProjectionFailure {
    ProjectionFailure::new(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn projectors_are_snapshot_specific() {
        let lead = CrmLeadProjector::new();
        let opportunity = CrmOpportunityProjector::new();
        assert_eq!(lead.event_types(), &[LEAD_SNAPSHOT_EVENT.to_owned()]);
        assert_eq!(
            opportunity.event_types(),
            &[OPPORTUNITY_SNAPSHOT_EVENT.to_owned()]
        );
        assert_eq!(stage_name(OpportunityStage::Discovery), "discovery");
    }
}
