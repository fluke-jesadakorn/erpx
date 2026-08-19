use chrono::{DateTime, Utc};
use erpx_kernel::{AuthorityScope, CorrelationId};
use thiserror::Error;

use crate::{
    domain::{
        PlanningDomainError, PlanningRecommendation, PlanningRun, PlanningScenario,
        RecommendationId, RecommendationRelease, ReleaseId, RunId, ScenarioId,
    },
    plan::PlanningPlan,
    ports::{
        ActorContext, AuthorizationPort, CommitReceipt, MutationIdentity, PlanningAction,
        PlanningAggregate, PlanningCommit, PlanningReadRepository, PlanningRunProjection,
        PlanningWriteRepository, PortError, RecommendationHandoffPort,
    },
};

pub struct PlanningCommandService<A, W, H> {
    authorization: A,
    write: W,
    handoff: H,
}
impl<A, W, H> PlanningCommandService<A, W, H>
where
    A: AuthorizationPort,
    W: PlanningWriteRepository,
    H: RecommendationHandoffPort,
{
    pub const fn new(authorization: A, write: W, handoff: H) -> Self {
        Self {
            authorization,
            write,
            handoff,
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn run_scenario(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        scenario_id: ScenarioId,
        scope: &AuthorityScope,
        requested_version: Option<u64>,
        run_id: RunId,
        correlation_id: CorrelationId,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, PlanningError> {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, scope, PlanningAction::RunScenario)
            .await?;
        let mut scenario = self
            .write
            .load_scenario(&authority, scenario_id)
            .await?
            .ok_or(PlanningError::Neutral)?;
        if scenario.scope() != scope {
            return Err(PlanningError::Neutral);
        }
        let expected = scenario.version();
        if requested_version != Some(expected) {
            return Err(PlanningError::Conflict);
        }
        scenario.mark_running(at)?;
        let run = PlanningRun::execute(run_id, &scenario, at)?;
        scenario.mark_completed(at)?;
        self.write.commit(PlanningCommit { identity, correlation_id, authority, expected_version: Some(expected), event_type: "planning.run_completed".into(), event_payload: serde_json::json!({"run_id": run_id.as_uuid(), "input_digest": run.input_digest}), aggregates: vec![PlanningAggregate::Scenario(scenario), PlanningAggregate::Run(run)] }).await.map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn release_recommendation(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        recommendation_id: RecommendationId,
        scope: &AuthorityScope,
        requested_version: Option<u64>,
        release_id: ReleaseId,
        correlation_id: CorrelationId,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, PlanningError> {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, scope, PlanningAction::ReleaseRecommendation)
            .await?;
        let mut recommendation = self
            .write
            .load_recommendation(&authority, recommendation_id)
            .await?
            .ok_or(PlanningError::Neutral)?;
        let expected = recommendation.version;
        if requested_version != Some(expected) {
            return Err(PlanningError::Conflict);
        }
        let release =
            RecommendationRelease::from_approved(release_id, &recommendation, correlation_id)?;
        let receipt = self.handoff.release(&authority, &release).await?;
        if !receipt.accepted {
            return Err(PlanningError::HandoffRejected(
                receipt
                    .rejection_reason
                    .unwrap_or_else(|| "owner context rejected recommendation".into()),
            ));
        }
        recommendation.mark_released(release_id)?;
        self.write.commit(PlanningCommit { identity, correlation_id, authority, expected_version: Some(expected), event_type: "planning.recommendation_released".into(), event_payload: serde_json::json!({"release_id": release_id.as_uuid(), "owner_correlation_id": receipt.owner_correlation_id, "released_at": at}), aggregates: vec![PlanningAggregate::Recommendation(recommendation)] }).await.map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn govern_plan<F>(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        plan_id: uuid::Uuid,
        scope: &AuthorityScope,
        correlation_id: CorrelationId,
        event_type: &str,
        change: F,
    ) -> Result<CommitReceipt, PlanningError>
    where
        F: FnOnce(&mut PlanningPlan) -> Result<(), PlanningDomainError>,
    {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, scope, PlanningAction::GovernPlan)
            .await?;
        let mut plan = self
            .write
            .load_plan(&authority, plan_id)
            .await?
            .ok_or(PlanningError::Neutral)?;
        if plan.scope() != scope {
            return Err(PlanningError::Neutral);
        }
        let expected = plan.version();
        change(&mut plan)?;
        self.write
            .commit(PlanningCommit {
                identity,
                correlation_id,
                authority,
                expected_version: Some(expected),
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version":plan.version()}),
                aggregates: vec![PlanningAggregate::Plan(plan)],
            })
            .await
            .map_err(Into::into)
    }

    pub async fn create_scenario(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        correlation_id: CorrelationId,
        scenario: PlanningScenario,
    ) -> Result<CommitReceipt, PlanningError> {
        identity.validate()?;
        scenario.validate()?;
        let authority = self
            .authorization
            .authorize(actor, scenario.scope(), PlanningAction::CreateScenario)
            .await?;
        if self
            .write
            .load_scenario(&authority, scenario.id())
            .await?
            .is_some()
        {
            return Err(PlanningError::Conflict);
        }
        self.write
            .commit(PlanningCommit {
                identity,
                correlation_id,
                authority,
                expected_version: None,
                event_type: "planning.scenario_created".into(),
                event_payload: serde_json::json!({"input_position": scenario.version()}),
                aggregates: vec![PlanningAggregate::Scenario(scenario)],
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn mutate_scenario<F>(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        scenario_id: ScenarioId,
        scope: &AuthorityScope,
        requested_version: Option<u64>,
        correlation_id: CorrelationId,
        action: PlanningAction,
        event_type: &str,
        change: F,
    ) -> Result<CommitReceipt, PlanningError>
    where
        F: FnOnce(&mut PlanningScenario) -> Result<(), PlanningDomainError>,
    {
        identity.validate()?;
        let authority = self.authorization.authorize(actor, scope, action).await?;
        let mut scenario = self
            .write
            .load_scenario(&authority, scenario_id)
            .await?
            .ok_or(PlanningError::Neutral)?;
        if scenario.scope() != scope {
            return Err(PlanningError::Neutral);
        }
        let expected = scenario.version();
        if requested_version != Some(expected) {
            return Err(PlanningError::Conflict);
        }
        change(&mut scenario)?;
        self.write
            .commit(PlanningCommit {
                identity,
                correlation_id,
                authority,
                expected_version: Some(expected),
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": scenario.version()}),
                aggregates: vec![PlanningAggregate::Scenario(scenario)],
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn mutate_recommendation<F>(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        recommendation_id: RecommendationId,
        scope: &AuthorityScope,
        requested_version: Option<u64>,
        correlation_id: CorrelationId,
        action: PlanningAction,
        event_type: &str,
        change: F,
    ) -> Result<CommitReceipt, PlanningError>
    where
        F: FnOnce(&mut PlanningRecommendation) -> Result<(), PlanningDomainError>,
    {
        identity.validate()?;
        let authority = self.authorization.authorize(actor, scope, action).await?;
        let mut recommendation = self
            .write
            .load_recommendation(&authority, recommendation_id)
            .await?
            .ok_or(PlanningError::Neutral)?;
        let expected = recommendation.version;
        if requested_version != Some(expected) {
            return Err(PlanningError::Conflict);
        }
        change(&mut recommendation)?;
        self.write
            .commit(PlanningCommit {
                identity,
                correlation_id,
                authority,
                expected_version: Some(expected),
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": recommendation.version}),
                aggregates: vec![PlanningAggregate::Recommendation(recommendation)],
            })
            .await
            .map_err(Into::into)
    }

    pub async fn create_plan(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        correlation_id: CorrelationId,
        plan: PlanningPlan,
        event_type: &str,
    ) -> Result<CommitReceipt, PlanningError> {
        identity.validate()?;
        plan.validate()?;
        if plan.version() != 1 {
            return Err(PlanningError::Conflict);
        }
        let authority = self
            .authorization
            .authorize(actor, plan.scope(), PlanningAction::GovernPlan)
            .await?;
        if self.write.load_plan(&authority, plan.id()).await?.is_some() {
            return Err(PlanningError::Conflict);
        }
        self.write
            .commit(PlanningCommit {
                identity,
                correlation_id,
                authority,
                expected_version: None,
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": plan.version()}),
                aggregates: vec![PlanningAggregate::Plan(plan)],
            })
            .await
            .map_err(Into::into)
    }
}

pub struct PlanningQueryService<A, R> {
    authorization: A,
    read: R,
}
impl<A, R> PlanningQueryService<A, R>
where
    A: AuthorizationPort,
    R: PlanningReadRepository,
{
    pub const fn new(authorization: A, read: R) -> Self {
        Self {
            authorization,
            read,
        }
    }
    pub async fn get_run(
        &self,
        actor: &ActorContext,
        scope: &AuthorityScope,
        id: RunId,
    ) -> Result<PlanningRunProjection, PlanningError> {
        let authority = self
            .authorization
            .authorize(actor, scope, PlanningAction::View)
            .await?;
        self.read
            .get_run(&authority, id)
            .await?
            .ok_or(PlanningError::Neutral)
    }

    pub async fn get_record(
        &self,
        actor: &ActorContext,
        scope: &AuthorityScope,
        id: uuid::Uuid,
    ) -> Result<crate::ports::PlanningRecordProjection, PlanningError> {
        let authority = self
            .authorization
            .authorize(actor, scope, PlanningAction::View)
            .await?;
        self.read
            .get_record(&authority, id)
            .await?
            .ok_or(PlanningError::Neutral)
    }

    pub async fn get_plan(
        &self,
        actor: &ActorContext,
        scope: &AuthorityScope,
        id: uuid::Uuid,
    ) -> Result<crate::ports::PlanningPlanProjection, PlanningError> {
        let authority = self
            .authorization
            .authorize(actor, scope, PlanningAction::View)
            .await?;
        self.read
            .get_plan(&authority, id)
            .await?
            .ok_or(PlanningError::Neutral)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanningError {
    #[error("planning item is unavailable")]
    Neutral,
    #[error("planning request conflicts with current state")]
    Conflict,
    #[error("invalid planning request: {0}")]
    Invalid(String),
    #[error("owner context rejected planning handoff: {0}")]
    HandoffRejected(String),
    #[error("planning dependency is unavailable")]
    Unavailable,
    #[error("planning operation failed")]
    Internal,
}
impl From<PlanningDomainError> for PlanningError {
    fn from(value: PlanningDomainError) -> Self {
        Self::Invalid(value.to_string())
    }
}
impl From<PortError> for PlanningError {
    fn from(value: PortError) -> Self {
        match value {
            PortError::Denied | PortError::NotFound => Self::Neutral,
            PortError::Conflict => Self::Conflict,
            PortError::Stale | PortError::Unavailable => Self::Unavailable,
            PortError::InvalidState => Self::Invalid("adapter rejected invalid state".into()),
            PortError::Internal => Self::Internal,
        }
    }
}
