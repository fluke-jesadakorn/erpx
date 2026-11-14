//! Native Talent requisition policy, command and read tests.
use erpx_server_shared::public::{
    AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, UserId,
};
use erpx_server_talent::public::requisition::{
    ChangePlan, CreatePlan, MAX_DESCRIPTION, MAX_REASON, MAX_TITLE, ReceiptSnapshot,
    RequisitionAction, RequisitionChange, RequisitionEventRow, RequisitionFuture,
    RequisitionReceipt, RequisitionRequest, RequisitionRow, TalentActor,
    TalentRequisitionCommandRepository, TalentRequisitionReadRepository, change_requisition,
    create_requisition, normalize_text, requisition_detail, requisition_workspace,
};
use serde_json::Value;
use std::sync::Mutex;

const NOW: i64 = 1_800_000_000_000;
const KEY: &str = "talent-requisition-test-0001";
const DIGEST: &str = "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";

fn actor(
    user: &str,
    branch: Option<&str>,
    department: Option<&str>,
    permissions: &[&str],
) -> TalentActor {
    TalentActor {
        user_id: UserId::new(user).unwrap(),
        holding_id: HoldingId::new("h001").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: branch.map(|value| RecordId::new(value).unwrap()),
        department_id: department.map(|value| RecordId::new(value).unwrap()),
        assignment_id: AssignmentId::new(format!("assignment-{user}")).unwrap(),
        role: Some("holding_owner".into()),
        authority_revision: Some(3),
        assignment_version: Some(2),
        permissions: permissions
            .iter()
            .map(|value| (*value).to_owned())
            .collect(),
        expires_at: NOW + 60_000,
    }
}

fn writer() -> TalentActor {
    actor(
        "creator",
        Some("branch"),
        None,
        &["records.read", "records.write", "records.review"],
    )
}

fn reviewer() -> TalentActor {
    actor(
        "reviewer",
        Some("branch"),
        None,
        &["records.read", "records.write", "records.review"],
    )
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn request() -> RequisitionRequest {
    RequisitionRequest {
        job_title: "  Service specialist  ".into(),
        openings: 2,
        target_start_date: "2026-11-01".into(),
        description: "  Serve customer cases and complete field handoffs.  ".into(),
    }
}

fn change(action: RequisitionAction, expected: i64) -> RequisitionChange {
    RequisitionChange {
        action,
        expected_version: expected,
        reason: "Headcount and scope reviewed".into(),
    }
}

fn row(id: &str, status: &str, version: i64) -> RequisitionRow {
    RequisitionRow {
        id: id.into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        department_id: None,
        job_title: "Service specialist".into(),
        openings: 2,
        target_start_date: "2026-11-01".into(),
        description: "Serve customer cases and complete field handoffs.".into(),
        status: status.into(),
        created_by: "creator".into(),
        reviewed_by: if status == "draft" {
            None
        } else {
            Some("reviewer".into())
        },
        version,
        created_at: NOW - 1_000,
        updated_at: NOW - 1_000,
    }
}

#[derive(Default)]
struct Fake {
    authority: bool,
    receipt: Option<ReceiptSnapshot>,
    row: Option<RequisitionRow>,
    rows: Vec<RequisitionRow>,
    events: Vec<RequisitionEventRow>,
    calls: Mutex<Vec<String>>,
    commits: Mutex<Vec<String>>,
    fail_commit: bool,
}

impl Fake {
    fn ready() -> Self {
        Self {
            authority: true,
            row: Some(row("talent-req-1", "draft", 1)),
            rows: vec![row("talent-req-1", "draft", 1)],
            ..Default::default()
        }
    }
    fn calls(&self) -> Vec<String> {
        self.calls.lock().unwrap().clone()
    }
    fn committed(&self) -> Vec<String> {
        self.commits.lock().unwrap().clone()
    }
}

impl TalentRequisitionCommandRepository for Fake {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a TalentActor,
        write: bool,
        _: i64,
    ) -> RequisitionFuture<'a, bool, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("authority:{write}"));
        let authority = self.authority;
        Box::pin(async move { Ok(authority) })
    }
    fn digest<'a>(&'a mut self, _: &'a str) -> RequisitionFuture<'a, String, Self::Error> {
        self.calls.lock().unwrap().push("digest".into());
        Box::pin(async move { Ok(DIGEST.into()) })
    }
    fn receipt<'a>(
        &'a mut self,
        _: &'a str,
    ) -> RequisitionFuture<'a, Option<ReceiptSnapshot>, Self::Error> {
        self.calls.lock().unwrap().push("receipt".into());
        let receipt = self.receipt.clone();
        Box::pin(async move { Ok(receipt) })
    }
    fn requisition_for_update<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
    ) -> RequisitionFuture<'a, Option<RequisitionRow>, Self::Error> {
        self.calls.lock().unwrap().push("record".into());
        let row = self.row.clone();
        Box::pin(async move { Ok(row) })
    }
    fn commit_create<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
        plan: &'a CreatePlan,
        _: &'a str,
    ) -> RequisitionFuture<'a, Value, Self::Error> {
        self.commits.lock().unwrap().push(format!(
            "create|{}|{}|{}|{}|{}|{}|{}",
            plan.id,
            plan.branch_id,
            plan.job_title,
            plan.openings,
            plan.target_start_date,
            plan.description,
            plan.created_by
        ));
        let fail = self.fail_commit;
        let result = RequisitionReceipt::new(plan.id.clone(), "draft", 1).to_wire();
        Box::pin(async move {
            if fail {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "SYNTHETIC_COMMIT_FAILED",
                    "Synthetic commit failure.",
                ));
            }
            Ok(result)
        })
    }
    fn commit_change<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
        plan: &'a ChangePlan,
        _: &'a str,
    ) -> RequisitionFuture<'a, Value, Self::Error> {
        self.commits.lock().unwrap().push(format!(
            "change|{}|{}|{}|{}|{}|{}",
            plan.id,
            plan.version,
            plan.status,
            plan.action.as_str(),
            plan.reason,
            plan.reviewed_by.as_deref().unwrap_or("none")
        ));
        let fail = self.fail_commit;
        let result = RequisitionReceipt::new(plan.id.clone(), &plan.status, plan.version).to_wire();
        Box::pin(async move {
            if fail {
                return Err(BusinessError::new(
                    FailureKind::Unavailable,
                    "SYNTHETIC_COMMIT_FAILED",
                    "Synthetic commit failure.",
                ));
            }
            Ok(result)
        })
    }
}

impl TalentRequisitionReadRepository for Fake {
    type Error = BusinessError;

    fn current_authority<'a>(
        &'a mut self,
        _: &'a TalentActor,
        write: bool,
        _: i64,
    ) -> RequisitionFuture<'a, bool, Self::Error> {
        self.calls
            .lock()
            .unwrap()
            .push(format!("authority:{write}"));
        let authority = self.authority;
        Box::pin(async move { Ok(authority) })
    }
    fn requisitions<'a>(
        &'a mut self,
        _: &'a TalentActor,
    ) -> RequisitionFuture<'a, Vec<RequisitionRow>, Self::Error> {
        self.calls.lock().unwrap().push("workspace".into());
        let rows = self.rows.clone();
        Box::pin(async move { Ok(rows) })
    }
    fn requisition<'a>(
        &'a mut self,
        _: &'a TalentActor,
        _: &'a str,
    ) -> RequisitionFuture<'a, Option<RequisitionRow>, Self::Error> {
        let row = self.row.clone();
        Box::pin(async move { Ok(row) })
    }
    fn events<'a>(
        &'a mut self,
        _: &'a str,
    ) -> RequisitionFuture<'a, Vec<RequisitionEventRow>, Self::Error> {
        let events = self.events.clone();
        Box::pin(async move { Ok(events) })
    }
}

fn receipt(
    actor: &TalentActor,
    operation: &str,
    id: &str,
    status: &str,
    version: i64,
) -> ReceiptSnapshot {
    ReceiptSnapshot {
        request_hash: DIGEST.into(),
        result_json: RequisitionReceipt::new(id.into(), status, version)
            .to_wire()
            .to_string(),
        operation: operation.into(),
        actor_id: actor.user_id.as_str().to_owned(),
        record_id: id.into(),
        authority_json: actor.authority_json(),
    }
}

#[test]
fn wire_shapes_accept_exact_commands_and_reject_unknown_or_malformed_fields() {
    let create = serde_json::json!({
        "jobTitle": "Service specialist",
        "openings": 2,
        "targetStartDate": "2026-11-01",
        "description": "Serve customer cases."
    });
    assert!(serde_json::from_value::<RequisitionRequest>(create).is_ok());
    assert!(
        serde_json::from_value::<RequisitionRequest>(serde_json::json!({
            "jobTitle": "Service specialist",
            "openings": 2,
            "targetStartDate": "2026-11-01",
            "description": "Serve customer cases.",
            "id": "talent-req-1"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<RequisitionRequest>(serde_json::json!({
            "jobTitle": "Service specialist",
            "openings": 2.5,
            "targetStartDate": "2026-11-01",
            "description": "Serve customer cases."
        }))
        .is_err()
    );

    let change = serde_json::json!({
        "action": "approve",
        "expectedVersion": 1,
        "reason": "Headcount and scope reviewed"
    });
    let parsed = serde_json::from_value::<RequisitionChange>(change).unwrap();
    assert_eq!(parsed.action, RequisitionAction::Approve);
    assert!(
        serde_json::from_value::<RequisitionChange>(serde_json::json!({
            "action": "send",
            "expectedVersion": 1,
            "reason": "Headcount and scope reviewed"
        }))
        .is_err()
    );
    assert!(
        serde_json::from_value::<RequisitionChange>(serde_json::json!({
            "action": "open",
            "expectedVersion": 2,
            "reason": "Approved role is ready",
            "ownerId": "user"
        }))
        .is_err()
    );
}

#[tokio::test]
async fn create_requires_current_iam_read_write_and_branch_before_effects() {
    let mut fake = Fake::ready();
    let no_write = actor(
        "creator",
        Some("branch"),
        None,
        &["records.read", "records.review"],
    );
    assert_eq!(
        create_requisition(&mut fake, &no_write, &key(KEY), &request(), NOW)
            .await
            .unwrap_err()
            .code,
        "TALENT_PERMISSION_REQUIRED"
    );
    assert!(fake.calls().iter().all(|call| call != "authority:true"));

    let no_branch = actor(
        "creator",
        None,
        None,
        &["records.read", "records.write", "records.review"],
    );
    assert_eq!(
        create_requisition(&mut fake, &no_branch, &key(KEY), &request(), NOW)
            .await
            .unwrap_err()
            .code,
        "BRANCH_SCOPE_REQUIRED"
    );

    let mut fake = Fake::ready();
    fake.authority = false;
    assert_eq!(
        create_requisition(&mut fake, &writer(), &key(KEY), &request(), NOW)
            .await
            .unwrap_err()
            .code,
        "TALENT_ACCESS_CHANGED"
    );
    assert_eq!(fake.committed().len(), 0);

    let mut expired = writer();
    expired.expires_at = NOW;
    assert_eq!(
        create_requisition(&mut fake, &expired, &key(KEY), &request(), NOW)
            .await
            .unwrap_err()
            .code,
        "WORK_CONTEXT_CHANGED"
    );
}

#[tokio::test]
async fn create_validates_exact_input_and_commits_trimmed_plan() {
    for input in [
        RequisitionRequest {
            job_title: " ".into(),
            ..request()
        },
        RequisitionRequest {
            openings: 0,
            ..request()
        },
        RequisitionRequest {
            openings: 1_001,
            ..request()
        },
        RequisitionRequest {
            target_start_date: "2026-02-30".into(),
            ..request()
        },
        RequisitionRequest {
            target_start_date: "2026-2-03".into(),
            ..request()
        },
        RequisitionRequest {
            description: "  ".into(),
            ..request()
        },
    ] {
        let mut fake = Fake::ready();
        assert_eq!(
            create_requisition(&mut fake, &writer(), &key(KEY), &input, NOW)
                .await
                .unwrap_err()
                .code,
            "TALENT_REQUISITION_INVALID"
        );
        assert!(fake.committed().is_empty());
    }

    let mut fake = Fake::ready();
    let result = create_requisition(&mut fake, &writer(), &key(KEY), &request(), NOW)
        .await
        .unwrap();
    assert_eq!(result["id"], format!("talent-req-{KEY}"));
    assert_eq!(result["status"], "draft");
    assert_eq!(result["version"], 1);
    assert_eq!(
        fake.committed(),
        vec![format!(
            "create|talent-req-{KEY}|branch|Service specialist|2|2026-11-01|Serve customer cases and complete field handoffs.|creator"
        )]
    );
    assert_eq!(
        fake.calls(),
        vec!["authority:true", "digest", "receipt", "commit-create"]
            .into_iter()
            .map(str::to_owned)
            .filter(|call| call != "commit-create")
            .collect::<Vec<_>>()
    );
}

#[tokio::test]
async fn create_replay_is_exact_and_mismatched_receipts_are_denied() {
    let id = format!("talent-req-{KEY}");
    let mut fake = Fake::ready();
    fake.receipt = Some(receipt(
        &writer(),
        "talent.requisition.create",
        &id,
        "draft",
        1,
    ));
    let result = create_requisition(&mut fake, &writer(), &key(KEY), &request(), NOW)
        .await
        .unwrap();
    assert_eq!(result["id"], id);
    assert!(fake.committed().is_empty());

    let mut fake = Fake::ready();
    fake.receipt = Some(ReceiptSnapshot {
        request_hash: "b".repeat(64),
        ..receipt(&writer(), "talent.requisition.create", &id, "draft", 1)
    });
    assert_eq!(
        create_requisition(&mut fake, &writer(), &key(KEY), &request(), NOW)
            .await
            .unwrap_err()
            .code,
        "TALENT_REQUISITION_IDEMPOTENCY_CONFLICT"
    );
    assert!(fake.committed().is_empty());
}

#[tokio::test]
async fn change_enforces_transition_cas_and_independent_review() {
    let mut fake = Fake::ready();
    assert_eq!(
        change_requisition(
            &mut fake,
            &writer(),
            &key("talent-requisition-change-0001"),
            "talent-req-1",
            &change(RequisitionAction::Approve, 1),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "CONFLICT"
    );
    assert!(fake.committed().is_empty());

    let mut fake = Fake::ready();
    let approved = change_requisition(
        &mut fake,
        &reviewer(),
        &key("talent-requisition-change-0002"),
        "talent-req-1",
        &change(RequisitionAction::Approve, 1),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(approved["status"], "approved");
    assert_eq!(approved["version"], 2);
    assert_eq!(
        fake.committed(),
        vec!["change|talent-req-1|2|approved|approve|Headcount and scope reviewed|reviewer"]
    );

    let mut fake = Fake::ready();
    fake.row = Some(row("talent-req-1", "approved", 2));
    let opened = change_requisition(
        &mut fake,
        &reviewer(),
        &key("talent-requisition-change-0003"),
        "talent-req-1",
        &change(RequisitionAction::Open, 2),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(opened["status"], "open");

    let mut fake = Fake::ready();
    fake.row = Some(row("talent-req-1", "open", 3));
    assert_eq!(
        change_requisition(
            &mut fake,
            &reviewer(),
            &key("talent-requisition-change-0004"),
            "talent-req-1",
            &change(RequisitionAction::Close, 3),
            NOW
        )
        .await
        .unwrap()["status"],
        "closed"
    );

    let mut fake = Fake::ready();
    assert_eq!(
        change_requisition(
            &mut fake,
            &writer(),
            &key("talent-requisition-change-0005"),
            "talent-req-1",
            &change(RequisitionAction::Cancel, 1),
            NOW
        )
        .await
        .unwrap()["status"],
        "cancelled"
    );

    let mut fake = Fake::ready();
    let stale = change_requisition(
        &mut fake,
        &reviewer(),
        &key("talent-requisition-change-0006"),
        "talent-req-1",
        &change(RequisitionAction::Approve, 2),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(stale.code, "CONFLICT");

    let mut fake = Fake::ready();
    fake.row = Some(row("talent-req-1", "open", 3));
    let wrong_status = change_requisition(
        &mut fake,
        &reviewer(),
        &key("talent-requisition-change-0007"),
        "talent-req-1",
        &change(RequisitionAction::Approve, 3),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(wrong_status.code, "CONFLICT");

    let mut fake = Fake::ready();
    let invalid_reason = change_requisition(
        &mut fake,
        &reviewer(),
        &key("talent-requisition-change-0008"),
        "talent-req-1",
        &RequisitionChange {
            reason: "  ".into(),
            ..change(RequisitionAction::Approve, 1)
        },
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(invalid_reason.code, "TALENT_REQUISITION_INVALID");
}

#[tokio::test]
async fn change_replay_is_exact_and_missing_records_are_not_found() {
    let mut fake = Fake::ready();
    fake.receipt = Some(receipt(
        &reviewer(),
        "talent.requisition.approve",
        "talent-req-1",
        "approved",
        2,
    ));
    let result = change_requisition(
        &mut fake,
        &reviewer(),
        &key("talent-requisition-change-0009"),
        "talent-req-1",
        &change(RequisitionAction::Approve, 1),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "approved");
    assert!(fake.committed().is_empty());

    let mut fake = Fake::ready();
    fake.row = None;
    assert_eq!(
        change_requisition(
            &mut fake,
            &reviewer(),
            &key("talent-requisition-change-0010"),
            "talent-req-1",
            &change(RequisitionAction::Approve, 1),
            NOW
        )
        .await
        .unwrap_err()
        .code,
        "TALENT_REQUISITION_NOT_FOUND"
    );
}

#[tokio::test]
async fn workspace_is_company_branch_and_department_scoped_with_flags() {
    let mut foreign = row("talent-req-foreign", "draft", 1);
    foreign.company_id = "foreign".into();
    let mut other_branch = row("talent-req-other-branch", "draft", 1);
    other_branch.branch_id = "other-branch".into();
    let mut department = row("talent-req-department", "draft", 1);
    department.department_id = Some("department".into());
    let mut scoped = Fake::ready();
    scoped.rows = vec![
        row("talent-req-1", "draft", 1),
        foreign,
        other_branch,
        department.clone(),
    ];
    let value = requisition_workspace(&mut scoped, &writer(), NOW)
        .await
        .unwrap();
    let mut ids = value["requisitions"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| row["id"].as_str().unwrap().to_owned())
        .collect::<Vec<_>>();
    ids.sort();
    assert_eq!(
        ids,
        vec![
            "talent-req-1".to_owned(),
            "talent-req-department".to_owned()
        ]
    );
    let own = value["requisitions"]
        .as_array()
        .unwrap()
        .iter()
        .find(|row| row["id"] == "talent-req-1")
        .unwrap();
    assert_eq!(own["canApprove"], false);
    assert_eq!(own["canCancel"], true);

    let mut department_scoped = Fake::ready();
    department_scoped.rows = vec![row("talent-req-1", "draft", 1), department];
    let department_actor = actor(
        "creator",
        Some("branch"),
        Some("department"),
        &["records.read", "records.write", "records.review"],
    );
    let value = requisition_workspace(&mut department_scoped, &department_actor, NOW)
        .await
        .unwrap();
    assert_eq!(value["requisitions"][0]["id"], "talent-req-department");
    assert_eq!(value["requisitions"].as_array().unwrap().len(), 1);

    let mut duplicate = Fake::ready();
    duplicate.rows = vec![
        row("talent-req-1", "draft", 1),
        row("talent-req-1", "draft", 1),
    ];
    assert_eq!(
        requisition_workspace(&mut duplicate, &writer(), NOW)
            .await
            .unwrap_err()
            .code,
        "TALENT_REQUISITION_SOURCE_INVALID"
    );

    let mut no_authority = Fake::ready();
    no_authority.authority = false;
    assert_eq!(
        requisition_workspace(&mut no_authority, &writer(), NOW)
            .await
            .unwrap_err()
            .code,
        "TALENT_ACCESS_CHANGED"
    );
}

#[tokio::test]
async fn detail_returns_exact_history_and_rejects_foreign_or_malformed_rows() {
    let mut fake = Fake::ready();
    fake.events = vec![
        RequisitionEventRow {
            version: 1,
            action: "create".into(),
            actor_id: "creator".into(),
            reason: "Hiring need drafted".into(),
            status: "draft".into(),
            created_at: NOW - 1_000,
        },
        RequisitionEventRow {
            version: 2,
            action: "approve".into(),
            actor_id: "reviewer".into(),
            reason: "Headcount and scope reviewed".into(),
            status: "approved".into(),
            created_at: NOW - 500,
        },
    ];
    let value = requisition_detail(&mut fake, &writer(), "talent-req-1", NOW)
        .await
        .unwrap();
    assert_eq!(value["requisition"]["status"], "draft");
    assert_eq!(value["events"].as_array().unwrap().len(), 2);

    let mut fake = Fake::ready();
    fake.row = Some({
        let mut row = row("talent-req-1", "draft", 1);
        row.department_id = Some("department".into());
        row
    });
    let other_department = actor(
        "creator",
        Some("branch"),
        Some("other-department"),
        &["records.read", "records.write", "records.review"],
    );
    assert_eq!(
        requisition_detail(&mut fake, &other_department, "talent-req-1", NOW)
            .await
            .unwrap_err()
            .code,
        "TALENT_REQUISITION_NOT_FOUND"
    );

    let mut fake = Fake::ready();
    fake.events = vec![
        RequisitionEventRow {
            version: 2,
            action: "approve".into(),
            actor_id: "reviewer".into(),
            reason: "Reviewed".into(),
            status: "approved".into(),
            created_at: NOW,
        },
        RequisitionEventRow {
            version: 1,
            action: "create".into(),
            actor_id: "creator".into(),
            reason: "Drafted".into(),
            status: "draft".into(),
            created_at: NOW,
        },
    ];
    assert_eq!(
        requisition_detail(&mut fake, &writer(), "talent-req-1", NOW)
            .await
            .unwrap_err()
            .code,
        "TALENT_REQUISITION_SOURCE_INVALID"
    );
}

#[tokio::test]
async fn commit_failure_returns_the_owner_failure_for_exact_retry() {
    let mut fake = Fake::ready();
    fake.fail_commit = true;
    assert_eq!(
        create_requisition(&mut fake, &writer(), &key(KEY), &request(), NOW)
            .await
            .unwrap_err()
            .code,
        "SYNTHETIC_COMMIT_FAILED"
    );
    fake.fail_commit = false;
    assert_eq!(
        create_requisition(&mut fake, &writer(), &key(KEY), &request(), NOW)
            .await
            .unwrap()["version"],
        1
    );
}

#[tokio::test]
async fn requisition_text_bounds_use_code_points_after_ecma_trim() {
    // Zod v4 counts code points: one emoji is one point and fails the
    // title minimum of 2, 160 meet the maximum of 160 and 161 exceed it.
    assert!(normalize_text("😀", 2, MAX_TITLE).is_err());
    assert!(normalize_text("😀😀", 2, MAX_TITLE).is_ok());
    assert!(normalize_text(&"😀".repeat(160), 2, MAX_TITLE).is_ok());
    assert!(normalize_text(&"😀".repeat(161), 2, MAX_TITLE).is_err());
    // BMP text counts one code point per scalar regardless of UTF-8 width:
    // 160 accented points are 320 bytes, 3,000 Thai points are 9,000 bytes.
    assert!(normalize_text(&"é".repeat(160), 2, MAX_TITLE).is_ok());
    assert!(normalize_text(&"é".repeat(161), 2, MAX_TITLE).is_err());
    assert!(normalize_text(&"ก".repeat(3_000), 3, MAX_DESCRIPTION).is_ok());
    assert!(normalize_text(&"ก".repeat(3_001), 3, MAX_DESCRIPTION).is_err());
    assert!(normalize_text(&"😀".repeat(3_000), 3, MAX_DESCRIPTION).is_ok());
    assert!(normalize_text(&"😀".repeat(3_001), 3, MAX_DESCRIPTION).is_err());
    // The ECMAScript trim set trims U+FEFF but not U+0085 NEL, unlike
    // Rust's Unicode White_Space trim.
    assert_eq!(
        normalize_text("\u{feff}ab\u{feff}", 2, MAX_TITLE).unwrap(),
        "ab"
    );
    assert_eq!(
        normalize_text("\u{00a0} Service specialist \u{3000}", 2, MAX_TITLE).unwrap(),
        "Service specialist"
    );
    assert!(normalize_text(&format!("\u{feff}{}", "a".repeat(160)), 2, MAX_TITLE).is_ok());
    assert!(normalize_text(&format!("\u{0085}{}", "a".repeat(160)), 2, MAX_TITLE).is_err());
    // Change reasons use the same code points: two emojis fail the
    // minimum of 3, three pass, and 3000/3001 bound the maximum.
    assert!(normalize_text("😀😀", 3, MAX_REASON).is_err());
    assert!(normalize_text("😀😀😀", 3, MAX_REASON).is_ok());
    assert!(normalize_text(&"😀".repeat(3_000), 3, MAX_REASON).is_ok());
    assert!(normalize_text(&"😀".repeat(3_001), 3, MAX_REASON).is_err());
    // Interior controls remain rejected.
    assert!(normalize_text("ab\u{0007}cd", 2, MAX_TITLE).is_err());

    // End to end: a two-emoji title commits verbatim through the owner.
    let mut fake = Fake::ready();
    let result = create_requisition(
        &mut fake,
        &writer(),
        &key(KEY),
        &RequisitionRequest {
            job_title: "😀😀".into(),
            openings: 1,
            target_start_date: "2026-11-01".into(),
            description: "abc".into(),
        },
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result["status"], "draft");
    assert!(fake.committed()[0].contains("|😀😀|"));

    // Persisted source rows validate by UTF-16 units, not UTF-8 bytes.
    let mut fake = Fake::ready();
    fake.rows = vec![RequisitionRow {
        job_title: "é".repeat(160),
        ..row("talent-req-1", "draft", 1)
    }];
    fake.row = Some(fake.rows[0].clone());
    let value = requisition_workspace(&mut fake, &writer(), NOW)
        .await
        .unwrap();
    assert_eq!(value["requisitions"][0]["job_title"], "é".repeat(160));
}
