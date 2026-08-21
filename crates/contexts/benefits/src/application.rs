use chrono::{DateTime, Utc};
use erpx_kernel::HumanDecisionProof;
use thiserror::Error;

use crate::{
    domain::{
        BenefitEnrollment, BenefitEnrollmentId, BenefitPlan, BenefitPlanId, BenefitsDomainError,
        EligibilityDecision, EnrollmentWindow, LifeEvent, PayrollHandoffId,
    },
    ports::{
        ActorContext, AuthorizationPort, BenefitsAction, BenefitsAggregate, BenefitsCommit,
        BenefitsReadRepository, BenefitsWriteRepository, CommitReceipt, MutationIdentity,
        PayrollHandoffPort, PortError,
    },
};

pub struct BenefitsCommandService<A, W, P> {
    authorization: A,
    write: W,
    payroll: P,
}
impl<A, W, P> BenefitsCommandService<A, W, P>
where
    A: AuthorizationPort,
    W: BenefitsWriteRepository,
    P: PayrollHandoffPort,
{
    pub const fn new(authorization: A, write: W, payroll: P) -> Self {
        Self {
            authorization,
            write,
            payroll,
        }
    }

    pub async fn publish_plan(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        mut plan: BenefitPlan,
        decision: HumanDecisionProof,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, BenefitsError> {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, plan.scope(), BenefitsAction::ManagePlan)
            .await?;
        if self.write.load_plan(&authority, plan.id()).await?.is_some() {
            return Err(BenefitsError::Conflict);
        }
        plan.publish(&decision, identity.command_id, at)?;
        self.write
            .commit(BenefitsCommit {
                identity,
                authority,
                expected_version: None,
                event_type: "benefits.plan_published".into(),
                event_payload: serde_json::json!({
                    "version": plan.version(),
                    "human_decision": {
                        "principal_ref": decision.principal_ref(),
                        "session_id": decision.actor().session_id.map(|value| value.as_uuid()),
                        "authority_scope": decision.authority_scope(),
                        "decision_ref": decision.decision_ref(),
                        "decided_at": decision.decided_at(),
                    }
                }),
                aggregate: BenefitsAggregate::Plan(Box::new(plan)),
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn open_enrollment(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        enrollment_id: BenefitEnrollmentId,
        scope: &erpx_kernel::AuthorityScope,
        plan_id: BenefitPlanId,
        plan_version: u64,
        window: EnrollmentWindow,
        employee: erpx_integration_contracts::RecordVersionRef,
        eligibility: EligibilityDecision,
        life_event: Option<LifeEvent>,
        on: chrono::NaiveDate,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, BenefitsError> {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, scope, BenefitsAction::OpenEnrollment)
            .await?;
        if self
            .write
            .load_enrollment(&authority, enrollment_id)
            .await?
            .is_some()
        {
            return Err(BenefitsError::Conflict);
        }
        let plan = self
            .write
            .load_plan(&authority, plan_id)
            .await?
            .ok_or(BenefitsError::Neutral)?;
        if plan.version() != plan_version || plan.scope() != scope {
            return Err(BenefitsError::Neutral);
        }
        let enrollment = BenefitEnrollment::open(
            enrollment_id,
            scope.clone(),
            &plan,
            &window,
            employee,
            eligibility,
            life_event,
            on,
            at,
        )?;
        self.write
            .commit(BenefitsCommit {
                identity,
                authority,
                expected_version: None,
                event_type: "benefits.enrollment_opened".into(),
                event_payload: serde_json::json!({"plan_id": plan_id.as_uuid(), "plan_version": plan_version}),
                aggregate: BenefitsAggregate::Enrollment(Box::new(enrollment)),
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn mutate_enrollment<F>(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        enrollment_id: BenefitEnrollmentId,
        scope: &erpx_kernel::AuthorityScope,
        requested_version: Option<u64>,
        action: BenefitsAction,
        event_type: &str,
        change: F,
    ) -> Result<CommitReceipt, BenefitsError>
    where
        F: FnOnce(&mut BenefitEnrollment, Option<&BenefitPlan>) -> Result<(), BenefitsDomainError>,
    {
        identity.validate()?;
        let authority = self.authorization.authorize(actor, scope, action).await?;
        let mut enrollment = self
            .write
            .load_enrollment(&authority, enrollment_id)
            .await?
            .ok_or(BenefitsError::Neutral)?;
        if enrollment.scope() != scope || requested_version != Some(enrollment.version()) {
            return Err(BenefitsError::Conflict);
        }
        let plan = self
            .write
            .load_plan(&authority, enrollment.plan().plan_id)
            .await?;
        let expected = enrollment.version();
        change(&mut enrollment, plan.as_ref())?;
        self.write
            .commit(BenefitsCommit {
                identity,
                authority,
                expected_version: Some(expected),
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": enrollment.version()}),
                aggregate: BenefitsAggregate::Enrollment(Box::new(enrollment)),
            })
            .await
            .map_err(Into::into)
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn release_payroll(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        enrollment_id: BenefitEnrollmentId,
        scope: &erpx_kernel::AuthorityScope,
        handoff_id: PayrollHandoffId,
        decision: HumanDecisionProof,
        at: DateTime<Utc>,
    ) -> Result<CommitReceipt, BenefitsError> {
        identity.validate()?;
        let authority = self
            .authorization
            .authorize(actor, scope, BenefitsAction::ReleasePayrollHandoff)
            .await?;
        let mut enrollment = self
            .write
            .load_enrollment(&authority, enrollment_id)
            .await?
            .ok_or(BenefitsError::Neutral)?;
        if enrollment.scope() != scope {
            return Err(BenefitsError::Neutral);
        }
        let expected = enrollment.version();
        // The owner domain must reject non-human or mismatched evidence before
        // the external Payroll handoff is attempted.
        enrollment.validate_payroll_release_decision(
            handoff_id,
            &decision,
            identity.command_id,
            at,
        )?;
        let handoff = enrollment.payroll_handoff(handoff_id, identity.correlation_id)?;
        let response = self.payroll.release(&authority, &handoff).await?;
        if !response.accepted {
            return Err(BenefitsError::HandoffRejected(
                response
                    .rejection_reason
                    .unwrap_or_else(|| "Payroll rejected contribution handoff".into()),
            ));
        }
        enrollment.mark_payroll_handed_off(handoff_id, &decision, identity.command_id, at)?;
        self.write
            .commit(BenefitsCommit {
                identity,
                authority,
                expected_version: Some(expected),
                event_type: "benefits.payroll_handoff_accepted".into(),
                event_payload: serde_json::json!({
                    "handoff_id": handoff_id.as_uuid(),
                    "owner_correlation_id": response.owner_correlation_id,
                    "human_decision": {
                        "principal_ref": decision.principal_ref(),
                        "session_id": decision.actor().session_id.map(|value| value.as_uuid()),
                        "authority_scope": decision.authority_scope(),
                        "decision_ref": decision.decision_ref(),
                        "decided_at": decision.decided_at(),
                    }
                }),
                aggregate: BenefitsAggregate::Enrollment(Box::new(enrollment)),
            })
            .await
            .map_err(Into::into)
    }

    pub async fn govern_record(
        &self,
        actor: &ActorContext,
        identity: MutationIdentity,
        record: crate::BenefitsRecord,
        expected_version: Option<u64>,
        event_type: &str,
    ) -> Result<CommitReceipt, BenefitsError> {
        identity.validate()?;
        record.validate()?;
        let action = match record.kind() {
            crate::BenefitsRecordKind::Window => BenefitsAction::OpenEnrollment,
            crate::BenefitsRecordKind::Handoff => BenefitsAction::ReleasePayrollHandoff,
        };
        let authority = self
            .authorization
            .authorize(actor, record.scope(), action)
            .await?;
        let current = self.write.load_record(&authority, record.id()).await?;
        match (expected_version, current.as_ref()) {
            (None, None) if record.version() == 1 => {}
            (Some(expected), Some(current))
                if current.version() == expected
                    && current.kind() == record.kind()
                    && record.version()
                        == expected.checked_add(1).ok_or(BenefitsError::Conflict)? => {}
            _ => return Err(BenefitsError::Conflict),
        }
        self.write
            .commit(BenefitsCommit {
                identity,
                authority,
                expected_version,
                event_type: event_type.into(),
                event_payload: serde_json::json!({"version": record.version()}),
                aggregate: BenefitsAggregate::Record(Box::new(record)),
            })
            .await
            .map_err(Into::into)
    }
}

pub struct BenefitsQueryService<A, R> {
    authorization: A,
    read: R,
}
impl<A, R> BenefitsQueryService<A, R>
where
    A: AuthorizationPort,
    R: BenefitsReadRepository,
{
    pub const fn new(authorization: A, read: R) -> Self {
        Self {
            authorization,
            read,
        }
    }
    pub async fn get_enrollment(
        &self,
        actor: &ActorContext,
        scope: &erpx_kernel::AuthorityScope,
        id: BenefitEnrollmentId,
    ) -> Result<crate::ports::BenefitEnrollmentProjection, BenefitsError> {
        let authority = self
            .authorization
            .authorize(actor, scope, BenefitsAction::View)
            .await?;
        self.read
            .get_enrollment(&authority, id)
            .await?
            .ok_or(BenefitsError::Neutral)
    }

    pub async fn get_record(
        &self,
        actor: &ActorContext,
        scope: &erpx_kernel::AuthorityScope,
        id: uuid::Uuid,
    ) -> Result<crate::ports::BenefitsRecordProjection, BenefitsError> {
        let authority = self
            .authorization
            .authorize(actor, scope, BenefitsAction::View)
            .await?;
        self.read
            .get_record(&authority, id)
            .await?
            .ok_or(BenefitsError::Neutral)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BenefitsError {
    #[error("benefits item unavailable")]
    Neutral,
    #[error("benefits conflict")]
    Conflict,
    #[error("invalid benefits request: {0}")]
    Invalid(String),
    #[error("Payroll handoff rejected: {0}")]
    HandoffRejected(String),
    #[error("benefits dependency unavailable")]
    Unavailable,
    #[error("benefits operation failed")]
    Internal,
}
impl From<BenefitsDomainError> for BenefitsError {
    fn from(value: BenefitsDomainError) -> Self {
        Self::Invalid(value.to_string())
    }
}
impl From<PortError> for BenefitsError {
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
