//! Real migrated Holding PostgreSQL for the native owner approval vertical.
//!
//! The ports run directly on the caller's retained transaction and touch only
//! Workflow-owned `approval_instances`/`approval_steps`/`approval_decisions`
//! rows; `hr_people` is read only for display names. Expense, leave,
//! leave_cancellation and overtime cycles are isolated by record type.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::BusinessError;
use erpx_workflow_server::public::approval_planning::{
    OpenOwnerApproval, StaffApprovalStep, StaffApprovalView, StaffDecision, close_owner_approvals,
    open_owner_approval, owner_approval_view, prepare_owner_decision,
};

type Error = PgTransactionError<BusinessError>;

struct OwnerArgs {
    key: String,
    phase: String,
    requester: String,
    assignee: Option<String>,
    steps: Vec<StaffApprovalStep>,
    kind: String,
    now: i64,
}

fn step(user: &str, candidates: &[&str]) -> StaffApprovalStep {
    StaffApprovalStep {
        user_id: user.into(),
        authority: "named".into(),
        candidate_user_ids: candidates.iter().map(|id| (*id).to_string()).collect(),
        required_role: None,
        stage: None,
        snapshot_json: None,
    }
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

async fn setup(pool: &DatabasePool) {
    let files = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO hr_people\
         (id,user_id,company_id,legal_name,country_code,status,version,created_by,created_at,updated_at) VALUES \
         ('person-alice','alice','company','Alice Approver','TH','active',1,'system',1,1),\
         ('person-bob','bob','company','Bob Reviewer','TH','active',1,'system',1,1),\
         ('person-carol','carol','company','Carol Reviewer','TH','active',1,'system',1,1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn open(pool: &DatabasePool, record: &str, request: OwnerArgs) -> Result<String, Error> {
    let record = record.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let record = record.clone();
            let key = request.key.clone();
            let phase = request.phase.clone();
            let requester = request.requester.clone();
            let assignee = request.assignee.clone();
            let steps = request.steps.clone();
            let kind = request.kind.clone();
            let now = request.now;
            Box::pin(async move {
                Ok(open_owner_approval(
                    c,
                    OpenOwnerApproval {
                        key: &key,
                        company_id: "company",
                        branch_id: None,
                        department_id: None,
                        record_id: &record,
                        phase: &phase,
                        requester_user_id: &requester,
                        assignee_user_id: assignee.as_deref(),
                        record_version: 1,
                        steps: &steps,
                        now,
                        kind: &kind,
                    },
                )
                .await?)
            })
        })
        .await
}

async fn view(
    pool: &DatabasePool,
    user: &str,
    role: &str,
    record: &str,
    kind: &str,
) -> Result<StaffApprovalView, Error> {
    let user = user.to_owned();
    let role = role.to_owned();
    let record = record.to_owned();
    let kind = kind.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let user = user.clone();
            let role = role.clone();
            let record = record.clone();
            let kind = kind.clone();
            Box::pin(async move {
                Ok(owner_approval_view(c, "company", &user, "assignment", &role, &record, 100, &kind).await?)
            })
        })
        .await
}

async fn decide(
    pool: &DatabasePool,
    user: &str,
    action: &str,
    note: &str,
    version: i64,
    key: &str,
    record: &str,
    kind: &str,
) -> Result<StaffDecision, Error> {
    let user = user.to_owned();
    let approve = action == "approve";
    let note = note.to_owned();
    let key = key.to_owned();
    let record = record.to_owned();
    let kind = kind.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let user = user.clone();
            let note = note.clone();
            let key = key.clone();
            let record = record.clone();
            let kind = kind.clone();
            Box::pin(async move {
                Ok(prepare_owner_decision(
                    c,
                    "company",
                    &user,
                    "assignment",
                    "admin",
                    &key,
                    &record,
                    approve,
                    &note,
                    version,
                    200,
                    &kind,
                )
                .await?)
            })
        })
        .await
}

async fn close(
    pool: &DatabasePool,
    key: &str,
    record: &str,
    reason: &str,
    version: i64,
    kind: &str,
) -> Result<(), Error> {
    let key = key.to_owned();
    let record = record.to_owned();
    let reason = reason.to_owned();
    let kind = kind.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let key = key.clone();
            let record = record.clone();
            let reason = reason.clone();
            let kind = kind.clone();
            Box::pin(async move {
                Ok(close_owner_approvals(
                    c,
                    "company",
                    &key,
                    &record,
                    "requester",
                    &reason,
                    version,
                    300,
                    &kind,
                )
                .await?)
            })
        })
        .await
}

async fn instance(pool: &DatabasePool, id: &str) -> (String, i64, i64) {
    sqlx::query_as("SELECT state,current_step,version FROM approval_instances WHERE id=$1")
        .bind(id)
        .fetch_one(pool.raw())
        .await
        .unwrap()
}

fn request(key: &str, phase: &str, kind: &str, steps: Vec<StaffApprovalStep>) -> OwnerArgs {
    OwnerArgs {
        key: key.into(),
        phase: phase.into(),
        requester: "requester".into(),
        assignee: None,
        steps,
        kind: kind.into(),
        now: 100,
    }
}

#[tokio::test]
async fn expense_open_view_decide_close_reaches_final_and_replays_idempotently() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let id = open(
            &pool,
            "expense-1",
            request(
                "open:expense:0001",
                "request",
                "expense",
                vec![step("alice", &[]), step("bob", &[])],
            ),
        )
        .await
        .unwrap();
        assert_eq!(id, "approval-open:expense:0001");
        assert_eq!(
            instance(&pool, &id).await,
            ("pending".into(), 0, 1)
        );

        let missing = view(&pool, "alice", "admin", "expense-missing", "expense")
            .await
            .unwrap();
        assert_eq!(missing.state, "not_required");
        assert!(!missing.can_decide);

        let first = view(&pool, "alice", "admin", "expense-1", "expense")
            .await
            .unwrap();
        assert_eq!(first.state, "pending");
        assert_eq!(first.phase.as_deref(), Some("request"));
        assert!(first.can_decide);
        let steps = first.steps.as_array().unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0]["id"], "approval-open:expense:0001-0");
        assert_eq!(steps[0]["approverUserId"], "alice");
        assert_eq!(steps[0]["approverName"], "Alice Approver");

        let first_decision = decide(
            &pool,
            "alice",
            "approve",
            "",
            1,
            "decision:expense:0001",
            "expense-1",
            "expense",
        )
        .await
        .unwrap();
        assert!(!first_decision.final_decision);
        assert_eq!(instance(&pool, &id).await, ("pending".into(), 1, 2));

        let final_decision = decide(
            &pool,
            "bob",
            "approve",
            "",
            2,
            "decision:expense:0002",
            "expense-1",
            "expense",
        )
        .await
        .unwrap();
        assert!(final_decision.final_decision);
        assert_eq!(instance(&pool, &id).await, ("approved".into(), 1, 3));

        let replay = decide(
            &pool,
            "bob",
            "approve",
            "",
            2,
            "decision:expense:0002",
            "expense-1",
            "expense",
        )
        .await
        .unwrap();
        assert!(replay.final_decision);
        let decisions: i64 =
            sqlx::query_scalar("SELECT count(*) FROM approval_decisions WHERE instance_id=$1")
                .bind(&id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(decisions, 2);

        close(
            &pool,
            "close:expense:0001",
            "expense-1",
            "cancelled by requester",
            3,
            "expense",
        )
        .await
        .unwrap();
        assert_eq!(instance(&pool, &id).await, ("superseded".into(), 1, 4));
        close(
            &pool,
            "close:expense:0001",
            "expense-1",
            "cancelled by requester",
            3,
            "expense",
        )
        .await
        .unwrap();
        let cancel_rows: i64 =
            sqlx::query_scalar("SELECT count(*) FROM approval_decisions WHERE action='cancel' AND instance_id=$1")
                .bind(&id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(cancel_rows, 1);
    })
    .await;
}

#[tokio::test]
async fn leave_overtime_and_leave_cancellation_cycles_are_isolated() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        for (kind, key) in [
            ("leave", "open:leave:0001"),
            ("overtime", "open:overtime:0001"),
            ("leave_cancellation", "open:cancellation:0001"),
        ] {
            let id = open(
                &pool,
                "record-shared",
                request(key, "request", kind, vec![step("alice", &[])]),
            )
            .await
            .unwrap();
            assert_eq!(id, format!("approval-{key}"));
            let stored: (String,) =
                sqlx::query_as("SELECT record_type FROM approval_instances WHERE id=$1")
                    .bind(&id)
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
            assert_eq!(stored.0, kind);

            let requester = view(&pool, "requester", "admin", "record-shared", kind)
                .await
                .unwrap();
            assert!(!requester.can_decide);
            assert_eq!(
                requester.blocked_reason.as_deref(),
                Some("Only the named approver of the current step can decide.")
            );

            let allowed = view(&pool, "alice", "admin", "record-shared", kind)
                .await
                .unwrap();
            assert!(allowed.can_decide);

            let decision = decide(
                &pool,
                "alice",
                "approve",
                "",
                1,
                &format!("decision:{key}"),
                "record-shared",
                kind,
            )
            .await
            .unwrap();
            assert!(decision.final_decision);
            assert_eq!(instance(&pool, &id).await, ("approved".into(), 0, 2));
        }
        let cycles: i64 =
            sqlx::query_scalar("SELECT count(*) FROM approval_instances WHERE record_id='record-shared'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(cycles, 3);

        // A requester placed on the owner route is still refused as self-approval.
        let self_id = open(
            &pool,
            "expense-self",
            request("open:self:0001", "request", "expense", vec![step("requester", &[])]),
        )
        .await
        .unwrap();
        let self_view = view(&pool, "requester", "admin", "expense-self", "expense")
            .await
            .unwrap();
        assert!(!self_view.can_decide);
        assert_eq!(
            self_view.blocked_reason.as_deref(),
            Some("This approval would be a self-approval.")
        );
        let self_decision = decide(
            &pool,
            "requester",
            "approve",
            "",
            1,
            "decision:self:0001",
            "expense-self",
            "expense",
        )
        .await
        .err()
        .expect("self-approval must be refused");
        assert_eq!(business(self_decision).code, "SELF_APPROVAL");
        assert_eq!(instance(&pool, &self_id).await, ("pending".into(), 0, 1));
    })
    .await;
}

#[tokio::test]
async fn legal_and_unknown_kinds_are_refused_before_any_write() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let legal = open(
            &pool,
            "legal-1",
            request("open:legal:0001", "request", "legal", vec![step("alice", &[])]),
        )
        .await
        .err()
        .expect("legal must stay in the Legal owner");
        assert_eq!(business(legal).code, "LEGAL_OWNER_REQUIRED");
        let unknown = open(
            &pool,
            "unknown-1",
            request("open:unknown:0001", "request", "bogus", vec![step("alice", &[])]),
        )
        .await
        .err()
        .expect("unknown kinds must be refused");
        assert_eq!(business(unknown).code, "OWNER_APPROVAL_KIND_INVALID");
        let legal_view = view(&pool, "alice", "admin", "legal-1", "legal")
            .await
            .err()
            .expect("legal view must stay in the Legal owner");
        assert_eq!(business(legal_view).code, "LEGAL_OWNER_REQUIRED");
        let rows: i64 =
            sqlx::query_scalar("SELECT count(*) FROM approval_instances WHERE company_id='company'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(rows, 0);
    })
    .await;
}

#[tokio::test]
async fn stale_version_and_missing_reason_leave_no_decision() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let id = open(
            &pool,
            "expense-2",
            request("open:expense:0002", "request", "expense", vec![step("alice", &[])]),
        )
        .await
        .unwrap();
        let stale = decide(
            &pool,
            "alice",
            "approve",
            "",
            99,
            "decision:stale:0001",
            "expense-2",
            "expense",
        )
        .await
        .err()
        .expect("stale version must be refused");
        assert_eq!(business(stale).code, "TASK_APPROVAL_CHANGED");
        let missing_reason = decide(
            &pool,
            "alice",
            "reject",
            " ",
            1,
            "decision:empty:0001",
            "expense-2",
            "expense",
        )
        .await
        .err()
        .expect("reject needs a reason");
        assert_eq!(business(missing_reason).code, "REASON_REQUIRED");
        assert_eq!(instance(&pool, &id).await, ("pending".into(), 0, 1));
        let decisions: i64 =
            sqlx::query_scalar("SELECT count(*) FROM approval_decisions WHERE instance_id=$1")
                .bind(&id)
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(decisions, 0);
    })
    .await;
}
