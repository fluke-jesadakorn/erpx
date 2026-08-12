use async_trait::async_trait;
use erpx_cqrs_sqlx::{ProjectionEvent, ProjectionFailure, Projector, ProjectorContext};
use erpx_integration_contracts::marketing;
use erpx_marketing::{
    CampaignBudgetStatus, CampaignBudgetView, LeadCandidateStatus, LeadCandidateView,
    MarketingAggregate,
};
use serde::{Deserialize, Serialize};
use sqlx::{Postgres, Transaction};

use crate::{
    BUDGET_AGGREGATE_TYPE, BUDGET_PROJECTION_NAME, BUDGET_SNAPSHOT_EVENT_TYPE, CONTEXT_NAME,
    EVENT_VERSION, LEAD_AGGREGATE_TYPE, LEAD_PROJECTION_NAME, LEAD_SNAPSHOT_EVENT_TYPE,
    budget_partition, command::scope_key, lead_partition,
};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum MarketingSnapshot {
    Lead { candidate: LeadCandidateView },
    CampaignBudget { budget: Box<CampaignBudgetView> },
}

impl From<&MarketingAggregate> for MarketingSnapshot {
    fn from(value: &MarketingAggregate) -> Self {
        match value {
            MarketingAggregate::LeadCandidate(value) => Self::Lead {
                candidate: value.as_ref().into(),
            },
            MarketingAggregate::CampaignBudget(value) => Self::CampaignBudget {
                budget: Box::new(
                    CampaignBudgetView::try_from(value.as_ref())
                        .expect("validated Marketing budget view"),
                ),
            },
        }
    }
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
            event_types: vec![LEAD_SNAPSHOT_EVENT_TYPE.into()],
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
        LEAD_PROJECTION_NAME
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
        let snapshot = validate_event(
            context,
            event,
            LEAD_PROJECTION_NAME,
            LEAD_AGGREGATE_TYPE,
            LEAD_SNAPSHOT_EVENT_TYPE,
        )?;
        let MarketingSnapshot::Lead { candidate: view } = snapshot else {
            return Err(ProjectionFailure::new(
                "Marketing lead projector received campaign budget",
            ));
        };
        let state = serde_json::to_value(&view).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_marketing_read.lead_candidate_projection (projection_name,generation,partition_key,lead_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,campaign_id,audience_member_id,source,existing_customer_id,score_basis_points,lead_status,engagement_count,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,now()) ON CONFLICT (projection_name,generation,partition_key,lead_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,campaign_id=EXCLUDED.campaign_id,audience_member_id=EXCLUDED.audience_member_id,source=EXCLUDED.source,existing_customer_id=EXCLUDED.existing_customer_id,score_basis_points=EXCLUDED.score_basis_points,lead_status=EXCLUDED.lead_status,engagement_count=EXCLUDED.engagement_count,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_marketing_read.lead_candidate_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(LEAD_PROJECTION_NAME)
            .bind(context.generation)
            .bind(&event.partition)
            .bind(view.id.as_uuid())
            .bind(to_i64(view.version)?)
            .bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?)
            .bind(scope_key(&view.scope).map_err(failure)?)
            .bind(view.campaign_id.as_uuid())
            .bind(view.audience_member_id.as_uuid())
            .bind(&view.source)
            .bind(view.existing_customer_id)
            .bind(view.score_basis_points.map(i32::from))
            .bind(lead_status_name(view.status))
            .bind(to_i64(view.engagement_count)?)
            .bind(state)
            .bind(view.created_at)
            .bind(view.updated_at)
            .execute(&mut **transaction)
            .await
            .map_err(failure)?;
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query("DELETE FROM erpx_marketing_read.lead_candidate_projection WHERE projection_name=$1 AND generation=$2")
            .bind(LEAD_PROJECTION_NAME)
            .bind(context.generation)
            .execute(&mut **transaction)
            .await
            .map_err(failure)?;
        Ok(())
    }
}

#[derive(Debug, Clone)]
pub struct MarketingBudgetProjector {
    event_types: Vec<String>,
}

impl MarketingBudgetProjector {
    pub fn new() -> Self {
        Self {
            event_types: vec![BUDGET_SNAPSHOT_EVENT_TYPE.into()],
        }
    }
}

impl Default for MarketingBudgetProjector {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl Projector for MarketingBudgetProjector {
    fn name(&self) -> &str {
        BUDGET_PROJECTION_NAME
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
        let snapshot = validate_event(
            context,
            event,
            BUDGET_PROJECTION_NAME,
            BUDGET_AGGREGATE_TYPE,
            BUDGET_SNAPSHOT_EVENT_TYPE,
        )?;
        let MarketingSnapshot::CampaignBudget { budget } = snapshot else {
            return Err(ProjectionFailure::new(
                "Marketing budget projector received lead candidate",
            ));
        };
        let view = *budget;
        let state = serde_json::to_value(&view).map_err(failure)?;
        sqlx::query("INSERT INTO erpx_marketing_read.campaign_budget_projection (projection_name,generation,partition_key,budget_id,source_aggregate_version,source_global_sequence,tenant_scope,scope_key,campaign_id,budget_code,budget_name,period,budget_status,currency,requested_minor,approved_minor,reserved_minor,available_minor,dimensions,allocations,reservations,state,aggregate_created_at,aggregate_updated_at,projected_at) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,$18,$19,$20,$21,$22,$23,$24,now()) ON CONFLICT (projection_name,generation,partition_key,budget_id) DO UPDATE SET source_aggregate_version=EXCLUDED.source_aggregate_version,source_global_sequence=EXCLUDED.source_global_sequence,tenant_scope=EXCLUDED.tenant_scope,scope_key=EXCLUDED.scope_key,campaign_id=EXCLUDED.campaign_id,budget_code=EXCLUDED.budget_code,budget_name=EXCLUDED.budget_name,period=EXCLUDED.period,budget_status=EXCLUDED.budget_status,currency=EXCLUDED.currency,requested_minor=EXCLUDED.requested_minor,approved_minor=EXCLUDED.approved_minor,reserved_minor=EXCLUDED.reserved_minor,available_minor=EXCLUDED.available_minor,dimensions=EXCLUDED.dimensions,allocations=EXCLUDED.allocations,reservations=EXCLUDED.reservations,state=EXCLUDED.state,aggregate_created_at=EXCLUDED.aggregate_created_at,aggregate_updated_at=EXCLUDED.aggregate_updated_at,projected_at=now() WHERE erpx_marketing_read.campaign_budget_projection.source_aggregate_version < EXCLUDED.source_aggregate_version")
            .bind(BUDGET_PROJECTION_NAME)
            .bind(context.generation)
            .bind(&event.partition)
            .bind(view.id.as_uuid())
            .bind(to_i64(view.version)?)
            .bind(to_i64(event.global_sequence)?)
            .bind(serde_json::to_value(&view.scope).map_err(failure)?)
            .bind(scope_key(&view.scope).map_err(failure)?)
            .bind(view.campaign_id.as_uuid())
            .bind(&view.code)
            .bind(&view.name)
            .bind(budget_period_name(view.period))
            .bind(budget_status_name(view.status))
            .bind(view.requested_amount.currency().as_str())
            .bind(view.requested_amount.minor_units())
            .bind(view.approved_amount.map(|amount| amount.minor_units()))
            .bind(view.reserved_minor)
            .bind(view.available_minor)
            .bind(serde_json::to_value(&view.dimensions).map_err(failure)?)
            .bind(serde_json::to_value(&view.allocations).map_err(failure)?)
            .bind(serde_json::to_value(&view.reservations).map_err(failure)?)
            .bind(state)
            .bind(view.created_at)
            .bind(view.updated_at)
            .execute(&mut **transaction)
            .await
            .map_err(failure)?;
        Ok(())
    }

    async fn reset(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &ProjectorContext<'_>,
    ) -> Result<(), ProjectionFailure> {
        sqlx::query("DELETE FROM erpx_marketing_read.campaign_budget_projection WHERE projection_name=$1 AND generation=$2")
            .bind(BUDGET_PROJECTION_NAME)
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
    projection: &str,
    aggregate_type: &str,
    event_type: &str,
) -> Result<MarketingSnapshot, ProjectionFailure> {
    if context.projection != projection
        || event.context != CONTEXT_NAME
        || event.aggregate_type != aggregate_type
        || event.event_type != event_type
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
    let matches = match &snapshot {
        MarketingSnapshot::Lead { candidate: value } => {
            value.version == event.aggregate_version
                && value.id.as_uuid() == event.aggregate_id.as_uuid()
                && value.scope == event.tenant_scope
                && event.partition == lead_partition(value.id)
                && aggregate_type == LEAD_AGGREGATE_TYPE
                && scope_key(&value.scope).is_ok()
        }
        MarketingSnapshot::CampaignBudget { budget: value } => {
            value.version == event.aggregate_version
                && value.id.as_uuid() == event.aggregate_id.as_uuid()
                && value.scope == event.tenant_scope
                && event.partition == budget_partition(value.id)
                && aggregate_type == BUDGET_AGGREGATE_TYPE
                && scope_key(&value.scope).is_ok()
        }
    };
    if !matches {
        return Err(ProjectionFailure::new(
            "Marketing snapshot metadata mismatch",
        ));
    }
    Ok(snapshot)
}

pub(crate) const fn lead_status_name(value: LeadCandidateStatus) -> &'static str {
    match value {
        LeadCandidateStatus::Captured => "open",
        LeadCandidateStatus::Qualified => "qualified",
        LeadCandidateStatus::Rejected => "rejected",
    }
}

pub(crate) const fn budget_status_name(value: CampaignBudgetStatus) -> &'static str {
    value.as_str()
}

pub(crate) const fn budget_period_name(value: marketing::CampaignBudgetPeriod) -> &'static str {
    match value {
        marketing::CampaignBudgetPeriod::Annual => "annual",
        marketing::CampaignBudgetPeriod::Quarterly => "quarterly",
        marketing::CampaignBudgetPeriod::Campaign => "campaign",
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
    fn projectors_are_private_snapshot_only() {
        let leads = MarketingLeadProjector::new();
        let budgets = MarketingBudgetProjector::new();
        assert_eq!(leads.name(), LEAD_PROJECTION_NAME);
        assert_eq!(leads.event_types(), &[LEAD_SNAPSHOT_EVENT_TYPE.to_owned()]);
        assert_eq!(budgets.name(), BUDGET_PROJECTION_NAME);
        assert_eq!(
            budgets.event_types(),
            &[BUDGET_SNAPSHOT_EVENT_TYPE.to_owned()]
        );
    }
}
