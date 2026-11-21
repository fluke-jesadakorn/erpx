//! Native leave request policy tests.
//!
//! These are owner unit facts: exact idempotency replay, requester/version
//! checks, approval handoff, evidence gates, balance effects, cancellation
//! review and the scheduled accrual plan. PostgreSQL transaction behaviour and
//! the composed Organization/Workflow/Files ports are proven by the sibling
//! integration target.
use erpx_server_people_payroll::public::leave::{
    LeaveAccrualDue, LeaveAccrualWrite, LeaveApprovalInstance, LeaveApprovalPort,
    LeaveApprovalView, LeaveBalance, LeaveBalanceWrite, LeaveCancelWrite, LeaveCancellationInsert,
    LeaveCancellationWrite, LeaveCommandFuture, LeaveCommandRepository, LeaveDecisionFact,
    LeaveDecisionWrite, LeaveEmployeeInsert, LeaveEvidenceBindRequest, LeaveEvidenceManifest,
    LeaveEvidenceManifestWrite, LeaveEvidencePolicyRule, LeaveInsert, LeaveLedgerEntry,
    LeaveRequestRow, LeaveRoute, LeaveRoutePort, LeaveStaffFacts, LeaveStaffPort, LeaveUpdate,
    NoLeaveApprovalPort, NoLeaveEvidencePort, StoredLeaveReceipt, create_leave, decide_leave,
    decide_leave_cancellation, run_leave_accruals, save_or_submit_leave, transition_leave,
};
use erpx_server_people_payroll::public::workforce::WorkforceCommandActor;
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, TimestampMillis,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::collections::BTreeMap;

const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

fn actor() -> WorkforceCommandActor {
    WorkforceCommandActor {
        user_id: "owner".into(),
        holding_id: HoldingId::new("holding").unwrap(),
        company_id: CompanyId::new("company").unwrap(),
        branch_id: None,
        department_id: None,
        assignment_id: "assignment".into(),
        role: "employee".into(),
        permissions: vec!["leave.self".into()],
        work_context: "a".repeat(64),
        expires_at: NOW.get() + 100_000,
        beta: true,
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn staff() -> LeaveStaffFacts {
    LeaveStaffFacts {
        person_id: "person".into(),
        engagement_id: "engagement".into(),
        engagement_version: 1,
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        legal_name: "Somchai Jaidee".into(),
        worker_type: "employee".into(),
        start_date: "2026-01-01".into(),
        end_date: None,
    }
}

fn row() -> LeaveRequestRow {
    LeaveRequestRow {
        id: "leave-request-1".into(),
        number: "LR-request-1".into(),
        employee_id: "employee-person".into(),
        company_id: "company".into(),
        branch_id: "branch".into(),
        department_id: Some("department".into()),
        person_id: Some("person".into()),
        engagement_id: Some("engagement".into()),
        start_date: "2026-03-02".into(),
        end_date: Some("2026-03-04".into()),
        days: 3,
        calculated_units: Some(3.0),
        handover_note: "Handover".into(),
        leave_type_id: Some("type".into()),
        leave_type_name: Some("Annual".into()),
        policy_version_id: Some("policy".into()),
        state: "draft".into(),
        version: 1,
        created_by: "owner".into(),
        prepared_by: None,
        decided_by: None,
        decision_note: None,
        cancellation_state: None,
        evidence_policy_snapshot: None,
        evidence_due_at: None,
        evidence_version: 0,
        submitted_at: None,
        created_at: NOW.get(),
        updated_at: NOW.get(),
    }
}

fn required_policy() -> Value {
    json!({
        "required": true,
        "minimumUnits": 2.0,
        "purposeKey": "medical_certificate",
        "minimumCount": 1,
        "dueStage": "before_final_approval",
        "graceDays": 0,
        "privacy": "requester_approvers_hr",
        "note": "Attach a certificate"
    })
}

#[derive(Default)]
struct FakeRepo {
    receipts: BTreeMap<String, StoredLeaveReceipt>,
    ledger: Vec<String>,
    row: Option<LeaveRequestRow>,
    employee: Option<String>,
    rule: Option<LeaveEvidencePolicyRule>,
    balance: Option<LeaveBalance>,
    accrual: Vec<LeaveAccrualDue>,
    create_calls: usize,
    employee_calls: usize,
    update_calls: usize,
    decide_calls: usize,
    cancel_calls: usize,
    cancellation_calls: usize,
    manifest_calls: usize,
    balance_calls: usize,
    accrual_keys: Vec<String>,
}

impl FakeRepo {
    fn with_row(row: LeaveRequestRow) -> Self {
        Self {
            row: Some(row),
            employee: Some("employee-person".into()),
            ..Self::default()
        }
    }
}

impl LeaveCommandRepository for FakeRepo {
    type Error = BusinessError;

    fn connection(&mut self) -> Option<&mut PgConnection> {
        None
    }
    fn lock_command<'a>(&'a mut self, _: &'a str) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async { Ok(()) })
    }
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> LeaveCommandFuture<'a, String, Self::Error> {
        Box::pin(async move {
            let mut hash = 0xcbf29ce484222325_u64;
            for byte in identity.bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x100000001b3);
            }
            Ok(format!("{hash:064x}"))
        })
    }
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> LeaveCommandFuture<'a, Option<StoredLeaveReceipt>, Self::Error> {
        Box::pin(async move { Ok(self.receipts.get(key).cloned()) })
    }
    fn ledger<'a>(
        &'a mut self,
        entry: &'a LeaveLedgerEntry<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.ledger.push(entry.operation.to_owned());
            self.receipts.insert(
                entry.key.to_owned(),
                StoredLeaveReceipt {
                    request_hash: entry.request_hash.to_owned(),
                    result: Some(entry.result.clone()),
                },
            );
            Ok(())
        })
    }
    fn leave_row<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: bool,
        _: Option<&'a str>,
        _: Option<&'a str>,
    ) -> LeaveCommandFuture<'a, Option<LeaveRequestRow>, Self::Error> {
        Box::pin(async move { Ok(self.row.clone()) })
    }
    fn leave_row_for_update<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> LeaveCommandFuture<'a, Option<LeaveRequestRow>, Self::Error> {
        Box::pin(async move { Ok(self.row.clone()) })
    }
    fn leave_row_for_cancellation<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> LeaveCommandFuture<'a, Option<LeaveRequestRow>, Self::Error> {
        Box::pin(async move { Ok(self.row.clone()) })
    }
    fn employee<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
    ) -> LeaveCommandFuture<'a, Option<String>, Self::Error> {
        Box::pin(async move { Ok(self.employee.clone()) })
    }
    fn create_employee<'a>(
        &'a mut self,
        _: &'a LeaveEmployeeInsert<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.employee_calls += 1;
            Ok(())
        })
    }
    fn create_leave<'a>(
        &'a mut self,
        _: &'a LeaveInsert<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.create_calls += 1;
            Ok(())
        })
    }
    fn update_leave<'a>(
        &'a mut self,
        _: &'a LeaveUpdate<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.update_calls += 1;
            Ok(())
        })
    }
    fn decide_leave<'a>(
        &'a mut self,
        _: &'a LeaveDecisionWrite<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.decide_calls += 1;
            Ok(())
        })
    }
    fn open_cancellation<'a>(
        &'a mut self,
        _: &'a LeaveCancellationInsert<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.cancellation_calls += 1;
            Ok(())
        })
    }
    fn decide_cancellation<'a>(
        &'a mut self,
        _: &'a LeaveCancellationWrite<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.cancellation_calls += 1;
            Ok(())
        })
    }
    fn cancel_leave<'a>(
        &'a mut self,
        _: &'a LeaveCancelWrite<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.cancel_calls += 1;
            Ok(())
        })
    }
    fn record_evidence_manifest<'a>(
        &'a mut self,
        _: &'a LeaveEvidenceManifestWrite<'a>,
    ) -> LeaveCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.manifest_calls += 1;
            Ok(())
        })
    }
    fn evidence_policy_rule<'a>(
        &'a mut self,
        _: &'a str,
        _: &'a str,
    ) -> LeaveCommandFuture<'a, Option<LeaveEvidencePolicyRule>, Self::Error> {
        Box::pin(async move { Ok(self.rule.clone()) })
    }
    fn balance<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: i32,
    ) -> LeaveCommandFuture<'a, Option<LeaveBalance>, Self::Error> {
        Box::pin(async move { Ok(self.balance.clone()) })
    }
    fn insert_balance<'a>(
        &'a mut self,
        _: &'a LeaveBalanceWrite<'a>,
    ) -> LeaveCommandFuture<'a, u64, Self::Error> {
        Box::pin(async move {
            self.balance_calls += 1;
            Ok(1)
        })
    }
    fn accrual_due<'a>(
        &'a mut self,
        _: &'a str,
        _: Option<&'a str>,
    ) -> LeaveCommandFuture<'a, Vec<LeaveAccrualDue>, Self::Error> {
        Box::pin(async move { Ok(self.accrual.clone()) })
    }
    fn insert_accrual<'a>(
        &'a mut self,
        command: &'a LeaveAccrualWrite<'a>,
    ) -> LeaveCommandFuture<'a, u64, Self::Error> {
        Box::pin(async move {
            self.accrual_keys.push(command.command_key.to_owned());
            Ok(1)
        })
    }
}

struct FakeStaff(Option<LeaveStaffFacts>);

impl LeaveStaffPort for FakeStaff {
    fn active_staff<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> LeaveCommandFuture<'a, Option<LeaveStaffFacts>, BusinessError> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

struct FakeRoute;

impl LeaveRoutePort for FakeRoute {
    fn route<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> LeaveCommandFuture<'a, LeaveRoute, BusinessError> {
        Box::pin(async { Ok(LeaveRoute::default()) })
    }
}

struct FakeApproval {
    final_decision: bool,
    opens: usize,
    decisions: usize,
    closes: usize,
}

impl Default for FakeApproval {
    fn default() -> Self {
        Self {
            final_decision: true,
            opens: 0,
            decisions: 0,
            closes: 0,
        }
    }
}

impl LeaveApprovalPort for FakeApproval {
    fn open<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a LeaveRoute,
        _: i64,
    ) -> LeaveCommandFuture<'a, LeaveApprovalInstance, BusinessError> {
        Box::pin(async move {
            self.opens += 1;
            Ok(LeaveApprovalInstance {
                instance_id: "approval".into(),
            })
        })
    }
    fn prepare_decision<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: bool,
        _: &'a str,
        _: i64,
        _: bool,
    ) -> LeaveCommandFuture<'a, LeaveDecisionFact, BusinessError> {
        Box::pin(async move {
            Ok(LeaveDecisionFact {
                instance_id: "approval".into(),
                final_decision: self.final_decision,
            })
        })
    }
    fn apply_decision<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: bool,
        _: &'a str,
        _: i64,
        _: bool,
    ) -> LeaveCommandFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.decisions += 1;
            Ok(())
        })
    }
    fn close<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: &'a str,
        _: i64,
    ) -> LeaveCommandFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.closes += 1;
            Ok(())
        })
    }
    fn view<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a WorkforceCommandActor,
    ) -> LeaveCommandFuture<'a, Option<LeaveApprovalView>, BusinessError> {
        Box::pin(async { Ok(None) })
    }
    fn recorded_decision<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> LeaveCommandFuture<'a, bool, BusinessError> {
        Box::pin(async { Ok(false) })
    }
}

struct FakeEvidence {
    manifest: LeaveEvidenceManifest,
    available: i64,
    binds: usize,
}

impl Default for FakeEvidence {
    fn default() -> Self {
        Self {
            manifest: LeaveEvidenceManifest::default(),
            available: 0,
            binds: 0,
        }
    }
}

impl erpx_server_people_payroll::public::leave::LeaveEvidencePort for FakeEvidence {
    fn manifest<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> LeaveCommandFuture<'a, LeaveEvidenceManifest, BusinessError> {
        Box::pin(async move { Ok(self.manifest.clone()) })
    }
    fn available<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> LeaveCommandFuture<
        'a,
        erpx_server_people_payroll::public::leave::LeaveEvidenceRequirement,
        BusinessError,
    > {
        Box::pin(async move {
            Ok(
                erpx_server_people_payroll::public::leave::LeaveEvidenceRequirement {
                    count: self.available,
                    set_ids: Vec::new(),
                },
            )
        })
    }
    fn bind<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a LeaveEvidenceBindRequest<'a>,
    ) -> LeaveCommandFuture<'a, (), BusinessError> {
        Box::pin(async move {
            self.binds += 1;
            Ok(())
        })
    }
}

fn create_body() -> Value {
    json!({
        "startDate": "2026-03-02",
        "days": 3,
        "handoverNote": "Handover"
    })
}

fn draft_body(save_only: bool) -> Value {
    json!({
        "startDate": "2026-03-02",
        "days": 3,
        "handoverNote": "Handover",
        "expectedVersion": 1,
        "saveOnly": save_only
    })
}

fn decision_body() -> Value {
    json!({"expectedVersion": 1, "note": "Reviewed"})
}

#[tokio::test]
async fn create_is_idempotent_and_writes_once() {
    let mut repo = FakeRepo::default();
    let mut staff = FakeStaff(Some(staff()));
    let mut route = FakeRoute;
    let mut approval = FakeApproval::default();
    let mut evidence = FakeEvidence::default();
    let first = create_leave(
        &actor(),
        &create_body(),
        &key("leave-create-1"),
        &mut repo,
        &mut staff,
        &mut route,
        &mut approval,
        &mut evidence,
        NOW,
    )
    .await
    .unwrap();
    let second = create_leave(
        &actor(),
        &create_body(),
        &key("leave-create-1"),
        &mut repo,
        &mut staff,
        &mut route,
        &mut approval,
        &mut evidence,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(first.id, "leave-leave-create-1");
    assert_eq!(first.state, "draft");
    assert_eq!(second.id, first.id);
    assert_eq!(repo.create_calls, 1);
    assert_eq!(repo.ledger, vec!["leave.create"]);
}

#[tokio::test]
async fn submit_requires_the_requester_and_current_version() {
    let mut repo = FakeRepo::with_row(row());
    let mut staff_port = FakeStaff(Some(staff()));
    let mut route = FakeRoute;
    let mut approval = FakeApproval::default();
    let mut evidence = FakeEvidence::default();
    let result = save_or_submit_leave(
        &actor(),
        "leave-request-1",
        &draft_body(false),
        &key("leave-submit-1"),
        &mut repo,
        &mut staff_port,
        &mut route,
        &mut approval,
        &mut evidence,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.state, "pending_approval");
    assert_eq!(result.version, 2);
    assert_eq!(approval.opens, 1);
    assert_eq!(repo.update_calls, 1);

    let mut foreign = FakeRepo::with_row(LeaveRequestRow {
        created_by: "other".into(),
        ..row()
    });
    let error = save_or_submit_leave(
        &actor(),
        "leave-request-1",
        &draft_body(false),
        &key("leave-submit-2"),
        &mut foreign,
        &mut FakeStaff(Some(staff())),
        &mut FakeRoute,
        &mut FakeApproval::default(),
        &mut FakeEvidence::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "REQUESTER_REQUIRED");

    let mut stale = FakeRepo::with_row(LeaveRequestRow {
        version: 2,
        ..row()
    });
    let error = save_or_submit_leave(
        &actor(),
        "leave-request-1",
        &draft_body(false),
        &key("leave-submit-3"),
        &mut stale,
        &mut FakeStaff(Some(staff())),
        &mut FakeRoute,
        &mut FakeApproval::default(),
        &mut FakeEvidence::default(),
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "LEAVE_CHANGED");
}

#[tokio::test]
async fn final_approval_requires_clean_evidence_and_commits_usage() {
    let mut pending = LeaveRequestRow {
        state: "pending_approval".into(),
        evidence_policy_snapshot: Some(required_policy()),
        ..row()
    };
    let mut repo = FakeRepo::with_row(pending.clone());
    let mut approval = FakeApproval::default();
    let mut evidence = FakeEvidence {
        manifest: LeaveEvidenceManifest {
            pending: 1,
            ..LeaveEvidenceManifest::default()
        },
        ..FakeEvidence::default()
    };
    let error = decide_leave(
        &actor(),
        "leave-request-1",
        &decision_body(),
        &key("leave-approve-1"),
        true,
        &mut repo,
        &mut approval,
        &mut evidence,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "LEAVE_EVIDENCE_SCANNING");

    let mut repo = FakeRepo::with_row(pending.clone());
    let mut evidence = FakeEvidence::default();
    let error = decide_leave(
        &actor(),
        "leave-request-1",
        &decision_body(),
        &key("leave-approve-2"),
        true,
        &mut repo,
        &mut approval,
        &mut evidence,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "LEAVE_EVIDENCE_REQUIRED");

    let mut repo = FakeRepo::with_row(pending.clone());
    let mut evidence = FakeEvidence {
        available: 1,
        ..FakeEvidence::default()
    };
    let result = decide_leave(
        &actor(),
        "leave-request-1",
        &decision_body(),
        &key("leave-approve-3"),
        true,
        &mut repo,
        &mut approval,
        &mut evidence,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.state, "approved");
    assert_eq!(repo.balance_calls, 2);
    assert_eq!(repo.decide_calls, 1);
    pending.state = "approved".into();
}

#[tokio::test]
async fn reject_releases_the_reservation_without_evidence() {
    let mut repo = FakeRepo::with_row(LeaveRequestRow {
        state: "pending_approval".into(),
        ..row()
    });
    let mut approval = FakeApproval {
        final_decision: false,
        ..FakeApproval::default()
    };
    let result = decide_leave(
        &actor(),
        "leave-request-1",
        &decision_body(),
        &key("leave-reject-1"),
        false,
        &mut repo,
        &mut approval,
        &mut FakeEvidence::default(),
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.state, "rejected");
    assert_eq!(repo.balance_calls, 1);
}

#[tokio::test]
async fn approved_cancellation_opens_review_and_reverses_usage() {
    let mut repo = FakeRepo::with_row(LeaveRequestRow {
        state: "approved".into(),
        ..row()
    });
    let mut route = FakeRoute;
    let mut approval = FakeApproval::default();
    let mut evidence = FakeEvidence::default();
    let body = json!({"action": "cancel", "expectedVersion": 1, "note": "Plans changed"});
    let result = transition_leave(
        &actor(),
        "leave-request-1",
        &body,
        &key("leave-cancel-1"),
        &mut repo,
        &mut route,
        &mut approval,
        &mut evidence,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.state, "approved");
    assert_eq!(result.cancellation_state.as_deref(), Some("pending_review"));
    assert_eq!(approval.opens, 1);

    let mut repo = FakeRepo::with_row(LeaveRequestRow {
        state: "approved".into(),
        cancellation_state: Some("pending_review".into()),
        ..row()
    });
    let mut approval = FakeApproval::default();
    let result = decide_leave_cancellation(
        &actor(),
        "leave-request-1",
        &decision_body(),
        &key("leave-cancel-approve-1"),
        true,
        &mut repo,
        &mut approval,
        NOW,
    )
    .await
    .unwrap();
    assert_eq!(result.state, "cancelled");
    assert_eq!(result.cancellation_state.as_deref(), Some("approved"));
    assert_eq!(repo.balance_calls, 1);
}

#[tokio::test]
async fn default_ports_fail_closed() {
    let mut repo = FakeRepo::with_row(LeaveRequestRow {
        state: "pending_approval".into(),
        ..row()
    });
    let error = decide_leave(
        &actor(),
        "leave-request-1",
        &decision_body(),
        &key("leave-approve-closed"),
        true,
        &mut repo,
        &mut NoLeaveApprovalPort,
        &mut NoLeaveEvidencePort,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "LEAVE_APPROVAL_PORT_UNAVAILABLE");

    let mut repo = FakeRepo::with_row(row());
    let error = save_or_submit_leave(
        &actor(),
        "leave-request-1",
        &draft_body(false),
        &key("leave-submit-closed"),
        &mut repo,
        &mut FakeStaff(Some(staff())),
        &mut FakeRoute,
        &mut NoLeaveApprovalPort,
        &mut NoLeaveEvidencePort,
        NOW,
    )
    .await
    .unwrap_err();
    assert_eq!(error.code, "LEAVE_APPROVAL_PORT_UNAVAILABLE");
}

#[tokio::test]
async fn accrual_drain_inserts_each_period_once_per_plan() {
    let mut repo = FakeRepo {
        accrual: vec![LeaveAccrualDue {
            company_id: "company".into(),
            engagement_id: "engagement".into(),
            leave_type_id: "type".into(),
            assignment_id: "assignment".into(),
            policy_version_id: "policy".into(),
            grant_kind: "accrual".into(),
            grant_units: 0.0,
            accrual_units: 1.5,
            accrual_period: Some("quarterly".into()),
            carryover_limit: None,
            first_day: "2026-01-01".into(),
            last_day: "2026-12-31".into(),
        }],
        ..FakeRepo::default()
    };
    let inserted = run_leave_accruals("2026-03-15", None, &mut repo, NOW)
        .await
        .unwrap();
    assert_eq!(inserted, 4);
    assert_eq!(
        repo.accrual_keys,
        vec![
            "leave:engagement:accrual:2026-01-01",
            "leave:engagement:accrual:2026-04-01",
            "leave:engagement:accrual:2026-07-01",
            "leave:engagement:accrual:2026-10-01",
        ]
    );
}

#[tokio::test]
async fn input_shapes_are_strict() {
    use erpx_server_people_payroll::public::leave::{
        LeaveCreateInput, LeaveDecisionInput, LeaveDraftInput, LeaveLifecycleInput,
    };
    assert!(
        LeaveCreateInput::parse(&json!({"startDate":"2026-02-30","days":1,"handoverNote":""}))
            .is_err()
    );
    assert!(
        LeaveDraftInput::parse(&json!({"startDate":"2026-01-01","days":0,"handoverNote":""}))
            .is_err()
    );
    assert!(
        LeaveDraftInput::parse(
            &json!({"startDate":"2026-01-01","days":2,"handoverNote":"","expectedVersion":0})
        )
        .is_err()
    );
    assert!(
        LeaveDecisionInput::parse(&json!({"expectedVersion":1,"note":"x".repeat(2049)})).is_err()
    );
    assert!(LeaveLifecycleInput::parse(&json!({"action":"approve","expectedVersion":1})).is_err());
    assert_eq!(
        BusinessError::new(FailureKind::Forbidden, "X", "Y").code,
        "X"
    );
}

#[tokio::test]
async fn self_service_create_maps_save_only_and_units_without_a_days_field() {
    use erpx_server_people_payroll::public::leave::LeaveCreateInput;
    let draft = LeaveCreateInput::parse(&json!({
        "leaveTypeId": "type",
        "startDate": "2026-03-02",
        "endDate": "2026-03-04",
        "calculatedUnits": 2.5,
        "saveOnly": true
    }))
    .unwrap();
    assert_eq!(draft.days, 3);
    assert!(!draft.submit);
    let submitted = LeaveCreateInput::parse(&json!({
        "leaveTypeId": "type",
        "startDate": "2026-03-02",
        "endDate": "2026-03-04",
        "calculatedUnits": 2.5,
        "saveOnly": false
    }))
    .unwrap();
    assert!(submitted.submit);
}
