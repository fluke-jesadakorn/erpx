//! Native leave requests on migrated PostgreSQL.
//!
//! Proves serializable owner SQL, command receipts/audit/outbox, request
//! version CAS, Company isolation, cancellation review, balance ledger effects
//! and the idempotent accrual drain. Workflow/Files/Operations ports are
//! deliberately owner-local fakes; their real composition is mounted by the
//! lead, and the fail-closed defaults are exercised separately.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_people_payroll::public::leave::{
    LeaveApprovalInstance, LeaveApprovalPort, LeaveApprovalView, LeaveCommandFuture,
    LeaveDecisionFact, LeaveEvidenceBindRequest, LeaveEvidenceManifest, LeaveEvidencePort,
    LeaveEvidenceRequirement, LeaveRequestRow, LeaveRoute, LeaveRoutePort, LeaveStaffFacts,
    LeaveStaffPort, NoLeaveApprovalPort, PgLeaveCommand, create_leave, decide_leave,
    decide_leave_cancellation, run_leave_accruals, save_or_submit_leave, transition_leave,
};
use erpx_server_people_payroll::public::workforce::WorkforceCommandActor;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, HoldingId, TimestampMillis,
};
use serde_json::{Value, json};
use sqlx::PgConnection;

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
        permissions: vec!["leave.self".into(), "leave.evidence.read".into()],
        work_context: "a".repeat(64),
        expires_at: NOW.get() + 100_000,
        beta: true,
    }
}

fn key(value: &str) -> CommandKey {
    CommandKey::new(value).unwrap()
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
}

async fn seed_worker(pool: &DatabasePool, company: &str, person: &str, engagement: &str) {
    sqlx::query(
        "INSERT INTO hr_people(id,company_id,legal_name,country_code,created_by,created_at,updated_at) \
         VALUES($1,$2,'Somchai Jaidee','TH','seed',0,0) ON CONFLICT DO NOTHING",
    )
    .bind(person)
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO hr_engagements(id,person_id,company_id,branch_id,department_id,worker_type,\
         employment_type,status,start_date,job_title,currency,created_by,created_at,updated_at) \
         VALUES($1,$2,$3,'branch','department','employee','salaried','active','2026-01-01',\
         'Engineer','THB','seed',0,0) ON CONFLICT DO NOTHING",
    )
    .bind(engagement)
    .bind(person)
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_leave_type(pool: &DatabasePool, company: &str) {
    sqlx::query(
        "INSERT INTO leave_types(id,company_id,code,name,description,tracked,effective_from,\
         created_by,created_at,updated_at) VALUES('type',$1,'annual','Annual','',true,\
         '2026-01-01','seed',0,0) ON CONFLICT DO NOTHING",
    )
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO leave_policy_versions(id,company_id,policy_key,version,name,status,\
         effective_from,timezone,workweek_json,allow_negative,created_by,created_at) \
         VALUES('policy',$1,'standard',1,'Standard','active','2026-01-01','UTC',\
         '[1,2,3,4,5]'::jsonb,false,'seed',0) ON CONFLICT DO NOTHING",
    )
    .bind(company)
    .execute(pool.raw())
    .await
    .unwrap();
    sqlx::query(
        "INSERT INTO leave_policy_rules(policy_version_id,leave_type_id,grant_kind,grant_units,\
         accrual_units,accrual_period,evidence_min_units,evidence_purpose,evidence_min_count,\
         employee_eligible) VALUES('policy','type','accrual',0,1.5,'monthly',2,\
         'medical_certificate',1,true) ON CONFLICT DO NOTHING",
    )
    .execute(pool.raw())
    .await
    .unwrap();
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

struct FakeStaff(LeaveStaffFacts);

impl LeaveStaffPort for FakeStaff {
    fn active_staff<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> LeaveCommandFuture<'a, Option<LeaveStaffFacts>, BusinessError> {
        Box::pin(async move { Ok(Some(self.0.clone())) })
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
}

impl FakeApproval {
    fn final_decision() -> Self {
        Self {
            final_decision: true,
            opens: 0,
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
        Box::pin(async { Ok(()) })
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
        Box::pin(async { Ok(()) })
    }
    fn view<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
        _: &'a WorkforceCommandActor,
    ) -> LeaveCommandFuture<'a, Option<LeaveApprovalView>, BusinessError> {
        Box::pin(async {
            Ok(Some(LeaveApprovalView {
                can_view: true,
                can_decide: true,
                can_override: false,
                blocked_reason: None,
            }))
        })
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
    available: i64,
}

impl LeaveEvidencePort for FakeEvidence {
    fn manifest<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> LeaveCommandFuture<'a, LeaveEvidenceManifest, BusinessError> {
        Box::pin(async { Ok(LeaveEvidenceManifest::default()) })
    }
    fn available<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a str,
        _: &'a str,
    ) -> LeaveCommandFuture<'a, LeaveEvidenceRequirement, BusinessError> {
        Box::pin(async move {
            Ok(LeaveEvidenceRequirement {
                count: self.available,
                set_ids: Vec::new(),
            })
        })
    }
    fn bind<'a>(
        &'a mut self,
        _: Option<&'a mut PgConnection>,
        _: &'a LeaveEvidenceBindRequest<'a>,
    ) -> LeaveCommandFuture<'a, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

fn create_body() -> Value {
    json!({
        "startDate": "2026-03-02",
        "days": 3,
        "handoverNote": "Handover"
    })
}

fn typed_create_body() -> Value {
    json!({
        "startDate": "2026-03-02",
        "days": 3,
        "handoverNote": "Handover",
        "leaveTypeId": "type",
        "policyVersionId": "policy",
        "evidencePolicy": {
            "required": true,
            "minimumUnits": 2.0,
            "purposeKey": "medical_certificate",
            "minimumCount": 1,
            "dueStage": "before_final_approval",
            "graceDays": 0,
            "privacy": "requester_approvers_hr",
            "note": "Attach a certificate"
        }
    })
}

fn submit_body() -> Value {
    json!({
        "startDate": "2026-03-02",
        "days": 3,
        "handoverNote": "Handover",
        "expectedVersion": 1,
        "saveOnly": false,
        "leaveTypeId": "type",
        "policyVersionId": "policy",
        "evidencePolicy": {
            "required": true,
            "minimumUnits": 2.0,
            "purposeKey": "medical_certificate",
            "minimumCount": 1,
            "dueStage": "before_final_approval",
            "graceDays": 0,
            "privacy": "requester_approvers_hr",
            "note": "Attach a certificate"
        }
    })
}

async fn count(pool: &DatabasePool, table: &str) -> i64 {
    sqlx::query_scalar::<_, i64>(sqlx::AssertSqlSafe(format!("SELECT count(*) FROM {table}")))
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

#[tokio::test]
async fn create_is_atomic_idempotent_and_company_scoped() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_worker(&pool, "company", "person", "engagement").await;
        let uow = PgUnitOfWork::new(pool.clone());
        let created = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgLeaveCommand::new(connection, &actor);
                    create_leave(
                        &actor,
                        &create_body(),
                        &key("leave-create-1"),
                        &mut command,
                        &mut FakeStaff(staff()),
                        &mut FakeRoute,
                        &mut FakeApproval::final_decision(),
                        &mut FakeEvidence { available: 0 },
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(created.id, "leave-leave-create-1");
        assert_eq!(created.state, "draft");
        assert_eq!(count(&pool, "leave_requests").await, 1);
        assert_eq!(count(&pool, "employees").await, 1);
        for table in ["command_receipts", "audit_events", "outbox_events"] {
            assert_eq!(count(&pool, table).await, 1, "{table}");
        }
        let replay = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgLeaveCommand::new(connection, &actor);
                    create_leave(
                        &actor,
                        &create_body(),
                        &key("leave-create-1"),
                        &mut command,
                        &mut FakeStaff(staff()),
                        &mut FakeRoute,
                        &mut FakeApproval::final_decision(),
                        &mut FakeEvidence { available: 0 },
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(replay.id, created.id);
        assert_eq!(count(&pool, "leave_requests").await, 1);
        let conflict = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgLeaveCommand::new(connection, &actor);
                    create_leave(
                        &actor,
                        &json!({"startDate":"2026-04-01","days":1,"handoverNote":"Other"}),
                        &key("leave-create-1"),
                        &mut command,
                        &mut FakeStaff(staff()),
                        &mut FakeRoute,
                        &mut FakeApproval::final_decision(),
                        &mut FakeEvidence { available: 0 },
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(conflict).code, "IDEMPOTENCY_CONFLICT");
    })
    .await;
}

#[tokio::test]
async fn submit_and_final_approval_commit_balances_and_versions() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_worker(&pool, "company", "person", "engagement").await;
        seed_leave_type(&pool, "company").await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgLeaveCommand::new(connection, &actor);
                create_leave(
                    &actor,
                    &typed_create_body(),
                    &key("typed-create-1"),
                    &mut command,
                    &mut FakeStaff(staff()),
                    &mut FakeRoute,
                    &mut FakeApproval::final_decision(),
                    &mut FakeEvidence { available: 0 },
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let submitted = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgLeaveCommand::new(connection, &actor);
                    save_or_submit_leave(
                        &actor,
                        "leave-typed-create-1",
                        &submit_body(),
                        &key("typed-submit-1"),
                        &mut command,
                        &mut FakeStaff(staff()),
                        &mut FakeRoute,
                        &mut FakeApproval::final_decision(),
                        &mut FakeEvidence { available: 0 },
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(submitted.state, "pending_approval");
        assert_eq!(submitted.version, 2);
        sqlx::query(
            "INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,\
             request_id,kind,units,effective_date,command_key,note,created_by,created_at) \
             VALUES('reservation-1','company','engagement','type','leave-typed-create-1',\
             'reservation',-3,'2026-01-01','reserved','Pending leave reservation','owner',0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let approved = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgLeaveCommand::new(connection, &actor);
                    decide_leave(
                        &actor,
                        "leave-typed-create-1",
                        &json!({"expectedVersion": 2, "note": "Reviewed"}),
                        &key("typed-approve-1"),
                        true,
                        &mut command,
                        &mut FakeApproval::final_decision(),
                        &mut FakeEvidence { available: 1 },
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(approved.state, "approved");
        assert_eq!(approved.version, 3);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM leave_balance_entries WHERE request_id='leave-typed-create-1' \
                 AND kind IN ('usage','reservation_release')"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM leave_requests WHERE id='leave-typed-create-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "approved"
        );
    })
    .await;
}

#[tokio::test]
async fn approved_cancellation_review_and_reversal_commit_together() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_worker(&pool, "company", "person", "engagement").await;
        seed_leave_type(&pool, "company").await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgLeaveCommand::new(connection, &actor);
                create_leave(
                    &actor,
                    &typed_create_body(),
                    &key("cancel-create-1"),
                    &mut command,
                    &mut FakeStaff(staff()),
                    &mut FakeRoute,
                    &mut FakeApproval::final_decision(),
                    &mut FakeEvidence { available: 0 },
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgLeaveCommand::new(connection, &actor);
                save_or_submit_leave(
                    &actor,
                    "leave-cancel-create-1",
                    &submit_body(),
                    &key("cancel-submit-1"),
                    &mut command,
                    &mut FakeStaff(staff()),
                    &mut FakeRoute,
                    &mut FakeApproval::final_decision(),
                    &mut FakeEvidence { available: 0 },
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        sqlx::query(
            "INSERT INTO leave_balance_entries(id,company_id,engagement_id,leave_type_id,\
             request_id,kind,units,effective_date,command_key,note,created_by,created_at) \
             VALUES('cancel-reservation','company','engagement','type','leave-cancel-create-1',\
             'reservation',-3,'2026-01-01','reserved','Pending leave reservation','owner',0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgLeaveCommand::new(connection, &actor);
                decide_leave(
                    &actor,
                    "leave-cancel-create-1",
                    &json!({"expectedVersion": 2, "note": "Approved"}),
                    &key("cancel-approve-1"),
                    true,
                    &mut command,
                    &mut FakeApproval::final_decision(),
                    &mut FakeEvidence { available: 1 },
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let requested = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgLeaveCommand::new(connection, &actor);
                    transition_leave(
                        &actor,
                        "leave-cancel-create-1",
                        &json!({"action":"cancel","expectedVersion":3,"note":"Plans changed"}),
                        &key("cancel-request-1"),
                        &mut command,
                        &mut FakeRoute,
                        &mut FakeApproval::final_decision(),
                        &mut FakeEvidence { available: 0 },
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(requested.state, "approved");
        assert_eq!(requested.cancellation_state.as_deref(), Some("pending_review"));
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM leave_cancellation_reviews WHERE request_id='leave-cancel-create-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "pending_approval"
        );
        let decided = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgLeaveCommand::new(connection, &actor);
                    decide_leave_cancellation(
                        &actor,
                        "leave-cancel-create-1",
                        &json!({"expectedVersion":4,"note":"Approved cancellation"}),
                        &key("cancel-decide-1"),
                        true,
                        &mut command,
                        &mut FakeApproval::final_decision(),
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap();
        assert_eq!(decided.state, "cancelled");
        assert_eq!(decided.cancellation_state.as_deref(), Some("approved"));
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM leave_cancellation_reviews WHERE request_id='leave-cancel-create-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "approved"
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM leave_balance_entries WHERE request_id='leave-cancel-create-1' \
                 AND kind='usage_reversal'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            1
        );
    })
    .await;
}

#[tokio::test]
async fn foreign_company_cannot_submit_another_requests_leave() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_worker(&pool, "company", "person", "engagement").await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgLeaveCommand::new(connection, &actor);
                create_leave(
                    &actor,
                    &create_body(),
                    &key("isolation-create-1"),
                    &mut command,
                    &mut FakeStaff(staff()),
                    &mut FakeRoute,
                    &mut FakeApproval::final_decision(),
                    &mut FakeEvidence { available: 0 },
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let mut foreign = actor();
        foreign.company_id = CompanyId::new("foreign").unwrap();
        let foreign_actor = foreign.clone();
        let error = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let foreign = foreign_actor.clone();
                Box::pin(async move {
                    let mut command = PgLeaveCommand::new(connection, &foreign);
                    save_or_submit_leave(
                        &foreign,
                        "leave-isolation-create-1",
                        &json!({
                            "startDate":"2026-03-02",
                            "days":3,
                            "handoverNote":"Handover",
                            "expectedVersion":1,
                            "saveOnly":false
                        }),
                        &key("isolation-submit-1"),
                        &mut command,
                        &mut FakeStaff(staff()),
                        &mut FakeRoute,
                        &mut FakeApproval::final_decision(),
                        &mut FakeEvidence { available: 0 },
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "LEAVE_NOT_FOUND");
    })
    .await;
}

#[tokio::test]
async fn default_approval_port_fails_closed_before_owner_rows_change() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_worker(&pool, "company", "person", "engagement").await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgLeaveCommand::new(connection, &actor);
                create_leave(
                    &actor,
                    &create_body(),
                    &key("closed-create-1"),
                    &mut command,
                    &mut FakeStaff(staff()),
                    &mut FakeRoute,
                    &mut NoLeaveApprovalPort,
                    &mut FakeEvidence { available: 0 },
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let error = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let actor = actor();
                    let mut command = PgLeaveCommand::new(connection, &actor);
                    save_or_submit_leave(
                        &actor,
                        "leave-closed-create-1",
                        &json!({
                            "startDate":"2026-03-02",
                            "days":3,
                            "handoverNote":"Handover",
                            "expectedVersion":1,
                            "saveOnly":false
                        }),
                        &key("closed-submit-1"),
                        &mut command,
                        &mut FakeStaff(staff()),
                        &mut FakeRoute,
                        &mut NoLeaveApprovalPort,
                        &mut FakeEvidence { available: 0 },
                        NOW,
                    )
                    .await
                })
            })
            .await
            .unwrap_err();
        assert_eq!(business(error).code, "LEAVE_APPROVAL_PORT_UNAVAILABLE");
        assert_eq!(
            sqlx::query_scalar::<_, String>(
                "SELECT state FROM leave_requests WHERE id='leave-closed-create-1'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            "draft"
        );
    })
    .await;
}

#[tokio::test]
async fn accrual_drain_is_idempotent_per_plan_period() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_worker(&pool, "company", "person", "engagement").await;
        seed_leave_type(&pool, "company").await;
        sqlx::query(
            "INSERT INTO leave_policy_assignments(id,company_id,engagement_id,policy_version_id,\
             effective_from,created_by,created_at) \
             VALUES('assignment','company','engagement','policy','2026-01-01','seed',0)",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        let uow = PgUnitOfWork::new(pool.clone());
        let first = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut command = PgLeaveCommand::new(connection, &actor());
                    run_leave_accruals("2026-03-15", Some("company"), &mut command, NOW).await
                })
            })
            .await
            .unwrap();
        assert_eq!(first, 3);
        assert_eq!(
            sqlx::query_scalar::<_, i64>(
                "SELECT count(*) FROM leave_balance_entries WHERE kind='accrual' \
                 AND company_id='company' AND leave_type_id='type'"
            )
            .fetch_one(pool.raw())
            .await
            .unwrap(),
            3
        );
        let second = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut command = PgLeaveCommand::new(connection, &actor());
                    run_leave_accruals("2026-03-15", Some("company"), &mut command, NOW).await
                })
            })
            .await
            .unwrap();
        assert_eq!(second, 0);
        assert_eq!(count(&pool, "leave_balance_entries").await, 3);
    })
    .await;
}

#[tokio::test]
async fn read_row_returns_none_for_another_company() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        seed_worker(&pool, "company", "person", "engagement").await;
        let uow = PgUnitOfWork::new(pool.clone());
        uow.run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
            Box::pin(async move {
                let actor = actor();
                let mut command = PgLeaveCommand::new(connection, &actor);
                create_leave(
                    &actor,
                    &create_body(),
                    &key("read-create-1"),
                    &mut command,
                    &mut FakeStaff(staff()),
                    &mut FakeRoute,
                    &mut FakeApproval::final_decision(),
                    &mut FakeEvidence { available: 0 },
                    NOW,
                )
                .await
            })
        })
        .await
        .unwrap();
        let row: Option<LeaveRequestRow> = uow
            .run::<_, BusinessError, _>(TransactionOptions::default(), |connection| {
                Box::pin(async move {
                    let mut reader =
                        erpx_server_people_payroll::public::leave::PgLeaveReader::new(connection);
                    Ok(reader
                        .leave_row(
                            &CompanyId::new("foreign").unwrap(),
                            "leave-read-create-1",
                            "owner",
                            true,
                            None,
                            None,
                        )
                        .await?)
                })
            })
            .await
            .unwrap();
        assert!(row.is_none());
    })
    .await;
}
