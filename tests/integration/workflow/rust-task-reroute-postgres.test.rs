//! Real migrated Holding PostgreSQL for the native task reroute command.
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
use erpx_server_workflow::public::task_reroute::{
    TaskRerouteEligibility, TaskRerouteTaskRow, execute_task_reroute,
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
    approver_ok: bool,
}
impl TaskRerouteEligibility for Eligibility {
    async fn can_reroute(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &TaskRerouteTaskRow,
    ) -> Result<bool, Error> {
        Ok(self.allow)
    }
    async fn approver_eligible(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &str,
    ) -> Result<bool, Error> {
        Ok(self.approver_ok)
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
         ('task-2','company','Task 2','creator','assignee',1000,'completion_approval',1,1,1),\
         ('task-3','company','Task 3','creator','assignee',1000,'completed',1,1,1);\
         INSERT INTO approval_instances\
         (id,company_id,record_type,record_id,phase,cycle,requester_user_id,assignee_user_id,state,current_step,version,created_at,updated_at) VALUES \
         ('approval-1','company','task','task-1','start',1,'creator','assignee','pending',0,2,1,1),\
         ('approval-2','company','task','task-2','completion',1,'creator','assignee','pending',0,1,1,1),\
         ('approval-3','company','task','task-3','start',1,'creator','assignee','pending',0,1,1,1);\
         INSERT INTO approval_steps(id,instance_id,ordinal,approver_user_id,authority,state) VALUES \
         ('step-1-0','approval-1',0,'assignee','named','pending'),\
         ('step-2-0','approval-2',0,'assignee','named','pending'),\
         ('step-3-0','approval-3',0,'assignee','named','pending');\
         INSERT INTO flow_routes(company_id,kind,record_id,version,defaults_revision,steps_json,created_at) VALUES \
         ('company','staff_task','task-1',1,1,'{\"start_0\":{\"primaryUserId\":\"assignee\"}}',1);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}
async fn execute(
    pool: &DatabasePool,
    user: &str,
    instance: &str,
    approver: &str,
    reason: &str,
    task_version: i64,
    instance_version: i64,
    allow: bool,
    approver_ok: bool,
) -> Result<CommandOutcome, Error> {
    let input = serde_json::json!({
        "approverUserId": approver,
        "reason": reason,
        "expectedVersion": task_version,
        "instanceVersion": instance_version,
    });
    let operation = "task.approval.reroute".to_owned();
    let instance = instance.to_owned();
    let user = user.to_owned();
    let input = input.to_string();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |c| {
            let user = user.clone();
            let command = WorkflowRequest {
                key: "task:reroute:0001".into(),
                operation: operation.clone(),
                record_id: instance.clone(),
                input: CommandJson::parse(&input).unwrap(),
            };
            Box::pin(async move {
                let mut authority = Authority { user };
                let mut eligibility = Eligibility { allow, approver_ok };
                execute_task_reroute(
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
async fn task_reroute_replaces_the_pending_approver_and_persists_flow_decision_and_notification() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let first = execute(
            &pool,
            "creator",
            "approval-1",
            "new-approver",
            "Current reviewer is unavailable",
            3,
            2,
            true,
            true,
        )
        .await
        .unwrap();
        let result: serde_json::Value = serde_json::from_str(first.result.as_str()).unwrap();
        assert_eq!(
            result,
            serde_json::json!({"id":"task-1","instanceId":"approval-1","version":4})
        );
        let step: String =
            sqlx::query_scalar("SELECT approver_user_id FROM approval_steps WHERE id='step-1-0'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(step, "new-approver");
        let instance: (i64, String) = sqlx::query_as(
            "SELECT version,state FROM approval_instances WHERE id='approval-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(instance, (3, "pending".into()));
        let task: i64 = sqlx::query_scalar("SELECT version FROM staff_tasks WHERE id='task-1'")
            .fetch_one(pool.raw())
            .await
            .unwrap();
        assert_eq!(task, 4);
        let decision: (String, String, String, i64) = sqlx::query_as(
            "SELECT action,previous_approver_user_id,approver_user_id,reviewed_version \
             FROM approval_decisions WHERE command_id='task:reroute:0001'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(
            decision,
            (
                "reroute".into(),
                "assignee".into(),
                "new-approver".into(),
                3
            )
        );
        let route: String = sqlx::query_scalar(
            "SELECT steps_json FROM flow_routes WHERE company_id='company' \
             AND kind='staff_task' AND record_id='task-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert!(route.contains("new-approver"));
        let notification: (String, i64) = sqlx::query_as(
            "SELECT user_id,record_version FROM staff_notifications \
             WHERE record_type='task' AND record_id='task-1' AND message='Approval rerouted to you'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(notification, ("new-approver".into(), 4));
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
            "approval-1",
            "new-approver",
            "Current reviewer is unavailable",
            3,
            2,
            true,
            true,
        )
        .await
        .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.result, first.result);
    })
    .await;
}

#[tokio::test]
async fn task_reroute_policy_failures_leave_no_receipt_or_decision() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let bad_reason = execute(
            &pool,
            "creator",
            "approval-1",
            "new-approver",
            "x",
            3,
            2,
            true,
            true,
        )
        .await
        .err()
        .expect("reason bounds are enforced");
        assert_eq!(business(bad_reason).code, "TASK_REROUTE_INPUT_INVALID");
        let self_assignee = execute(
            &pool,
            "creator",
            "approval-1",
            "assignee",
            "Self approval",
            3,
            2,
            true,
            true,
        )
        .await
        .err()
        .expect("assignee cannot approve");
        assert_eq!(business(self_assignee).code, "APPROVER_INELIGIBLE");
        let self_requester = execute(
            &pool,
            "creator",
            "approval-1",
            "creator",
            "Requester cannot reroute to self",
            3,
            2,
            true,
            true,
        )
        .await
        .err()
        .expect("requester cannot approve at start");
        assert_eq!(business(self_requester).code, "APPROVER_INELIGIBLE");
        let denied = execute(
            &pool,
            "creator",
            "approval-1",
            "new-approver",
            "Not authorized",
            3,
            2,
            false,
            true,
        )
        .await
        .err()
        .expect("manager authority is required");
        assert_eq!(business(denied).code, "TASK_REROUTE_FORBIDDEN");
        let ineligible = execute(
            &pool,
            "creator",
            "approval-1",
            "new-approver",
            "Inactive approver",
            3,
            2,
            true,
            false,
        )
        .await
        .err()
        .expect("approver must be eligible");
        assert_eq!(business(ineligible).code, "APPROVER_INELIGIBLE");
        let stale_instance = execute(
            &pool,
            "creator",
            "approval-1",
            "new-approver",
            "Stale instance",
            3,
            9,
            true,
            true,
        )
        .await
        .err()
        .expect("stale instance version is rejected");
        assert_eq!(business(stale_instance).code, "TASK_NOT_REROUTABLE");
        let stale_task = execute(
            &pool,
            "creator",
            "approval-1",
            "new-approver",
            "Stale task",
            9,
            2,
            true,
            true,
        )
        .await
        .err()
        .expect("stale task version is rejected");
        assert_eq!(business(stale_task).code, "TASK_NOT_REROUTABLE");
        let completed = execute(
            &pool,
            "creator",
            "approval-3",
            "new-approver",
            "Completed task",
            1,
            1,
            true,
            true,
        )
        .await
        .err()
        .expect("completed task cannot be rerouted");
        assert_eq!(business(completed).code, "TASK_NOT_REROUTABLE");
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id='task:reroute:0001'",
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
