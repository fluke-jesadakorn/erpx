//! Real migrated Holding PostgreSQL for the first native task decision command.
//! Current authority is an explicitly synthetic test port; the command still
//! locks the real task/approval rows and writes the real receipt, decision,
//! audit, outbox and notification effects.
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
use erpx_server_workflow::public::task_decision::{NamedTaskEligibility, execute_task_decision};
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
    deny: bool,
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
        if self.deny {
            return Err(BusinessError::invalid(
                "TEST_CURRENT_AUTHORITY_DENIED",
                "Synthetic current permission denied.",
            )
            .into());
        }
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
         ('task-1','company','Task 1','creator','assignee',1000,'start_approval',4,1,1),\
         ('task-2','company','Task 2','creator','assignee',1000,'start_approval',1,1,1),\
         ('task-3','company','Task 3','creator','approver',1000,'start_approval',1,1,1),\
         ('task-4','company','Task 4','creator','assignee',1000,'start_approval',1,1,1);\
         INSERT INTO approval_instances\
         (id,company_id,record_type,record_id,phase,cycle,requester_user_id,assignee_user_id,state,current_step,version,created_at,updated_at) VALUES \
         ('approval-1','company','task','task-1','start',1,'creator','assignee','pending',0,2,1,1),\
         ('approval-2','company','task','task-2','start',1,'creator','assignee','pending',0,1,1,1),\
         ('approval-3','company','task','task-3','start',1,'creator','approver','pending',0,1,1,1),\
         ('approval-4','company','task','task-4','start',1,'creator','assignee','pending',0,1,1,1);\
         INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority,state) VALUES \
         ('step-1-0','approval-1',0,'approver','named','pending'),\
         ('step-1-1','approval-1',1,'next-approver','named','pending'),\
         ('step-2-0','approval-2',0,'approver','named','pending'),\
         ('step-3-0','approval-3',0,'approver','named','pending'),\
         ('step-4-0','approval-4',0,'approver','named','pending');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn execute(
    pool: &DatabasePool,
    user: &str,
    task: &str,
    action: &str,
    note: &str,
    version: i64,
    key: &str,
) -> Result<CommandOutcome, Error> {
    let input = serde_json::json!({
        "action": action,
        "note": note,
        "expectedVersion": version,
    });
    let input = input.to_string();
    let operation = format!("task.{action}");
    let task = task.to_owned();
    let key = key.to_owned();
    let user = user.to_owned();
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
                let mut authority = Authority { user, deny: false };
                let mut eligibility = NamedTaskEligibility;
                execute_task_decision(
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
async fn two_step_task_approval_persists_receipt_decision_audit_outbox_and_notifications() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let first = execute(
            &pool,
            "approver",
            "task-1",
            "approve",
            "",
            4,
            "task:approve:0001",
        )
        .await
        .unwrap();
        assert!(!first.replayed);
        let result: serde_json::Value = serde_json::from_str(first.result.as_str()).unwrap();
        assert_eq!(
            result,
            serde_json::json!({"id":"task-1","state":"start_approval","version":5})
        );
        let steps: Vec<(String, String)> = sqlx::query_as(
            "SELECT id,state FROM approval_steps WHERE instance_id='approval-1' ORDER BY ordinal",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert_eq!(steps[0].1, "approved");
        assert_eq!(steps[1].1, "pending");
        let instance: (String, i64, i64) = sqlx::query_as(
            "SELECT state,current_step,version FROM approval_instances WHERE id='approval-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(instance, ("pending".into(), 1, 3));
        let task: (String, i64) =
            sqlx::query_as("SELECT state,version FROM staff_tasks WHERE id='task-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(task, ("start_approval".into(), 5));
        let decision: (String, String, i64) = sqlx::query_as(
            "SELECT action,approver_user_id,reviewed_version FROM approval_decisions \
             WHERE command_id='task:approve:0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(decision, ("approve".into(), "approver".into(), 4));
        let messages: Vec<String> = sqlx::query_scalar(
            "SELECT message FROM staff_notifications WHERE record_type='task' \
             AND record_id='task-1' AND record_version=5 ORDER BY message",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert!(messages.contains(&"Approval needs your decision".into()));
        assert!(messages.contains(&"Step approved; awaiting the next decision".into()));
        assert!(messages.contains(&"Task start approval".into()));
        let counts: (i64, i64, i64) = sqlx::query_as(
            "SELECT (SELECT count(*) FROM command_receipts),\
             (SELECT count(*) FROM audit_events),(SELECT count(*) FROM outbox_events)",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(counts, (1, 1, 1));
        let payload: String = sqlx::query_scalar(
            "SELECT payload_json FROM outbox_events WHERE command_id='task:approve:0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let payload: serde_json::Value = serde_json::from_str(&payload).unwrap();
        assert_eq!(payload["state"], "start_approval");

        let replay = execute(
            &pool,
            "approver",
            "task-1",
            "approve",
            "",
            4,
            "task:approve:0001",
        )
        .await
        .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.result, first.result);
        let decisions: i64 = sqlx::query_scalar("SELECT count(*) FROM approval_decisions")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(decisions, 1);
        let conflict = execute(
            &pool,
            "approver",
            "task-1",
            "approve",
            "different",
            4,
            "task:approve:0001",
        )
        .await
        .err()
        .expect("expected a policy failure");
        assert_eq!(business(conflict).code, "IDEMPOTENCY_CONFLICT");

        let second = execute(
            &pool,
            "next-approver",
            "task-1",
            "approve",
            "",
            5,
            "task:approve:0002",
        )
        .await
        .unwrap();
        let result: serde_json::Value = serde_json::from_str(second.result.as_str()).unwrap();
        assert_eq!(
            result,
            serde_json::json!({"id":"task-1","state":"ready","version":6})
        );
        let instance: (String, i64, i64) = sqlx::query_as(
            "SELECT state,current_step,version FROM approval_instances WHERE id='approval-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(instance, ("approved".into(), 1, 4));
        let task: (String, i64) =
            sqlx::query_as("SELECT state,version FROM staff_tasks WHERE id='task-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(task, ("ready".into(), 6));
        let messages: Vec<String> = sqlx::query_scalar(
            "SELECT message FROM staff_notifications WHERE record_type='task' \
             AND record_id='task-1' AND record_version=6 ORDER BY message",
        )
        .fetch_all(pool.raw())
        .await
        .unwrap();
        assert!(messages.contains(&"Approval complete".into()));
        assert!(messages.contains(&"Task ready".into()));
    })
    .await;
}

#[tokio::test]
async fn task_reject_and_policy_failures_leave_no_receipt_or_decision() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let reject = execute(
            &pool,
            "approver",
            "task-2",
            "reject",
            "Needs correction",
            1,
            "task:reject:0001",
        )
        .await
        .unwrap();
        let result: serde_json::Value = serde_json::from_str(reject.result.as_str()).unwrap();
        assert_eq!(
            result,
            serde_json::json!({"id":"task-2","state":"draft","version":2})
        );
        let instance: (String, i64) =
            sqlx::query_as("SELECT state,version FROM approval_instances WHERE id='approval-2'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(instance, ("rejected".into(), 2));
        let decision: (String, String, i64) = sqlx::query_as(
            "SELECT action,approver_user_id,reviewed_version FROM approval_decisions \
             WHERE command_id='task:reject:0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(decision, ("reject".into(), "approver".into(), 1));

        let empty = execute(
            &pool,
            "approver",
            "task-2",
            "reject",
            "",
            2,
            "task:reject:empty",
        )
        .await
        .err()
        .expect("expected a policy failure");
        assert_eq!(business(empty).code, "REASON_REQUIRED");
        let wrong = execute(
            &pool,
            "other",
            "task-3",
            "approve",
            "",
            1,
            "task:approve:wrong",
        )
        .await
        .err()
        .expect("expected a policy failure");
        assert_eq!(business(wrong).code, "CURRENT_APPROVER_REQUIRED");
        let self_approval = execute(
            &pool,
            "approver",
            "task-3",
            "approve",
            "",
            1,
            "task:approve:self",
        )
        .await
        .err()
        .expect("expected a policy failure");
        assert_eq!(business(self_approval).code, "CURRENT_APPROVER_REQUIRED");
        let stale = execute(
            &pool,
            "approver",
            "task-4",
            "approve",
            "",
            99,
            "task:approve:stale",
        )
        .await
        .err()
        .expect("expected a policy failure");
        assert_eq!(business(stale).code, "TASK_DECISION_STATE_INVALID");

        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id IN \
             ('task:reject:empty','task:approve:wrong','task:approve:self','task:approve:stale')",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 0);
        let decisions: i64 = sqlx::query_scalar("SELECT count(*) FROM approval_decisions")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(decisions, 1);
        let task: (String, i64) =
            sqlx::query_as("SELECT state,version FROM staff_tasks WHERE id='task-3'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(task, ("start_approval".into(), 1));
    })
    .await;
}
