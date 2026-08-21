use chrono::{DateTime, Utc};
use erpx_kernel::{
    ActorAttribution, AuditContext, AuthorityScope, Clock, CommandId, CorrelationId,
    HumanDecisionError, HumanDecisionProof,
};
use thiserror::Error;

use crate::{
    domain::{
        AgentDefinition, AgentDefinitionId, AgentDomainError, AgentManifest, AgentPin,
        AgentVersionId, Assignment, AssignmentFailure, AssignmentId, BudgetDecision, BudgetRequest,
        BudgetReservation, BudgetReservationId, DelegationLimits, PayloadManifest, RootInvocation,
        agent_version_approval_decision_ref,
    },
    ports::{
        AgentAuthorizationPort, AgentDefinitionRepository, AssignmentCommit, AssignmentRepository,
        AuthorizationRequest, BudgetLedgerPort, BudgetSubjects, DefinitionCommit, DelegationCommit,
        OrchestrationAction, OrchestrationUnitOfWork, PortError, RootAssignmentCommit,
        RootAssignmentReceipt,
    },
};

#[derive(Debug, Clone)]
pub struct RegisterAgent {
    pub definition_id: AgentDefinitionId,
    pub scope: AuthorityScope,
    pub agent_principal_ref: String,
    pub business_owner_ref: String,
    pub technical_owner_ref: String,
    pub name: String,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct AddAgentVersion {
    pub definition_id: AgentDefinitionId,
    pub expected_version: u64,
    pub manifest: AgentManifest,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct ApproveAgentVersion {
    pub definition_id: AgentDefinitionId,
    pub expected_version: u64,
    pub version_id: AgentVersionId,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct SuspendAgent {
    pub definition_id: AgentDefinitionId,
    pub expected_version: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct RetireAgent {
    pub definition_id: AgentDefinitionId,
    pub expected_version: u64,
    pub idempotency_key: String,
}

#[derive(Debug, Clone)]
pub struct StartAgentWork {
    pub command_id: CommandId,
    pub correlation_id: CorrelationId,
    pub assignment_id: AssignmentId,
    pub agent_definition_id: AgentDefinitionId,
    pub agent_version_id: AgentVersionId,
    pub scope: AuthorityScope,
    pub invocation: RootInvocation,
    pub budget_reservation_id: BudgetReservationId,
    pub budget_subjects: BudgetSubjects,
    pub budget_request: BudgetRequest,
    pub prompt_ref: String,
    pub payload_manifest: PayloadManifest,
    pub human_decision: HumanDecisionProof,
}

pub fn agent_assignment_decision_ref(assignment_id: AssignmentId, command_id: CommandId) -> String {
    format!("agent-assignment:{}:{command_id}", assignment_id.as_uuid())
}

#[derive(Debug, Clone)]
pub struct DelegateWork {
    pub child_assignment_id: AssignmentId,
    pub parent_assignment_id: AssignmentId,
    pub child_agent_definition_id: AgentDefinitionId,
    pub child_agent_version_id: AgentVersionId,
    pub child_scope: AuthorityScope,
    pub child_budget_reservation_id: BudgetReservationId,
    pub budget_subjects: BudgetSubjects,
    pub budget_request: BudgetRequest,
    pub limits: DelegationLimits,
    pub idempotency_key: String,
    pub reason_digest: String,
    pub prompt_ref: String,
    pub payload_manifest: PayloadManifest,
}

#[derive(Debug, Clone)]
pub struct RetryWork {
    pub assignment_id: AssignmentId,
    pub expected_version: u64,
    pub security_revoked: bool,
    pub remaining_budget: bool,
}

#[derive(Debug, Clone)]
pub struct CompleteWork {
    pub assignment_id: AssignmentId,
    pub expected_version: u64,
    pub lease_token: uuid::Uuid,
    pub output_digest: String,
}

#[derive(Debug, Clone)]
pub struct FailWork {
    pub assignment_id: AssignmentId,
    pub expected_version: u64,
    pub lease_token: uuid::Uuid,
    pub failure: AssignmentFailure,
}

#[derive(Debug, Clone)]
pub struct CancelWork {
    pub assignment_id: AssignmentId,
    pub expected_version: u64,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentOrchestrationError {
    #[error(transparent)]
    Domain(#[from] AgentDomainError),
    #[error(transparent)]
    Port(#[from] PortError),
    #[error("record not found")]
    NotFound,
    #[error("expected version does not match current aggregate")]
    VersionConflict,
    #[error("budget requires explicit over-limit approval")]
    BudgetApprovalRequired,
    #[error(transparent)]
    HumanDecision(#[from] HumanDecisionError),
}

pub struct AgentOrchestrationService<D, R, B, U, A, C> {
    definitions: D,
    assignments: R,
    budgets: B,
    unit_of_work: U,
    authorization: A,
    clock: C,
}

impl<D, R, B, U, A, C> AgentOrchestrationService<D, R, B, U, A, C> {
    pub const fn new(
        definitions: D,
        assignments: R,
        budgets: B,
        unit_of_work: U,
        authorization: A,
        clock: C,
    ) -> Self {
        Self {
            definitions,
            assignments,
            budgets,
            unit_of_work,
            authorization,
            clock,
        }
    }
}

impl<D, R, B, U, A, C> AgentOrchestrationService<D, R, B, U, A, C>
where
    D: AgentDefinitionRepository,
    R: AssignmentRepository,
    B: BudgetLedgerPort,
    U: OrchestrationUnitOfWork,
    A: AgentAuthorizationPort,
    C: Clock,
{
    pub async fn register_agent(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: RegisterAgent,
    ) -> Result<AgentDefinition, AgentOrchestrationError> {
        self.authorization
            .authorize(AuthorizationRequest {
                actor: &actor,
                authority_scope: &authority_scope,
                target_scope: &command.scope,
                action: OrchestrationAction::RegisterAgent,
                agent_definition_id: Some(command.definition_id),
                assignment_id: None,
            })
            .await?;
        let definition = AgentDefinition::register(
            command.definition_id,
            command.scope,
            command.agent_principal_ref,
            command.business_owner_ref,
            command.technical_owner_ref,
            command.name,
            self.clock.now(),
        )?;
        self.unit_of_work
            .commit_definition(DefinitionCommit {
                actor,
                authority_scope,
                audit,
                expected_version: None,
                idempotency_key: command.idempotency_key,
                definition,
            })
            .await
            .map_err(Into::into)
    }

    pub async fn start_work(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: StartAgentWork,
    ) -> Result<RootAssignmentReceipt, AgentOrchestrationError> {
        command.invocation.validate()?;
        if command.human_decision.actor() != &actor
            || command.human_decision.authority_scope() != &command.scope
            || command.human_decision.decision_ref()
                != agent_assignment_decision_ref(command.assignment_id, command.command_id)
            || command.human_decision.decided_at() > self.clock.now()
        {
            return Err(AgentOrchestrationError::HumanDecision(
                HumanDecisionError::InvalidDecisionReference,
            ));
        }
        let definition = self
            .definitions
            .load_definition(&authority_scope, command.agent_definition_id)
            .await?
            .ok_or(AgentOrchestrationError::NotFound)?;
        let pin = definition.pin(command.agent_version_id)?;
        self.authorize_assignment(
            &actor,
            &authority_scope,
            &command.scope,
            OrchestrationAction::StartAssignment,
            Some(command.agent_definition_id),
            Some(command.assignment_id),
        )
        .await?;
        let reservation = self
            .reserve_budget(
                command.budget_reservation_id,
                &command.budget_subjects,
                command.budget_request,
            )
            .await?;
        let assignment = Assignment::start_root(
            command.assignment_id,
            command.invocation,
            pin,
            command.scope,
            reservation.id,
            self.clock.now(),
        )?;
        self.unit_of_work
            .commit_root_assignment(RootAssignmentCommit {
                command_id: command.command_id,
                correlation_id: command.correlation_id,
                human_decision: command.human_decision,
                actor,
                authority_scope,
                audit,
                assignment,
                reservation,
                prompt_ref: command.prompt_ref,
                payload_manifest: command.payload_manifest,
            })
            .await
            .map_err(Into::into)
    }

    pub async fn add_agent_version(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: AddAgentVersion,
    ) -> Result<AgentDefinition, AgentOrchestrationError> {
        let mut definition = self
            .load_definition_versioned(
                &authority_scope,
                command.definition_id,
                command.expected_version,
            )
            .await?;
        self.authorize_assignment(
            &actor,
            &authority_scope,
            definition.scope(),
            OrchestrationAction::VersionAgent,
            Some(definition.id()),
            None,
        )
        .await?;
        definition.add_draft_version(command.manifest, self.clock.now())?;
        self.commit_definition_mutation(
            actor,
            authority_scope,
            audit,
            command.expected_version,
            command.idempotency_key,
            definition,
        )
        .await
    }

    pub async fn approve_agent_version(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: ApproveAgentVersion,
    ) -> Result<AgentDefinition, AgentOrchestrationError> {
        let decision = HumanDecisionProof::try_new(
            actor.clone(),
            authority_scope.clone(),
            agent_version_approval_decision_ref(command.definition_id, command.version_id),
            self.clock.now(),
        )?;
        let mut definition = self
            .load_definition_versioned(
                &authority_scope,
                command.definition_id,
                command.expected_version,
            )
            .await?;
        self.authorize_assignment(
            &actor,
            &authority_scope,
            definition.scope(),
            OrchestrationAction::ApproveAgent,
            Some(definition.id()),
            None,
        )
        .await?;
        definition.approve_version(command.version_id, decision)?;
        self.commit_definition_mutation(
            actor,
            authority_scope,
            audit,
            command.expected_version,
            command.idempotency_key,
            definition,
        )
        .await
    }

    pub async fn suspend_agent(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: SuspendAgent,
    ) -> Result<AgentDefinition, AgentOrchestrationError> {
        self.change_agent_lifecycle(
            actor,
            authority_scope,
            audit,
            command.definition_id,
            command.expected_version,
            command.idempotency_key,
            false,
        )
        .await
    }

    pub async fn retire_agent(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: RetireAgent,
    ) -> Result<AgentDefinition, AgentOrchestrationError> {
        self.change_agent_lifecycle(
            actor,
            authority_scope,
            audit,
            command.definition_id,
            command.expected_version,
            command.idempotency_key,
            true,
        )
        .await
    }

    pub async fn delegate(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: DelegateWork,
    ) -> Result<Assignment, AgentOrchestrationError> {
        let parent = self
            .assignments
            .load_assignment(&authority_scope, command.parent_assignment_id)
            .await?
            .ok_or(AgentOrchestrationError::NotFound)?;
        let definition = self
            .definitions
            .load_definition(&authority_scope, command.child_agent_definition_id)
            .await?
            .ok_or(AgentOrchestrationError::NotFound)?;
        let child_pin = definition.pin(command.child_agent_version_id)?;
        self.authorize_assignment(
            &actor,
            &authority_scope,
            &command.child_scope,
            OrchestrationAction::DelegateAssignment,
            Some(command.child_agent_definition_id),
            Some(command.parent_assignment_id),
        )
        .await?;
        let reservation = self
            .reserve_budget(
                command.child_budget_reservation_id,
                &command.budget_subjects,
                command.budget_request,
            )
            .await?;
        let sibling_count = self
            .assignments
            .child_count(parent.root_assignment_id(), parent.id())
            .await?;
        let child = parent.delegate(
            command.child_assignment_id,
            child_pin,
            command.child_scope,
            reservation.id,
            sibling_count,
            &command.limits,
            self.clock.now(),
        )?;
        self.unit_of_work
            .commit_delegation(DelegationCommit {
                actor,
                authority_scope,
                audit,
                parent_expected_version: parent.version(),
                parent_assignment_id: parent.id(),
                child,
                reservation,
                idempotency_key: command.idempotency_key,
                reason_digest: command.reason_digest,
                prompt_ref: command.prompt_ref,
                payload_manifest: command.payload_manifest,
            })
            .await
            .map_err(Into::into)
    }

    pub async fn retry(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: RetryWork,
    ) -> Result<Assignment, AgentOrchestrationError> {
        let mut assignment = self
            .load_versioned(
                &authority_scope,
                command.assignment_id,
                command.expected_version,
            )
            .await?;
        self.authorize_assignment(
            &actor,
            &authority_scope,
            assignment.scope(),
            OrchestrationAction::RetryAssignment,
            Some(assignment.pin().definition_id),
            Some(assignment.id()),
        )
        .await?;
        assignment.retry(
            command.security_revoked,
            command.remaining_budget,
            self.clock.now(),
        )?;
        self.commit_mutation(
            actor,
            authority_scope,
            audit,
            command.expected_version,
            assignment,
        )
        .await
    }

    pub async fn cancel(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        command: CancelWork,
    ) -> Result<Assignment, AgentOrchestrationError> {
        let mut assignment = self
            .load_versioned(
                &authority_scope,
                command.assignment_id,
                command.expected_version,
            )
            .await?;
        self.authorize_assignment(
            &actor,
            &authority_scope,
            assignment.scope(),
            OrchestrationAction::CancelAssignment,
            Some(assignment.pin().definition_id),
            Some(assignment.id()),
        )
        .await?;
        assignment.cancel(command.reason, self.clock.now())?;
        self.commit_mutation(
            actor,
            authority_scope,
            audit,
            command.expected_version,
            assignment,
        )
        .await
    }

    async fn load_versioned(
        &self,
        authority_scope: &AuthorityScope,
        id: AssignmentId,
        expected_version: u64,
    ) -> Result<Assignment, AgentOrchestrationError> {
        let assignment = self
            .assignments
            .load_assignment(authority_scope, id)
            .await?
            .ok_or(AgentOrchestrationError::NotFound)?;
        if assignment.version() != expected_version {
            return Err(AgentOrchestrationError::VersionConflict);
        }
        Ok(assignment)
    }

    async fn load_definition_versioned(
        &self,
        authority_scope: &AuthorityScope,
        id: AgentDefinitionId,
        expected_version: u64,
    ) -> Result<AgentDefinition, AgentOrchestrationError> {
        let definition = self
            .definitions
            .load_definition(authority_scope, id)
            .await?
            .ok_or(AgentOrchestrationError::NotFound)?;
        if definition.version() != expected_version {
            return Err(AgentOrchestrationError::VersionConflict);
        }
        Ok(definition)
    }

    #[allow(clippy::too_many_arguments)]
    async fn change_agent_lifecycle(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        definition_id: AgentDefinitionId,
        expected_version: u64,
        idempotency_key: String,
        retire: bool,
    ) -> Result<AgentDefinition, AgentOrchestrationError> {
        let mut definition = self
            .load_definition_versioned(&authority_scope, definition_id, expected_version)
            .await?;
        self.authorize_assignment(
            &actor,
            &authority_scope,
            definition.scope(),
            if retire {
                OrchestrationAction::RetireAgent
            } else {
                OrchestrationAction::SuspendAgent
            },
            Some(definition.id()),
            None,
        )
        .await?;
        if retire {
            definition.retire(self.clock.now())?;
        } else {
            definition.suspend(self.clock.now())?;
        }
        self.commit_definition_mutation(
            actor,
            authority_scope,
            audit,
            expected_version,
            idempotency_key,
            definition,
        )
        .await
    }

    #[allow(clippy::too_many_arguments)]
    async fn authorize_assignment(
        &self,
        actor: &ActorAttribution,
        authority_scope: &AuthorityScope,
        target_scope: &AuthorityScope,
        action: OrchestrationAction,
        definition_id: Option<AgentDefinitionId>,
        assignment_id: Option<AssignmentId>,
    ) -> Result<(), AgentOrchestrationError> {
        self.authorization
            .authorize(AuthorizationRequest {
                actor,
                authority_scope,
                target_scope,
                action,
                agent_definition_id: definition_id,
                assignment_id,
            })
            .await
            .map_err(Into::into)
    }

    async fn reserve_budget(
        &self,
        reservation_id: BudgetReservationId,
        subjects: &BudgetSubjects,
        request: BudgetRequest,
    ) -> Result<BudgetReservation, AgentOrchestrationError> {
        let at = self.clock.now();
        let layers = self.budgets.quote_layers(subjects, &request, at).await?;
        match BudgetReservation::decide(reservation_id, request, layers, at)? {
            BudgetDecision::Reserved(reservation) => Ok(reservation),
            BudgetDecision::ApprovalRequired { .. } => {
                Err(AgentOrchestrationError::BudgetApprovalRequired)
            }
        }
    }

    async fn commit_mutation(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        expected_version: u64,
        assignment: Assignment,
    ) -> Result<Assignment, AgentOrchestrationError> {
        self.unit_of_work
            .commit_assignment(AssignmentCommit {
                actor,
                authority_scope,
                audit,
                expected_version: Some(expected_version),
                assignment,
            })
            .await
            .map_err(Into::into)
    }

    #[allow(clippy::too_many_arguments)]
    async fn commit_definition_mutation(
        &self,
        actor: ActorAttribution,
        authority_scope: AuthorityScope,
        audit: AuditContext,
        expected_version: u64,
        idempotency_key: String,
        definition: AgentDefinition,
    ) -> Result<AgentDefinition, AgentOrchestrationError> {
        self.unit_of_work
            .commit_definition(DefinitionCommit {
                actor,
                authority_scope,
                audit,
                expected_version: Some(expected_version),
                idempotency_key,
                definition,
            })
            .await
            .map_err(Into::into)
    }
}

pub fn pinned_agent(pin: AgentPin) -> AgentPin {
    pin
}

pub fn at_or_now(value: Option<DateTime<Utc>>, fallback: DateTime<Utc>) -> DateTime<Utc> {
    value.unwrap_or(fallback)
}
