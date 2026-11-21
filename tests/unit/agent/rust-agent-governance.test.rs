//! Retained Agent governance values and command decisions: strict inputs,
//! source coverage, lifecycle/CAS rules, independent review and replay.
use erpx_server_agent::public::governance::*;
use erpx_server_shared::public::{AssignmentId, BusinessError, CompanyId, FailureKind, UserId};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandField, CommandJson, WorkAuthority,
};
use serde_json::{Value, json};
use std::collections::HashMap;

fn command_actor(company: &str, user: &str, assignment: &str, role: &str) -> CommandActor {
    CommandActor {
        user_id: UserId::new(user).unwrap(),
        assignment_id: AssignmentId::new(assignment).unwrap(),
        role: role.into(),
        company_id: CompanyId::new(company).unwrap(),
        branch_id: CommandField::Omitted,
        department_id: CommandField::Omitted,
        authority: CommandField::Value(WorkAuthority {
            session_id: "session".into(),
            revision: 1,
            assignment_version: 1,
        }),
        iam: CommandField::Value(
            CommandJson::parse(
                r#"{"roleRevision":1,"identityVersion":1,"permissions":[],"grantablePermissions":[],"principalId":"principal","personId":null,"personUserIds":[],"freshUntil":null}"#,
            )
            .unwrap(),
        ),
        assignment_scope_kind: "holding".into(),
        assignment_scope_node_id: None,
    }
}

fn actor(user: &str, permissions: &[&str]) -> AgentActor {
    AgentActor::new(
        "h001",
        command_actor("company", user, "assign-1", "holding_owner"),
        vec!["agent".into()],
        permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
    )
}

struct Fake {
    actor: AgentActor,
    now: i64,
    commits: Vec<(String, Value)>,
    receipts: HashMap<String, Value>,
    record: Option<RecordState>,
    assignment: Option<AssignmentState>,
    next: Option<i64>,
    candidates: Vec<PolicyCandidate>,
    active_session: bool,
}

impl Default for Fake {
    fn default() -> Self {
        Self {
            actor: actor("creator", &["ai.manage", "records.read"]),
            now: 1_000,
            commits: Vec::new(),
            receipts: HashMap::new(),
            record: None,
            assignment: None,
            next: Some(0),
            candidates: Vec::new(),
            active_session: true,
        }
    }
}

impl AgentGovernancePorts for Fake {
    type Error = BusinessError;

    fn actor(&self) -> &AgentActor {
        &self.actor
    }
    fn now(&self) -> i64 {
        self.now
    }
    async fn authorize(&mut self, _write: bool) -> Result<(), Self::Error> {
        Ok(())
    }
    async fn replay(
        &mut self,
        key: &str,
        _operation: &str,
        _record_id: &str,
        _input: &CommandJson,
    ) -> Result<Option<Value>, Self::Error> {
        Ok(self.receipts.get(key).cloned())
    }
    async fn commit(
        &mut self,
        mutation: &AgentMutation,
        result: &Value,
    ) -> Result<(), Self::Error> {
        self.commits
            .push((mutation.common.operation.to_owned(), result.clone()));
        self.receipts
            .insert(mutation.common.key.clone(), result.clone());
        Ok(())
    }
    async fn next_version_number(&mut self, _agent_id: &str) -> Result<Option<i64>, Self::Error> {
        Ok(self.next)
    }
    async fn record(
        &mut self,
        _kind: RecordKind,
        _id: &str,
    ) -> Result<Option<RecordState>, Self::Error> {
        Ok(self.record.clone())
    }
    async fn assignment(&mut self, _id: &str) -> Result<Option<AssignmentState>, Self::Error> {
        Ok(self.assignment.clone())
    }
    async fn policy_candidates(&mut self) -> Result<Vec<PolicyCandidate>, Self::Error> {
        Ok(self.candidates.clone())
    }
    async fn active_private_session(
        &mut self,
        _session_id: &str,
        _assignment_id: Option<&str>,
    ) -> Result<bool, Self::Error> {
        Ok(self.active_session)
    }
}

fn fake(user: &str) -> Fake {
    Fake {
        actor: actor(user, &["ai.manage", "records.read"]),
        now: 1_000,
        next: Some(0),
        ..Fake::default()
    }
}

#[test]
fn agent_input_trims_and_bounds_values() {
    let parsed = AgentInput::parse(&json!({
        "name": "  Support agent  ",
        "purpose": "  Answer support questions  ",
        "unexpected": true,
    }))
    .unwrap();
    assert_eq!(parsed.name, "Support agent");
    assert_eq!(parsed.purpose, "Answer support questions");
    for invalid in [
        json!({"name": "A", "purpose": "Answer questions"}),
        json!({"name": "Support", "purpose": "ab"}),
        json!({"name": 7, "purpose": "Answer questions"}),
        json!({"name": "Support"}),
    ] {
        assert_eq!(
            AgentInput::parse(&invalid).unwrap_err().code,
            "VALIDATION_FAILED",
            "{invalid}"
        );
    }
    assert_eq!(
        AgentInput::parse(&json!({
            "name": "Support",
            "purpose": "x".repeat(3001),
        }))
        .unwrap_err()
        .code,
        "VALIDATION_FAILED"
    );
}

#[test]
fn version_and_assignment_inputs_preserve_retained_rules() {
    let version = AgentVersionInput::parse(&json!({
        "agentId": "agent-1",
        "prompt": " Answer with cited sources ",
        "allowedSources": ["/finance", "/records?tab=open"],
    }))
    .unwrap();
    assert_eq!(version.prompt, "Answer with cited sources");
    for invalid in [
        json!({"agentId": "agent-1", "prompt": "Prompt", "allowedSources": []}),
        json!({"agentId": "", "prompt": "Prompt", "allowedSources": ["/"]}),
        json!({"agentId": "agent-1", "prompt": "Prompt", "allowedSources": ["finance"]}),
        json!({"agentId": "agent-1", "prompt": "Prompt", "allowedSources": [1]}),
    ] {
        assert_eq!(
            AgentVersionInput::parse(&invalid).unwrap_err().code,
            "VALIDATION_FAILED",
            "{invalid}"
        );
    }
    let assignment = AgentAssignmentInput::parse(&json!({
        "agentVersionId": "version-1",
        "poolId": "pool-1",
        "scopePath": "/finance/invoices",
    }))
    .unwrap();
    assert_eq!(assignment.pool_id.as_deref(), Some("pool-1"));
    assert_eq!(assignment.user_id, None);
    assert_eq!(assignment.scope_path, "/finance/invoices");
    for invalid in [
        json!({"agentVersionId": "version-1", "scopePath": "/finance"}),
        json!({"agentVersionId": "version-1", "poolId": "p", "userId": "u", "scopePath": "/finance"}),
        json!({"agentVersionId": "version-1", "userId": "u", "scopePath": "/finance?tab=1"}),
        json!({"agentVersionId": "version-1", "userId": "u", "scopePath": "finance"}),
    ] {
        assert_eq!(
            AgentAssignmentInput::parse(&invalid).unwrap_err().code,
            "VALIDATION_FAILED",
            "{invalid}"
        );
    }
}

#[test]
fn pool_members_are_deduplicated_and_lifecycle_table_is_exact() {
    let pool = AgentPoolInput::parse(&json!({
        "name": "  Finance routing  ",
        "userIds": ["user-1", "user-1", "user-2"],
    }))
    .unwrap();
    assert_eq!(pool.name, "Finance routing");
    assert_eq!(pool.user_ids, vec!["user-1", "user-1", "user-2"]);
    assert_eq!(
        AgentPoolChange::parse(&json!({
            "action": "replace-members",
            "expectedVersion": 2,
            "userIds": ["user-2", "user-1"],
            "reason": "Reviewed routing change",
        }))
        .unwrap()
        .user_ids,
        vec!["user-2", "user-1"]
    );
    assert_eq!(
        lifecycle_transition(RecordKind::Agent, LifecycleAction::Approve),
        Some(("draft", "approved"))
    );
    assert_eq!(
        lifecycle_transition(RecordKind::Version, LifecycleAction::Retire),
        Some(("approved", "retired"))
    );
    assert_eq!(
        lifecycle_transition(RecordKind::Assignment, LifecycleAction::Revoke),
        Some(("active", "revoked"))
    );
    assert_eq!(
        lifecycle_transition(RecordKind::Assignment, LifecycleAction::Approve),
        None
    );
    assert_eq!(
        lifecycle_transition(RecordKind::Agent, LifecycleAction::Revoke),
        None
    );
    assert!(assert_independent_review("creator", "reviewer", LifecycleAction::Approve).is_ok());
    let error =
        assert_independent_review("creator", "creator", LifecycleAction::Approve).unwrap_err();
    assert_eq!(error.code, "CONFLICT");
    assert!(assert_independent_review("creator", "creator", LifecycleAction::Retire).is_ok());
}

#[test]
fn source_coverage_and_assistant_policy_fail_closed() {
    assert!(agent_source_allowed(
        "/finance/invoices/1",
        &["/finance".into()]
    ));
    assert!(agent_source_allowed(
        "/finance?tab=open",
        &["/finance".into()]
    ));
    assert!(!agent_source_allowed(
        "/finance-other",
        &["/finance".into()]
    ));
    assert!(agent_source_allowed("/anything", &["/".into()]));

    let candidate = PolicyCandidate {
        assignment_id: "assignment-1".into(),
        assignment_version: 3,
        scope_path: "/finance".into(),
        pool_id: Some("pool-1".into()),
        agent_version_id: "version-1".into(),
        agent_version_revision: 2,
        prompt: "Answer with citations".into(),
        allowed_sources: vec!["/finance".into()],
        agent_version_status: "approved".into(),
        catalog_version: 4,
        catalog_status: "approved".into(),
        pool_version: Some(5),
        pool_status: Some("active".into()),
    };
    let policy = assistant_policy(std::slice::from_ref(&candidate), "/finance/invoices").unwrap();
    assert_eq!(policy.assignment_id.as_deref(), Some("assignment-1"));
    assert_eq!(policy.pool_id.as_deref(), Some("pool-1"));
    assert_eq!(policy.catalog_version, Some(4));
    assert_eq!(
        assistant_policy(std::slice::from_ref(&candidate), "/people").unwrap(),
        AssistantPolicy::manual()
    );
    let mut stale = candidate;
    stale.pool_status = Some("retired".into());
    assert_eq!(
        assistant_policy(&[stale], "/finance").unwrap_err().code,
        "AI_AGENT_UNAVAILABLE"
    );
}

#[tokio::test]
async fn create_agent_is_trimmed_idempotent_and_recorded_once() {
    let mut ports = fake("creator");
    let raw = json!({"name": "  Support agent  ", "purpose": "Answer support questions"});
    let first = create_agent(&mut ports, &raw, "agent-key").await.unwrap();
    assert_eq!(
        first,
        json!({"id": "agent-agent-key", "status": "draft", "version": 1})
    );
    let second = create_agent(&mut ports, &raw, "agent-key").await.unwrap();
    assert_eq!(second, first);
    assert_eq!(ports.commits.len(), 1);
    assert_eq!(ports.commits[0].0, "agent.create");
}

#[tokio::test]
async fn lifecycle_requires_current_version_and_independent_review() {
    let mut ports = fake("creator");
    ports.record = Some(RecordState {
        status: "draft".into(),
        version: 2,
        created_by: "creator".into(),
    });
    let raw = json!({"action": "approve", "expectedVersion": 2, "reason": "Reviewed agent"});
    let error = change_agent_record(&mut ports, RecordKind::Agent, "agent-1", &raw, "key")
        .await
        .unwrap_err();
    assert_eq!(error.code, "CONFLICT");

    ports.actor = actor("reviewer", &["ai.manage", "records.read"]);
    let result = change_agent_record(&mut ports, RecordKind::Agent, "agent-1", &raw, "key")
        .await
        .unwrap();
    assert_eq!(
        result,
        json!({"id": "agent-1", "status": "approved", "version": 3})
    );

    ports.record = Some(RecordState {
        status: "draft".into(),
        version: 2,
        created_by: "reviewer".into(),
    });
    let stale = json!({"action": "approve", "expectedVersion": 1, "reason": "Reviewed agent"});
    assert_eq!(
        change_agent_record(&mut ports, RecordKind::Agent, "agent-1", &stale, "key-2")
            .await
            .unwrap_err()
            .code,
        "CONFLICT"
    );
    let invalid = json!({"action": "revoke", "expectedVersion": 2, "reason": "Reviewed agent"});
    assert_eq!(
        change_agent_record(&mut ports, RecordKind::Agent, "agent-1", &invalid, "key-3")
            .await
            .unwrap_err()
            .code,
        "AGENT_ACTION_INVALID"
    );
}

#[tokio::test]
async fn assignment_delegate_requires_an_active_current_version() {
    let mut ports = fake("manager");
    ports.assignment = Some(AssignmentState {
        status: "revoked".into(),
        version: 1,
        agent_version_id: "version-1".into(),
        pool_id: Some("pool-1".into()),
        user_id: None,
        scope_path: "/finance".into(),
    });
    let raw = json!({
        "expectedVersion": 1,
        "userId": "user-2",
        "scopePath": "/finance",
        "reason": "Delegate the finance scope",
    });
    assert_eq!(
        delegate_agent_assignment(&mut ports, "assignment-1", &raw, "delegate-key")
            .await
            .unwrap_err()
            .code,
        "CONFLICT"
    );
    ports.assignment = Some(AssignmentState {
        status: "active".into(),
        version: 1,
        agent_version_id: "version-1".into(),
        pool_id: Some("pool-1".into()),
        user_id: None,
        scope_path: "/finance".into(),
    });
    let result = delegate_agent_assignment(&mut ports, "assignment-1", &raw, "delegate-key")
        .await
        .unwrap();
    assert_eq!(result["previousId"], "assignment-1");
    assert_eq!(result["id"], "agent-assignment-delegate-key");
    assert_eq!(result["version"], 1);

    let retried = retry_agent_assignment(
        &mut ports,
        "agent-assignment-delegate-key",
        &json!({"expectedVersion": 1, "reason": "Retry the delegated scope"}),
        "retry-key",
    )
    .await
    .unwrap();
    assert_eq!(retried["id"], "agent-assignment-retry-key");
    assert_eq!(retried["previousId"], "agent-assignment-delegate-key");

    let cancelled = cancel_agent_assignment(
        &mut ports,
        "agent-assignment-retry-key",
        &json!({"expectedVersion": 1, "reason": "End the delegated scope"}),
        "cancel-key",
    )
    .await
    .unwrap();
    assert_eq!(
        cancelled,
        json!({"id": "agent-assignment-retry-key", "status": "revoked", "version": 2})
    );
}

#[test]
fn private_session_input_allows_hold_release_and_expire_only() {
    let hold = PrivateSessionInput::parse(&json!({
        "action": "hold",
        "expectedVersion": 2,
        "reason": "Legal retention review",
    }))
    .unwrap();
    assert_eq!(hold.action, PrivateSessionAction::Hold);
    assert_eq!(
        PrivateSessionInput::parse(&json!({
            "action": "release-hold",
            "expectedVersion": 2,
            "reason": "Review complete",
        }))
        .unwrap()
        .action,
        PrivateSessionAction::ReleaseHold
    );
    assert_eq!(
        PrivateSessionInput::parse(&json!({
            "action": "delete",
            "expectedVersion": 2,
            "reason": "Review complete",
        }))
        .unwrap_err()
        .kind,
        FailureKind::InvalidInput
    );
}
