//! Agent governance use cases. Ports own current authority, replay, snapshots
//! and the commit; this layer never opens a transaction or reads HTTP.
use crate::domain::governance::{
    AgentAssignmentActionInput, AgentAssignmentDelegateInput, AgentAssignmentInput, AgentInput,
    AgentLifecycleInput, AgentPoolChange, AgentPoolInput, AgentVersionInput, AssignmentState,
    AssistantPolicy, LifecycleAction, PolicyCandidate, PoolAction, PrivateSessionAction,
    PrivateSessionInput, RecordKind, RecordState, assert_current_assistant_policy,
    assert_independent_review, assistant_policy, lifecycle_transition, pool_member_id,
    session_expired,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::commands::{CommandActor, CommandJson};
use serde_json::{Value, json};
use std::future::Future;

#[derive(Clone, Debug)]
pub struct AgentActor {
    pub holding_id: String,
    pub command: CommandActor,
    pub modules: Vec<String>,
    pub permissions: Vec<String>,
}

impl AgentActor {
    pub fn new(
        holding_id: impl Into<String>,
        command: CommandActor,
        modules: Vec<String>,
        permissions: Vec<String>,
    ) -> Self {
        Self {
            holding_id: holding_id.into(),
            command,
            modules,
            permissions,
        }
    }

    pub fn company_id(&self) -> &str {
        self.command.company_id.as_str()
    }

    pub fn user_id(&self) -> &str {
        self.command.user_id.as_str()
    }

    pub fn assignment_id(&self) -> &str {
        self.command.assignment_id.as_str()
    }

    pub fn has_permission(&self, permission: &str) -> bool {
        self.permissions.iter().any(|value| value == permission)
    }

    pub fn require_permission(&self, permission: &str) -> Result<(), BusinessError> {
        if self.has_permission(permission) {
            Ok(())
        } else {
            Err(BusinessError::new(
                FailureKind::Forbidden,
                "PERMISSION_REQUIRED",
                "Your selected role does not grant this permission.",
            ))
        }
    }

    pub fn require_agent_module(&self) -> Result<(), BusinessError> {
        if self.modules.iter().any(|value| value == "agent") {
            Ok(())
        } else {
            Err(BusinessError::new(
                FailureKind::Forbidden,
                "MODULE_FORBIDDEN",
                "Your current role cannot access Agent governance.",
            ))
        }
    }

    pub fn can_manage(&self) -> bool {
        self.has_permission("ai.manage")
    }

    pub fn require_manage(&self, write: bool) -> Result<(), BusinessError> {
        self.require_permission(if write { "ai.manage" } else { "records.read" })?;
        self.require_agent_module()
    }
}

#[derive(Clone, Debug)]
pub struct AgentMutationCommon {
    pub key: String,
    pub operation: &'static str,
    pub record_id: String,
    pub version: i64,
    pub input: CommandJson,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct AssignmentBinding {
    pub agent_version_id: String,
    pub pool_id: Option<String>,
    pub user_id: Option<String>,
    pub scope_path: String,
}

#[derive(Clone, Debug)]
#[allow(clippy::large_enum_variant)] // one owned mutation per command, not a hot collection
pub enum AgentMutationAction {
    CreateAgent {
        name: String,
        purpose: String,
    },
    CreateVersion {
        agent_id: String,
        number: i64,
        prompt: String,
        allowed_sources_json: String,
    },
    CreatePool {
        name: String,
        user_ids: Vec<String>,
        member_ids: Vec<String>,
    },
    ChangePool {
        action: PoolAction,
        expected_version: i64,
        user_ids: Vec<String>,
        member_ids: Vec<String>,
        reason: String,
    },
    CreateAssignment {
        binding: AssignmentBinding,
    },
    DelegateAssignment {
        prior_id: String,
        prior_version: i64,
        binding: AssignmentBinding,
        reason: String,
    },
    RetryAssignment {
        prior_id: String,
        prior_version: i64,
        prior_status: String,
        binding: AssignmentBinding,
        reason: String,
    },
    CancelAssignment {
        expected_version: i64,
        reason: String,
    },
    Lifecycle {
        kind: RecordKind,
        expected_version: i64,
        expected_status: &'static str,
        status: &'static str,
    },
    Session {
        action: PrivateSessionAction,
        expected_version: i64,
        reason: String,
        manage: bool,
    },
}

#[derive(Clone, Debug)]
pub struct AgentMutation {
    pub common: AgentMutationCommon,
    pub action: AgentMutationAction,
}

/// All methods are mandatory. No permissive default or serialized authority is
/// supplied; the adapter repeats every snapshot guard inside the commit.
pub trait AgentGovernancePorts {
    type Error: From<BusinessError> + Send;

    fn actor(&self) -> &AgentActor;
    fn now(&self) -> i64;
    fn authorize(&mut self, write: bool) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn replay(
        &mut self,
        key: &str,
        operation: &str,
        record_id: &str,
        input: &CommandJson,
    ) -> impl Future<Output = Result<Option<Value>, Self::Error>> + Send;
    fn commit(
        &mut self,
        mutation: &AgentMutation,
        result: &Value,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn next_version_number(
        &mut self,
        agent_id: &str,
    ) -> impl Future<Output = Result<Option<i64>, Self::Error>> + Send;
    fn record(
        &mut self,
        kind: RecordKind,
        id: &str,
    ) -> impl Future<Output = Result<Option<RecordState>, Self::Error>> + Send;
    fn assignment(
        &mut self,
        id: &str,
    ) -> impl Future<Output = Result<Option<AssignmentState>, Self::Error>> + Send;
    fn policy_candidates(
        &mut self,
    ) -> impl Future<Output = Result<Vec<PolicyCandidate>, Self::Error>> + Send;
    fn active_private_session(
        &mut self,
        session_id: &str,
        assignment_id: Option<&str>,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
}

fn rule(kind: FailureKind, code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(kind, code, message)
}

fn input_json<T: serde::Serialize>(input: &T) -> Result<CommandJson, BusinessError> {
    CommandJson::from_serializable(input)
}

fn conflict() -> BusinessError {
    rule(
        FailureKind::Conflict,
        "CONFLICT",
        "Record changed or transition is invalid.",
    )
}

fn record_not_found() -> BusinessError {
    rule(
        FailureKind::NotFound,
        "AGENT_RECORD_NOT_FOUND",
        "Agent record not found.",
    )
}

pub async fn create_agent<P: AgentGovernancePorts>(
    ports: &mut P,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(true).await?;
    let input = AgentInput::parse(raw)?;
    let record_id = format!("agent-{key}");
    let operation = "agent.create";
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, &record_id, &canonical).await? {
        return Ok(prior);
    }
    let result = json!({"id": record_id, "status": "draft", "version": 1});
    ports
        .commit(
            &AgentMutation {
                common: AgentMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id,
                    version: 1,
                    input: canonical,
                    now: ports.now(),
                },
                action: AgentMutationAction::CreateAgent {
                    name: input.name,
                    purpose: input.purpose,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

pub async fn create_agent_version<P: AgentGovernancePorts>(
    ports: &mut P,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(true).await?;
    let input = AgentVersionInput::parse(raw)?;
    let record_id = format!("agent-version-{key}");
    let operation = "agent.version.create";
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, &record_id, &canonical).await? {
        return Ok(prior);
    }
    let current = ports
        .next_version_number(&input.agent_id)
        .await?
        .ok_or_else(|| rule(FailureKind::NotFound, "AGENT_NOT_FOUND", "Agent not found."))?;
    let number = current + 1;
    let result = json!({
        "id": record_id,
        "versionNumber": number,
        "status": "draft",
        "version": 1,
    });
    ports
        .commit(
            &AgentMutation {
                common: AgentMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id,
                    version: 1,
                    input: canonical,
                    now: ports.now(),
                },
                action: AgentMutationAction::CreateVersion {
                    agent_id: input.agent_id,
                    number,
                    prompt: input.prompt,
                    allowed_sources_json: serde_json::to_string(&input.allowed_sources).map_err(
                        |_| {
                            BusinessError::invalid("VALIDATION_FAILED", "Choose assistant sources.")
                        },
                    )?,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

pub async fn create_agent_pool<P: AgentGovernancePorts>(
    ports: &mut P,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(true).await?;
    let input = AgentPoolInput::parse(raw)?;
    let record_id = format!("agent-pool-{key}");
    let operation = "agent.pool.create";
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, &record_id, &canonical).await? {
        return Ok(prior);
    }
    let mut members = input.user_ids.clone();
    members.sort();
    members.dedup();
    let member_ids = members
        .iter()
        .map(|user_id| pool_member_id(key, user_id))
        .collect::<Vec<_>>();
    let result = json!({"id": record_id, "status": "active", "version": 1});
    ports
        .commit(
            &AgentMutation {
                common: AgentMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id,
                    version: 1,
                    input: canonical,
                    now: ports.now(),
                },
                action: AgentMutationAction::CreatePool {
                    name: input.name,
                    user_ids: members,
                    member_ids,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

pub async fn change_agent_pool<P: AgentGovernancePorts>(
    ports: &mut P,
    pool_id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(true).await?;
    let input = AgentPoolChange::parse(raw)?;
    let operation = match input.action {
        PoolAction::ReplaceMembers => "agent.pool.replace-members",
        PoolAction::Retire => "agent.pool.retire",
    };
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, pool_id, &canonical).await? {
        return Ok(prior);
    }
    let mut members = input.user_ids.clone();
    members.sort();
    members.dedup();
    let member_ids = members
        .iter()
        .map(|user_id| pool_member_id(key, user_id))
        .collect::<Vec<_>>();
    let version = input.expected_version + 1;
    let status = if input.action == PoolAction::Retire {
        "retired"
    } else {
        "active"
    };
    let result = json!({"id": pool_id, "status": status, "version": version});
    ports
        .commit(
            &AgentMutation {
                common: AgentMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id: pool_id.to_owned(),
                    version,
                    input: canonical,
                    now: ports.now(),
                },
                action: AgentMutationAction::ChangePool {
                    action: input.action,
                    expected_version: input.expected_version,
                    user_ids: members,
                    member_ids,
                    reason: input.reason,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

pub async fn start_agent_assignment<P: AgentGovernancePorts>(
    ports: &mut P,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(true).await?;
    let input = AgentAssignmentInput::parse(raw)?;
    let record_id = format!("agent-assignment-{key}");
    let operation = "agent.assignment.create";
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, &record_id, &canonical).await? {
        return Ok(prior);
    }
    let result = json!({"id": record_id, "status": "active", "version": 1});
    ports
        .commit(
            &AgentMutation {
                common: AgentMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id,
                    version: 1,
                    input: canonical,
                    now: ports.now(),
                },
                action: AgentMutationAction::CreateAssignment {
                    binding: AssignmentBinding {
                        agent_version_id: input.agent_version_id,
                        pool_id: input.pool_id,
                        user_id: input.user_id,
                        scope_path: input.scope_path,
                    },
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

async fn current_assignment<P: AgentGovernancePorts>(
    ports: &mut P,
    id: &str,
    expected_version: i64,
) -> Result<AssignmentState, P::Error> {
    let current = ports.assignment(id).await?.ok_or_else(record_not_found)?;
    if !current.is_active() || current.version != expected_version {
        return Err(conflict().into());
    }
    Ok(current)
}

/// Delegation starts a replacement assignment for the same approved version
/// with a new pool/user binding. Assignments are immutable, so the prior row is
/// revoked with CAS and the replacement is a new active row.
pub async fn delegate_agent_assignment<P: AgentGovernancePorts>(
    ports: &mut P,
    id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(true).await?;
    let input = AgentAssignmentDelegateInput::parse(raw)?;
    let operation = "agent.assignment.delegate";
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, id, &canonical).await? {
        return Ok(prior);
    }
    let current = current_assignment(ports, id, input.expected_version).await?;
    let replacement = format!("agent-assignment-{key}");
    let result = json!({
        "id": replacement,
        "previousId": id,
        "status": "active",
        "version": 1,
    });
    ports
        .commit(
            &AgentMutation {
                common: AgentMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id: id.to_owned(),
                    version: 1,
                    input: canonical,
                    now: ports.now(),
                },
                action: AgentMutationAction::DelegateAssignment {
                    prior_id: id.to_owned(),
                    prior_version: current.version,
                    binding: AssignmentBinding {
                        agent_version_id: current.agent_version_id,
                        pool_id: input.pool_id,
                        user_id: input.user_id,
                        scope_path: input.scope_path,
                    },
                    reason: input.reason,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

/// Retry starts a fresh active row carrying the same approved binding after the
/// prior assignment was cancelled or superseded. An active prior row is revoked
/// with CAS in the same commit; a revoked prior row is retained unchanged.
pub async fn retry_agent_assignment<P: AgentGovernancePorts>(
    ports: &mut P,
    id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(true).await?;
    let input = AgentAssignmentActionInput::parse(raw)?;
    let operation = "agent.assignment.retry";
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, id, &canonical).await? {
        return Ok(prior);
    }
    let current = ports.assignment(id).await?.ok_or_else(record_not_found)?;
    if !matches!(current.status.as_str(), "active" | "revoked")
        || current.version != input.expected_version
    {
        return Err(conflict().into());
    }
    let replacement = format!("agent-assignment-{key}");
    let result = json!({
        "id": replacement,
        "previousId": id,
        "status": "active",
        "version": 1,
    });
    ports
        .commit(
            &AgentMutation {
                common: AgentMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id: id.to_owned(),
                    version: 1,
                    input: canonical,
                    now: ports.now(),
                },
                action: AgentMutationAction::RetryAssignment {
                    prior_id: id.to_owned(),
                    prior_version: current.version,
                    prior_status: current.status,
                    binding: AssignmentBinding {
                        agent_version_id: current.agent_version_id,
                        pool_id: current.pool_id,
                        user_id: current.user_id,
                        scope_path: current.scope_path,
                    },
                    reason: input.reason,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

pub async fn cancel_agent_assignment<P: AgentGovernancePorts>(
    ports: &mut P,
    id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(true).await?;
    let input = AgentAssignmentActionInput::parse(raw)?;
    let operation = "agent.assignment.cancel";
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, id, &canonical).await? {
        return Ok(prior);
    }
    current_assignment(ports, id, input.expected_version).await?;
    let version = input.expected_version + 1;
    let result = json!({"id": id, "status": "revoked", "version": version});
    ports
        .commit(
            &AgentMutation {
                common: AgentMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id: id.to_owned(),
                    version,
                    input: canonical,
                    now: ports.now(),
                },
                action: AgentMutationAction::CancelAssignment {
                    expected_version: input.expected_version,
                    reason: input.reason,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

pub async fn change_agent_record<P: AgentGovernancePorts>(
    ports: &mut P,
    kind: RecordKind,
    id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    ports.authorize(true).await?;
    let input = AgentLifecycleInput::parse(raw)?;
    let Some((expected, status)) = lifecycle_transition(kind, input.action) else {
        return Err(rule(
            FailureKind::InvalidInput,
            "AGENT_ACTION_INVALID",
            "Choose an available lifecycle action for this record.",
        )
        .into());
    };
    let operation: &'static str = match kind {
        RecordKind::Agent => match input.action {
            LifecycleAction::Approve => "agent.agent.approve",
            LifecycleAction::Reject => "agent.agent.reject",
            LifecycleAction::Retire => "agent.agent.retire",
            LifecycleAction::Revoke => unreachable!(),
        },
        RecordKind::Version => match input.action {
            LifecycleAction::Approve => "agent.version.approve",
            LifecycleAction::Reject => "agent.version.reject",
            LifecycleAction::Retire => "agent.version.retire",
            LifecycleAction::Revoke => unreachable!(),
        },
        RecordKind::Assignment => "agent.assignment.revoke",
    };
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, id, &canonical).await? {
        return Ok(prior);
    }
    let row = ports.record(kind, id).await?.ok_or_else(record_not_found)?;
    if row.status != expected || row.version != input.expected_version {
        return Err(conflict().into());
    }
    assert_independent_review(&row.created_by, ports.actor().user_id(), input.action)?;
    let version = input.expected_version + 1;
    let result = json!({"id": id, "status": status, "version": version});
    ports
        .commit(
            &AgentMutation {
                common: AgentMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id: id.to_owned(),
                    version,
                    input: canonical,
                    now: ports.now(),
                },
                action: AgentMutationAction::Lifecycle {
                    kind,
                    expected_version: input.expected_version,
                    expected_status: expected,
                    status,
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

pub async fn change_private_session<P: AgentGovernancePorts>(
    ports: &mut P,
    id: &str,
    raw: &Value,
    key: &str,
) -> Result<Value, P::Error> {
    let input = PrivateSessionInput::parse(raw)?;
    ports
        .authorize(input.action != PrivateSessionAction::Expire)
        .await?;
    let operation = match input.action {
        PrivateSessionAction::Hold => "agent.session.hold",
        PrivateSessionAction::ReleaseHold => "agent.session.release-hold",
        PrivateSessionAction::Expire => "agent.session.expire",
    };
    let canonical = input_json(&input)?;
    if let Some(prior) = ports.replay(key, operation, id, &canonical).await? {
        return Ok(prior);
    }
    let version = input.expected_version + 1;
    let result = json!({
        "id": id,
        "version": version,
        "action": input.action.as_str(),
    });
    ports
        .commit(
            &AgentMutation {
                common: AgentMutationCommon {
                    key: key.to_owned(),
                    operation,
                    record_id: id.to_owned(),
                    version,
                    input: canonical,
                    now: ports.now(),
                },
                action: AgentMutationAction::Session {
                    action: input.action,
                    expected_version: input.expected_version,
                    reason: input.reason,
                    manage: ports.actor().can_manage(),
                },
            },
            &result,
        )
        .await?;
    Ok(result)
}

/// Current approved routing decision for one assistant page.
pub async fn assistant_policy_for<P: AgentGovernancePorts>(
    ports: &mut P,
    path: &str,
) -> Result<AssistantPolicy, P::Error> {
    let candidates = ports.policy_candidates().await?;
    Ok(assistant_policy(&candidates, path)?)
}

/// Recheck the supplied approved policy and private session before an
/// invocation. A changed assignment or expired session fails closed.
pub async fn validate_assistant_governance<P: AgentGovernancePorts>(
    ports: &mut P,
    path: &str,
    supplied: &AssistantPolicy,
    session_id: &str,
) -> Result<(), P::Error> {
    let current = assistant_policy_for(ports, path).await?;
    assert_current_assistant_policy(&current, supplied)?;
    let active = ports
        .active_private_session(session_id, supplied.assignment_id.as_deref())
        .await?;
    if !active {
        return Err(session_expired().into());
    }
    Ok(())
}
