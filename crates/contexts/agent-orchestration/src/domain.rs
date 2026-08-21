use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use erpx_kernel::{AuthorityScope, CommandId, DataClassification, HumanDecisionProof, RecordId};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

macro_rules! uuid_id {
    ($name:ident) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, AgentDomainError> {
                if value.is_nil() {
                    Err(AgentDomainError::InvalidIdentifier)
                } else {
                    Ok(Self(value))
                }
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }
    };
}

uuid_id!(AgentDefinitionId);
uuid_id!(AgentVersionId);
uuid_id!(AssignmentId);
uuid_id!(CheckpointId);
uuid_id!(AgentPoolId);
uuid_id!(AgentSessionId);
uuid_id!(SessionMessageId);

pub fn session_message_publication_decision_ref(
    session_id: AgentSessionId,
    message_id: SessionMessageId,
    expected_version: u64,
    command_id: CommandId,
) -> String {
    format!(
        "agent.session:{}:message:{}:publish:v{}:command:{}",
        session_id.as_uuid(),
        message_id.as_uuid(),
        expected_version,
        command_id.as_uuid()
    )
}
uuid_id!(ActionCardId);
uuid_id!(BudgetReservationId);
uuid_id!(ProviderPolicyId);
uuid_id!(PoolRoutingDecisionId);
uuid_id!(ProviderInvocationId);

pub fn agent_version_approval_decision_ref(
    definition_id: AgentDefinitionId,
    version_id: AgentVersionId,
) -> String {
    format!(
        "agent.definition:{}:version:{}:approve",
        definition_id.as_uuid(),
        version_id.as_uuid()
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum AgentDomainError {
    #[error("identifier must not be nil")]
    InvalidIdentifier,
    #[error("required text is missing or exceeds its limit")]
    InvalidText,
    #[error("holding scope is invalid")]
    InvalidScope,
    #[error("agent ownership is incomplete")]
    MissingOwner,
    #[error("agent manifest is incomplete")]
    IncompleteManifest,
    #[error("agent version is not approved")]
    VersionNotApproved,
    #[error("agent definition is not available for new work")]
    AgentUnavailable,
    #[error("only a human may initiate root work")]
    RootMustBeHuman,
    #[error("proactive triggers are outside the baseline")]
    ProactiveTriggerDenied,
    #[error("assignment transition is invalid")]
    InvalidAssignmentTransition,
    #[error("assignment lease is missing, stale, or owned by another worker")]
    InvalidLease,
    #[error("assignment retry is not permitted")]
    RetryDenied,
    #[error("delegation would widen scope")]
    DelegationScopeExpansion,
    #[error("delegation limit was exceeded")]
    DelegationLimitExceeded,
    #[error("delegation cycle was detected")]
    DelegationCycle,
    #[error("no eligible agent exists")]
    NoEligibleAgent,
    #[error("compare requires an explicit, bounded fan-out")]
    InvalidCompare,
    #[error("budget layers are incomplete or duplicated")]
    InvalidBudgetLayers,
    #[error("budget or concurrency limit was exceeded")]
    BudgetExceeded,
    #[error("provider policy denied data egress")]
    ProviderDenied,
    #[error("private session is expired")]
    SessionExpired,
    #[error("private publication requires current human approval")]
    PublicationDenied,
    #[error("agent output cannot be attributed to a human")]
    ImpersonationDenied,
    #[error("business-changing output must remain a typed proposal")]
    DirectMutationDenied,
    #[error("optimistic aggregate version is invalid")]
    InvalidVersion,
    #[error("provider invocation evidence transition is invalid")]
    InvalidInvocationEvidence,
    #[error("agent governance record is invalid")]
    InvalidGovernanceRecord,
    #[error("arithmetic overflow")]
    ArithmeticOverflow,
}

fn valid_text(value: &str, max: usize) -> bool {
    let value = value.trim();
    !value.is_empty() && value.len() <= max && !value.chars().any(char::is_control)
}

fn validate_digest(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalKind {
    Human,
    Agent,
    Service,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentDefinitionStatus {
    Draft,
    Approved,
    Suspended,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AgentVersionStatus {
    Draft,
    Approved,
    Suspended,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentManifest {
    pub version_id: AgentVersionId,
    pub version_number: u32,
    pub purpose: String,
    pub provider_ref: String,
    pub model_ref: String,
    pub use_case: String,
    pub prompt_digest: String,
    pub policy_digest: String,
    pub capabilities: BTreeSet<String>,
    pub tool_refs: BTreeSet<String>,
    pub knowledge_scopes: BTreeSet<String>,
    pub organization_scopes: Vec<AuthorityScope>,
    pub maximum_classification: DataClassification,
    pub security_review_ref: String,
    pub privacy_review_ref: String,
    pub evaluation_refs: BTreeSet<String>,
    pub max_attempts: u16,
    pub timeout_seconds: u32,
}

impl AgentManifest {
    pub fn validate(&self) -> Result<(), AgentDomainError> {
        if self.version_number == 0
            || !valid_text(&self.purpose, 512)
            || !valid_text(&self.provider_ref, 128)
            || !valid_text(&self.model_ref, 128)
            || !valid_text(&self.use_case, 128)
            || !validate_digest(&self.prompt_digest)
            || !validate_digest(&self.policy_digest)
            || self.capabilities.is_empty()
            || self.organization_scopes.is_empty()
            || self
                .organization_scopes
                .iter()
                .any(|scope| scope.validate().is_err())
            || !valid_text(&self.security_review_ref, 256)
            || !valid_text(&self.privacy_review_ref, 256)
            || self.evaluation_refs.is_empty()
            || self.max_attempts == 0
            || self.timeout_seconds == 0
        {
            return Err(AgentDomainError::IncompleteManifest);
        }
        for value in self
            .capabilities
            .iter()
            .chain(&self.tool_refs)
            .chain(&self.knowledge_scopes)
            .chain(&self.evaluation_refs)
        {
            if !valid_text(value, 256) {
                return Err(AgentDomainError::IncompleteManifest);
            }
        }
        Ok(())
    }

    pub fn permits_scope(&self, requested: &AuthorityScope) -> bool {
        self.organization_scopes
            .iter()
            .any(|allowed| allowed.contains(requested))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentVersion {
    pub manifest: AgentManifest,
    pub status: AgentVersionStatus,
    pub approved_by: Option<String>,
    pub approved_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentDefinition {
    id: AgentDefinitionId,
    scope: AuthorityScope,
    agent_principal_ref: String,
    business_owner_ref: String,
    technical_owner_ref: String,
    name: String,
    status: AgentDefinitionStatus,
    versions: BTreeMap<AgentVersionId, AgentVersion>,
    version: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AgentDefinition {
    #[allow(clippy::too_many_arguments)]
    pub fn register(
        id: AgentDefinitionId,
        scope: AuthorityScope,
        agent_principal_ref: String,
        business_owner_ref: String,
        technical_owner_ref: String,
        name: String,
        at: DateTime<Utc>,
    ) -> Result<Self, AgentDomainError> {
        if scope.validate().is_err() {
            return Err(AgentDomainError::InvalidScope);
        }
        if !valid_text(&agent_principal_ref, 256)
            || !valid_text(&business_owner_ref, 256)
            || !valid_text(&technical_owner_ref, 256)
            || !valid_text(&name, 160)
        {
            return Err(AgentDomainError::MissingOwner);
        }
        Ok(Self {
            id,
            scope,
            agent_principal_ref,
            business_owner_ref,
            technical_owner_ref,
            name,
            status: AgentDefinitionStatus::Draft,
            versions: BTreeMap::new(),
            version: 1,
            created_at: at,
            updated_at: at,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rehydrate(
        id: AgentDefinitionId,
        scope: AuthorityScope,
        agent_principal_ref: String,
        business_owner_ref: String,
        technical_owner_ref: String,
        name: String,
        status: AgentDefinitionStatus,
        versions: BTreeMap<AgentVersionId, AgentVersion>,
        version: u64,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AgentDomainError> {
        let mut definition = Self::register(
            id,
            scope,
            agent_principal_ref,
            business_owner_ref,
            technical_owner_ref,
            name,
            created_at,
        )?;
        if version == 0
            || updated_at < created_at
            || versions.iter().any(|(id, agent_version)| {
                *id != agent_version.manifest.version_id
                    || agent_version.manifest.validate().is_err()
                    || (agent_version.status == AgentVersionStatus::Approved
                        && (agent_version.approved_by.is_none()
                            || agent_version.approved_at.is_none()))
                    || agent_version
                        .approved_at
                        .is_some_and(|approved_at| approved_at > updated_at)
            })
            || (status == AgentDefinitionStatus::Approved
                && !versions
                    .values()
                    .any(|version| version.status == AgentVersionStatus::Approved))
            || (status == AgentDefinitionStatus::Retired
                && versions
                    .values()
                    .any(|version| version.status != AgentVersionStatus::Retired))
        {
            return Err(AgentDomainError::InvalidVersion);
        }
        definition.status = status;
        definition.versions = versions;
        definition.version = version;
        definition.updated_at = updated_at;
        Ok(definition)
    }

    pub fn add_draft_version(
        &mut self,
        manifest: AgentManifest,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        if self.status == AgentDefinitionStatus::Retired
            || self.versions.contains_key(&manifest.version_id)
        {
            return Err(AgentDomainError::AgentUnavailable);
        }
        manifest.validate()?;
        self.advance(at)?;
        self.versions.insert(
            manifest.version_id,
            AgentVersion {
                manifest,
                status: AgentVersionStatus::Draft,
                approved_by: None,
                approved_at: None,
            },
        );
        Ok(())
    }

    pub fn approve_version(
        &mut self,
        version_id: AgentVersionId,
        decision: HumanDecisionProof,
    ) -> Result<(), AgentDomainError> {
        if self.status == AgentDefinitionStatus::Retired
            || decision.authority_scope() != &self.scope
            || decision.decision_ref() != agent_version_approval_decision_ref(self.id, version_id)
        {
            return Err(AgentDomainError::AgentUnavailable);
        }
        let version = self
            .versions
            .get(&version_id)
            .ok_or(AgentDomainError::IncompleteManifest)?;
        if version.status != AgentVersionStatus::Draft {
            return Err(AgentDomainError::InvalidAssignmentTransition);
        }
        version.manifest.validate()?;
        let approver = decision.principal_ref().to_owned();
        let at = decision.decided_at();
        self.advance(at)?;
        let version = self
            .versions
            .get_mut(&version_id)
            .expect("agent version was checked before aggregate advance");
        version.status = AgentVersionStatus::Approved;
        version.approved_by = Some(approver);
        version.approved_at = Some(at);
        self.status = AgentDefinitionStatus::Approved;
        Ok(())
    }

    pub fn suspend(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        if !matches!(
            self.status,
            AgentDefinitionStatus::Approved | AgentDefinitionStatus::Draft
        ) {
            return Err(AgentDomainError::AgentUnavailable);
        }
        self.advance(at)?;
        self.status = AgentDefinitionStatus::Suspended;
        for version in self.versions.values_mut() {
            if version.status == AgentVersionStatus::Approved {
                version.status = AgentVersionStatus::Suspended;
            }
        }
        Ok(())
    }

    pub fn retire(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        if self.status == AgentDefinitionStatus::Retired {
            return Err(AgentDomainError::AgentUnavailable);
        }
        self.advance(at)?;
        self.status = AgentDefinitionStatus::Retired;
        for version in self.versions.values_mut() {
            version.status = AgentVersionStatus::Retired;
        }
        Ok(())
    }

    pub fn pin(&self, version_id: AgentVersionId) -> Result<AgentPin, AgentDomainError> {
        if self.status != AgentDefinitionStatus::Approved {
            return Err(AgentDomainError::AgentUnavailable);
        }
        let version = self
            .versions
            .get(&version_id)
            .filter(|version| version.status == AgentVersionStatus::Approved)
            .ok_or(AgentDomainError::VersionNotApproved)?;
        Ok(AgentPin {
            definition_id: self.id,
            agent_principal_ref: self.agent_principal_ref.clone(),
            manifest: version.manifest.clone(),
        })
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        if at < self.updated_at {
            return Err(AgentDomainError::InvalidAssignmentTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(AgentDomainError::ArithmeticOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> AgentDefinitionId {
        self.id
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn agent_principal_ref(&self) -> &str {
        &self.agent_principal_ref
    }
    pub fn business_owner_ref(&self) -> &str {
        &self.business_owner_ref
    }
    pub fn technical_owner_ref(&self) -> &str {
        &self.technical_owner_ref
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub const fn status(&self) -> AgentDefinitionStatus {
        self.status
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
    pub fn versions(&self) -> &BTreeMap<AgentVersionId, AgentVersion> {
        &self.versions
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPin {
    pub definition_id: AgentDefinitionId,
    pub agent_principal_ref: String,
    pub manifest: AgentManifest,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InvocationOrigin {
    Mention,
    Assign,
    Run,
    Delegation,
    Compare,
    Schedule,
    Event,
}

impl InvocationOrigin {
    pub const fn is_proactive(self) -> bool {
        matches!(self, Self::Schedule | Self::Event)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RootInvocation {
    pub initiating_principal_ref: String,
    pub initiating_principal_kind: PrincipalKind,
    pub origin: InvocationOrigin,
    pub source_action_ref: String,
    pub idempotency_key: String,
}

impl RootInvocation {
    pub fn validate(&self) -> Result<(), AgentDomainError> {
        if self.initiating_principal_kind != PrincipalKind::Human {
            return Err(AgentDomainError::RootMustBeHuman);
        }
        if self.origin.is_proactive() {
            return Err(AgentDomainError::ProactiveTriggerDenied);
        }
        if !matches!(
            self.origin,
            InvocationOrigin::Mention
                | InvocationOrigin::Assign
                | InvocationOrigin::Run
                | InvocationOrigin::Compare
        ) || !valid_text(&self.initiating_principal_ref, 256)
            || !valid_text(&self.source_action_ref, 256)
            || !valid_text(&self.idempotency_key, 256)
        {
            return Err(AgentDomainError::InvalidText);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssignmentState {
    Queued,
    Running,
    Waiting,
    Completed,
    Failed,
    Cancelled,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExpiredLeaseRecovery {
    Requeued,
    FailedUnsafeCheckpoint,
    FailedAttemptLimit,
    FailedBudgetUnavailable,
}

impl AssignmentState {
    pub const fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentLease {
    pub token: Uuid,
    pub worker_ref: String,
    pub acquired_at: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkpoint {
    pub checkpoint_id: CheckpointId,
    pub step: u32,
    pub state_digest: String,
    pub retry_safe: bool,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssignmentFailure {
    pub code: String,
    pub retryable: bool,
    pub detail_digest: String,
    pub failed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DelegationLimits {
    pub max_depth: u16,
    pub max_hops: u16,
    pub max_fan_out: u16,
    pub max_duration_seconds: u32,
}

impl DelegationLimits {
    pub fn validate(&self) -> Result<(), AgentDomainError> {
        if self.max_depth == 0
            || self.max_hops == 0
            || self.max_fan_out == 0
            || self.max_duration_seconds == 0
        {
            Err(AgentDomainError::DelegationLimitExceeded)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Assignment {
    id: AssignmentId,
    root_assignment_id: AssignmentId,
    parent_assignment_id: Option<AssignmentId>,
    root_invocation: RootInvocation,
    pin: AgentPin,
    scope: AuthorityScope,
    state: AssignmentState,
    lineage: Vec<AgentDefinitionId>,
    depth: u16,
    hops: u16,
    attempt: u16,
    lease: Option<AssignmentLease>,
    checkpoints: Vec<Checkpoint>,
    failure: Option<AssignmentFailure>,
    output_digest: Option<String>,
    cancellation_reason: Option<String>,
    budget_reservation_id: BudgetReservationId,
    version: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl Assignment {
    #[allow(clippy::too_many_arguments)]
    pub fn start_root(
        id: AssignmentId,
        invocation: RootInvocation,
        pin: AgentPin,
        scope: AuthorityScope,
        budget_reservation_id: BudgetReservationId,
        at: DateTime<Utc>,
    ) -> Result<Self, AgentDomainError> {
        invocation.validate()?;
        pin.manifest.validate()?;
        if scope.validate().is_err() || !pin.manifest.permits_scope(&scope) {
            return Err(AgentDomainError::DelegationScopeExpansion);
        }
        let definition_id = pin.definition_id;
        Ok(Self {
            id,
            root_assignment_id: id,
            parent_assignment_id: None,
            root_invocation: invocation,
            pin,
            scope,
            state: AssignmentState::Queued,
            lineage: vec![definition_id],
            depth: 0,
            hops: 0,
            attempt: 0,
            lease: None,
            checkpoints: Vec::new(),
            failure: None,
            output_digest: None,
            cancellation_reason: None,
            budget_reservation_id,
            version: 1,
            created_at: at,
            updated_at: at,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rehydrate(
        id: AssignmentId,
        root_assignment_id: AssignmentId,
        parent_assignment_id: Option<AssignmentId>,
        root_invocation: RootInvocation,
        pin: AgentPin,
        scope: AuthorityScope,
        state: AssignmentState,
        lineage: Vec<AgentDefinitionId>,
        depth: u16,
        hops: u16,
        attempt: u16,
        lease: Option<AssignmentLease>,
        checkpoints: Vec<Checkpoint>,
        failure: Option<AssignmentFailure>,
        output_digest: Option<String>,
        cancellation_reason: Option<String>,
        budget_reservation_id: BudgetReservationId,
        version: u64,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AgentDomainError> {
        root_invocation.validate()?;
        pin.manifest.validate()?;
        let parent_shape_valid = if parent_assignment_id.is_none() {
            id == root_assignment_id && depth == 0 && hops == 0
        } else {
            id != root_assignment_id && depth > 0 && hops > 0
        };
        let lease_valid = match (&state, &lease) {
            (AssignmentState::Running, Some(lease)) => {
                !lease.token.is_nil()
                    && valid_text(&lease.worker_ref, 256)
                    && lease.expires_at > lease.acquired_at
                    && lease.acquired_at >= created_at
            }
            (AssignmentState::Running, None) => false,
            (_, None) => true,
            (_, Some(_)) => false,
        };
        let checkpoints_valid = checkpoints.iter().enumerate().all(|(index, checkpoint)| {
            checkpoint.step == u32::try_from(index + 1).unwrap_or(u32::MAX)
                && validate_digest(&checkpoint.state_digest)
                && checkpoint.created_at >= created_at
                && checkpoint.created_at <= updated_at
        });
        if version == 0
            || updated_at < created_at
            || scope.validate().is_err()
            || !pin.manifest.permits_scope(&scope)
            || !parent_shape_valid
            || lineage.len() != usize::from(depth) + 1
            || lineage.last() != Some(&pin.definition_id)
            || lineage.iter().collect::<BTreeSet<_>>().len() != lineage.len()
            || attempt > pin.manifest.max_attempts
            || !lease_valid
            || !checkpoints_valid
            || failure.as_ref().is_some_and(|failure| {
                !valid_text(&failure.code, 128)
                    || !validate_digest(&failure.detail_digest)
                    || failure.failed_at > updated_at
            })
            || (state == AssignmentState::Failed && failure.is_none())
            || (failure.is_some()
                && !matches!(state, AssignmentState::Failed | AssignmentState::Cancelled))
            || output_digest
                .as_deref()
                .is_some_and(|digest| !validate_digest(digest))
            || (state == AssignmentState::Completed) != output_digest.is_some()
            || cancellation_reason.as_deref().is_some_and(|reason| {
                state != AssignmentState::Cancelled || !valid_text(reason, 512)
            })
        {
            return Err(AgentDomainError::InvalidVersion);
        }
        Ok(Self {
            id,
            root_assignment_id,
            parent_assignment_id,
            root_invocation,
            pin,
            scope,
            state,
            lineage,
            depth,
            hops,
            attempt,
            lease,
            checkpoints,
            failure,
            output_digest,
            cancellation_reason,
            budget_reservation_id,
            version,
            created_at,
            updated_at,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn delegate(
        &self,
        child_id: AssignmentId,
        child_pin: AgentPin,
        child_scope: AuthorityScope,
        child_budget_reservation_id: BudgetReservationId,
        sibling_count: u16,
        limits: &DelegationLimits,
        at: DateTime<Utc>,
    ) -> Result<Self, AgentDomainError> {
        limits.validate()?;
        if self.state != AssignmentState::Running {
            return Err(AgentDomainError::InvalidAssignmentTransition);
        }
        if self.lineage.contains(&child_pin.definition_id) {
            return Err(AgentDomainError::DelegationCycle);
        }
        let depth = self
            .depth
            .checked_add(1)
            .ok_or(AgentDomainError::ArithmeticOverflow)?;
        let hops = self
            .hops
            .checked_add(1)
            .ok_or(AgentDomainError::ArithmeticOverflow)?;
        if depth > limits.max_depth || hops > limits.max_hops || sibling_count >= limits.max_fan_out
        {
            return Err(AgentDomainError::DelegationLimitExceeded);
        }
        if !self.scope.contains(&child_scope) || !child_pin.manifest.permits_scope(&child_scope) {
            return Err(AgentDomainError::DelegationScopeExpansion);
        }
        let mut lineage = self.lineage.clone();
        lineage.push(child_pin.definition_id);
        Ok(Self {
            id: child_id,
            root_assignment_id: self.root_assignment_id,
            parent_assignment_id: Some(self.id),
            root_invocation: self.root_invocation.clone(),
            pin: child_pin,
            scope: child_scope,
            state: AssignmentState::Queued,
            lineage,
            depth,
            hops,
            attempt: 0,
            lease: None,
            checkpoints: Vec::new(),
            failure: None,
            output_digest: None,
            cancellation_reason: None,
            budget_reservation_id: child_budget_reservation_id,
            version: 1,
            created_at: at,
            updated_at: at,
        })
    }

    pub fn claim(
        &mut self,
        token: Uuid,
        worker_ref: String,
        expires_at: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        if self.state != AssignmentState::Queued
            || token.is_nil()
            || !valid_text(&worker_ref, 256)
            || expires_at <= at
        {
            return Err(AgentDomainError::InvalidAssignmentTransition);
        }
        self.advance(at)?;
        self.state = AssignmentState::Running;
        self.attempt = self
            .attempt
            .checked_add(1)
            .ok_or(AgentDomainError::ArithmeticOverflow)?;
        self.lease = Some(AssignmentLease {
            token,
            worker_ref,
            acquired_at: at,
            expires_at,
        });
        Ok(())
    }

    pub fn checkpoint(
        &mut self,
        lease_token: Uuid,
        checkpoint: Checkpoint,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        self.require_lease(lease_token, at)?;
        if !validate_digest(&checkpoint.state_digest)
            || checkpoint.created_at != at
            || self
                .checkpoints
                .last()
                .is_some_and(|prior| checkpoint.step <= prior.step)
        {
            return Err(AgentDomainError::InvalidAssignmentTransition);
        }
        self.advance(at)?;
        self.checkpoints.push(checkpoint);
        Ok(())
    }

    pub fn wait_for_input(
        &mut self,
        lease_token: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        self.require_lease(lease_token, at)?;
        self.advance(at)?;
        self.state = AssignmentState::Waiting;
        self.lease = None;
        Ok(())
    }

    pub fn resume(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        if self.state != AssignmentState::Waiting {
            return Err(AgentDomainError::InvalidAssignmentTransition);
        }
        self.advance(at)?;
        self.state = AssignmentState::Queued;
        Ok(())
    }

    pub fn complete(
        &mut self,
        lease_token: Uuid,
        output_digest: String,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        self.require_lease(lease_token, at)?;
        if !validate_digest(&output_digest) {
            return Err(AgentDomainError::InvalidText);
        }
        self.advance(at)?;
        self.state = AssignmentState::Completed;
        self.output_digest = Some(output_digest);
        self.lease = None;
        Ok(())
    }

    pub fn fail(
        &mut self,
        lease_token: Uuid,
        failure: AssignmentFailure,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        self.require_lease(lease_token, at)?;
        if !valid_text(&failure.code, 128)
            || !validate_digest(&failure.detail_digest)
            || failure.failed_at != at
        {
            return Err(AgentDomainError::InvalidText);
        }
        self.advance(at)?;
        self.state = AssignmentState::Failed;
        self.failure = Some(failure);
        self.lease = None;
        Ok(())
    }

    pub fn retry(
        &mut self,
        security_revoked: bool,
        remaining_budget: bool,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        let retryable = self
            .failure
            .as_ref()
            .is_some_and(|failure| failure.retryable);
        if self.state != AssignmentState::Failed
            || !retryable
            || security_revoked
            || !remaining_budget
            || self.attempt >= self.pin.manifest.max_attempts
        {
            return Err(AgentDomainError::RetryDenied);
        }
        self.advance(at)?;
        self.state = AssignmentState::Queued;
        self.failure = None;
        Ok(())
    }

    pub fn cancel(&mut self, reason: String, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        if self.state.is_terminal() || !valid_text(&reason, 512) {
            return Err(AgentDomainError::InvalidAssignmentTransition);
        }
        self.advance(at)?;
        self.state = AssignmentState::Cancelled;
        self.cancellation_reason = Some(reason);
        self.lease = None;
        Ok(())
    }

    /// Recovers only a running assignment whose durable lease has expired.
    /// A retry-safe checkpoint (or no checkpoint) may resume under the same
    /// pinned agent/model/tool/policy manifest. Unsafe checkpoints, exhausted
    /// attempts, and revoked budget become explicit terminal failures rather
    /// than adapter-authored state rewrites.
    pub fn recover_expired_lease(
        &mut self,
        remaining_budget: bool,
        at: DateTime<Utc>,
    ) -> Result<ExpiredLeaseRecovery, AgentDomainError> {
        let lease = self
            .lease
            .as_ref()
            .filter(|lease| self.state == AssignmentState::Running && lease.expires_at <= at)
            .ok_or(AgentDomainError::InvalidLease)?;
        let _expired_lease_token = lease.token;
        let outcome = if !remaining_budget {
            ExpiredLeaseRecovery::FailedBudgetUnavailable
        } else if self.attempt >= self.pin.manifest.max_attempts {
            ExpiredLeaseRecovery::FailedAttemptLimit
        } else if self
            .checkpoints
            .last()
            .is_some_and(|checkpoint| !checkpoint.retry_safe)
        {
            ExpiredLeaseRecovery::FailedUnsafeCheckpoint
        } else {
            ExpiredLeaseRecovery::Requeued
        };
        self.advance(at)?;
        self.lease = None;
        match outcome {
            ExpiredLeaseRecovery::Requeued => {
                self.state = AssignmentState::Queued;
            }
            ExpiredLeaseRecovery::FailedUnsafeCheckpoint
            | ExpiredLeaseRecovery::FailedAttemptLimit
            | ExpiredLeaseRecovery::FailedBudgetUnavailable => {
                let code = match outcome {
                    ExpiredLeaseRecovery::FailedUnsafeCheckpoint => {
                        "lease_expired_unsafe_checkpoint"
                    }
                    ExpiredLeaseRecovery::FailedAttemptLimit => "lease_expired_attempt_limit",
                    ExpiredLeaseRecovery::FailedBudgetUnavailable => {
                        "lease_expired_budget_unavailable"
                    }
                    ExpiredLeaseRecovery::Requeued => unreachable!("matched terminal outcome"),
                };
                self.state = AssignmentState::Failed;
                self.failure = Some(AssignmentFailure {
                    code: code.into(),
                    retryable: false,
                    detail_digest: "0".repeat(64),
                    failed_at: at,
                });
            }
        }
        Ok(outcome)
    }

    fn require_lease(&self, token: Uuid, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        if self.state != AssignmentState::Running
            || self
                .lease
                .as_ref()
                .is_none_or(|lease| lease.token != token || lease.expires_at <= at)
        {
            Err(AgentDomainError::InvalidLease)
        } else {
            Ok(())
        }
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        if at < self.updated_at {
            return Err(AgentDomainError::InvalidAssignmentTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(AgentDomainError::ArithmeticOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> AssignmentId {
        self.id
    }
    pub const fn root_assignment_id(&self) -> AssignmentId {
        self.root_assignment_id
    }
    pub const fn parent_assignment_id(&self) -> Option<AssignmentId> {
        self.parent_assignment_id
    }
    pub const fn state(&self) -> AssignmentState {
        self.state
    }
    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn pin(&self) -> &AgentPin {
        &self.pin
    }
    pub fn lineage(&self) -> &[AgentDefinitionId] {
        &self.lineage
    }
    pub const fn attempt(&self) -> u16 {
        self.attempt
    }
    pub const fn depth(&self) -> u16 {
        self.depth
    }
    pub const fn hops(&self) -> u16 {
        self.hops
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn lease(&self) -> Option<&AssignmentLease> {
        self.lease.as_ref()
    }
    pub const fn failure(&self) -> Option<&AssignmentFailure> {
        self.failure.as_ref()
    }
    pub fn output_digest(&self) -> Option<&str> {
        self.output_digest.as_deref()
    }
    pub fn cancellation_reason(&self) -> Option<&str> {
        self.cancellation_reason.as_deref()
    }
    pub fn checkpoints(&self) -> &[Checkpoint] {
        &self.checkpoints
    }
    pub const fn budget_reservation_id(&self) -> BudgetReservationId {
        self.budget_reservation_id
    }
    pub const fn root_invocation(&self) -> &RootInvocation {
        &self.root_invocation
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BudgetLayerKind {
    Holding,
    Workspace,
    Agent,
    Principal,
    RootAssignment,
}

impl BudgetLayerKind {
    pub const ALL: [Self; 5] = [
        Self::Holding,
        Self::Workspace,
        Self::Agent,
        Self::Principal,
        Self::RootAssignment,
    ];
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetLayer {
    pub kind: BudgetLayerKind,
    pub subject_ref: String,
    pub remaining_units: u64,
    pub remaining_cost_micros: u64,
    pub active_concurrency: u32,
    pub concurrency_limit: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetRequest {
    pub estimated_units: u64,
    pub estimated_cost_micros: u64,
    pub queue_priority: u16,
    pub stop_after_units: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BudgetDecision {
    Reserved(BudgetReservation),
    ApprovalRequired {
        exceeded_layers: Vec<BudgetLayerKind>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BudgetReservation {
    pub id: BudgetReservationId,
    pub request: BudgetRequest,
    pub layers: Vec<BudgetLayer>,
    pub reserved_at: DateTime<Utc>,
}

impl BudgetReservation {
    pub fn decide(
        id: BudgetReservationId,
        request: BudgetRequest,
        layers: Vec<BudgetLayer>,
        at: DateTime<Utc>,
    ) -> Result<BudgetDecision, AgentDomainError> {
        if request.estimated_units == 0
            || request.stop_after_units < request.estimated_units
            || layers.len() != BudgetLayerKind::ALL.len()
        {
            return Err(AgentDomainError::InvalidBudgetLayers);
        }
        let kinds = layers
            .iter()
            .map(|layer| layer.kind)
            .collect::<BTreeSet<_>>();
        if kinds.len() != BudgetLayerKind::ALL.len()
            || BudgetLayerKind::ALL
                .iter()
                .any(|kind| !kinds.contains(kind))
            || layers.iter().any(|layer| {
                !valid_text(&layer.subject_ref, 256)
                    || layer.concurrency_limit == 0
                    || layer.active_concurrency > layer.concurrency_limit
            })
        {
            return Err(AgentDomainError::InvalidBudgetLayers);
        }
        let exceeded_layers = layers
            .iter()
            .filter(|layer| {
                layer.remaining_units < request.estimated_units
                    || layer.remaining_cost_micros < request.estimated_cost_micros
                    || layer.active_concurrency >= layer.concurrency_limit
            })
            .map(|layer| layer.kind)
            .collect::<Vec<_>>();
        if exceeded_layers.is_empty() {
            Ok(BudgetDecision::Reserved(Self {
                id,
                request,
                layers,
                reserved_at: at,
            }))
        } else {
            Ok(BudgetDecision::ApprovalRequired { exceeded_layers })
        }
    }

    pub fn reconcile(&self, actual_units: u64, actual_cost_micros: u64) -> UsageReconciliation {
        UsageReconciliation {
            actual_units,
            actual_cost_micros,
            exceeded_reservation: actual_units > self.request.estimated_units
                || actual_cost_micros > self.request.estimated_cost_micros
                || actual_units > self.request.stop_after_units,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct UsageReconciliation {
    pub actual_units: u64,
    pub actual_cost_micros: u64,
    pub exceeded_reservation: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolCandidate {
    pub pin: AgentPin,
    pub active_assignments: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolRoutingDecision {
    pub id: PoolRoutingDecisionId,
    pub pool_id: AgentPoolId,
    pub pool_version: u64,
    pub root_action_ref: String,
    pub capability: String,
    pub requested_scope: AuthorityScope,
    pub candidate_versions: Vec<AgentVersionId>,
    pub selected: AgentPin,
    pub selected_versions: Vec<AgentVersionId>,
    pub compare_requested: bool,
    pub previewed_cost_micros: Option<u64>,
    pub initiating_human_ref: Option<String>,
    pub decision_digest: String,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PoolRouteRequest {
    pub decision_id: PoolRoutingDecisionId,
    pub root_action_ref: String,
    pub capability: String,
    pub requested_scope: AuthorityScope,
    pub decided_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentPool {
    id: AgentPoolId,
    scope: AuthorityScope,
    name: String,
    member_versions: BTreeSet<AgentVersionId>,
    enabled: bool,
    version: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AgentPool {
    pub fn register(
        id: AgentPoolId,
        scope: AuthorityScope,
        name: String,
        member_versions: BTreeSet<AgentVersionId>,
        at: DateTime<Utc>,
    ) -> Result<Self, AgentDomainError> {
        if scope.validate().is_err()
            || !valid_text(&name, 128)
            || member_versions.is_empty()
            || member_versions.len() > 64
        {
            return Err(AgentDomainError::NoEligibleAgent);
        }
        Ok(Self {
            id,
            scope,
            name,
            member_versions,
            enabled: true,
            version: 1,
            created_at: at,
            updated_at: at,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rehydrate(
        id: AgentPoolId,
        scope: AuthorityScope,
        name: String,
        member_versions: BTreeSet<AgentVersionId>,
        enabled: bool,
        version: u64,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AgentDomainError> {
        let mut pool = Self::register(id, scope, name, member_versions, created_at)?;
        if version == 0 || updated_at < created_at {
            return Err(AgentDomainError::InvalidVersion);
        }
        pool.enabled = enabled;
        pool.version = version;
        pool.updated_at = updated_at;
        Ok(pool)
    }

    pub fn replace_members(
        &mut self,
        member_versions: BTreeSet<AgentVersionId>,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        if member_versions.is_empty()
            || member_versions.len() > 64
            || at < self.updated_at
            || member_versions == self.member_versions
        {
            return Err(AgentDomainError::NoEligibleAgent);
        }
        self.member_versions = member_versions;
        self.advance(at)
    }

    pub fn set_enabled(
        &mut self,
        enabled: bool,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        if enabled == self.enabled || at < self.updated_at {
            return Err(AgentDomainError::NoEligibleAgent);
        }
        self.enabled = enabled;
        self.advance(at)
    }

    pub fn route(
        &self,
        request: PoolRouteRequest,
        mut candidates: Vec<PoolCandidate>,
    ) -> Result<PoolRoutingDecision, AgentDomainError> {
        if !self.enabled
            || !valid_text(&request.capability, 128)
            || !valid_text(&request.root_action_ref, 256)
            || !self.scope.contains(&request.requested_scope)
            || request.decided_at < self.updated_at
        {
            return Err(AgentDomainError::NoEligibleAgent);
        }
        candidates.retain(|candidate| {
            self.member_versions
                .contains(&candidate.pin.manifest.version_id)
                && candidate
                    .pin
                    .manifest
                    .capabilities
                    .contains(&request.capability)
                && candidate
                    .pin
                    .manifest
                    .permits_scope(&request.requested_scope)
        });
        let unique_candidates = candidates
            .iter()
            .map(|candidate| candidate.pin.manifest.version_id)
            .collect::<BTreeSet<_>>();
        if unique_candidates.len() != candidates.len() {
            return Err(AgentDomainError::NoEligibleAgent);
        }
        candidates.sort_by_key(|candidate| {
            (
                candidate.active_assignments,
                candidate.pin.definition_id,
                candidate.pin.manifest.version_number,
            )
        });
        let selected = candidates
            .first()
            .ok_or(AgentDomainError::NoEligibleAgent)?
            .pin
            .clone();
        let candidate_versions = candidates
            .iter()
            .map(|candidate| candidate.pin.manifest.version_id)
            .collect::<Vec<_>>();
        let selected_versions = vec![selected.manifest.version_id];
        let decision_digest = routing_digest(
            self,
            &request,
            &candidate_versions,
            &selected_versions,
            false,
            None,
            None,
        );
        Ok(PoolRoutingDecision {
            id: request.decision_id,
            pool_id: self.id,
            pool_version: self.version,
            root_action_ref: request.root_action_ref,
            capability: request.capability,
            requested_scope: request.requested_scope,
            candidate_versions,
            selected,
            selected_versions,
            compare_requested: false,
            previewed_cost_micros: None,
            initiating_human_ref: None,
            decision_digest,
            decided_at: request.decided_at,
        })
    }

    pub fn route_compare(
        &self,
        request: PoolRouteRequest,
        compare: CompareRequest,
        max_fan_out: u16,
    ) -> Result<PoolRoutingDecision, AgentDomainError> {
        compare.validate(max_fan_out)?;
        if !self.enabled
            || !valid_text(&request.capability, 128)
            || !valid_text(&request.root_action_ref, 256)
            || !self.scope.contains(&request.requested_scope)
            || request.decided_at < self.updated_at
        {
            return Err(AgentDomainError::NoEligibleAgent);
        }
        let mut selected = compare.pins;
        if selected.iter().any(|pin| {
            !self.member_versions.contains(&pin.manifest.version_id)
                || !pin.manifest.capabilities.contains(&request.capability)
                || !pin.manifest.permits_scope(&request.requested_scope)
        }) {
            return Err(AgentDomainError::NoEligibleAgent);
        }
        selected.sort_by_key(|pin| {
            (
                pin.definition_id,
                pin.manifest.version_number,
                pin.manifest.version_id,
            )
        });
        let selected_versions = selected
            .iter()
            .map(|pin| pin.manifest.version_id)
            .collect::<Vec<_>>();
        let decision_digest = routing_digest(
            self,
            &request,
            &selected_versions,
            &selected_versions,
            true,
            Some(compare.previewed_cost_micros),
            Some(&compare.initiating_human_ref),
        );
        Ok(PoolRoutingDecision {
            id: request.decision_id,
            pool_id: self.id,
            pool_version: self.version,
            root_action_ref: request.root_action_ref,
            capability: request.capability,
            requested_scope: request.requested_scope,
            candidate_versions: selected_versions.clone(),
            selected: selected
                .first()
                .ok_or(AgentDomainError::InvalidCompare)?
                .clone(),
            selected_versions,
            compare_requested: true,
            previewed_cost_micros: Some(compare.previewed_cost_micros),
            initiating_human_ref: Some(compare.initiating_human_ref),
            decision_digest,
            decided_at: request.decided_at,
        })
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(AgentDomainError::ArithmeticOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> AgentPoolId {
        self.id
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn name(&self) -> &str {
        &self.name
    }
    pub fn member_versions(&self) -> &BTreeSet<AgentVersionId> {
        &self.member_versions
    }
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

fn routing_digest(
    pool: &AgentPool,
    request: &PoolRouteRequest,
    candidates: &[AgentVersionId],
    selected: &[AgentVersionId],
    compare: bool,
    previewed_cost_micros: Option<u64>,
    initiating_human_ref: Option<&str>,
) -> String {
    let mut digest = Sha256::new();
    digest.update(b"erpx.agent.pool-routing.v1");
    digest.update(pool.id.as_uuid().as_bytes());
    digest.update(pool.version.to_be_bytes());
    digest.update(request.root_action_ref.as_bytes());
    digest.update(request.capability.as_bytes());
    digest.update(scope_key(&request.requested_scope).as_bytes());
    for id in candidates {
        digest.update(id.as_uuid().as_bytes());
    }
    digest.update([u8::from(compare)]);
    for id in selected {
        digest.update(id.as_uuid().as_bytes());
    }
    digest.update(previewed_cost_micros.unwrap_or_default().to_be_bytes());
    if let Some(principal) = initiating_human_ref {
        digest.update(principal.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

fn scope_key(scope: &AuthorityScope) -> String {
    match scope {
        AuthorityScope::Platform => "platform".into(),
        AuthorityScope::Holding { holding_id } => format!("h:{holding_id}"),
        AuthorityScope::Company {
            holding_id,
            company_id,
        } => format!("h:{holding_id}/c:{company_id}"),
        AuthorityScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => format!("h:{holding_id}/c:{company_id}/b:{branch_id}"),
        AuthorityScope::Subtree {
            holding_id,
            organization_node_id,
        } => format!("h:{holding_id}/node:{organization_node_id}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompareRequest {
    pub initiating_human_ref: String,
    pub confirmed: bool,
    pub pins: Vec<AgentPin>,
    pub previewed_cost_micros: u64,
}

impl CompareRequest {
    pub fn validate(&self, max_fan_out: u16) -> Result<(), AgentDomainError> {
        let unique = self
            .pins
            .iter()
            .map(|pin| pin.manifest.version_id)
            .collect::<BTreeSet<_>>();
        if !self.confirmed
            || !valid_text(&self.initiating_human_ref, 256)
            || self.pins.len() < 2
            || self.pins.len() > usize::from(max_fan_out)
            || unique.len() != self.pins.len()
            || self.previewed_cost_micros == 0
        {
            Err(AgentDomainError::InvalidCompare)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PayloadManifest {
    pub payload_digest: String,
    pub classification: DataClassification,
    pub field_refs: BTreeSet<String>,
    pub document_refs: BTreeSet<RecordId>,
    pub authorization_checked_at: DateTime<Utc>,
    pub contains_secret: bool,
    pub residency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderPolicy {
    pub id: ProviderPolicyId,
    pub agent_definition_id: AgentDefinitionId,
    pub agent_version_id: AgentVersionId,
    pub policy_version: u64,
    pub provider_ref: String,
    pub external: bool,
    pub approved: bool,
    pub dpa_ref: Option<String>,
    pub allowed_residencies: BTreeSet<String>,
    pub allowed_models: BTreeSet<String>,
    pub allowed_use_cases: BTreeSet<String>,
    pub maximum_classification: DataClassification,
    pub policy_digest: String,
    pub approved_evaluation_refs: BTreeSet<String>,
    pub freshness_checked_at: DateTime<Utc>,
    pub fresh_until: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderPolicyDecision {
    pub provider_ref: String,
    pub model_ref: String,
    pub policy_version: u64,
    pub payload_digest: String,
    pub policy_digest: String,
    pub authorized_at: DateTime<Utc>,
}

impl ProviderPolicy {
    pub fn authorize(
        &self,
        pin: &AgentPin,
        payload: &PayloadManifest,
        at: DateTime<Utc>,
    ) -> Result<ProviderPolicyDecision, AgentDomainError> {
        let dpa_ok = !self.external
            || self
                .dpa_ref
                .as_deref()
                .is_some_and(|value| valid_text(value, 256));
        if !self.approved
            || self.policy_version == 0
            || self.agent_definition_id != pin.definition_id
            || self.agent_version_id != pin.manifest.version_id
            || self.expires_at <= at
            || self.freshness_checked_at > at
            || self.fresh_until <= at
            || self.provider_ref != pin.manifest.provider_ref
            || self.policy_digest != pin.manifest.policy_digest
            || !validate_digest(&payload.payload_digest)
            || payload.contains_secret
            || payload.authorization_checked_at > at
            || !payload
                .classification
                .is_permitted_by(self.maximum_classification)
            || !payload
                .classification
                .is_permitted_by(pin.manifest.maximum_classification)
            || !self.allowed_models.contains(&pin.manifest.model_ref)
            || !self.allowed_use_cases.contains(&pin.manifest.use_case)
            || self.approved_evaluation_refs != pin.manifest.evaluation_refs
            || !self.allowed_residencies.contains(&payload.residency)
            || !dpa_ok
        {
            return Err(AgentDomainError::ProviderDenied);
        }
        Ok(ProviderPolicyDecision {
            provider_ref: self.provider_ref.clone(),
            model_ref: pin.manifest.model_ref.clone(),
            policy_version: self.policy_version,
            payload_digest: payload.payload_digest.clone(),
            policy_digest: self.policy_digest.clone(),
            authorized_at: at,
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderInvocationOutcome {
    Completed,
    Failed,
    Cancelled,
}

/// Immutable provider-call provenance plus a versioned terminal result. Raw
/// prompt and output material are deliberately excluded from this aggregate.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderInvocationEvidence {
    id: ProviderInvocationId,
    assignment_id: AssignmentId,
    assignment_version: u64,
    attempt: u32,
    agent_definition_id: AgentDefinitionId,
    agent_version_id: AgentVersionId,
    provider_ref: String,
    model_ref: String,
    policy_version: u64,
    policy_digest: String,
    payload_digest: String,
    field_refs: BTreeSet<String>,
    document_refs: BTreeSet<RecordId>,
    classification: DataClassification,
    residency: String,
    output_digest: Option<String>,
    provider_units: Option<u64>,
    provider_cost_micros: Option<u64>,
    outcome: Option<ProviderInvocationOutcome>,
    version: u64,
    invoked_at: DateTime<Utc>,
    completed_at: Option<DateTime<Utc>>,
}

impl ProviderInvocationEvidence {
    #[allow(clippy::too_many_arguments)]
    pub fn begin(
        id: ProviderInvocationId,
        assignment_id: AssignmentId,
        assignment_version: u64,
        attempt: u32,
        pin: &AgentPin,
        payload: &PayloadManifest,
        decision: &ProviderPolicyDecision,
        at: DateTime<Utc>,
    ) -> Result<Self, AgentDomainError> {
        if assignment_version == 0
            || attempt == 0
            || decision.policy_version == 0
            || decision.authorized_at > at
            || decision.provider_ref != pin.manifest.provider_ref
            || decision.model_ref != pin.manifest.model_ref
            || decision.policy_digest != pin.manifest.policy_digest
            || decision.payload_digest != payload.payload_digest
            || !validate_digest(&decision.policy_digest)
            || !validate_digest(&payload.payload_digest)
            || payload.contains_secret
            || !valid_text(&payload.residency, 64)
        {
            return Err(AgentDomainError::InvalidInvocationEvidence);
        }
        Ok(Self {
            id,
            assignment_id,
            assignment_version,
            attempt,
            agent_definition_id: pin.definition_id,
            agent_version_id: pin.manifest.version_id,
            provider_ref: decision.provider_ref.clone(),
            model_ref: decision.model_ref.clone(),
            policy_version: decision.policy_version,
            policy_digest: decision.policy_digest.clone(),
            payload_digest: payload.payload_digest.clone(),
            field_refs: payload.field_refs.clone(),
            document_refs: payload.document_refs.clone(),
            classification: payload.classification,
            residency: payload.residency.clone(),
            output_digest: None,
            provider_units: None,
            provider_cost_micros: None,
            outcome: None,
            version: 1,
            invoked_at: at,
            completed_at: None,
        })
    }

    pub fn complete(
        &mut self,
        output_digest: String,
        provider_units: u64,
        provider_cost_micros: u64,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        if !validate_digest(&output_digest) || provider_units == 0 {
            return Err(AgentDomainError::InvalidInvocationEvidence);
        }
        self.finish(
            ProviderInvocationOutcome::Completed,
            Some(output_digest),
            Some(provider_units),
            Some(provider_cost_micros),
            at,
        )
    }

    pub fn fail(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        self.finish(ProviderInvocationOutcome::Failed, None, None, None, at)
    }

    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        self.finish(ProviderInvocationOutcome::Cancelled, None, None, None, at)
    }

    fn finish(
        &mut self,
        outcome: ProviderInvocationOutcome,
        output_digest: Option<String>,
        provider_units: Option<u64>,
        provider_cost_micros: Option<u64>,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        if self.outcome.is_some() || at < self.invoked_at {
            return Err(AgentDomainError::InvalidInvocationEvidence);
        }
        self.output_digest = output_digest;
        self.provider_units = provider_units;
        self.provider_cost_micros = provider_cost_micros;
        self.outcome = Some(outcome);
        self.completed_at = Some(at);
        self.version = self
            .version
            .checked_add(1)
            .ok_or(AgentDomainError::ArithmeticOverflow)?;
        Ok(())
    }

    pub const fn id(&self) -> ProviderInvocationId {
        self.id
    }
    pub const fn assignment_id(&self) -> AssignmentId {
        self.assignment_id
    }
    pub const fn assignment_version(&self) -> u64 {
        self.assignment_version
    }
    pub const fn attempt(&self) -> u32 {
        self.attempt
    }
    pub const fn agent_definition_id(&self) -> AgentDefinitionId {
        self.agent_definition_id
    }
    pub const fn agent_version_id(&self) -> AgentVersionId {
        self.agent_version_id
    }
    pub fn provider_ref(&self) -> &str {
        &self.provider_ref
    }
    pub fn model_ref(&self) -> &str {
        &self.model_ref
    }
    pub const fn policy_version(&self) -> u64 {
        self.policy_version
    }
    pub fn policy_digest(&self) -> &str {
        &self.policy_digest
    }
    pub fn payload_digest(&self) -> &str {
        &self.payload_digest
    }
    pub fn field_refs(&self) -> &BTreeSet<String> {
        &self.field_refs
    }
    pub fn document_refs(&self) -> &BTreeSet<RecordId> {
        &self.document_refs
    }
    pub const fn classification(&self) -> DataClassification {
        self.classification
    }
    pub fn residency(&self) -> &str {
        &self.residency
    }
    pub fn output_digest(&self) -> Option<&str> {
        self.output_digest.as_deref()
    }
    pub const fn provider_units(&self) -> Option<u64> {
        self.provider_units
    }
    pub const fn provider_cost_micros(&self) -> Option<u64> {
        self.provider_cost_micros
    }
    pub const fn outcome(&self) -> Option<ProviderInvocationOutcome> {
        self.outcome
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn invoked_at(&self) -> DateTime<Utc> {
        self.invoked_at
    }
    pub const fn completed_at(&self) -> Option<DateTime<Utc>> {
        self.completed_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionContext {
    pub record_id: RecordId,
    pub descriptor_version: u64,
    pub source_event_position: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SessionMessage {
    pub id: SessionMessageId,
    pub author_kind: PrincipalKind,
    pub author_ref: String,
    pub encrypted_payload_ref: String,
    pub content_digest: String,
    pub version: u64,
    pub created_at: DateTime<Utc>,
    pub published_at: Option<DateTime<Utc>>,
    pub publishing_approver_human_ref: Option<String>,
    pub publication_anchor_ref: Option<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AgentSession {
    id: AgentSessionId,
    scope: AuthorityScope,
    owner_human_ref: String,
    context: Option<SessionContext>,
    messages: Vec<SessionMessage>,
    expires_at: DateTime<Utc>,
    legal_hold: bool,
    expired_at: Option<DateTime<Utc>>,
    version: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
}

impl AgentSession {
    pub fn start(
        id: AgentSessionId,
        scope: AuthorityScope,
        owner_human_ref: String,
        context: Option<SessionContext>,
        expires_at: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<Self, AgentDomainError> {
        if scope.validate().is_err()
            || matches!(scope, AuthorityScope::Platform)
            || !valid_text(&owner_human_ref, 256)
            || expires_at <= at
            || expires_at > at + chrono::Duration::days(90)
            || context.as_ref().is_some_and(|context| {
                context.descriptor_version == 0 || context.source_event_position == 0
            })
        {
            return Err(AgentDomainError::InvalidScope);
        }
        Ok(Self {
            id,
            scope,
            owner_human_ref,
            context,
            messages: Vec::new(),
            expires_at,
            legal_hold: false,
            expired_at: None,
            version: 1,
            created_at: at,
            updated_at: at,
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn rehydrate(
        id: AgentSessionId,
        scope: AuthorityScope,
        owner_human_ref: String,
        context: Option<SessionContext>,
        messages: Vec<SessionMessage>,
        expires_at: DateTime<Utc>,
        legal_hold: bool,
        expired_at: Option<DateTime<Utc>>,
        version: u64,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, AgentDomainError> {
        let mut session = Self::start(id, scope, owner_human_ref, context, expires_at, created_at)?;
        if version == 0
            || updated_at < created_at
            || expired_at.is_some_and(|expired| expired < expires_at || expired > updated_at)
            || messages.iter().any(|message| {
                message.version == 0
                    || !valid_text(&message.author_ref, 256)
                    || !valid_text(&message.encrypted_payload_ref, 512)
                    || !validate_digest(&message.content_digest)
                    || message.created_at < created_at
                    || message.created_at > updated_at
                    || (message.author_kind == PrincipalKind::Human
                        && message.author_ref != session.owner_human_ref)
                    || message.author_kind == PrincipalKind::Service
                    || (message.published_at.is_some()
                        && (message.author_kind != PrincipalKind::Agent
                            || message.publishing_approver_human_ref.as_deref()
                                != Some(session.owner_human_ref.as_str())
                            || message.publication_anchor_ref.is_none()))
            })
        {
            return Err(AgentDomainError::InvalidVersion);
        }
        session.messages = messages;
        session.legal_hold = legal_hold;
        session.expired_at = expired_at;
        session.version = version;
        session.updated_at = updated_at;
        Ok(session)
    }

    pub fn add_message(
        &mut self,
        message: SessionMessage,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        if self.expires_at <= at || self.expired_at.is_some() {
            return Err(AgentDomainError::SessionExpired);
        }
        if !valid_text(&message.author_ref, 256)
            || !valid_text(&message.encrypted_payload_ref, 512)
            || !validate_digest(&message.content_digest)
            || message.version != 1
            || message.created_at != at
            || message.published_at.is_some()
            || message.publishing_approver_human_ref.is_some()
            || message.publication_anchor_ref.is_some()
            || self.messages.iter().any(|stored| stored.id == message.id)
            || (message.author_kind == PrincipalKind::Human
                && message.author_ref != self.owner_human_ref)
            || message.author_kind == PrincipalKind::Service
        {
            return Err(AgentDomainError::InvalidText);
        }
        self.messages.push(message);
        self.advance(at)
    }

    pub fn publish_agent_message(
        &mut self,
        message_id: SessionMessageId,
        audience_reauthorized: bool,
        publication_anchor_ref: Uuid,
        command_id: CommandId,
        decision: &HumanDecisionProof,
    ) -> Result<(), AgentDomainError> {
        let at = decision.decided_at();
        if self.expires_at <= at || self.expired_at.is_some() {
            return Err(AgentDomainError::SessionExpired);
        }
        if decision.authority_scope() != &self.scope
            || decision.principal_ref() != self.owner_human_ref
            || decision.decision_ref()
                != session_message_publication_decision_ref(
                    self.id,
                    message_id,
                    self.version,
                    command_id,
                )
            || !audience_reauthorized
            || publication_anchor_ref.is_nil()
        {
            return Err(AgentDomainError::PublicationDenied);
        }
        let message = self
            .messages
            .iter_mut()
            .find(|message| message.id == message_id)
            .ok_or(AgentDomainError::PublicationDenied)?;
        if message.author_kind != PrincipalKind::Agent || message.published_at.is_some() {
            return Err(AgentDomainError::ImpersonationDenied);
        }
        message.version = message
            .version
            .checked_add(1)
            .ok_or(AgentDomainError::ArithmeticOverflow)?;
        message.published_at = Some(at);
        message.publishing_approver_human_ref = Some(decision.principal_ref().to_owned());
        message.publication_anchor_ref = Some(publication_anchor_ref);
        self.advance(at)
    }

    pub fn expire(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        if self.legal_hold || at < self.expires_at || self.expired_at.is_some() {
            return Err(AgentDomainError::SessionExpired);
        }
        self.messages
            .retain(|message| message.published_at.is_some());
        self.expired_at = Some(at);
        self.advance(at)
    }

    pub fn set_legal_hold(
        &mut self,
        enabled: bool,
        at: DateTime<Utc>,
    ) -> Result<(), AgentDomainError> {
        if self.expired_at.is_some() || enabled == self.legal_hold || at < self.updated_at {
            return Err(AgentDomainError::SessionExpired);
        }
        self.legal_hold = enabled;
        self.advance(at)
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), AgentDomainError> {
        if at < self.updated_at {
            return Err(AgentDomainError::InvalidVersion);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(AgentDomainError::ArithmeticOverflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn id(&self) -> AgentSessionId {
        self.id
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn owner_human_ref(&self) -> &str {
        &self.owner_human_ref
    }
    pub const fn context(&self) -> Option<&SessionContext> {
        self.context.as_ref()
    }
    pub fn messages(&self) -> &[SessionMessage] {
        &self.messages
    }
    pub const fn expires_at(&self) -> DateTime<Utc> {
        self.expires_at
    }
    pub const fn legal_hold(&self) -> bool {
        self.legal_hold
    }
    pub const fn expired_at(&self) -> Option<DateTime<Utc>> {
        self.expired_at
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProposedActionStatus {
    Proposed,
    Rejected,
    ConfirmedForHandoff,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActionCardProposal {
    pub id: ActionCardId,
    pub assignment_id: AssignmentId,
    pub agent_principal_ref: String,
    pub owner_context: String,
    pub command_type: String,
    pub payload_digest: String,
    pub status: ProposedActionStatus,
    pub confirmed_by_human_ref: Option<String>,
}

impl ActionCardProposal {
    #[allow(clippy::too_many_arguments)]
    pub fn rehydrate(
        id: ActionCardId,
        assignment_id: AssignmentId,
        agent_principal_ref: String,
        owner_context: String,
        command_type: String,
        payload_digest: String,
        status: ProposedActionStatus,
        confirmed_by_human_ref: Option<String>,
    ) -> Result<Self, AgentDomainError> {
        if !valid_text(&agent_principal_ref, 256)
            || !valid_text(&owner_context, 128)
            || !valid_text(&command_type, 128)
            || !validate_digest(&payload_digest)
            || match status {
                ProposedActionStatus::ConfirmedForHandoff => confirmed_by_human_ref
                    .as_ref()
                    .is_none_or(|actor| !valid_text(actor, 256)),
                ProposedActionStatus::Proposed | ProposedActionStatus::Rejected => {
                    confirmed_by_human_ref.is_some()
                }
            }
        {
            return Err(AgentDomainError::DirectMutationDenied);
        }
        Ok(Self {
            id,
            assignment_id,
            agent_principal_ref,
            owner_context,
            command_type,
            payload_digest,
            status,
            confirmed_by_human_ref,
        })
    }

    pub fn confirm_for_handoff(
        &mut self,
        actor_kind: PrincipalKind,
        actor_ref: String,
        authorized: bool,
    ) -> Result<(), AgentDomainError> {
        if actor_kind != PrincipalKind::Human
            || !authorized
            || self.status != ProposedActionStatus::Proposed
            || !valid_text(&actor_ref, 256)
        {
            return Err(AgentDomainError::DirectMutationDenied);
        }
        self.status = ProposedActionStatus::ConfirmedForHandoff;
        self.confirmed_by_human_ref = Some(actor_ref);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};
    use erpx_kernel::{ActorAttribution, ActorKind, SessionId};

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn human_decision(scope: AuthorityScope, reference: &str) -> HumanDecisionProof {
        HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: "reviewer:1".into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(SessionId::try_from_uuid(id(99)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope,
            reference,
            at(),
        )
        .unwrap()
    }

    fn session_human_decision(reference: &str) -> HumanDecisionProof {
        HumanDecisionProof::try_new(
            ActorAttribution {
                principal_ref: "human:1".into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(SessionId::try_from_uuid(id(98)).unwrap()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope(),
            reference,
            at(),
        )
        .unwrap()
    }

    fn at() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 15, 9, 0, 0).single().unwrap()
    }

    fn scope() -> AuthorityScope {
        AuthorityScope::Branch {
            holding_id: id(1),
            company_id: id(2),
            branch_id: id(3),
        }
    }

    fn manifest(definition: u128, version: u128) -> AgentManifest {
        AgentManifest {
            version_id: AgentVersionId::try_from_uuid(id(version)).unwrap(),
            version_number: 1,
            purpose: "Summarize authorized records".into(),
            provider_ref: "ollama-local".into(),
            model_ref: "model-1".into(),
            use_case: "record-summary".into(),
            prompt_digest: "a".repeat(64),
            policy_digest: "b".repeat(64),
            capabilities: BTreeSet::from(["summarize".into()]),
            tool_refs: BTreeSet::from(["record.read".into()]),
            knowledge_scopes: BTreeSet::from([format!("agent-{definition}")]),
            organization_scopes: vec![AuthorityScope::Holding { holding_id: id(1) }],
            maximum_classification: DataClassification::Restricted,
            security_review_ref: "SEC-1".into(),
            privacy_review_ref: "PRI-1".into(),
            evaluation_refs: BTreeSet::from(["EVAL-1".into()]),
            max_attempts: 2,
            timeout_seconds: 300,
        }
    }

    fn pin(definition: u128, version: u128) -> AgentPin {
        AgentPin {
            definition_id: AgentDefinitionId::try_from_uuid(id(definition)).unwrap(),
            agent_principal_ref: format!("agent:{definition}"),
            manifest: manifest(definition, version),
        }
    }

    fn root() -> Assignment {
        Assignment::start_root(
            AssignmentId::try_from_uuid(id(20)).unwrap(),
            RootInvocation {
                initiating_principal_ref: "human:1".into(),
                initiating_principal_kind: PrincipalKind::Human,
                origin: InvocationOrigin::Mention,
                source_action_ref: "mention:1".into(),
                idempotency_key: "mention:1:agent:10".into(),
            },
            pin(10, 11),
            scope(),
            BudgetReservationId::try_from_uuid(id(30)).unwrap(),
            at(),
        )
        .unwrap()
    }

    #[test]
    fn governance_requires_approved_complete_version() {
        let definition_id = AgentDefinitionId::try_from_uuid(id(10)).unwrap();
        let mut definition = AgentDefinition::register(
            definition_id,
            AuthorityScope::Holding { holding_id: id(1) },
            "agent:10".into(),
            "owner:business".into(),
            "owner:technical".into(),
            "Summary agent".into(),
            at(),
        )
        .unwrap();
        let manifest = manifest(10, 11);
        definition
            .add_draft_version(manifest.clone(), at())
            .unwrap();
        assert_eq!(
            definition.pin(manifest.version_id),
            Err(AgentDomainError::AgentUnavailable)
        );
        assert_eq!(
            definition.approve_version(
                manifest.version_id,
                human_decision(
                    AuthorityScope::Holding { holding_id: id(1) },
                    &agent_version_approval_decision_ref(
                        AgentDefinitionId::try_from_uuid(id(99)).unwrap(),
                        manifest.version_id,
                    ),
                ),
            ),
            Err(AgentDomainError::AgentUnavailable)
        );
        definition
            .approve_version(
                manifest.version_id,
                human_decision(
                    AuthorityScope::Holding { holding_id: id(1) },
                    &agent_version_approval_decision_ref(definition_id, manifest.version_id),
                ),
            )
            .unwrap();
        assert_eq!(
            definition.pin(manifest.version_id).unwrap().manifest,
            manifest
        );
        definition.suspend(at()).unwrap();
        assert_eq!(
            definition.pin(AgentVersionId::try_from_uuid(id(11)).unwrap()),
            Err(AgentDomainError::AgentUnavailable)
        );
    }

    #[test]
    fn proactive_and_non_human_root_invocations_are_rejected() {
        for (kind, origin, expected) in [
            (
                PrincipalKind::Agent,
                InvocationOrigin::Run,
                AgentDomainError::RootMustBeHuman,
            ),
            (
                PrincipalKind::Human,
                InvocationOrigin::Event,
                AgentDomainError::ProactiveTriggerDenied,
            ),
        ] {
            let mut invocation = root().root_invocation().clone();
            invocation.initiating_principal_kind = kind;
            invocation.origin = origin;
            assert_eq!(invocation.validate(), Err(expected));
        }
    }

    #[test]
    fn durable_lifecycle_requires_current_lease_and_pinned_retry() {
        let mut assignment = root();
        let token = id(40);
        assignment
            .claim(token, "worker:1".into(), at() + Duration::minutes(1), at())
            .unwrap();
        assert_eq!(assignment.attempt(), 1);
        assert_eq!(
            assignment.checkpoint(
                id(41),
                Checkpoint {
                    checkpoint_id: CheckpointId::try_from_uuid(id(42)).unwrap(),
                    step: 1,
                    state_digest: "c".repeat(64),
                    retry_safe: true,
                    created_at: at(),
                },
                at(),
            ),
            Err(AgentDomainError::InvalidLease)
        );
        assignment
            .fail(
                token,
                AssignmentFailure {
                    code: "provider_timeout".into(),
                    retryable: true,
                    detail_digest: "d".repeat(64),
                    failed_at: at(),
                },
                at(),
            )
            .unwrap();
        assignment.retry(false, true, at()).unwrap();
        assignment
            .claim(id(43), "worker:2".into(), at() + Duration::minutes(1), at())
            .unwrap();
        assignment.complete(id(43), "e".repeat(64), at()).unwrap();
        assert_eq!(assignment.state(), AssignmentState::Completed);
        assert_eq!(assignment.attempt(), 2);
    }

    #[test]
    fn expired_lease_requeues_only_from_a_retry_safe_checkpoint_and_fences_stale_worker() {
        let mut assignment = root();
        let stale_token = id(44);
        let expires_at = at() + Duration::minutes(1);
        assignment
            .claim(stale_token, "worker:stale".into(), expires_at, at())
            .unwrap();
        assignment
            .checkpoint(
                stale_token,
                Checkpoint {
                    checkpoint_id: CheckpointId::try_from_uuid(id(45)).unwrap(),
                    step: 1,
                    state_digest: "4".repeat(64),
                    retry_safe: true,
                    created_at: at(),
                },
                at(),
            )
            .unwrap();
        let pinned = assignment.pin().clone();
        let checkpoints = assignment.checkpoints().to_vec();
        let version = assignment.version();
        assert_eq!(
            assignment.recover_expired_lease(true, expires_at - Duration::seconds(1)),
            Err(AgentDomainError::InvalidLease)
        );
        assert_eq!(
            assignment.recover_expired_lease(true, expires_at).unwrap(),
            ExpiredLeaseRecovery::Requeued
        );
        assert_eq!(assignment.state(), AssignmentState::Queued);
        assert_eq!(assignment.pin(), &pinned);
        assert_eq!(assignment.checkpoints(), checkpoints);
        assert_eq!(assignment.version(), version + 1);
        assert_eq!(
            assignment.complete(stale_token, "5".repeat(64), expires_at),
            Err(AgentDomainError::InvalidLease)
        );
    }

    #[test]
    fn expired_lease_with_unsafe_checkpoint_or_no_budget_fails_terminally() {
        let expires_at = at() + Duration::minutes(1);
        let mut unsafe_assignment = root();
        let unsafe_token = id(46);
        unsafe_assignment
            .claim(unsafe_token, "worker:unsafe".into(), expires_at, at())
            .unwrap();
        unsafe_assignment
            .checkpoint(
                unsafe_token,
                Checkpoint {
                    checkpoint_id: CheckpointId::try_from_uuid(id(47)).unwrap(),
                    step: 1,
                    state_digest: "6".repeat(64),
                    retry_safe: false,
                    created_at: at(),
                },
                at(),
            )
            .unwrap();
        assert_eq!(
            unsafe_assignment
                .recover_expired_lease(true, expires_at)
                .unwrap(),
            ExpiredLeaseRecovery::FailedUnsafeCheckpoint
        );
        assert_eq!(unsafe_assignment.state(), AssignmentState::Failed);
        assert_eq!(
            unsafe_assignment.retry(false, true, expires_at),
            Err(AgentDomainError::RetryDenied)
        );

        let mut budget_revoked = root();
        budget_revoked
            .claim(id(48), "worker:budget".into(), expires_at, at())
            .unwrap();
        assert_eq!(
            budget_revoked
                .recover_expired_lease(false, expires_at)
                .unwrap(),
            ExpiredLeaseRecovery::FailedBudgetUnavailable
        );
        assert_eq!(budget_revoked.state(), AssignmentState::Failed);
    }

    #[test]
    fn delegation_rejects_cycles_scope_expansion_and_limits() {
        let mut parent = root();
        parent
            .claim(id(50), "worker:1".into(), at() + Duration::minutes(1), at())
            .unwrap();
        let limits = DelegationLimits {
            max_depth: 2,
            max_hops: 2,
            max_fan_out: 1,
            max_duration_seconds: 300,
        };
        assert_eq!(
            parent.delegate(
                AssignmentId::try_from_uuid(id(51)).unwrap(),
                pin(10, 11),
                scope(),
                BudgetReservationId::try_from_uuid(id(52)).unwrap(),
                0,
                &limits,
                at(),
            ),
            Err(AgentDomainError::DelegationCycle)
        );
        let child = parent
            .delegate(
                AssignmentId::try_from_uuid(id(53)).unwrap(),
                pin(12, 13),
                scope(),
                BudgetReservationId::try_from_uuid(id(54)).unwrap(),
                0,
                &limits,
                at(),
            )
            .unwrap();
        assert_eq!(child.root_assignment_id(), parent.id());
        assert_eq!(child.parent_assignment_id(), Some(parent.id()));
        assert_eq!(child.lineage().len(), 2);
        assert_eq!(
            parent.delegate(
                AssignmentId::try_from_uuid(id(55)).unwrap(),
                pin(14, 15),
                AuthorityScope::Holding { holding_id: id(1) },
                BudgetReservationId::try_from_uuid(id(56)).unwrap(),
                0,
                &limits,
                at(),
            ),
            Err(AgentDomainError::DelegationScopeExpansion)
        );
        assert_eq!(
            parent.delegate(
                AssignmentId::try_from_uuid(id(57)).unwrap(),
                pin(14, 15),
                scope(),
                BudgetReservationId::try_from_uuid(id(58)).unwrap(),
                1,
                &limits,
                at(),
            ),
            Err(AgentDomainError::DelegationLimitExceeded)
        );
    }

    fn layers() -> Vec<BudgetLayer> {
        BudgetLayerKind::ALL
            .into_iter()
            .map(|kind| BudgetLayer {
                kind,
                subject_ref: format!("{kind:?}:1"),
                remaining_units: 1_000,
                remaining_cost_micros: 10_000,
                active_concurrency: 0,
                concurrency_limit: 1,
            })
            .collect()
    }

    #[test]
    fn layered_budget_is_all_or_approval_required() {
        let request = BudgetRequest {
            estimated_units: 100,
            estimated_cost_micros: 1_000,
            queue_priority: 5,
            stop_after_units: 150,
        };
        let decision = BudgetReservation::decide(
            BudgetReservationId::try_from_uuid(id(60)).unwrap(),
            request.clone(),
            layers(),
            at(),
        )
        .unwrap();
        assert!(matches!(decision, BudgetDecision::Reserved(_)));
        let mut blocked = layers();
        blocked[3].active_concurrency = 1;
        assert!(matches!(
            BudgetReservation::decide(
                BudgetReservationId::try_from_uuid(id(61)).unwrap(),
                request,
                blocked,
                at(),
            )
            .unwrap(),
            BudgetDecision::ApprovalRequired { exceeded_layers }
                if exceeded_layers == vec![BudgetLayerKind::Principal]
        ));
    }

    #[test]
    fn pool_route_is_deterministic_and_compare_is_explicit() {
        let first = pin(10, 11);
        let second = pin(12, 13);
        let pool = AgentPool::register(
            AgentPoolId::try_from_uuid(id(70)).unwrap(),
            AuthorityScope::Holding { holding_id: id(1) },
            "summarizers".into(),
            BTreeSet::from([first.manifest.version_id, second.manifest.version_id]),
            at(),
        )
        .unwrap();
        let request = PoolRouteRequest {
            decision_id: PoolRoutingDecisionId::try_from_uuid(id(71)).unwrap(),
            root_action_ref: "assignment:root:1".into(),
            capability: "summarize".into(),
            requested_scope: scope(),
            decided_at: at(),
        };
        let decision = pool
            .route(
                request.clone(),
                vec![
                    PoolCandidate {
                        pin: second.clone(),
                        active_assignments: 1,
                    },
                    PoolCandidate {
                        pin: first.clone(),
                        active_assignments: 0,
                    },
                ],
            )
            .unwrap();
        assert_eq!(decision.selected, first);
        let mut compare = CompareRequest {
            initiating_human_ref: "human:1".into(),
            confirmed: false,
            pins: vec![first.clone(), second.clone()],
            previewed_cost_micros: 2_000,
        };
        assert_eq!(compare.validate(3), Err(AgentDomainError::InvalidCompare));
        compare.confirmed = true;
        let comparison = pool.route_compare(request, compare, 3).unwrap();
        assert!(comparison.compare_requested);
        assert_eq!(comparison.selected_versions.len(), 2);
        assert_eq!(comparison.initiating_human_ref.as_deref(), Some("human:1"));
        assert_ne!(comparison.decision_digest, decision.decision_digest);
    }

    #[test]
    fn external_provider_requires_dpa_residency_and_classification_match() {
        let pin = pin(10, 11);
        let payload = PayloadManifest {
            payload_digest: "1".repeat(64),
            classification: DataClassification::Restricted,
            field_refs: BTreeSet::from(["invoice.amount".into()]),
            document_refs: BTreeSet::new(),
            authorization_checked_at: at(),
            contains_secret: false,
            residency: "TH".into(),
        };
        let mut policy = ProviderPolicy {
            id: ProviderPolicyId::try_from_uuid(id(90)).unwrap(),
            agent_definition_id: pin.definition_id,
            agent_version_id: pin.manifest.version_id,
            policy_version: 1,
            provider_ref: "ollama-local".into(),
            external: true,
            approved: true,
            dpa_ref: None,
            allowed_residencies: BTreeSet::from(["TH".into()]),
            allowed_models: BTreeSet::from(["model-1".into()]),
            allowed_use_cases: BTreeSet::from(["record-summary".into()]),
            maximum_classification: DataClassification::Restricted,
            policy_digest: "b".repeat(64),
            approved_evaluation_refs: pin.manifest.evaluation_refs.clone(),
            freshness_checked_at: at(),
            fresh_until: at() + Duration::minutes(5),
            expires_at: at() + Duration::days(1),
        };
        assert_eq!(
            policy.authorize(&pin, &payload, at()),
            Err(AgentDomainError::ProviderDenied)
        );
        policy.dpa_ref = Some("DPA-1".into());
        assert!(policy.authorize(&pin, &payload, at()).is_ok());
    }

    #[test]
    fn provider_evidence_pins_policy_and_only_accepts_one_terminal_result() {
        let pin = pin(10, 11);
        let payload = PayloadManifest {
            payload_digest: "1".repeat(64),
            classification: DataClassification::Internal,
            field_refs: BTreeSet::from(["invoice.amount".into()]),
            document_refs: BTreeSet::new(),
            authorization_checked_at: at(),
            contains_secret: false,
            residency: "TH".into(),
        };
        let decision = ProviderPolicyDecision {
            provider_ref: pin.manifest.provider_ref.clone(),
            model_ref: pin.manifest.model_ref.clone(),
            policy_version: 7,
            payload_digest: payload.payload_digest.clone(),
            policy_digest: pin.manifest.policy_digest.clone(),
            authorized_at: at(),
        };
        let mut evidence = ProviderInvocationEvidence::begin(
            ProviderInvocationId::try_from_uuid(id(92)).unwrap(),
            AssignmentId::try_from_uuid(id(20)).unwrap(),
            3,
            2,
            &pin,
            &payload,
            &decision,
            at(),
        )
        .unwrap();
        let output_digest = "5".repeat(64);
        evidence
            .complete(output_digest.clone(), 21, 450, at() + Duration::seconds(1))
            .unwrap();
        assert_eq!(evidence.version(), 2);
        assert_eq!(evidence.policy_version(), 7);
        assert_eq!(evidence.output_digest(), Some(output_digest.as_str()));
        assert_eq!(
            evidence.outcome(),
            Some(ProviderInvocationOutcome::Completed)
        );
        assert_eq!(
            evidence.fail(at() + Duration::seconds(2)),
            Err(AgentDomainError::InvalidInvocationEvidence)
        );
        let serialized = serde_json::to_string(&evidence).unwrap();
        assert!(!serialized.contains("prompt_material"));
        assert!(!serialized.contains("output_material"));
    }

    #[test]
    fn private_session_publishes_only_agent_authorship_and_expires_unpublished_content() {
        let mut session = AgentSession::start(
            AgentSessionId::try_from_uuid(id(80)).unwrap(),
            scope(),
            "human:1".into(),
            None,
            at() + Duration::hours(1),
            at(),
        )
        .unwrap();
        let human_message = SessionMessage {
            id: SessionMessageId::try_from_uuid(id(81)).unwrap(),
            author_kind: PrincipalKind::Human,
            author_ref: "human:1".into(),
            encrypted_payload_ref: "secret://message/81".into(),
            content_digest: "2".repeat(64),
            version: 1,
            created_at: at(),
            published_at: None,
            publishing_approver_human_ref: None,
            publication_anchor_ref: None,
        };
        session.add_message(human_message, at()).unwrap();
        assert_eq!(
            session.publish_agent_message(
                SessionMessageId::try_from_uuid(id(81)).unwrap(),
                true,
                id(180),
                CommandId::try_from_uuid(id(580)).unwrap(),
                &session_human_decision(&session_message_publication_decision_ref(
                    session.id(),
                    SessionMessageId::try_from_uuid(id(81)).unwrap(),
                    session.version(),
                    CommandId::try_from_uuid(id(580)).unwrap(),
                )),
            ),
            Err(AgentDomainError::ImpersonationDenied)
        );
        session
            .add_message(
                SessionMessage {
                    id: SessionMessageId::try_from_uuid(id(82)).unwrap(),
                    author_kind: PrincipalKind::Agent,
                    author_ref: "agent:10".into(),
                    encrypted_payload_ref: "secret://message/82".into(),
                    content_digest: "3".repeat(64),
                    version: 1,
                    created_at: at(),
                    published_at: None,
                    publishing_approver_human_ref: None,
                    publication_anchor_ref: None,
                },
                at(),
            )
            .unwrap();
        session
            .publish_agent_message(
                SessionMessageId::try_from_uuid(id(82)).unwrap(),
                true,
                id(181),
                CommandId::try_from_uuid(id(581)).unwrap(),
                &session_human_decision(&session_message_publication_decision_ref(
                    session.id(),
                    SessionMessageId::try_from_uuid(id(82)).unwrap(),
                    session.version(),
                    CommandId::try_from_uuid(id(581)).unwrap(),
                )),
            )
            .unwrap();
        session.expire(at() + Duration::hours(1)).unwrap();
        assert_eq!(session.messages().len(), 1);
        assert_eq!(session.messages()[0].author_kind, PrincipalKind::Agent);
        assert_eq!(session.messages()[0].version, 2);
        assert_eq!(
            session.messages()[0]
                .publishing_approver_human_ref
                .as_deref(),
            Some("human:1")
        );
    }

    #[test]
    fn action_cards_never_grant_agent_business_authority() {
        let mut card = ActionCardProposal {
            id: ActionCardId::try_from_uuid(id(90)).unwrap(),
            assignment_id: AssignmentId::try_from_uuid(id(20)).unwrap(),
            agent_principal_ref: "agent:10".into(),
            owner_context: "finance".into(),
            command_type: "post_journal".into(),
            payload_digest: "4".repeat(64),
            status: ProposedActionStatus::Proposed,
            confirmed_by_human_ref: None,
        };
        assert_eq!(
            card.confirm_for_handoff(PrincipalKind::Agent, "agent:10".into(), true),
            Err(AgentDomainError::DirectMutationDenied)
        );
        card.confirm_for_handoff(PrincipalKind::Human, "human:1".into(), true)
            .unwrap();
        assert_eq!(card.status, ProposedActionStatus::ConfirmedForHandoff);
    }
}
