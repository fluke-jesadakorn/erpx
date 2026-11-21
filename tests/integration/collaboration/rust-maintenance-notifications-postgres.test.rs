//! Collaboration component tests. This explicitly synthetic owner port proves
//! notification persistence/rollback only; actual Work/IAM/purchase authority is
//! a separate mandatory Maintenance composition, never granted by this fixture.
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_collaboration::public::maintenance_notifications::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{
    BusinessError, CommandKey, CompanyId, FailureKind, HoldingId, RecordId, TimestampMillis,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
type Error = PgTransactionError<BusinessError>;
const NOW: i64 = 1800000000007;
struct SyntheticOwner {
    facts: MaintenanceNotificationFacts,
}
impl SyntheticOwner {
    fn new() -> Self {
        Self {
            facts: MaintenanceNotificationFacts {
                owner_key: key(),
                kind: MaintenanceNotificationKind::Updated,
                holding_id: HoldingId::new("holding").unwrap(),
                company_id: CompanyId::new("company").unwrap(),
                work_id: RecordId::new("work").unwrap(),
                owner_user_id: "owner".into(),
                work_version_after: 8,
                updated_at_after: TimestampMillis::new(NOW),
            },
        }
    }
    async fn write(&self, c: &mut PgConnection) -> Result<(), Error> {
        sqlx::query(
            "UPDATE synthetic_notification_owner SET version=8,updated_at=$1 WHERE id='work'",
        )
        .bind(NOW)
        .execute(c)
        .await?;
        Ok(())
    }
    async fn matches(&self, c: &mut PgConnection, after: bool) -> Result<(), Error> {
        let actual: Option<Value> = sqlx::query_scalar(
            "SELECT to_jsonb(w) FROM synthetic_notification_owner w WHERE id='work'",
        )
        .fetch_optional(c)
        .await?;
        let expected = json!({"id":"work","company_id":"company","owner_id":"owner","version":if after{8}else{7},"updated_at":if after{NOW}else{0}});
        if actual.as_ref() != Some(&expected) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "SYNTHETIC_OWNER_SOURCE_CHANGED",
                "The explicit test owner source changed.",
            )
            .into());
        }
        Ok(())
    }
}
impl PgMaintenanceNotificationSource for SyntheticOwner {
    fn facts(&self) -> &MaintenanceNotificationFacts {
        &self.facts
    }
    async fn require_before(&self, c: &mut PgConnection, _: TimestampMillis) -> Result<(), Error> {
        self.matches(c, false).await
    }
    async fn require_after(&self, c: &mut PgConnection, _: TimestampMillis) -> Result<(), Error> {
        self.matches(c, true).await
    }
}
fn key() -> CommandKey {
    CommandKey::new("notification-command").unwrap()
}
fn now() -> TimestampMillis {
    TimestampMillis::new(NOW)
}
fn denied<T>(result: Result<T, Error>, code: &str) {
    assert!(
        matches!(result,Err(PgTransactionError::Business(ref error))if error.code==code),
        "expected {code}"
    );
}
async fn setup(pool: &DatabasePool) {
    let migrations = load_migrations(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres"),
        MigrationTrack::HoldingWrite,
    )
    .await
    .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &migrations)
        .await
        .unwrap();
    sqlx::raw_sql("CREATE TABLE synthetic_notification_owner(id TEXT PRIMARY KEY,company_id TEXT,owner_id TEXT,version BIGINT,updated_at BIGINT);INSERT INTO synthetic_notification_owner VALUES('work','company','owner',7,0)").execute(pool.raw()).await.unwrap();
}
async fn retained(c: &mut PgConnection) -> Result<RetainedMaintenanceNotification, Error> {
    let source = SyntheticOwner::new();
    let plan = prepare_maintenance_notification(
        c,
        &source,
        &key(),
        MaintenanceNotificationKind::Updated,
        now(),
    )
    .await?
    .into_owned();
    source.write(c).await?;
    plan.persist(c, &source, now()).await
}
async fn empty(pool: &DatabasePool) {
    let count:i64=sqlx::query_scalar("SELECT(SELECT count(*)FROM staff_notifications)+(SELECT count(*)FROM inbox_events)+(SELECT count(*)FROM inbox_deliveries)").fetch_one(pool.raw()).await.unwrap();
    assert_eq!(count, 0);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT version FROM synthetic_notification_owner WHERE id='work'"
        )
        .fetch_one(pool.raw())
        .await
        .unwrap(),
        7
    );
}
#[tokio::test]
async fn original_insert_and_real_inbox_bridge_commit_exact_images_but_receipt_cannot_cross_units()
{
    fixture::with_database(1,|pool|async move{setup(&pool).await;
        let receipt=PgUnitOfWork::new(pool.clone()).run(TransactionOptions::default(),|c|Box::pin(async move{
            lock_maintenance_notification_sources(c).await?;
            let receipt=retained(c).await?;
            assert_eq!(receipt.notification().id().as_str(),"maintenance-notice-notification-command");
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut*c).await?;
            receipt.require_current(c).await?;Ok(receipt)
        })).await.unwrap();
        let values:(String,String,i64,i64)=sqlx::query_as("SELECT n.user_id,e.preview,n.created_at,d.delivered_at FROM staff_notifications n JOIN inbox_events e ON e.id='legacy:'||n.id JOIN inbox_deliveries d ON d.id=n.id").fetch_one(pool.raw()).await.unwrap();
        assert_eq!(values,("owner".into(),"Asset maintenance work updated: review current next action".into(),NOW,NOW));
        let result=PgUnitOfWork::new(pool.clone()).run_once(TransactionOptions::default(),move|c|Box::pin(async move{receipt.require_current(c).await})).await;
        denied(result,"MAINTENANCE_NOTIFICATION_SOURCE_CHANGED");
    }).await;
}
#[tokio::test]
async fn original_owned_preparation_requires_exact_source_facts_and_the_same_managed_unit() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        denied(
            lock_maintenance_notification_sources(&mut c).await,
            "MAINTENANCE_NOTIFICATION_SOURCE_CHANGED",
        );
        drop(c);
        let mut read_committed = pool.raw().begin().await.unwrap();
        let source = SyntheticOwner::new();
        denied(
            prepare_maintenance_notification(
                &mut read_committed,
                &source,
                &key(),
                MaintenanceNotificationKind::Updated,
                now(),
            )
            .await,
            "MAINTENANCE_NOTIFICATION_SOURCE_CHANGED",
        );
        read_committed.rollback().await.unwrap();
        let result = PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_maintenance_notification_sources(c).await?;
                    let source = SyntheticOwner::new();
                    let plan = prepare_maintenance_notification(
                        c,
                        &source,
                        &key(),
                        MaintenanceNotificationKind::Updated,
                        now(),
                    )
                    .await?
                    .into_owned();
                    source.write(c).await?;
                    let foreign = SyntheticOwner {
                        facts: MaintenanceNotificationFacts {
                            owner_user_id: "other".into(),
                            ..source.facts.clone()
                        },
                    };
                    plan.persist(c, &foreign, now()).await.map(|_| ())
                })
            })
            .await;
        denied(result, "MAINTENANCE_NOTIFICATION_SOURCE_CHANGED");
        empty(&pool).await;
        let plan = PgUnitOfWork::new(pool.clone())
            .run(TransactionOptions::default(), |c| {
                Box::pin(async move {
                    lock_maintenance_notification_sources(c).await?;
                    let source = SyntheticOwner::new();
                    Ok(prepare_maintenance_notification(
                        c,
                        &source,
                        &key(),
                        MaintenanceNotificationKind::Updated,
                        now(),
                    )
                    .await?
                    .into_owned())
                })
            })
            .await
            .unwrap();
        let result = PgUnitOfWork::new(pool.clone())
            .run_once(TransactionOptions::default(), move |c| {
                Box::pin(async move {
                    plan.persist(c, &SyntheticOwner::new(), now())
                        .await
                        .map(|_| ())
                })
            })
            .await;
        denied(result, "MAINTENANCE_NOTIFICATION_SOURCE_CHANGED");
        empty(&pool).await;
    })
    .await;
}
#[tokio::test]
async fn every_owned_notification_event_delivery_mutation_denies_and_rolls_back_the_owner() {
    fixture::with_database(1, |pool| async move {
        setup(&pool).await;
        for mutation in [
            "UPDATE staff_notifications SET message='tampered'",
            "UPDATE inbox_events SET preview='tampered'",
            "UPDATE inbox_deliveries SET read_at=1",
        ] {
            let result = PgUnitOfWork::new(pool.clone())
                .run(TransactionOptions::default(), |c| {
                    Box::pin(async move {
                        lock_maintenance_notification_sources(c).await?;
                        let receipt = retained(c).await?;
                        sqlx::query(mutation).execute(&mut *c).await?;
                        receipt.require_current(c).await
                    })
                })
                .await;
            denied(result, "MAINTENANCE_NOTIFICATION_SOURCE_CHANGED");
            empty(&pool).await;
        }
    })
    .await;
}
#[tokio::test]
async fn source_after_image_and_insert_collision_remain_actual_errors_without_noop_adoption() {
    fixture::with_database(1,|pool|async move{setup(&pool).await;
        let result=PgUnitOfWork::new(pool.clone()).run(TransactionOptions::default(),|c|Box::pin(async move{
            lock_maintenance_notification_sources(c).await?;let source=SyntheticOwner::new();let plan=prepare_maintenance_notification(c,&source,&key(),MaintenanceNotificationKind::Updated,now()).await?;
            sqlx::query("UPDATE synthetic_notification_owner SET owner_id='other'").execute(&mut*c).await?;
            plan.persist(c,now()).await.map(|_|())
        })).await;denied(result,"SYNTHETIC_OWNER_SOURCE_CHANGED");empty(&pool).await;
        let result=PgUnitOfWork::new(pool.clone()).run(TransactionOptions::default(),|c|Box::pin(async move{
            lock_maintenance_notification_sources(c).await?;let first=retained(c).await?;
            sqlx::query("UPDATE synthetic_notification_owner SET version=7,updated_at=0").execute(&mut*c).await?;
            let _=retained(c).await?;first.require_current(c).await
        })).await;
        assert!(matches!(result,Err(PgTransactionError::Persistence(ref e))if e.sql_state()==Some("23505")));empty(&pool).await;
    }).await;
}
#[tokio::test]
async fn deferred_bridge_tampering_is_compared_to_precomputed_images_and_rolls_back_every_effect() {
    fixture::with_database(1,|pool|async move{setup(&pool).await;
        sqlx::raw_sql("CREATE FUNCTION synthetic_notification_deferred_tamper()RETURNS TRIGGER LANGUAGE plpgsql AS $$BEGIN UPDATE inbox_events SET preview='deferred-tamper' WHERE id='legacy:'||NEW.id;RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER synthetic_notification_tamper AFTER INSERT ON staff_notifications DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_notification_deferred_tamper()").execute(pool.raw()).await.unwrap();
        let result=PgUnitOfWork::new(pool.clone()).run(TransactionOptions::default(),|c|Box::pin(async move{
            lock_maintenance_notification_sources(c).await?;let receipt=retained(c).await?;
            sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut*c).await?;
            receipt.require_current(c).await
        })).await;
        denied(result,"MAINTENANCE_NOTIFICATION_SOURCE_CHANGED");empty(&pool).await;
    }).await;
}
#[tokio::test]
async fn current_receipt_keeps_positive_rows_locked_and_rejects_oversized_private_evidence() {
    fixture::with_database(2,|pool|async move{setup(&pool).await;
        // Exactly matching, committed legacy Inbox rows exercise the original
        // trigger's ON CONFLICT DO NOTHING path and its retained positive tuples.
        sqlx::raw_sql("INSERT INTO inbox_events(id,company_id,event_key,source_type,source_id,source_version,category,preview,created_at)VALUES('legacy:maintenance-notice-notification-command','company','legacy:maintenance-notice-notification-command','maintenance_work','work',8,'work','Asset maintenance work updated: review current next action',1800000000007);INSERT INTO inbox_deliveries(id,event_id,company_id,user_id,delivered_at)VALUES('maintenance-notice-notification-command','legacy:maintenance-notice-notification-command','company','owner',1800000000007)").execute(pool.raw()).await.unwrap();
        let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut*tx).await.unwrap();lock_maintenance_notification_sources(&mut tx).await.unwrap();let receipt=retained(&mut tx).await.unwrap();
        let writer_pool=pool.clone();let writer=tokio::spawn(async move{let mut c=writer_pool.raw().begin().await.unwrap();sqlx::query("SET LOCAL lock_timeout='100ms'").execute(&mut*c).await.unwrap();let error=sqlx::query("UPDATE inbox_events SET preview='other' WHERE id='legacy:maintenance-notice-notification-command'").execute(&mut*c).await.unwrap_err();assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("55P03"));c.rollback().await.unwrap();});
        writer.await.unwrap();
        sqlx::query("UPDATE inbox_events SET preview=repeat('x',10000)").execute(&mut*tx).await.unwrap();
        denied(receipt.require_current(&mut tx).await,"MAINTENANCE_NOTIFICATION_SOURCE_CHANGED");tx.rollback().await.unwrap();
        sqlx::raw_sql("DELETE FROM inbox_deliveries;DELETE FROM inbox_events").execute(pool.raw()).await.unwrap();empty(&pool).await;
    }).await;
}
