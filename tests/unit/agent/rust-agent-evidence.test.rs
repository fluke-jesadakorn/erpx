//! Native Agent evidence audit-read policy, privacy and validation tests.
use erpx_server_platform_administration::public::agent_evidence::{
    AgentEvidenceActor, AgentEvidenceFuture, AgentEvidenceKind, AgentEvidenceRecord,
    AgentEvidenceRepository, AgentEvidenceWorkspace, AgentInvocationEvidence, AgentPrivateSession,
    INVOCATION_LIMIT, SESSION_LIMIT, agent_evidence_record, agent_evidence_workspace,
    validate_invocation, validate_record, validate_session, validate_workspace,
};
use erpx_server_runtime::public::http_support::serde_json;
use erpx_server_shared::public::{BusinessError, FailureKind};

const NOW: i64 = 1_800_000_000_000;

fn actor() -> AgentEvidenceActor {
    AgentEvidenceActor {
        user_id: "claimant".into(),
        company_id: "company".into(),
        permissions: vec!["records.read".into()],
        modules: vec!["agent".into()],
        expires_at: NOW + 60_000,
    }
}

fn manager() -> AgentEvidenceActor {
    AgentEvidenceActor {
        permissions: vec!["records.read".into(), "ai.manage".into()],
        ..actor()
    }
}

fn digest() -> String {
    "a".repeat(64)
}

fn invocation(id: &str, user: &str, created_at: i64) -> AgentInvocationEvidence {
    AgentInvocationEvidence {
        id: id.into(),
        session_id: "session-1".into(),
        turn_id: format!("turn-{id}"),
        company_id: "company".into(),
        user_id: user.into(),
        agent_version_id: Some("version-1".into()),
        question_digest: digest(),
        sources_digest: digest(),
        outcome: "succeeded".into(),
        input_tokens: Some(10),
        output_tokens: Some(20),
        error: None,
        created_at,
    }
}

fn session(
    id: &str,
    user: &str,
    status: &str,
    expires_at: i64,
    updated_at: i64,
) -> AgentPrivateSession {
    AgentPrivateSession {
        id: id.into(),
        company_id: "company".into(),
        user_id: user.into(),
        assignment_id: "assignment-1".into(),
        conversation_key: format!("conversation-{id}"),
        generation: 1,
        created_at: 10,
        status: status.into(),
        expires_at,
        legal_hold: false,
        hold_reason: None,
        version: 1,
        last_used_at: 10,
        updated_at,
        effective_status: String::new(),
    }
}

struct Repo {
    invocations: Vec<AgentInvocationEvidence>,
    sessions: Vec<AgentPrivateSession>,
    invocation: Option<AgentInvocationEvidence>,
    session: Option<AgentPrivateSession>,
    fail: Option<BusinessError>,
    calls: usize,
}

impl Repo {
    fn new() -> Self {
        Self {
            invocations: Vec::new(),
            sessions: Vec::new(),
            invocation: None,
            session: None,
            fail: None,
            calls: 0,
        }
    }
    fn failing(error: BusinessError) -> Self {
        Self {
            fail: Some(error),
            ..Self::new()
        }
    }
    fn call(&mut self) -> Result<(), BusinessError> {
        self.calls += 1;
        match self.fail.clone() {
            Some(error) => Err(error),
            None => Ok(()),
        }
    }
}

impl AgentEvidenceRepository for Repo {
    type Error = BusinessError;
    fn invocations<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> AgentEvidenceFuture<'a, Vec<AgentInvocationEvidence>, Self::Error> {
        let rows = self.invocations.clone();
        let result = self.call().map(|_| rows);
        Box::pin(async move { result })
    }
    fn sessions<'a>(
        &'a mut self,
        _company_id: &'a str,
        _user_id: &'a str,
        _manage: bool,
    ) -> AgentEvidenceFuture<'a, Vec<AgentPrivateSession>, Self::Error> {
        let rows = self.sessions.clone();
        let result = self.call().map(|_| rows);
        Box::pin(async move { result })
    }
    fn invocation<'a>(
        &'a mut self,
        _company_id: &'a str,
        _id: &'a str,
    ) -> AgentEvidenceFuture<'a, Option<AgentInvocationEvidence>, Self::Error> {
        let row = self.invocation.clone();
        let result = self.call().map(|_| row);
        Box::pin(async move { result })
    }
    fn session<'a>(
        &'a mut self,
        _company_id: &'a str,
        _id: &'a str,
        _user_id: &'a str,
        _manage: bool,
    ) -> AgentEvidenceFuture<'a, Option<AgentPrivateSession>, Self::Error> {
        let row = self.session.clone();
        let result = self.call().map(|_| row);
        Box::pin(async move { result })
    }
}

async fn run_workspace(
    actor: &AgentEvidenceActor,
    repo: &mut Repo,
) -> Result<AgentEvidenceWorkspace, BusinessError> {
    agent_evidence_workspace(actor, repo, NOW).await
}

#[tokio::test]
async fn workspace_requires_permission_module_and_current_context_before_source_read() {
    let mut denied = actor();
    denied.permissions.clear();
    let mut repo = Repo::new();
    let error = run_workspace(&denied, &mut repo).await.unwrap_err();
    assert_eq!(error.code, "PERMISSION_REQUIRED");
    assert_eq!(error.kind, FailureKind::Forbidden);
    assert_eq!(repo.calls, 0);

    let mut no_module = actor();
    no_module.modules.clear();
    let error = run_workspace(&no_module, &mut repo).await.unwrap_err();
    assert_eq!(error.code, "MODULE_FORBIDDEN");
    assert_eq!(repo.calls, 0);

    let mut stale = actor();
    stale.expires_at = NOW;
    let error = run_workspace(&stale, &mut repo).await.unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
    assert_eq!(error.kind, FailureKind::Conflict);
    assert_eq!(repo.calls, 0);
}

#[tokio::test]
async fn workspace_applies_own_session_privacy_and_effective_expiry() {
    let mut repo = Repo::new();
    repo.invocations = vec![
        invocation("i1", "claimant", 20),
        invocation("i2", "other", 10),
    ];
    repo.sessions = vec![
        session("s1", "claimant", "active", NOW + 1, 30),
        session("s2", "claimant", "active", NOW, 20),
        session("s3", "claimant", "expired", NOW + 100, 10),
    ];
    let workspace = run_workspace(&actor(), &mut repo).await.unwrap();
    assert_eq!(workspace.invocations.len(), 2);
    assert_eq!(workspace.sessions.len(), 3);
    assert_eq!(workspace.invocations[0].id, "i1");
    assert_eq!(workspace.sessions[0].id, "s1");
    assert_eq!(workspace.sessions[0].effective_status, "active");
    assert_eq!(workspace.sessions[1].id, "s2");
    assert_eq!(workspace.sessions[1].effective_status, "expired");
    assert_eq!(workspace.sessions[2].id, "s3");
    assert_eq!(workspace.sessions[2].effective_status, "expired");
}

#[tokio::test]
async fn manager_sees_all_company_sessions_and_foreign_rows_fail_closed() {
    let mut repo = Repo::new();
    repo.sessions = vec![
        session("s1", "claimant", "active", NOW + 1, 20),
        session("s2", "other", "active", NOW + 1, 10),
    ];
    let workspace = run_workspace(&manager(), &mut repo).await.unwrap();
    assert_eq!(workspace.sessions.len(), 2);

    let mut foreign = session("s3", "other", "active", NOW + 1, 10);
    foreign.company_id = "foreign".into();
    repo.sessions = vec![foreign];
    let error = run_workspace(&manager(), &mut repo).await.unwrap_err();
    assert_eq!(error.code, "AGENT_EVIDENCE_SOURCE_INVALID");
    assert_eq!(error.kind, FailureKind::Unavailable);

    let mut other_user = session("s4", "other", "active", NOW + 1, 10);
    other_user.company_id = "company".into();
    repo.sessions = vec![other_user];
    let error = run_workspace(&actor(), &mut repo).await.unwrap_err();
    assert_eq!(error.code, "AGENT_EVIDENCE_SOURCE_INVALID");
}

#[tokio::test]
async fn workspace_rejects_duplicate_ids_bad_digest_outcome_and_hold_reason() {
    let mut repo = Repo::new();
    repo.invocations = vec![
        invocation("i1", "claimant", 10),
        invocation("i1", "claimant", 20),
    ];
    let error = run_workspace(&actor(), &mut repo).await.unwrap_err();
    assert_eq!(error.code, "AGENT_EVIDENCE_SOURCE_INVALID");

    repo.invocations = vec![invocation("i1", "claimant", 10)];
    repo.invocations[0].question_digest = "A".repeat(64);
    let error = run_workspace(&actor(), &mut repo).await.unwrap_err();
    assert_eq!(error.code, "AGENT_EVIDENCE_SOURCE_INVALID");

    repo.invocations[0].question_digest = digest();
    repo.invocations[0].outcome = "unknown".into();
    let error = run_workspace(&actor(), &mut repo).await.unwrap_err();
    assert_eq!(error.code, "AGENT_EVIDENCE_SOURCE_INVALID");

    repo.invocations[0].outcome = "failed".into();
    repo.invocations[0].error = Some("provider unavailable".into());
    repo.sessions = vec![session("s1", "claimant", "active", NOW + 1, 10)];
    repo.sessions[0].legal_hold = true;
    let error = run_workspace(&actor(), &mut repo).await.unwrap_err();
    assert_eq!(error.code, "AGENT_EVIDENCE_SOURCE_INVALID");

    repo.sessions[0].hold_reason = Some("retention request".into());
    let workspace = run_workspace(&actor(), &mut repo).await.unwrap();
    assert!(workspace.sessions[0].legal_hold);
}

#[tokio::test]
async fn workspace_bounds_collections_and_sorts_newest_first() {
    let mut repo = Repo::new();
    repo.invocations = (0..INVOCATION_LIMIT as i64)
        .map(|index| invocation(&format!("i{index:02}"), "claimant", index))
        .collect();
    repo.sessions = (0..SESSION_LIMIT as i64)
        .map(|index| {
            session(
                &format!("s{index:03}"),
                "claimant",
                "active",
                NOW + 1,
                index,
            )
        })
        .collect();
    let workspace = run_workspace(&actor(), &mut repo).await.unwrap();
    assert_eq!(workspace.invocations.len(), INVOCATION_LIMIT);
    assert_eq!(workspace.sessions.len(), SESSION_LIMIT);
    assert_eq!(workspace.invocations[0].id, "i49");
    assert_eq!(workspace.sessions[0].id, "s099");

    repo.invocations.push(invocation("extra", "claimant", 0));
    let error = run_workspace(&actor(), &mut repo).await.unwrap_err();
    assert_eq!(error.code, "AGENT_EVIDENCE_SOURCE_INVALID");

    repo.invocations.truncate(1);
    repo.sessions
        .push(session("extra", "claimant", "active", NOW + 1, 0));
    let error = run_workspace(&actor(), &mut repo).await.unwrap_err();
    assert_eq!(error.code, "AGENT_EVIDENCE_SOURCE_INVALID");
}

#[tokio::test]
async fn record_reads_invocation_company_wide_and_session_private() {
    let mut repo = Repo::new();
    repo.invocation = Some(invocation("i1", "other", 10));
    let record = agent_evidence_record(
        &actor(),
        AgentEvidenceKind::Invocation,
        "i1",
        &mut repo,
        NOW,
    )
    .await
    .unwrap();
    let erpx_server_platform_administration::public::agent_evidence::AgentEvidenceRecord::Invocation {
        record,
    } = record
    else {
        panic!("expected invocation record");
    };
    assert_eq!(record.id, "i1");

    repo.session = Some(session("s1", "other", "active", NOW + 1, 10));
    let error = agent_evidence_record(&actor(), AgentEvidenceKind::Session, "s1", &mut repo, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "AGENT_EVIDENCE_SOURCE_INVALID");

    repo.session = None;
    let error = agent_evidence_record(&actor(), AgentEvidenceKind::Session, "s1", &mut repo, NOW)
        .await
        .unwrap_err();
    assert_eq!(error.code, "AGENT_RECORD_NOT_FOUND");
    assert_eq!(error.kind, FailureKind::NotFound);

    repo.session = Some(session("s1", "other", "active", NOW + 1, 10));

    let record =
        agent_evidence_record(&manager(), AgentEvidenceKind::Session, "s1", &mut repo, NOW)
            .await
            .unwrap();
    let erpx_server_platform_administration::public::agent_evidence::AgentEvidenceRecord::Session {
        record,
    } = record
    else {
        panic!("expected session record");
    };
    assert_eq!(record.id, "s1");
    assert_eq!(record.effective_status, "active");
}

#[tokio::test]
async fn record_rejects_foreign_company_payloads_and_missing_rows() {
    let mut repo = Repo::new();
    let mut foreign = invocation("i1", "claimant", 10);
    foreign.company_id = "foreign".into();
    repo.invocation = Some(foreign);
    let error = agent_evidence_record(
        &actor(),
        AgentEvidenceKind::Invocation,
        "i1",
        &mut repo,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "AGENT_EVIDENCE_SOURCE_INVALID");

    repo.invocation = None;
    let error = agent_evidence_record(
        &actor(),
        AgentEvidenceKind::Invocation,
        "i1",
        &mut repo,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "AGENT_RECORD_NOT_FOUND");
}

#[tokio::test]
async fn repository_failure_propagates_without_partial_output() {
    let mut repo = Repo::failing(BusinessError::new(
        FailureKind::Unavailable,
        "AGENT_EVIDENCE_SOURCE_INVALID",
        "The current Agent evidence source is invalid.",
    ));
    let error = run_workspace(&actor(), &mut repo).await.unwrap_err();
    assert_eq!(error.code, "AGENT_EVIDENCE_SOURCE_INVALID");
    assert_eq!(repo.calls, 1);
}

#[test]
fn row_validators_reject_malformed_identity_and_retention_facts() {
    let mut row = session("s1", "claimant", "active", NOW + 1, 10);
    row.generation = 0;
    assert_eq!(
        validate_session(&actor(), row, NOW, false)
            .unwrap_err()
            .code,
        "AGENT_EVIDENCE_SOURCE_INVALID"
    );

    let mut row = invocation("i1", "claimant", 10);
    row.created_at = -1;
    assert_eq!(
        validate_invocation(&actor(), row).unwrap_err().code,
        "AGENT_EVIDENCE_SOURCE_INVALID"
    );

    let mut row = invocation("i1", "claimant", 10);
    row.error = Some("x".repeat(2_001));
    assert_eq!(
        validate_invocation(&actor(), row).unwrap_err().code,
        "AGENT_EVIDENCE_SOURCE_INVALID"
    );
}

#[test]
fn serialized_workspace_uses_the_bounded_transport_shape() {
    let workspace = validate_workspace(
        &manager(),
        NOW,
        vec![invocation("i1", "claimant", 10)],
        vec![session("s1", "other", "active", NOW + 1, 10)],
    )
    .unwrap();
    let value = serde_json::to_value(workspace).unwrap();
    assert_eq!(
        value.as_object().unwrap().keys().collect::<Vec<_>>(),
        vec!["invocations", "now", "sessions"]
    );
    let invocation = value["invocations"][0].as_object().unwrap();
    assert_eq!(
        invocation.keys().collect::<Vec<_>>(),
        vec![
            "agent_version_id",
            "company_id",
            "created_at",
            "error",
            "id",
            "input_tokens",
            "outcome",
            "output_tokens",
            "question_digest",
            "session_id",
            "sources_digest",
            "turn_id",
            "user_id",
        ]
    );
    let session_row = value["sessions"][0].as_object().unwrap();
    assert_eq!(
        session_row.keys().collect::<Vec<_>>(),
        vec![
            "assignment_id",
            "company_id",
            "conversation_key",
            "created_at",
            "effective_status",
            "expires_at",
            "generation",
            "hold_reason",
            "id",
            "last_used_at",
            "legal_hold",
            "status",
            "updated_at",
            "user_id",
            "version",
        ]
    );
    let record = validate_record(
        &manager(),
        AgentEvidenceKind::Session,
        AgentEvidenceRecord::Session {
            record: session("s1", "other", "active", NOW + 1, 10),
        },
        NOW,
    )
    .unwrap();
    let record = serde_json::to_value(record).unwrap();
    assert_eq!(record["kind"], "session");
    assert_eq!(record["record"]["id"], "s1");
    assert_eq!(record["record"]["effective_status"], "active");
}
