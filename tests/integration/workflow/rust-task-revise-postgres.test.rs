//! Real migrated Holding PostgreSQL for the native draft task revision command.
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
use erpx_server_workflow::public::task_revise::{
    TaskReviseEligibility, TaskReviseTaskRow, execute_task_revise,
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
struct Eligibility {
    allow: bool,
}
impl TaskReviseEligibility for Eligibility {
    async fn can_revise(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &TaskReviseTaskRow,
    ) -> Result<bool, Error> {
        Ok(self.allow)
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
         (id,company_id,title,description,creator_user_id,assignee_user_id,due_at,state,version,created_at,updated_at) VALUES \
         ('task-1','company','Draft task','','creator','assignee',NULL,'draft',1,1,1),\
         ('task-2','company','Started task','','creator','assignee',NULL,'start_approval',1,1,1),\
         ('task-3','company','Ordinary draft','','creator','assignee',NULL,'draft',1,1,1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}
fn input(
    title: &str,
    approvers: &[&str],
    due_at: Option<i64>,
    reason: &str,
    version: i64,
) -> String {
    serde_json::json!({
        "title": title,
        "description": "Revised description",
        "assigneeUserId": "assignee",
        "projectId": null,
        "startApprovers": approvers,
        "completionApprovers": [],
        "dueAt": due_at,
        "expectedVersion": version,
        "reason": reason,
    })
    .to_string()
}
async fn execute(
    pool: &DatabasePool,
    user: &str,
    task: &str,
    payload: &str,
    allow: bool,
) -> Result<CommandOutcome, Error> {
    let operation = "task.revise".to_owned();
    let task = task.to_owned();
    let key = format!("task:revise:{task}");
    let user = user.to_owned();
    let payload = payload.to_owned();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let user = user.clone();
            let command = WorkflowRequest {
                key: key.clone(),
                operation: operation.clone(),
                record_id: task.clone(),
                input: CommandJson::parse(&payload).unwrap(),
            };
            Box::pin(async move {
                let mut authority = Authority { user };
                let mut eligibility = Eligibility { allow };
                execute_task_revise(
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
async fn task_revise_updates_draft_configuration_and_persists_receipt_audit_and_outbox() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let first = execute(
            &pool,
            "creator",
            "task-1",
            &input(
                "Revised task",
                &["approver"],
                Some(1_893_463_200_000),
                "Material revision",
                1,
            ),
            true,
        )
        .await
        .unwrap();
        let result: serde_json::Value = serde_json::from_str(first.result.as_str()).unwrap();
        assert_eq!(
            result,
            serde_json::json!({"id":"task-1","state":"draft","version":2})
        );
        let row: (String, String, String, Option<i64>, String, i64) = sqlx::query_as(
            "SELECT title,description,start_approvers_json,due_at,state,version \
             FROM staff_tasks WHERE id='task-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row.0, "Revised task");
        assert_eq!(row.1, "Revised description");
        assert!(row.2.contains("approver"));
        assert_eq!(row.3, Some(1_893_463_200_000));
        assert_eq!(row.4, "draft");
        assert_eq!(row.5, 2);
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
            &input(
                "Revised task",
                &["approver"],
                Some(1_893_463_200_000),
                "Material revision",
                1,
            ),
            true,
        )
        .await
        .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.result, first.result);

        let ordinary = execute(
            &pool,
            "creator",
            "task-3",
            &input("Ordinary revised", &[], None, "", 1),
            true,
        )
        .await
        .unwrap();
        let result: serde_json::Value = serde_json::from_str(ordinary.result.as_str()).unwrap();
        assert_eq!(
            result,
            serde_json::json!({"id":"task-3","state":"ready","version":2})
        );
    })
    .await;
}

#[tokio::test]
async fn task_revise_policy_failures_leave_no_receipt() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let non_draft = execute(
            &pool,
            "creator",
            "task-2",
            &input(
                "Started",
                &["approver"],
                Some(1_893_463_200_000),
                "Reason",
                1,
            ),
            true,
        )
        .await
        .err()
        .expect("non-draft tasks require approval restart");
        assert_eq!(
            business(non_draft).code,
            "TASK_REVISION_REQUIRES_APPROVAL_RESTART"
        );
        let stale = execute(
            &pool,
            "creator",
            "task-1",
            &input("Stale", &["approver"], Some(1_893_463_200_000), "Reason", 9),
            true,
        )
        .await
        .err()
        .expect("stale version is rejected");
        assert_eq!(
            business(stale).code,
            "TASK_REVISION_REQUIRES_APPROVAL_RESTART"
        );
        let self_approval = execute(
            &pool,
            "creator",
            "task-1",
            &input("Self", &["assignee"], Some(1_893_463_200_000), "Reason", 1),
            true,
        )
        .await
        .err()
        .expect("assignee cannot approve");
        assert_eq!(business(self_approval).code, "SELF_APPROVAL");
        let creator_approval = execute(
            &pool,
            "creator",
            "task-1",
            &input(
                "Creator",
                &["creator"],
                Some(1_893_463_200_000),
                "Reason",
                1,
            ),
            true,
        )
        .await
        .err()
        .expect("creator cannot approve start");
        assert_eq!(business(creator_approval).code, "SELF_APPROVAL");
        let no_due = execute(
            &pool,
            "creator",
            "task-1",
            &input("No due", &["approver"], None, "Reason", 1),
            true,
        )
        .await
        .err()
        .expect("reviewed work needs a due date");
        assert_eq!(business(no_due).code, "TASK_DUE_REQUIRED");
        let no_reason = execute(
            &pool,
            "creator",
            "task-1",
            &input("No reason", &["approver"], Some(1_893_463_200_000), "", 1),
            true,
        )
        .await
        .err()
        .expect("material revision needs a reason");
        assert_eq!(business(no_reason).code, "REVISION_REASON_REQUIRED");
        let unauthorized = execute(
            &pool,
            "other",
            "task-1",
            &input("Other", &[], None, "", 1),
            false,
        )
        .await
        .err()
        .expect("only the creator manages configuration");
        assert_eq!(business(unauthorized).code, "TASK_CREATOR_REQUIRED");
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id='task:revise:0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(receipts, 0);
    })
    .await;
}
