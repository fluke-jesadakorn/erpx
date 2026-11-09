//! Native Work Projection work-card preferences on a disposable Holding DB.
#![allow(dead_code)]
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
use erpx_server_work_projection::public::work_card_preferences::{
    execute_work_card_preferences, read_work_card_preferences,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandJson, CommandOutcome, PgCommandAuthority,
    WorkflowRequest,
};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

struct Time;
impl Clock for Time {
    fn now(&self) -> TimestampMillis {
        TimestampMillis::new(1_000)
    }
}
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).unwrap()
    }
}
fn actor() -> CommandActor {
    actor_fixture::parse_frozen_actor(
        r#"{"userId":"user-1","assignmentId":"assignment-1","role":"employee","companyId":"company","branchId":null,"departmentId":null,"authority":{"sessionId":"session","revision":1,"assignmentVersion":1}}"#,
    )
}
struct Authority {
    actor: CommandActor,
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
        Ok(self.actor.clone())
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
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status) \
         VALUES('company','company','acme','Acme',NULL,'company',NULL,'active');\
         INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) \
         VALUES('assignment-1','user-1','employee','[]','company',0,9000000000000,1,'holding');",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}
fn command(task: &str, input: &str, record: &str) -> WorkflowRequest {
    WorkflowRequest {
        key: format!("work:cards:{task}"),
        operation: "work.cards.preferences".into(),
        record_id: record.into(),
        input: CommandJson::parse(input).unwrap(),
    }
}
async fn execute(
    pool: &DatabasePool,
    actor: CommandActor,
    request: &WorkflowRequest,
    available: &[String],
) -> Result<CommandOutcome, Error> {
    let actor = actor.clone();
    let request = request.clone();
    let available = available.to_vec();
    PgUnitOfWork::new(pool.clone())
        .run(TransactionOptions::default(), move |connection| {
            let actor = actor.clone();
            let request = request.clone();
            let available = available.clone();
            Box::pin(async move {
                let mut authority = Authority { actor };
                execute_work_card_preferences(
                    CompanyId::new("company").unwrap(),
                    connection,
                    &mut authority,
                    &Time,
                    &mut Ids,
                    &request,
                    &available,
                )
                .await
            })
        })
        .await
}
fn available() -> Vec<String> {
    ["tasks", "expenses", "leave"]
        .into_iter()
        .map(str::to_owned)
        .collect()
}

#[tokio::test]
async fn saves_replays_clears_and_guards_version_shortcuts_and_membership() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let first = execute(
            &pool,
            actor(),
            &command(
                "1",
                r#"{"pinnedIds":["tasks","expenses"],"expectedVersion":0}"#,
                "company:user-1",
            ),
            &available(),
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(first.result.as_str()).unwrap(),
            serde_json::json!({"pinnedIds":["tasks","expenses"],"version":1})
        );
        let row: (Option<String>, i64) = sqlx::query_as(
            "SELECT pinned_ids_json,version FROM work_card_preferences \
             WHERE company_id='company' AND user_id='user-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, (Some(r#"["tasks","expenses"]"#.into()), 1));
        let read =
            read_work_card_preferences(&mut *pool.raw().acquire().await.unwrap(), &actor(), 1_000)
                .await
                .unwrap();
        assert_eq!(
            read.pinned_ids,
            Some(vec!["tasks".into(), "expenses".into()])
        );
        assert_eq!(read.version, 1);
        let replay = execute(
            &pool,
            actor(),
            &command(
                "1",
                r#"{"pinnedIds":["tasks","expenses"],"expectedVersion":0}"#,
                "company:user-1",
            ),
            &available(),
        )
        .await
        .unwrap();
        assert!(replay.replayed);
        assert_eq!(replay.result.as_str(), first.result.as_str());
        let conflict = execute(
            &pool,
            actor(),
            &command(
                "2",
                r#"{"pinnedIds":["tasks"],"expectedVersion":0}"#,
                "company:user-1",
            ),
            &available(),
        )
        .await
        .err()
        .expect("stale version conflicts");
        assert_eq!(business(conflict).code, "VERSION_CONFLICT");
        let unavailable = execute(
            &pool,
            actor(),
            &command(
                "3",
                r#"{"pinnedIds":["purchases"],"expectedVersion":1}"#,
                "company:user-1",
            ),
            &available(),
        )
        .await
        .err()
        .expect("unavailable shortcut is refused");
        assert_eq!(business(unavailable).code, "SHORTCUT_UNAVAILABLE");
        let cleared = execute(
            &pool,
            actor(),
            &command(
                "4",
                r#"{"pinnedIds":null,"expectedVersion":1}"#,
                "company:user-1",
            ),
            &available(),
        )
        .await
        .unwrap();
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(cleared.result.as_str()).unwrap(),
            serde_json::json!({"pinnedIds":null,"version":2})
        );
        let row: (Option<String>, i64) = sqlx::query_as(
            "SELECT pinned_ids_json,version FROM work_card_preferences \
             WHERE company_id='company' AND user_id='user-1'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!(row, (None, 2));
        let duplicate = execute(
            &pool,
            actor(),
            &command(
                "5",
                r#"{"pinnedIds":["tasks","tasks"],"expectedVersion":2}"#,
                "company:user-1",
            ),
            &available(),
        )
        .await
        .err()
        .expect("duplicate pins are refused");
        assert_eq!(business(duplicate).code, "WORK_CARD_PREFERENCES_INVALID");
        let mut outsider = actor();
        outsider.user_id = erpx_server_shared::public::UserId::new("user-2").unwrap();
        let denied = execute(
            &pool,
            outsider,
            &command(
                "6",
                r#"{"pinnedIds":[],"expectedVersion":0}"#,
                "company:user-2",
            ),
            &available(),
        )
        .await
        .err()
        .expect("non-member is refused");
        assert_eq!(business(denied).code, "MEMBERSHIP_REQUIRED");
        let receipts: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM command_receipts WHERE command_id LIKE 'work:cards:%'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let audits: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM audit_events WHERE command_id LIKE 'work:cards:%'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        let outbox: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox_events WHERE command_id LIKE 'work:cards:%'",
        )
        .fetch_one(pool.raw())
        .await
        .unwrap();
        assert_eq!((receipts, audits, outbox), (2, 2, 2));
    })
    .await;
}

#[tokio::test]
async fn read_defaults_empty_and_is_company_scoped() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let read =
            read_work_card_preferences(&mut *pool.raw().acquire().await.unwrap(), &actor(), 1_000)
                .await
                .unwrap();
        assert_eq!(read.pinned_ids, None);
        assert_eq!(read.version, 0);
        let mut other_company = actor();
        other_company.company_id = CompanyId::new("other").unwrap();
        let denied = read_work_card_preferences(
            &mut *pool.raw().acquire().await.unwrap(),
            &other_company,
            1_000,
        )
        .await
        .err()
        .expect("foreign Company membership is refused");
        assert_eq!(business(denied).code, "MEMBERSHIP_REQUIRED");
    })
    .await;
}
