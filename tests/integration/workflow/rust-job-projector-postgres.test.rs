//! Actual native Workflow projection over the retained read/write split.
//!
//! The owner facts live in the writer database and the receipt/work/checkpoint
//! statements run on the caller's read transaction, exactly like the retained
//! TypeScript composition. The projector never commits independently.
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use erpx_server_workflow::public::job_projector::{
    EventMessage, EventPayload, ProjectionOwnerSources, ProjectionResult, ProjectionSource,
    ProjectionSourceFuture, project_event,
};
use sqlx::{PgConnection, Row};

const TASK_SQL: &str =
    "SELECT state,version,company_id,branch_id,department_id FROM staff_tasks WHERE id=$1";
const LEAVE_SQL: &str =
    "SELECT state,version,company_id,branch_id,department_id FROM leave_requests WHERE id=$1";
const OVERTIME_SQL: &str =
    "SELECT state,version,company_id,branch_id,department_id FROM overtime_requests WHERE id=$1";
const PAYROLL_SQL: &str = "SELECT status state,version,company_id,NULL branch_id,NULL department_id \
     FROM hr_payroll_runs WHERE id=$1";

async fn migrate(pool: &DatabasePool, track: MigrationTrack) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let migrations = load_migrations(&root, track).await.unwrap();
    migrate_database(pool, track, &migrations).await.unwrap();
}

fn read_failed(_error: sqlx::Error) -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TEST_SOURCE_READ_FAILED",
        "The test owner source could not be read.",
    )
}

async fn read_source(
    connection: &mut PgConnection,
    sql: &str,
    id: &str,
) -> Result<Option<ProjectionSource>, BusinessError> {
    let Some(row) = sqlx::query(sqlx::AssertSqlSafe(sql))
        .bind(id)
        .fetch_optional(connection)
        .await
        .map_err(read_failed)?
    else {
        return Ok(None);
    };
    Ok(Some(ProjectionSource {
        state: row.try_get("state").map_err(read_failed)?,
        version: row.try_get("version").map_err(read_failed)?,
        company_id: row.try_get("company_id").map_err(read_failed)?,
        branch_id: row.try_get("branch_id").map_err(read_failed)?,
        department_id: row.try_get("department_id").map_err(read_failed)?,
    }))
}

/// Composition-shaped source implementation over the selected Holding writer.
struct WriterSources<'a> {
    connection: &'a mut PgConnection,
    calls: usize,
}

impl ProjectionOwnerSources for WriterSources<'_> {
    fn task<'a>(&'a mut self, id: &'a str) -> ProjectionSourceFuture<'a> {
        self.calls += 1;
        let connection = &mut *self.connection;
        Box::pin(async move { read_source(connection, TASK_SQL, id).await })
    }

    fn leave<'a>(&'a mut self, id: &'a str) -> ProjectionSourceFuture<'a> {
        self.calls += 1;
        let connection = &mut *self.connection;
        Box::pin(async move { read_source(connection, LEAVE_SQL, id).await })
    }

    fn overtime<'a>(&'a mut self, id: &'a str) -> ProjectionSourceFuture<'a> {
        self.calls += 1;
        let connection = &mut *self.connection;
        Box::pin(async move { read_source(connection, OVERTIME_SQL, id).await })
    }

    fn payroll<'a>(&'a mut self, id: &'a str) -> ProjectionSourceFuture<'a> {
        self.calls += 1;
        let connection = &mut *self.connection;
        Box::pin(async move { read_source(connection, PAYROLL_SQL, id).await })
    }
}

fn message(
    id: &str,
    event_type: &str,
    aggregate_id: &str,
    aggregate_version: i64,
    state: Option<&str>,
    company_id: &str,
) -> EventMessage {
    EventMessage {
        holding_id: "holding".into(),
        id: id.into(),
        event_type: event_type.into(),
        aggregate_id: aggregate_id.into(),
        aggregate_version,
        payload: EventPayload {
            id: aggregate_id.into(),
            state: state.map(str::to_owned),
            version: aggregate_version,
            company_id: company_id.into(),
            branch_id: Some("payload-branch".into()),
            department_id: None,
        },
    }
}

async fn counts(connection: &mut PgConnection) -> (i64, i64, i64) {
    let receipts = sqlx::query_scalar("SELECT count(*) FROM projection_receipts")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let work = sqlx::query_scalar("SELECT count(*) FROM work_items")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    let checkpoints = sqlx::query_scalar("SELECT count(*) FROM projection_checkpoints")
        .fetch_one(&mut *connection)
        .await
        .unwrap();
    (receipts, work, checkpoints)
}

async fn seed_task(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO staff_tasks(id,company_id,branch_id,department_id,title,creator_user_id,\
         assignee_user_id,due_at,state,version,created_at,updated_at) VALUES\
         ('task-1','fresh-company','fresh-branch',NULL,'Inspect deliveries','creator','assignee',\
          1790000000000,'completed',8,0,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

async fn seed_payroll(pool: &DatabasePool) {
    sqlx::raw_sql(
        "INSERT INTO hr_pay_groups(id,company_id,name,country_code,currency,frequency,created_by,created_at,updated_at) VALUES\
         ('group','payroll-company','Monthly','TH','THB','monthly','fixture',0,0);
         INSERT INTO hr_payroll_runs(id,company_id,pay_group_id,period_start,period_end,pay_date,\
         currency,mode,status,revision,version,created_by,created_at,updated_at) VALUES\
         ('payroll-1','payroll-company','group','2026-01-01','2026-01-31','2026-01-31','THB',\
          'preview','posted',1,6,'creator',0,0);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn projects_task_refresh_and_work_upsert_ordering_with_a_checkpoint() {
    fixture::with_database(2, |write| async move {
        migrate(&write, MigrationTrack::HoldingWrite).await;
        seed_task(&write).await;
        fixture::with_database(2, |read| async move {
            migrate(&read, MigrationTrack::HoldingRead).await;
            let now = 1_790_000_000_000_i64;
            let mut write_connection = write.raw().acquire().await.unwrap();
            let mut sources = WriterSources {
                connection: &mut write_connection,
                calls: 0,
            };
            let mut tx = read.raw().begin().await.unwrap();

            let task = message(
                "event-task",
                "task.start",
                "task-1",
                2,
                Some("ready"),
                "stale-company",
            );
            assert_eq!(
                project_event(&mut tx, &mut sources, &task, now)
                    .await
                    .unwrap(),
                ProjectionResult::Projected
            );
            // The task family always re-reads the owner instead of trusting the
            // delayed event snapshot.
            assert_eq!(sources.calls, 1);

            let receipt: (String, i64) = sqlx::query_as(
                "SELECT aggregate_id,aggregate_version FROM projection_receipts \
                 WHERE event_id='event-task'",
            )
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            assert_eq!(receipt, ("task-1".into(), 2));

            let work = sqlx::query(
                "SELECT id,source_type,source_id,source_version,title,subtitle,queue,company_id,\
                 branch_id,department_id,updated_at FROM work_items \
                 WHERE source_type='staff_task' AND source_id='task-1'",
            )
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            assert_eq!(work.get::<String, _>("id"), "work_task-1");
            assert_eq!(work.get::<i64, _>("source_version"), 8);
            assert_eq!(work.get::<String, _>("title"), "Staff task");
            assert_eq!(work.get::<String, _>("subtitle"), "completed");
            assert_eq!(work.get::<String, _>("queue"), "completed");
            assert_eq!(work.get::<String, _>("company_id"), "fresh-company");
            assert_eq!(
                work.get::<Option<String>, _>("branch_id").as_deref(),
                Some("fresh-branch")
            );
            assert_eq!(work.get::<Option<String>, _>("department_id"), None);
            assert_eq!(work.get::<i64, _>("updated_at"), now);

            // Replaying the durable event is receipt-idempotent.
            assert_eq!(
                project_event(&mut tx, &mut sources, &task, now)
                    .await
                    .unwrap(),
                ProjectionResult::Projected
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM projection_receipts WHERE event_id='event-task'",
                )
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
                1
            );
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM work_items WHERE source_id='task-1'",
                )
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
                1
            );

            // Explicit People state does not re-read the owner.
            let leave_new = message(
                "event-leave-9",
                "leave.submit",
                "leave-1",
                9,
                Some("approved"),
                "leave-company",
            );
            assert_eq!(
                project_event(&mut tx, &mut sources, &leave_new, now)
                    .await
                    .unwrap(),
                ProjectionResult::Projected
            );
            // The task was projected twice (initial + replay), so it re-read
            // exactly twice; explicit leave state adds no owner read.
            assert_eq!(sources.calls, 2);

            // A delayed lower-version event writes its receipt but cannot
            // downgrade the current work item.
            let leave_old = message(
                "event-leave-5",
                "leave.submit",
                "leave-1",
                5,
                Some("pending_approval"),
                "leave-company",
            );
            assert_eq!(
                project_event(&mut tx, &mut sources, &leave_old, now)
                    .await
                    .unwrap(),
                ProjectionResult::Projected
            );
            let leave = sqlx::query(
                "SELECT source_version,title,subtitle,queue FROM work_items \
                 WHERE source_type='leave_request' AND source_id='leave-1'",
            )
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            assert_eq!(leave.get::<i64, _>("source_version"), 9);
            assert_eq!(leave.get::<String, _>("title"), "Leave request");
            assert_eq!(leave.get::<String, _>("subtitle"), "approved");
            assert_eq!(leave.get::<String, _>("queue"), "completed");
            assert_eq!(
                sqlx::query_scalar::<_, i64>(
                    "SELECT count(*) FROM projection_receipts WHERE event_id='event-leave-5'",
                )
                .fetch_one(&mut *tx)
                .await
                .unwrap(),
                1
            );

            // A newer event updates the work item.
            let leave_newer = message(
                "event-leave-10",
                "leave.submit",
                "leave-1",
                10,
                Some("pending_approval"),
                "leave-company",
            );
            assert_eq!(
                project_event(&mut tx, &mut sources, &leave_newer, now)
                    .await
                    .unwrap(),
                ProjectionResult::Projected
            );
            let leave = sqlx::query(
                "SELECT source_version,subtitle,queue FROM work_items \
                 WHERE source_type='leave_request' AND source_id='leave-1'",
            )
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            assert_eq!(leave.get::<i64, _>("source_version"), 10);
            assert_eq!(leave.get::<String, _>("subtitle"), "pending_approval");
            assert_eq!(leave.get::<String, _>("queue"), "todo");

            let checkpoint: (String, i64, i64) = sqlx::query_as(
                "SELECT last_event_id,last_event_at,updated_at FROM projection_checkpoints \
                 WHERE projector='work-items'",
            )
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            assert_eq!(checkpoint, ("event-leave-10".into(), now, now));

            // The adapter never commits independently: rollback removes every
            // receipt, work item and checkpoint.
            tx.rollback().await.unwrap();
            let mut read_connection = read.raw().acquire().await.unwrap();
            assert_eq!(counts(&mut read_connection).await, (0, 0, 0));
        })
        .await;
    })
    .await;
}

#[tokio::test]
async fn leaves_unhandled_families_unacknowledged_and_rejects_missing_sources() {
    fixture::with_database(2, |write| async move {
        migrate(&write, MigrationTrack::HoldingWrite).await;
        seed_payroll(&write).await;
        fixture::with_database(2, |read| async move {
            migrate(&read, MigrationTrack::HoldingRead).await;
            let now = 1_790_000_000_000_i64;
            let mut write_connection = write.raw().acquire().await.unwrap();
            let mut sources = WriterSources {
                connection: &mut write_connection,
                calls: 0,
            };
            let mut tx = read.raw().begin().await.unwrap();

            for (event_type, aggregate_id) in [
                ("finance.journal.post", "journal-1"),
                ("leave.policy.update", "policy-1"),
                ("overtime.eligibility", "engagement-1"),
                ("field.order.create", "order-1"),
                ("operations.exception.raise", "exception-1"),
                ("commerce.invoice.create", "order-1"),
            ] {
                let event = message("event-unhandled", event_type, aggregate_id, 1, None, "company");
                assert_eq!(
                    project_event(&mut tx, &mut sources, &event, now)
                        .await
                        .unwrap(),
                    ProjectionResult::Unhandled,
                    "{event_type}"
                );
            }
            assert_eq!(sources.calls, 0);
            let mut read_connection = read.raw().acquire().await.unwrap();
            assert_eq!(counts(&mut read_connection).await, (0, 0, 0));
            drop(read_connection);

            // A missing owner source refuses projection and writes nothing.
            let missing = message(
                "event-payroll-missing",
                "people.payroll.preview",
                "missing-run",
                3,
                None,
                "stale-company",
            );
            let error = project_event(&mut tx, &mut sources, &missing, now)
                .await
                .unwrap_err();
            assert_eq!(error.code, "PROJECTION_SOURCE_UNAVAILABLE");
            assert_eq!(sources.calls, 1);
            assert_eq!(counts(&mut tx).await, (0, 0, 0));

            // Payroll without an explicit state refreshes `status`/version from
            // the owner and projects completed work with no branch/department.
            let payroll = message(
                "event-payroll",
                "people.payroll.preview",
                "payroll-1",
                3,
                None,
                "stale-company",
            );
            assert_eq!(
                project_event(&mut tx, &mut sources, &payroll, now)
                    .await
                    .unwrap(),
                ProjectionResult::Projected
            );
            let work = sqlx::query(
                "SELECT source_version,title,subtitle,queue,company_id,branch_id,department_id \
                 FROM work_items WHERE source_type='hr_payroll_run' AND source_id='payroll-1'",
            )
            .fetch_one(&mut *tx)
            .await
            .unwrap();
            assert_eq!(work.get::<i64, _>("source_version"), 6);
            assert_eq!(work.get::<String, _>("title"), "Payroll preview");
            assert_eq!(work.get::<String, _>("subtitle"), "posted");
            assert_eq!(work.get::<String, _>("queue"), "completed");
            assert_eq!(work.get::<String, _>("company_id"), "payroll-company");
            assert_eq!(work.get::<Option<String>, _>("branch_id"), None);
            assert_eq!(work.get::<Option<String>, _>("department_id"), None);

            // The caller's commit is the only commit.
            tx.commit().await.unwrap();
            let mut read_connection = read.raw().acquire().await.unwrap();
            assert_eq!(counts(&mut read_connection).await, (1, 1, 1));
            let checkpoint: String = sqlx::query_scalar(
                "SELECT last_event_id FROM projection_checkpoints WHERE projector='work-items'",
            )
            .fetch_one(&mut *read_connection)
            .await
            .unwrap();
            assert_eq!(checkpoint, "event-payroll");
        })
        .await;
    })
    .await;
}
