//! Real migrated Holding PostgreSQL for the native Staff Work approval ports.
//!
//! The ports run directly on the caller's retained serializable transaction and
//! touch only Workflow-owned `approval_instances`/`approval_steps`/
//! `approval_decisions` rows; `hr_people` is read only for display names.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::BusinessError;
use erpx_server_workflow::public::staff_approval::{
    OpenStaffApproval, StaffApprovalStep, StaffApprovalView, StaffDecision, close_staff_approvals,
    open_staff_approval, prepare_staff_decision, staff_approval_view,
};

type Error = PgTransactionError<BusinessError>;

struct OpenArgs {
    key: String,
    phase: String,
    requester: String,
    assignee: Option<String>,
    steps: Vec<StaffApprovalStep>,
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
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
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

async fn open(pool: &DatabasePool, task: &str, request: OpenArgs) -> Result<String, Error> {
    let task = task.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let task = task.clone();
            let key = request.key.clone();
            let phase = request.phase.clone();
            let requester = request.requester.clone();
            let assignee = request.assignee.clone();
            let steps = request.steps.clone();
            let now = request.now;
            Box::pin(async move {
                Ok(open_staff_approval(
                    c,
                    OpenStaffApproval {
                        key: &key,
                        company_id: "company",
                        branch_id: None,
                        department_id: None,
                        task_id: &task,
                        phase: &phase,
                        requester_user_id: &requester,
                        assignee_user_id: assignee.as_deref(),
                        record_version: 1,
                        steps: &steps,
                        now,
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
    task: &str,
) -> Result<StaffApprovalView, Error> {
    let user = user.to_owned();
    let role = role.to_owned();
    let task = task.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let user = user.clone();
            let role = role.clone();
            let task = task.clone();
            Box::pin(async move {
                Ok(
                    staff_approval_view(c, "company", &user, "assignment", &role, &task, 100)
                        .await?,
                )
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
    task: &str,
) -> Result<StaffDecision, Error> {
    let user = user.to_owned();
    let approve = action == "approve";
    let note = note.to_owned();
    let key = key.to_owned();
    let task = task.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let user = user.clone();
            let note = note.clone();
            let key = key.clone();
            let task = task.clone();
            Box::pin(async move {
                Ok(prepare_staff_decision(
                    c,
                    "company",
                    &user,
                    "assignment",
                    "admin",
                    &key,
                    &task,
                    approve,
                    &note,
                    version,
                    200,
                )
                .await?)
            })
        })
        .await
}

async fn close(
    pool: &DatabasePool,
    key: &str,
    task: &str,
    reason: &str,
    version: i64,
) -> Result<(), Error> {
    let key = key.to_owned();
    let task = task.to_owned();
    let reason = reason.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let key = key.clone();
            let task = task.clone();
            let reason = reason.clone();
            Box::pin(async move {
                Ok(close_staff_approvals(
                    c,
                    "company",
                    &key,
                    &task,
                    "requester",
                    &reason,
                    version,
                    300,
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

#[tokio::test]
async fn open_view_decide_order_reaches_final_and_replays_idempotently() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let id = open(
            &pool,
            "task-1",
            OpenArgs {
                key: "open:0001".into(),
                phase: "start".into(),
                requester: "requester".into(),
                assignee: Some("assignee".into()),
                steps: vec![step("alice", &[]), step("bob", &[])],
                now: 100,
            },
        )
        .await
        .unwrap();
        assert_eq!(id, "approval-open:0001");

        let missing = view(&pool, "alice", "admin", "task-missing").await.unwrap();
        assert_eq!(missing.state, "not_required");
        assert!(!missing.can_decide);
        assert_eq!(missing.steps, serde_json::json!([]));

        let first = view(&pool, "alice", "admin", "task-1").await.unwrap();
        assert_eq!(first.state, "pending");
        assert_eq!(first.phase.as_deref(), Some("start"));
        assert!(first.can_decide);
        let steps = first.steps.as_array().unwrap();
        assert_eq!(steps.len(), 2);
        assert_eq!(steps[0]["id"], "approval-open:0001-0");
        assert_eq!(steps[0]["approverUserId"], "alice");
        assert_eq!(steps[0]["approverName"], "Alice Approver");
        assert_eq!(steps[0]["state"], "pending");
        assert_eq!(steps[0]["decidedAt"], serde_json::Value::Null);

        let assignee = view(&pool, "assignee", "admin", "task-1").await.unwrap();
        assert!(!assignee.can_decide);
        assert_eq!(
            assignee.blocked_reason.as_deref(),
            Some("Only the named approver of the current step can decide.")
        );

        let first_decision = decide(&pool, "alice", "approve", "", 1, "decision:0001", "task-1")
            .await
            .unwrap();
        assert_eq!(first_decision.phase, "start");
        assert!(!first_decision.final_decision);
        assert_eq!(
            instance(&pool, "approval-open:0001").await,
            ("pending".into(), 1, 2)
        );

        let next = view(&pool, "bob", "admin", "task-1").await.unwrap();
        assert!(next.can_decide);
        let second = view(&pool, "alice", "admin", "task-1").await.unwrap();
        assert!(!second.can_decide);

        let final_decision = decide(&pool, "bob", "approve", "", 2, "decision:0002", "task-1")
            .await
            .unwrap();
        assert_eq!(final_decision.phase, "start");
        assert!(final_decision.final_decision);
        assert_eq!(
            instance(&pool, "approval-open:0001").await,
            ("approved".into(), 1, 3)
        );
        let done = view(&pool, "bob", "admin", "task-1").await.unwrap();
        assert_eq!(done.state, "approved");
        assert!(!done.can_decide);
        let steps = done.steps.as_array().unwrap();
        assert_eq!(steps[1]["state"], "approved");
        assert_eq!(steps[1]["approverName"], "Bob Reviewer");
        assert_eq!(steps[1]["decidedAt"], serde_json::json!(200));
        assert_eq!(steps[1]["decision"]["action"], "approve");

        let replay = decide(&pool, "bob", "approve", "", 2, "decision:0002", "task-1")
            .await
            .unwrap();
        assert!(replay.final_decision);
        assert_eq!(replay.phase, "start");
        let conflicting = decide(
            &pool,
            "bob",
            "reject",
            "Different outcome",
            2,
            "decision:0002",
            "task-1",
        )
        .await
        .err()
        .expect("expected an idempotency failure");
        assert_eq!(business(conflicting).code, "IDEMPOTENCY_CONFLICT");
        let decisions: i64 =
            sqlx::query_scalar("SELECT count(*) FROM approval_decisions WHERE instance_id=$1")
                .bind("approval-open:0001")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(decisions, 2);
    })
    .await;
}

#[tokio::test]
async fn reject_pool_self_approval_and_reason_policy_refusals_leave_no_decision() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        open(
            &pool,
            "task-2",
            OpenArgs {
                key: "open:0002".into(),
                phase: "start".into(),
                requester: "requester".into(),
                assignee: Some("assignee".into()),
                steps: vec![StaffApprovalStep {
                    user_id: "alice".into(),
                    authority: "named".into(),
                    candidate_user_ids: vec!["alice".into(), "bob".into()],
                    required_role: None,
                    stage: None,
                    snapshot_json: None,
                }],
                now: 100,
            },
        )
        .await
        .unwrap();
        let pool_view = view(&pool, "bob", "admin", "task-2").await.unwrap();
        assert!(pool_view.can_decide);
        let missing_reason = decide(&pool, "bob", "reject", " ", 1, "decision:empty", "task-2")
            .await
            .err()
            .expect("expected a reason failure");
        assert_eq!(business(missing_reason).code, "REASON_REQUIRED");
        let reject = decide(
            &pool,
            "bob",
            "reject",
            "Needs correction",
            1,
            "decision:reject:1",
            "task-2",
        )
        .await
        .unwrap();
        assert!(reject.final_decision);
        assert_eq!(
            instance(&pool, "approval-open:0002").await,
            ("rejected".into(), 0, 2)
        );
        let decision: (String, String, i64) = sqlx::query_as(
            "SELECT action,actor_user_id,reviewed_version FROM approval_decisions \
             WHERE command_id='decision:reject:1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(decision, ("reject".into(), "bob".into(), 1));

        open(
            &pool,
            "task-3",
            OpenArgs {
                key: "open:0003".into(),
                phase: "start".into(),
                requester: "requester".into(),
                assignee: Some("assignee".into()),
                steps: vec![step("requester", &[])],
                now: 100,
            },
        )
        .await
        .unwrap();
        let requester = view(&pool, "requester", "admin", "task-3").await.unwrap();
        assert!(!requester.can_decide);
        assert_eq!(
            requester.blocked_reason.as_deref(),
            Some("This approval would be a self-approval.")
        );
        let self_requester = decide(
            &pool,
            "requester",
            "approve",
            "",
            1,
            "decision:self:1",
            "task-3",
        )
        .await
        .err()
        .expect("expected a self-approval failure");
        assert_eq!(business(self_requester).code, "SELF_APPROVAL");

        open(
            &pool,
            "task-4",
            OpenArgs {
                key: "open:0004".into(),
                phase: "start".into(),
                requester: "requester".into(),
                assignee: Some("assignee".into()),
                steps: vec![step("assignee", &[])],
                now: 100,
            },
        )
        .await
        .unwrap();
        let self_assignee = decide(
            &pool,
            "assignee",
            "approve",
            "",
            1,
            "decision:self:2",
            "task-4",
        )
        .await
        .err()
        .expect("expected a self-approval failure");
        assert_eq!(business(self_assignee).code, "SELF_APPROVAL");
        assert_eq!(
            instance(&pool, "approval-open:0003").await,
            ("pending".into(), 0, 1)
        );
        assert_eq!(
            instance(&pool, "approval-open:0004").await,
            ("pending".into(), 0, 1)
        );
        let decisions: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM approval_decisions WHERE command_id IN \
             ('decision:empty','decision:self:1','decision:self:2')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(decisions, 0);
    })
    .await;
}

#[tokio::test]
async fn stale_version_and_unprivileged_actor_are_refused() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        sqlx::raw_sql(
            "INSERT INTO staff_tasks\
             (id,company_id,title,creator_user_id,assignee_user_id,due_at,state,version,created_at,updated_at) VALUES \
             ('task-5','company','Task 5','requester','assignee',1000,'completion_approval',7,1,1);",
        )
        .execute(pool.raw())
        .await
        .unwrap();
        open(
            &pool,
            "task-5",
            OpenArgs {
                key: "open:0005".into(),
                phase: "completion".into(),
                requester: "requester".into(),
                assignee: Some("assignee".into()),
                steps: vec![step("alice", &[])],
                now: 100,
            },
        )
        .await
        .unwrap();
        let stale = decide(
            &pool,
            "alice",
            "approve",
            "",
            99,
            "decision:stale",
            "task-5",
        )
        .await
        .err()
        .expect("expected a stale failure");
        assert_eq!(business(stale).code, "TASK_APPROVAL_CHANGED");
        let other = decide(&pool, "carol", "approve", "", 7, "decision:other", "task-5")
            .await
            .err()
            .expect("expected an eligibility failure");
        assert_eq!(business(other).code, "CURRENT_APPROVER_REQUIRED");
        assert_eq!(
            instance(&pool, "approval-open:0005").await,
            ("pending".into(), 0, 1)
        );
        let decisions: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM approval_decisions WHERE command_id IN \
             ('decision:stale','decision:other')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(decisions, 0);
        // The caller's expected version is the task version, not the instance
        // version; the instance is still guarded by its own locked version.
        let accepted = decide(&pool, "alice", "approve", "", 7, "decision:stale", "task-5")
            .await
            .unwrap();
        assert!(accepted.final_decision);
        assert_eq!(
            instance(&pool, "approval-open:0005").await,
            ("approved".into(), 0, 2)
        );
        let reviewed: i64 = sqlx::query_scalar(
            "SELECT reviewed_version FROM approval_decisions WHERE command_id='decision:stale'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(reviewed, 7);
        // Completion approvals may be decided by the requester when named.
        open(
            &pool,
            "task-6",
            OpenArgs {
                key: "open:0006".into(),
                phase: "completion".into(),
                requester: "requester".into(),
                assignee: Some("assignee".into()),
                steps: vec![step("requester", &[])],
                now: 100,
            },
        )
        .await
        .unwrap();
        let completion = decide(
            &pool,
            "requester",
            "approve",
            "",
            1,
            "decision:completion",
            "task-6",
        )
        .await
        .unwrap();
        assert!(completion.final_decision);
        assert_eq!(completion.phase, "completion");
    })
    .await;
}

#[tokio::test]
async fn open_cycles_are_idempotent_and_close_supersedes_open_task_approvals() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let args = || OpenArgs {
            key: "open:0007".into(),
            phase: "start".into(),
            requester: "requester".into(),
            assignee: Some("assignee".into()),
            steps: vec![step("alice", &[])],
            now: 100,
        };
        let first = open(&pool, "task-7", args()).await.unwrap();
        let replay = open(&pool, "task-7", args()).await.unwrap();
        assert_eq!(first, replay);
        let cycles: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT id,cycle,version FROM approval_instances WHERE record_id='task-7' \
             ORDER BY insertion_order",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(cycles, vec![("approval-open:0007".into(), 1, 1)]);

        open(
            &pool,
            "task-7",
            OpenArgs {
                key: "open:0008".into(),
                phase: "start".into(),
                requester: "requester".into(),
                assignee: Some("assignee".into()),
                steps: vec![step("bob", &[])],
                now: 150,
            },
        )
        .await
        .unwrap();
        let cycles: Vec<(String, i64, i64)> = sqlx::query_as(
            "SELECT id,cycle,version FROM approval_instances WHERE record_id='task-7' \
             ORDER BY insertion_order",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            cycles,
            vec![
                ("approval-open:0007".into(), 1, 2),
                ("approval-open:0008".into(), 2, 1)
            ]
        );
        let current = view(&pool, "bob", "admin", "task-7").await.unwrap();
        assert_eq!(current.state, "pending");
        assert!(current.can_decide);

        close(&pool, "close:0007", "task-7", "Task cancelled", 2)
            .await
            .unwrap();
        close(&pool, "close:0007", "task-7", "Task cancelled", 2)
            .await
            .unwrap();
        let closed = view(&pool, "bob", "admin", "task-7").await.unwrap();
        assert_eq!(closed.state, "superseded");
        assert!(!closed.can_decide);
        let instances: Vec<(String, i64)> = sqlx::query_as(
            "SELECT state,version FROM approval_instances WHERE record_id='task-7' \
             ORDER BY insertion_order",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            instances,
            vec![("superseded".into(), 2), ("superseded".into(), 2)]
        );
        let decisions: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM approval_decisions WHERE command_id='close:0007' AND action='cancel'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(decisions, 1);
        // Closing a record with no open approval and a fresh key is a no-op.
        close(&pool, "close:0008", "task-8", "Nothing open", 1)
            .await
            .unwrap();
    })
    .await;
}
