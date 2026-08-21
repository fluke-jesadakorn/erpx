use chrono::{DateTime, Utc};
use erpx_kernel::{AuthorityScope, CorrelationId, HumanDecisionProof};
use thiserror::Error;

use crate::{
    domain::{
        PlanningDomainError, PlanningRecommendation, PlanningRun, PlanningScenario,
        RecommendationId, RecommendationRelease, ReleaseId, RunId, ScenarioId,
        recommendation_approval_decision_ref, recommendation_rejection_decision_ref,
        recommendation_release_decision_ref,
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
        self.write.commit(PlanningCommit { identity, correlation_id, authority, human_decision: None, expected_version: Some(expected), event_type: "planning.run_completed".into(), event_payload: serde_json::json!({"run_id": run_id.as_uuid(), "input_digest": run.input_digest}), aggregates: vec![PlanningAggregate::Scenario(scenario), PlanningAggregate::Run(run)] }).await.map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn approve_recommendation(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        recommendation_id: RecommendationId,
        scope: &AuthorityScope,
        requested_version: Option<u64>,
        correlation_id: CorrelationId,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, PlanningError> {
        self.decide_recommendation(
            actor,
            identity,
            recommendation_id,
            scope,
            requested_version,
            correlation_id,
            at,
            true,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn reject_recommendation(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        recommendation_id: RecommendationId,
        scope: &AuthorityScope,
        requested_version: Option<u64>,
        correlation_id: CorrelationId,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, PlanningError> {
        self.decide_recommendation(
            actor,
            identity,
            recommendation_id,
            scope,
            requested_version,
            correlation_id,
            at,
            false,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn decide_recommendation(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        recommendation_id: RecommendationId,
        scope: &AuthorityScope,
        requested_version: Option<u64>,
        correlation_id: CorrelationId,
        at: DateTime<Utc>,
        approve: bool,
    ) -> Result<CommitReceipt, PlanningError> {
        require_authenticated_human(actor)?;
        identity.validate()?;
        let expected = requested_version.ok_or(PlanningError::Conflict)?;
        let decision_ref = if approve {
            recommendation_approval_decision_ref(recommendation_id, expected, identity.command_id)
        } else {
            recommendation_rejection_decision_ref(recommendation_id, expected, identity.command_id)
        };
        let human_decision = bind_human_decision(actor, scope.clone(), decision_ref, at)?;
        let action = if approve {
            PlanningAction::ApproveRecommendation
        } else {
            PlanningAction::RejectRecommendation
        };
        let authority = self.authorization.authorize(actor, scope, action).await?;
        let mut recommendation = self
            .write
            .load_recommendation(&authority, recommendation_id)
            .await?
            .ok_or(PlanningError::Neutral)?;
        if recommendation.scope.as_ref() != Some(scope) {
            return Err(PlanningError::Neutral);
        }
        if recommendation.version != expected {
            return Err(PlanningError::Conflict);
        }
        if approve {
            recommendation.approve(identity.command_id, &human_decision)?;
        } else {
            recommendation.reject(identity.command_id, &human_decision)?;
        }
        let event_type = if approve {
            "planning.recommendation_approved"
        } else {
            "planning.recommendation_rejected"
        };
        self.write
            .commit(PlanningCommit {
                identity,
                correlation_id,
                authority,
                human_decision: Some(human_decision),
                expected_version: Some(expected),
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": recommendation.version}),
                aggregates: vec![PlanningAggregate::Recommendation(recommendation)],
            })
            .await
            .map_err(Into::into)
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
        require_authenticated_human(actor)?;
        identity.validate()?;
        let expected = requested_version.ok_or(PlanningError::Conflict)?;
        let human_decision = bind_human_decision(
            actor,
            scope.clone(),
            recommendation_release_decision_ref(
                recommendation_id,
                release_id,
                expected,
                identity.command_id,
            ),
            at,
        )?;
        let authority = self
            .authorization
            .authorize(actor, scope, PlanningAction::ReleaseRecommendation)
            .await?;
        let mut recommendation = self
            .write
            .load_recommendation(&authority, recommendation_id)
            .await?
            .ok_or(PlanningError::Neutral)?;
        if recommendation.scope.as_ref() != Some(scope) || recommendation.version != expected {
            return Err(PlanningError::Conflict);
        }
        let release =
            RecommendationRelease::from_approved(release_id, &recommendation, correlation_id)?;
        recommendation.mark_released(release_id, identity.command_id, &human_decision)?;
        let receipt = self.handoff.release(&authority, &release).await?;
        if !receipt.accepted {
            return Err(PlanningError::HandoffRejected(
                receipt
                    .rejection_reason
                    .unwrap_or_else(|| "owner context rejected recommendation".into()),
            ));
        }
        self.write.commit(PlanningCommit { identity, correlation_id, authority, human_decision: Some(human_decision), expected_version: Some(expected), event_type: "planning.recommendation_released".into(), event_payload: serde_json::json!({"release_id": release_id.as_uuid(), "owner_correlation_id": receipt.owner_correlation_id, "released_at": at}), aggregates: vec![PlanningAggregate::Recommendation(recommendation)] }).await.map_err(Into::into)
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
                human_decision: None,
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
                human_decision: None,
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
                human_decision: None,
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
        if matches!(
            action,
            PlanningAction::ApproveRecommendation
                | PlanningAction::RejectRecommendation
                | PlanningAction::ReleaseRecommendation
        ) {
            return Err(PlanningError::HumanDecisionRequired);
        }
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
                human_decision: None,
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
                human_decision: None,
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
    #[error("an authenticated direct human decision is required")]
    HumanDecisionRequired,
    #[error("invalid planning request: {0}")]
    Invalid(String),
    #[error("owner context rejected planning handoff: {0}")]
    HandoffRejected(String),
    #[error("planning dependency is unavailable")]
    Unavailable,
    #[error("planning operation failed")]
    Internal,
}

fn require_authenticated_human(actor: &ActorContext) -> Result<(), PlanningError> {
    let actor = &actor.attribution;
    if actor.is_human()
        && actor.session_id.is_some()
        && actor.represented_principal_ref.is_none()
        && actor.support_capability_id.is_none()
    {
        Ok(())
    } else {
        Err(PlanningError::HumanDecisionRequired)
    }
}

fn bind_human_decision(
    actor: &ActorContext,
    scope: AuthorityScope,
    decision_ref: String,
    at: DateTime<Utc>,
) -> Result<HumanDecisionProof, PlanningError> {
    HumanDecisionProof::try_new(actor.attribution.clone(), scope, decision_ref, at)
        .map_err(|_| PlanningError::HumanDecisionRequired)
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

#[cfg(test)]
mod tests {
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };

    use async_trait::async_trait;
    use erpx_kernel::{ActorAttribution, ActorKind, CommandId, HoldingId, PrincipalId, SessionId};
    use uuid::Uuid;

    use super::*;
    use crate::ports::{AuthorizedPlanningAccess, PlanningCommit, RecommendationReleaseReceipt};

    fn uuid(value: u128) -> Uuid {
        let mut bytes = value.to_be_bytes();
        bytes[6] = (bytes[6] & 0x0f) | 0x40;
        bytes[8] = (bytes[8] & 0x3f) | 0x80;
        Uuid::from_bytes(bytes)
    }

    #[derive(Clone, Default)]
    struct NeverPlanning {
        authorizations: Arc<AtomicUsize>,
        loads: Arc<AtomicUsize>,
        handoffs: Arc<AtomicUsize>,
        commits: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl AuthorizationPort for NeverPlanning {
        async fn authorize(
            &self,
            actor: &ActorContext,
            target_scope: &AuthorityScope,
            action: PlanningAction,
        ) -> Result<AuthorizedPlanningAccess, PortError> {
            self.authorizations.fetch_add(1, Ordering::SeqCst);
            Ok(AuthorizedPlanningAccess {
                actor: actor.clone(),
                target_scope: target_scope.clone(),
                action,
            })
        }
    }

    #[async_trait]
    impl PlanningWriteRepository for NeverPlanning {
        async fn load_scenario(
            &self,
            _: &AuthorizedPlanningAccess,
            _: ScenarioId,
        ) -> Result<Option<PlanningScenario>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn load_run(
            &self,
            _: &AuthorizedPlanningAccess,
            _: RunId,
        ) -> Result<Option<PlanningRun>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn load_recommendation(
            &self,
            _: &AuthorizedPlanningAccess,
            _: RecommendationId,
        ) -> Result<Option<PlanningRecommendation>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn load_plan(
            &self,
            _: &AuthorizedPlanningAccess,
            _: Uuid,
        ) -> Result<Option<PlanningPlan>, PortError> {
            self.loads.fetch_add(1, Ordering::SeqCst);
            Ok(None)
        }

        async fn commit(&self, _: PlanningCommit) -> Result<CommitReceipt, PortError> {
            self.commits.fetch_add(1, Ordering::SeqCst);
            Err(PortError::Internal)
        }
    }

    #[async_trait]
    impl RecommendationHandoffPort for NeverPlanning {
        async fn release(
            &self,
            _: &AuthorizedPlanningAccess,
            _: &RecommendationRelease,
        ) -> Result<RecommendationReleaseReceipt, PortError> {
            self.handoffs.fetch_add(1, Ordering::SeqCst);
            Err(PortError::Internal)
        }
    }

    fn actor(kind: ActorKind) -> ActorContext {
        let holding_id = HoldingId::try_from_uuid(uuid(1)).unwrap();
        ActorContext {
            holding_id,
            principal_id: PrincipalId::try_from_uuid(uuid(2)).unwrap(),
            authority_scope: AuthorityScope::Holding {
                holding_id: holding_id.as_uuid(),
            },
            attribution: ActorAttribution {
                principal_ref: "principal:planning-test".into(),
                kind,
                session_id: Some(SessionId::try_from_uuid(uuid(3)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
        }
    }

    fn identity() -> MutationIdentity {
        MutationIdentity {
            command_id: CommandId::try_from_uuid(uuid(4)).unwrap(),
            idempotency_key: "planning-human-decision".into(),
            request_digest: "a".repeat(64),
        }
    }

    #[tokio::test]
    async fn recommendation_decisions_reject_invalid_actors_before_any_port() {
        let mut actors = vec![
            actor(ActorKind::Agent),
            actor(ActorKind::Service),
            actor(ActorKind::SystemWorker),
        ];
        let mut sessionless = actor(ActorKind::HoldingUser);
        sessionless.attribution.session_id = None;
        actors.push(sessionless);
        let mut represented = actor(ActorKind::HoldingUser);
        represented.attribution.represented_principal_ref = Some("principal:represented".into());
        actors.push(represented);
        let scope = AuthorityScope::Holding {
            holding_id: uuid(1),
        };
        let recommendation_id = RecommendationId::try_from_uuid(uuid(5)).unwrap();
        let correlation_id = CorrelationId::try_from_uuid(uuid(6)).unwrap();
        let at = DateTime::from_timestamp(1_800_000_000, 0).unwrap();

        for actor in actors {
            for operation in 0..3 {
                let ports = NeverPlanning::default();
                let service =
                    PlanningCommandService::new(ports.clone(), ports.clone(), ports.clone());
                let result = match operation {
                    0 => {
                        service
                            .approve_recommendation(
                                &actor,
                                identity(),
                                recommendation_id,
                                &scope,
                                Some(3),
                                correlation_id,
                                at,
                            )
                            .await
                    }
                    1 => {
                        service
                            .reject_recommendation(
                                &actor,
                                identity(),
                                recommendation_id,
                                &scope,
                                Some(3),
                                correlation_id,
                                at,
                            )
                            .await
                    }
                    _ => {
                        service
                            .release_recommendation(
                                &actor,
                                identity(),
                                recommendation_id,
                                &scope,
                                Some(3),
                                ReleaseId::try_from_uuid(uuid(7)).unwrap(),
                                correlation_id,
                                at,
                            )
                            .await
                    }
                };
                assert_eq!(result.unwrap_err(), PlanningError::HumanDecisionRequired);
                assert_eq!(ports.authorizations.load(Ordering::SeqCst), 0);
                assert_eq!(ports.loads.load(Ordering::SeqCst), 0);
                assert_eq!(ports.handoffs.load(Ordering::SeqCst), 0);
                assert_eq!(ports.commits.load(Ordering::SeqCst), 0);
            }
        }
    }
}
