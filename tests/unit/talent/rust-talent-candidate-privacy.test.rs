//! Native Talent candidate creation and privacy policy, command and read tests.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use erpx_server_talent::public::candidate::{
    CandidateReceipt, CandidateRequest, CandidateRow, CreatePlan, PgTalentCandidates,
    PrivacyAction, PrivacyEventRow, PrivacyPlan, PrivacyRequest, TalentActor,
    TalentCandidateCommandRepository, TalentCandidateReadRepository, candidate_privacy_workspace,
    change_candidate_privacy, create_candidate,
};
use erpx_server_talent::public::requisition::ReceiptSnapshot;
use serde_json::Value;
use std::sync::Mutex;

const NOW: i64 = 1_800_000_000_000;
const TODAY: &str = "2026-11-11";
const DIGEST: &str = "bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb";

fn actor(user: &str, branch: Option<&str>, permissions: &[&str]) -> TalentActor {
    TalentActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: None,
        assignment_id: AssignmentId::new(format!("assignment-{user}")).unwrap(),
        role: Some("hr_admin".into()),
        authority_revision: Some(1),
        assignment_version: Some(1),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        expires_at: NOW + 60_000,
    }
}

fn recruiter() -> TalentActor {
    actor(
        "recruiter",
        Some("branch"),
        &["records.read", "records.write", "records.review"],
    )
}

fn privacy(user: &str, permissions: &[&str]) -> TalentActor {
    actor(user, None, permissions)
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn request(retention: &str) -> CandidateRequest {
    CandidateRequest {
        legal_name: "  Anong Candidate  ".into(),
        email: "  Anong@Example.TEST ".into(),
        phone: Some("  0812345678 ".into()),
        consent_evidence: "  Signed recruitment consent form  ".into(),
        retention_until: retention.into(),
    }
}

fn row(status: &str, version: i64, hold: bool) -> CandidateRow {
    CandidateRow {
        id: "talent-candidate-1".into(),
        company_id: "company".into(),
        legal_name: "Anong Candidate".into(),
        email: "anong@example.test".into(),
        phone: Some("0812345678".into()),
        consent_evidence: "Signed recruitment consent form".into(),
        retention_until: "2027-01-01".into(),
        status: status.into(),
        privacy_hold: hold,
        privacy_requested_by: None,
        requested_by_self: false,
        created_by: "recruiter".into(),
        version,
        created_at: NOW - 1_000,
        updated_at: NOW - 1_000,
    }
}

#[derive(Default)]
struct Fake {
    command_authority: bool,
    privacy_authority: bool,
    receipt: Option<ReceiptSnapshot>,
    row: Option<CandidateRow>,
    events: Vec<PrivacyEventRow>,
    calls: Mutex<Vec<String>>,
    commits: Mutex<Vec<String>>,
    fail_create: bool,
}

impl Fake {
    fn ready() -> Self {
        Self {
            command_authority: true,
            privacy_authority: true,
            row: Some(row("active", 1, false)),
            ..Self::default()
        }
    }
}

impl TalentCandidateCommandRepository for Fake {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a TalentActor,
        write: bool,
        _: i64,
    ) -> erpx_server_talent::public::candidate::CandidateFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(format!("authority:{write}"));
            Ok(self.command_authority)
        })
    }

    fn current_privacy_authority<'a>(
        &'a mut self,
        _: &'a TalentActor,
        permission: &'a str,
        _: i64,
    ) -> erpx_server_talent::public::candidate::CandidateFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.calls
                .lock()
                .unwrap()
                .push(format!("privacy-authority:{permission}"));
            Ok(self.privacy_authority)
        })
    }

    fn digest<'a>(
        &'a mut self,
        _: &'a str,
    ) -> erpx_server_talent::public::candidate::CandidateFuture<'a, String, Self::Error> {
        Box::pin(async move { Ok(DIGEST.into()) })
    }

    fn receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> erpx_server_talent::public::candidate::CandidateFuture<
        'a,
        Option<ReceiptSnapshot>,
        Self::Error,
    > {
        Box::pin(async move { Ok(self.receipt.clone()) })
    }

    fn candidate_for_update<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
    ) -> erpx_server_talent::public::candidate::CandidateFuture<'a, Option<CandidateRow>, Self::Error>
    {
        Box::pin(async move { Ok(self.row.clone()) })
    }

    fn commit_create<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
        plan: &'a CreatePlan,
        _: &'a str,
    ) -> erpx_server_talent::public::candidate::CandidateFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            if self.fail_create {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SYNTHETIC",
                    "Synthetic create failure.",
                ));
            }
            self.commits
                .lock()
                .unwrap()
                .push(format!("create:{}:{}", plan.id, plan.email));
            Ok(CandidateReceipt::new(plan.id.clone(), "active", 1).to_wire())
        })
    }

    fn commit_privacy<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
        plan: &'a PrivacyPlan,
        _: &'a str,
    ) -> erpx_server_talent::public::candidate::CandidateFuture<'a, Value, Self::Error> {
        Box::pin(async move {
            self.commits.lock().unwrap().push(format!(
                "{}:{}:{}:{}",
                plan.action.as_str(),
                plan.id,
                plan.status,
                plan.privacy_hold
            ));
            Ok(CandidateReceipt::privacy(
                plan.id.clone(),
                &plan.status,
                plan.version,
                plan.privacy_hold,
            )
            .to_wire())
        })
    }
}

impl TalentCandidateReadRepository for Fake {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: bool,
        _: i64,
    ) -> erpx_server_talent::public::candidate::CandidateFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.command_authority) })
    }

    fn current_privacy_authority<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: i64,
    ) -> erpx_server_talent::public::candidate::CandidateFuture<'a, bool, Self::Error> {
        Box::pin(async move { Ok(self.privacy_authority) })
    }

    fn candidate<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
    ) -> erpx_server_talent::public::candidate::CandidateFuture<'a, Option<CandidateRow>, Self::Error>
    {
        Box::pin(async move { Ok(self.row.clone()) })
    }

    fn privacy_events<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
    ) -> erpx_server_talent::public::candidate::CandidateFuture<'a, Vec<PrivacyEventRow>, Self::Error>
    {
        Box::pin(async move { Ok(self.events.clone()) })
    }
}

#[tokio::test]
async fn create_persists_normalized_consent_evidence() {
    let mut fake = Fake::ready();
    let result = create_candidate(
        &mut fake,
        &recruiter(),
        &key("talent-candidate-test-0001"),
        &request("2027-01-01"),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result["id"], "talent-candidate-talent-candidate-test-0001");
    assert_eq!(result["status"], "active");
    assert_eq!(result["version"], 1);
    assert_eq!(
        fake.commits.lock().unwrap().clone(),
        vec!["create:talent-candidate-talent-candidate-test-0001:anong@example.test".to_owned()]
    );
    assert_eq!(
        fake.calls.lock().unwrap().clone(),
        vec!["authority:true".to_owned()]
    );
}

#[tokio::test]
async fn create_rejects_past_retention_and_bad_email_before_commit() {
    for (name, input, code) in [
        (
            "past",
            CandidateRequest {
                retention_until: "2026-11-10".into(),
                ..request("2026-11-10")
            },
            "INVALID_RETENTION",
        ),
        (
            "same-day",
            CandidateRequest {
                retention_until: TODAY.into(),
                ..request(TODAY)
            },
            "INVALID_RETENTION",
        ),
        (
            "email",
            CandidateRequest {
                email: "not-an-email".into(),
                ..request("2027-01-01")
            },
            "TALENT_CANDIDATE_INVALID",
        ),
        (
            "name",
            CandidateRequest {
                legal_name: " A ".into(),
                ..request("2027-01-01")
            },
            "TALENT_CANDIDATE_INVALID",
        ),
    ] {
        let mut fake = Fake::ready();
        let error = create_candidate(
            &mut fake,
            &recruiter(),
            &key(&format!("talent-candidate-test-{name}")),
            &input,
            TODAY,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, code, "{name}");
        assert_eq!(error.kind, FailureKind::InvalidInput, "{name}");
        assert!(fake.commits.lock().unwrap().is_empty(), "{name}");
    }
}

#[tokio::test]
async fn create_denials_are_permission_and_current_authority_bound() {
    let mut no_write = Fake::ready();
    let error = create_candidate(
        &mut no_write,
        &actor(
            "reader",
            Some("branch"),
            &["records.read", "records.review"],
        ),
        &key("talent-candidate-test-0002"),
        &request("2027-01-01"),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "TALENT_PERMISSION_REQUIRED");
    assert!(no_write.commits.lock().unwrap().is_empty());

    let mut stale = Fake::ready();
    stale.command_authority = false;
    let error = create_candidate(
        &mut stale,
        &recruiter(),
        &key("talent-candidate-test-0003"),
        &request("2027-01-01"),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "TALENT_ACCESS_CHANGED");

    let mut expired = Fake::ready();
    let mut actor = recruiter();
    actor.expires_at = NOW;
    let error = create_candidate(
        &mut expired,
        &actor,
        &key("talent-candidate-test-0004"),
        &request("2027-01-01"),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "WORK_CONTEXT_CHANGED");
}

#[tokio::test]
async fn replay_returns_the_original_receipt_and_conflicts_on_a_new_request() {
    let mut fake = Fake::ready();
    let first = create_candidate(
        &mut fake,
        &recruiter(),
        &key("talent-candidate-test-0005"),
        &request("2027-01-01"),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    fake.receipt = Some(ReceiptSnapshot {
        request_hash: DIGEST.into(),
        result_json: first.to_string(),
        operation: "talent.candidate.create".into(),
        actor_id: "recruiter".into(),
        record_id: "talent-candidate-talent-candidate-test-0005".into(),
        authority_json: r#"{"companyId":"company","assignmentId":"assignment-recruiter"}"#.into(),
    });
    let replay = create_candidate(
        &mut fake,
        &recruiter(),
        &key("talent-candidate-test-0005"),
        &request("2027-01-01"),
        TODAY,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(replay, first);
    assert_eq!(fake.commits.lock().unwrap().len(), 1);

    fake.receipt.as_mut().unwrap().request_hash = "e".repeat(64);
    let error = create_candidate(
        &mut fake,
        &recruiter(),
        &key("talent-candidate-test-0005"),
        &request("2027-02-01"),
        TODAY,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "TALENT_CANDIDATE_IDEMPOTENCY_CONFLICT");
}

fn privacy_request(action: PrivacyAction, version: i64) -> PrivacyRequest {
    PrivacyRequest {
        action,
        expected_version: version,
        reason: "Independent privacy review recorded".into(),
    }
}

#[tokio::test]
async fn privacy_state_machine_holds_releases_withdraws_and_erases() {
    let mut fake = Fake::ready();
    let legal = privacy(
        "legal",
        &["records.read", "legal.hold", "legal.hold.release"],
    );
    let held = change_candidate_privacy(
        &mut fake,
        &legal,
        &key("talent-privacy-hold-0001"),
        "talent-candidate-1",
        &privacy_request(PrivacyAction::Hold, 1),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(held["status"], "active");
    assert_eq!(held["privacyHold"], true);
    assert_eq!(held["version"], 2);

    fake.row = Some(row("active", 2, true));
    let hold_again = change_candidate_privacy(
        &mut fake,
        &legal,
        &key("talent-privacy-hold-0002"),
        "talent-candidate-1",
        &privacy_request(PrivacyAction::Hold, 2),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(hold_again.code, "CANDIDATE_PRIVACY_STATE");

    fake.row = Some(CandidateRow {
        privacy_hold: true,
        version: 3,
        ..row("active", 3, true)
    });
    let released = change_candidate_privacy(
        &mut fake,
        &legal,
        &key("talent-privacy-release-0001"),
        "talent-candidate-1",
        &privacy_request(PrivacyAction::Release, 3),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(released["privacyHold"], false);

    let people = privacy("hr", &["records.read", "people.manage"]);
    fake.row = Some(row("active", 4, false));
    let withdrawn = change_candidate_privacy(
        &mut fake,
        &people,
        &key("talent-privacy-withdraw-0001"),
        "talent-candidate-1",
        &privacy_request(PrivacyAction::Withdraw, 4),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(withdrawn["status"], "withdrawn");
    assert_eq!(withdrawn["version"], 5);

    fake.row = Some(CandidateRow {
        privacy_requested_by: Some("requester".into()),
        ..row("withdrawn", 5, false)
    });
    let eraser = privacy("legal-erase", &["records.read", "legal.destroy"]);
    let erased = change_candidate_privacy(
        &mut fake,
        &eraser,
        &key("talent-privacy-erase-0001"),
        "talent-candidate-1",
        &privacy_request(PrivacyAction::Erase, 5),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(erased["status"], "erased");
    assert_eq!(
        fake.commits.lock().unwrap().last().map(String::as_str),
        Some("erase:talent-candidate-1:erased:false")
    );
}

#[tokio::test]
async fn privacy_requires_current_version_independent_erasure_and_scope() {
    let mut fake = Fake::ready();
    let eraser = privacy("legal-erase", &["records.read", "legal.destroy"]);

    fake.row = Some(row("withdrawn", 6, false));
    let stale = change_candidate_privacy(
        &mut fake,
        &eraser,
        &key("talent-privacy-stale-0001"),
        "talent-candidate-1",
        &privacy_request(PrivacyAction::Erase, 5),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(stale.code, "CANDIDATE_CHANGED");

    fake.row = Some(row("active", 6, false));
    let blocked = change_candidate_privacy(
        &mut fake,
        &eraser,
        &key("talent-privacy-blocked-0001"),
        "talent-candidate-1",
        &privacy_request(PrivacyAction::Erase, 6),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(blocked.code, "CANDIDATE_ERASURE_BLOCKED");

    fake.row = Some(CandidateRow {
        privacy_requested_by: Some("legal-erase".into()),
        requested_by_self: true,
        ..row("withdrawn", 6, false)
    });
    let self_approval = change_candidate_privacy(
        &mut fake,
        &eraser,
        &key("talent-privacy-self-0001"),
        "talent-candidate-1",
        &privacy_request(PrivacyAction::Erase, 6),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(self_approval.code, "SELF_APPROVAL");

    let branch_bound = self_approval.kind;
    assert_eq!(branch_bound, FailureKind::Forbidden);

    let mut held = Fake::ready();
    held.row = Some(row("active", 1, true));
    let eraser_hold = change_candidate_privacy(
        &mut held,
        &eraser,
        &key("talent-privacy-hold-block-0001"),
        "talent-candidate-1",
        &privacy_request(PrivacyAction::Erase, 1),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(eraser_hold.code, "CANDIDATE_ERASURE_BLOCKED");

    let branch_actor = actor(
        "legal-branch",
        Some("branch"),
        &["records.read", "legal.hold"],
    );
    let scope = change_candidate_privacy(
        &mut fake,
        &branch_actor,
        &key("talent-privacy-scope-0001"),
        "talent-candidate-1",
        &privacy_request(PrivacyAction::Hold, 6),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(scope.code, "COMPANY_HR_SCOPE_REQUIRED");

    let no_permission = change_candidate_privacy(
        &mut fake,
        &privacy("legal-readonly", &["records.read"]),
        &key("talent-privacy-perm-0001"),
        "talent-candidate-1",
        &privacy_request(PrivacyAction::Hold, 6),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(no_permission.code, "CANDIDATE_PRIVACY_FORBIDDEN");

    let stale_authority = change_candidate_privacy(
        &mut {
            let mut value = Fake::ready();
            value.privacy_authority = false;
            value
        },
        &eraser,
        &key("talent-privacy-authority-0001"),
        "talent-candidate-1",
        &privacy_request(PrivacyAction::Erase, 1),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(stale_authority.code, "TALENT_ACCESS_CHANGED");
}

#[tokio::test]
async fn privacy_workspace_returns_safe_actions_and_bounded_history() {
    let mut fake = Fake::ready();
    fake.events = vec![
        PrivacyEventRow {
            id: "candidate-privacy-a".into(),
            action: "hold".into(),
            candidate_version: 2,
            created_at: NOW - 10,
        },
        PrivacyEventRow {
            id: "candidate-privacy-b".into(),
            action: "release".into(),
            candidate_version: 3,
            created_at: NOW - 5,
        },
    ];
    fake.row = Some(row("withdrawn", 4, false));
    let view = candidate_privacy_workspace(
        &mut fake,
        &privacy(
            "legal",
            &[
                "records.read",
                "legal.hold",
                "legal.hold.release",
                "legal.destroy",
            ],
        ),
        "talent-candidate-1",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(view["candidate"]["status"], "withdrawn");
    assert_eq!(view["candidate"]["privacyHold"], false);
    assert_eq!(view["actions"]["hold"], true);
    assert_eq!(view["actions"]["withdraw"], false);
    assert_eq!(view["actions"]["erase"], true);
    assert_eq!(view["history"].as_array().unwrap().len(), 2);
    assert_eq!(view["history"][0]["action"], "hold");
    assert_eq!(view["history"][1]["candidateVersion"], 3);

    fake.events = Vec::new();
    fake.row = Some(CandidateRow {
        requested_by_self: true,
        ..row("withdrawn", 4, false)
    });
    let view = candidate_privacy_workspace(
        &mut fake,
        &privacy("legal-erase", &["records.read", "legal.destroy"]),
        "talent-candidate-1",
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(view["actions"]["erase"], false);

    let forbidden = candidate_privacy_workspace(
        &mut fake,
        &privacy("plain", &["records.read"]),
        "talent-candidate-1",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(forbidden.code, "CANDIDATE_PRIVACY_FORBIDDEN");

    fake.row = None;
    let missing = candidate_privacy_workspace(
        &mut fake,
        &privacy("legal", &["records.read", "legal.hold"]),
        "talent-candidate-1",
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(missing.code, "CANDIDATE_NOT_FOUND");
}

#[test]
fn postgres_adapter_is_exported_for_composition() {
    fn assert_send_sync<T: Send>() {}
    assert_send_sync::<PgTalentCandidates<'static>>();
}
