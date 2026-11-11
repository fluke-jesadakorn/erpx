//! Actual Staff Work owner commands and bounded reads over migrated PostgreSQL.
#![allow(dead_code)]
#[path = "../../support/operations/rust-operations-database.rs"]
mod fixture;
use erpx_server_operations::public::staff_tasks::{
    AlertPlan, PgStaffTasks, SaveTaskCommand, SaveTaskPlan, StaffTaskActor, StaffTaskRepository,
    TaskReceipt, TransitionTaskCommand, TransitionTaskPlan, lock_staff_task_sources,
    plan_transition,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::CommandKey;
use std::sync::atomic::{AtomicI64, Ordering};

static NOW: AtomicI64 = AtomicI64::new(1_790_000_000_000);

async fn migrate(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
}

async fn serializable(pool: &DatabasePool) -> sqlx::Transaction<'static, sqlx::Postgres> {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

fn actor() -> StaffTaskActor {
    StaffTaskActor {
        user_id: "creator".into(),
        assignment_id: "assignment-creator".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        permissions: vec!["records.read".into(), "records.write".into()],
        modules: vec!["operations".into()],
        expires_at: 9_999_999_999_999,
    }
}

fn save_plan(key: &str, now: i64) -> SaveTaskPlan {
    let input = SaveTaskCommand {
        title: "Inspect deliveries".into(),
        description: "Check the receiving log".into(),
        assignee_user_id: "worker".into(),
        project_id: None,
        start_approvers: Vec::new(),
        completion_approvers: Vec::new(),
        due_at: Some("2026-10-01T02:00:00Z".into()),
        expected_version: 1,
        reason: String::new(),
    };
    SaveTaskPlan {
        key: CommandKey::new(key).unwrap(),
        operation: "task.create".into(),
        record_id: "task-1".into(),
        company_id: "company".into(),
        branch_id: Some("branch".into()),
        department_id: Some("department".into()),
        actor_id: "creator".into(),
        assignment_id: "assignment-creator".into(),
        input,
        due_at: Some(1_790_820_000_000),
        state: "ready".into(),
        version: 1,
        revision: false,
        now,
        request_fingerprint: "task.create\ntask-1\n{}".into(),
        receipt: TaskReceipt::new("task-1", "ready", 1),
    }
}

#[tokio::test]
async fn creates_transitions_and_replays_staff_tasks_with_a_ledger() {
    fixture::with_database(2, |holding| async move {
        migrate(&holding).await;
        let now = NOW.load(Ordering::SeqCst);
        let mut tx = serializable(&holding).await;
        lock_staff_task_sources(&mut tx).await.unwrap();
        let mut repository = PgStaffTasks::new(&mut tx);
        let created = repository.save(save_plan("key-create", now)).await.unwrap();
        assert_eq!(created, TaskReceipt::new("task-1", "ready", 1));

        let stored = repository
            .stored_receipt("key-create")
            .await
            .unwrap()
            .unwrap();
        assert_eq!(stored.operation, "task.create");
        assert_eq!(stored.company_id, "company");
        assert_eq!(stored.branch_id.as_deref(), Some("branch"));
        assert_eq!(stored.actor_id, "creator");

        let row = repository
            .task(&actor(), "task-1", now)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.state, "ready");
        assert_eq!(row.version, 1);
        assert_eq!(row.due_at, Some(1_790_820_000_000));

        let input = TransitionTaskCommand {
            action: erpx_server_operations::public::staff_tasks::TaskAction::Start,
            expected_version: 1,
            note: String::new(),
            attachment_set_ids: Vec::new(),
        };
        let facts = erpx_server_operations::public::staff_tasks::TaskAccessFacts {
            same_assignee: true,
            ..Default::default()
        };
        let plan = plan_transition(&row, &input, &facts, None, None).unwrap();
        let transitioned = repository
            .transition(TransitionTaskPlan {
                key: CommandKey::new("key-start").unwrap(),
                operation: "task.start".into(),
                record_id: "task-1".into(),
                company_id: "company".into(),
                branch_id: row.branch_id.clone(),
                department_id: row.department_id.clone(),
                actor_id: "creator".into(),
                assignment_id: "assignment-creator".into(),
                input,
                row,
                plan,
                now,
                request_fingerprint: "task.start\ntask-1\n{}".into(),
                receipt: TaskReceipt::new("task-1", "in_progress", 2),
            })
            .await
            .unwrap();
        assert_eq!(transitioned, TaskReceipt::new("task-1", "in_progress", 2));
        let row = repository
            .task(&actor(), "task-1", now)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(row.state, "in_progress");
        assert_eq!(row.version, 2);
        let tasks = repository.list(&actor(), None).await.unwrap();
        assert_eq!(tasks.len(), 1);
        assert_eq!(tasks[0].id, "task-1");
        drop(repository);

        let audit: i64 =
            sqlx::query_scalar("SELECT count(*) FROM audit_events WHERE record_id='task-1'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        let outbox: i64 =
            sqlx::query_scalar("SELECT count(*) FROM outbox_events WHERE aggregate_id='task-1'")
                .fetch_one(&mut *tx)
                .await
                .unwrap();
        assert_eq!((audit, outbox), (2, 2));
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn rejects_a_replayed_key_with_a_different_fingerprint() {
    fixture::with_database(1, |holding| async move {
        migrate(&holding).await;
        let now = NOW.load(Ordering::SeqCst);
        let mut tx = serializable(&holding).await;
        lock_staff_task_sources(&mut tx).await.unwrap();
        let mut repository = PgStaffTasks::new(&mut tx);
        repository.save(save_plan("key-create", now)).await.unwrap();
        let mut retry = save_plan("key-create", now);
        retry.request_fingerprint = "task.create\ntask-1\n{\"title\":\"other\"}".into();
        let error = repository.save(retry).await.unwrap_err();
        assert!(
            matches!(
                error,
                erpx_server_runtime::public::postgres::PgTransactionError::Business(
                    erpx_server_shared::public::BusinessError {
                        code: "IDEMPOTENCY_CONFLICT",
                        ..
                    }
                )
            ),
            "a key may never commit a second request"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn alerts_are_deduplicated_by_owner_notification_identity() {
    fixture::with_database(1, |holding| async move {
        migrate(&holding).await;
        let now = NOW.load(Ordering::SeqCst);
        sqlx::raw_sql(
            "INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,creator_user_id,\
             assignee_user_id,due_at,state,version,created_at,updated_at) VALUES\
             ('task-alert','company','branch','department','Review receiving exception','creator',\
              'assignee',1790000000000,'ready',1,0,0);",
        )
        .execute(holding.raw())
        .await
        .unwrap();
        let mut tx = serializable(&holding).await;
        lock_staff_task_sources(&mut tx).await.unwrap();
        let mut repository = PgStaffTasks::new(&mut tx);
        let rows = repository.alert_rows(now, 86_400_000).await.unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].id, "task-alert");
        let plan = AlertPlan {
            company_id: rows[0].company_id.clone(),
            task_id: rows[0].id.clone(),
            user_id: rows[0].assignee_user_id.clone(),
            message: "Task due within 24 hours: Review receiving exception.".into(),
            phase: "due-soon",
            version: rows[0].version,
            now,
        };
        assert!(repository.stage_alert(plan.clone()).await.unwrap());
        assert!(
            !repository.stage_alert(plan).await.unwrap(),
            "the same task/version/user alert is emitted once"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}
