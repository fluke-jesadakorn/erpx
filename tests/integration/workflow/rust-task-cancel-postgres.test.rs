//! Real migrated Holding PostgreSQL for the native task cancellation command.
#[path = "../../support/workflow/command-actor.rs"]
mod actor_fixture;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, IdentifierSource, RecordId, TimestampMillis,
};
use erpx_server_workflow::public::commands::*;
use erpx_server_workflow::public::task_cancel::{
    ParticipantCancelEligibility, execute_task_cancel,
};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

struct Time;
impl Clock for Time {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(100)
    }
}
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).unwrap()
    }
}
fn actor(user: &str) -> CommandActor {
    actor_fixture::parse_frozen_actor(&format!(
        r#"{{"userId":"{user}","assignmentId":"assignment","role":"admin","companyId":"company","branchId":null,"departmentId":null,"authority":{{"sessionId":"session","revision":1,"assignmentVersion":1}}}}"#
    ))
}
struct Authority {
    user: String,
}
impl PgCommandAuthority for Authority {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        Ok(actor(&self.user))
    }
    async fn verify_command(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
        _: CommandDisposition,
    ) -> Result<(), Error> {
        Ok(())
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
        "INSERT INTO staff_tasks\
         (id,company_id,title,creator_user_id,assignee_user_id,due_at,state,version,created_at,updated_at) VALUES \
         ('task-1','company','Task 1','creator','assignee',1000,'start_approval',3,1,1),\
         ('task-2','company','Task 2','other','other',1000,'start_approval',1,1,1),\
         ('task-3','company','Task 3','creator','assignee',1000,'completed',1,1,1),\
         ('task-4','company','Task 4','creator','assignee',1000,'start_approval',1,1,1);\
         INSERT INTO approval_instances\
         (id,company_id,record_type,record_id,phase,cycle,requester_user_id,assignee_user_id,state,current_step,version,created_at,updated_at) VALUES \
         ('approval-1','company','task','task-1','start',1,'creator','assignee','pending',0,2,1,1),\
         ('approval-1-done','company','task','task-1','completion',1,'creator','assignee','approved',0,1,1,1),\
         ('approval-2','company','task','task-2','start',1,'other','other','pending',0,1,1,1),\
         ('approval-4','company','task','task-4','start',1,'creator','assignee','pending',0,1,1,1);\
         INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority,state) VALUES \
         ('step-1-0','approval-1',0,'assignee','named','pending'),\
         ('step-4-0','approval-4',0,'assignee','named','pending');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}
async fn execute(
    pool: &DatabasePool,
    user: &str,
    task: &str,
    note: &str,
    version: i64,
    key: &str,
) -> Result<CommandOutcome, Error> {
    let input = serde_json::json!({"note": note, "expectedVersion": version});
    let operation = "task.cancel".to_owned();
    let task = task.to_owned();
    let key = key.to_owned();
    let user = user.to_owned();
    let input = input.to_string();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let user = user.clone();
            let command = WorkflowRequest {
                key: key.clone(),
                operation: operation.clone(),
                record_id: task.clone(),
                input: CommandJson::parse(&input).unwrap(),
            };
            Box::pin(async move {
                let mut authority = Authority { user };
                let mut eligibility = ParticipantCancelEligibility;
                execute_task_cancel(
                    CompanyId::new("company").unwrap(),
                    c,
                    &mut authority,
                    &Time,
                    &mut Ids,
                    &mut eligibility,
                    &command,
                )
                .await
            })
        })
        .await
}

#[tokio::test]
async fn task_cancel_closes_open_approvals_and_persists_decisions_notifications_and_replay() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let first = execute(
            &pool,
            "creator",
            "task-1",
            "No longer needed",
            3,
            "task:cancel:0001",
        )
        .await
        .unwrap();
        let result: serde_json::Value = serde_json::from_str(first.result.as_str()).unwrap();
        assert_eq!(
            result,
            serde_json::json!({"id":"task-1","state":"cancelled","version":4})
        );
        let task: (String, i64) =
            sqlx::query_as("SELECT state,version FROM staff_tasks WHERE id='task-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(task, ("cancelled".into(), 4));
        let instances: Vec<(String, i64)> = sqlx::query_as(
            "SELECT state,version FROM approval_instances WHERE record_id='task-1' ORDER BY id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            instances,
            vec![("cancelled".into(), 3), ("cancelled".into(), 2)]
        );
        let decisions: Vec<(String, i64)> = sqlx::query_as(
            "SELECT action,reviewed_version FROM approval_decisions \
             WHERE command_id='task:cancel:0001' ORDER BY instance_id",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(decisions, vec![("cancel".into(), 3), ("cancel".into(), 3)]);
        let notification: (String, i64) = sqlx::query_as(
            "SELECT user_id,record_version FROM staff_notifications \
             WHERE record_type='task' AND record_id='task-1' AND message='Task cancelled'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(notification, ("assignee".into(), 4));
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts),\
             (SELECT count(*) FROM audit_events),(SELECT count(*) FROM outbox_events)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1));
        let replay = execute(
            &pool,
            "creator",
            "task-1",
            "No longer needed",
            3,
            "task:cancel:0001",
        )
        .await
        .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.result, first.result);
        let decisions: i64 = sqlx::query_scalar("SELECT count(*) FROM approval_decisions")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(decisions, 2);
    })
    .await;
}

#[tokio::test]
async fn task_cancel_policy_failures_leave_no_receipt_or_decision() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let empty = execute(&pool, "creator", "task-4", "", 1, "task:cancel:empty")
            .await
            .err()
            .expect("a cancellation reason is required");
        assert_eq!(business(empty).code, "REASON_REQUIRED");
        let wrong = execute(
            &pool,
            "creator",
            "task-2",
            "Not mine",
            1,
            "task:cancel:wrong",
        )
        .await
        .err()
        .expect("only a participant may cancel");
        assert_eq!(business(wrong).code, "TASK_CANCEL_FORBIDDEN");
        let completed = execute(
            &pool,
            "creator",
            "task-3",
            "Already done",
            1,
            "task:cancel:completed",
        )
        .await
        .err()
        .expect("completed tasks cannot be cancelled");
        assert_eq!(business(completed).code, "TASK_NOT_CANCELLABLE");
        let stale = execute(
            &pool,
            "creator",
            "task-4",
            "Stale version",
            9,
            "task:cancel:stale",
        )
        .await
        .err()
        .expect("stale task version is rejected");
        assert_eq!(business(stale).code, "TASK_NOT_CANCELLABLE");
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id LIKE 'task:cancel:%'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 0);
        let decisions: i64 = sqlx::query_scalar("SELECT count(*) FROM approval_decisions")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(decisions, 0);
    })
    .await;
}
