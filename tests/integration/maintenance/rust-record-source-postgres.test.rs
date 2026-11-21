//! Actual owner record-source reads for Maintenance plans and work orders over
//! real migrated PostgreSQL. These are read facts, not command, purchase or
//! browser evidence.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_maintenance::public::record_source::{lock_record_sources, read_record_source};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::record_source_contract::SourceKind;
use erpx_server_shared::public::{BusinessError, CompanyId};

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

async fn seed(pool: &DatabasePool) {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::raw_sql(
        "INSERT INTO operational_assets(id,company_id,branch_id,code,name,category,currency,created_by,created_at,updated_at)VALUES\
         ('asset','company','branch','SYNTHETIC','Synthetic asset','synthetic','THB','fixture',0,0),\
         ('limit-asset','company','branch','LIMIT','Limit asset','synthetic','THB','fixture',0,0),\
         ('foreign-asset','other','branch','FOREIGN','Foreign asset','synthetic','THB','fixture',0,0);\
         INSERT INTO maintenance_preventive_plans(id,company_id,branch_id,department_id,asset_id,title,priority,owner_id,interval_days,time_zone,first_due_local,next_due_local,next_due_at,lead_days,status,version,created_by,edited_by,created_at,updated_at)VALUES\
         ('plan','company','branch',NULL,'asset','Synthetic preventive plan','normal','owner',30,'Asia/Bangkok','2026-11-10T09:00','2026-11-10T09:00',100,7,'draft',2,'maker','maker',0,5),\
         ('foreign-plan','other','branch',NULL,'foreign-asset','Foreign plan','normal','owner',30,'Asia/Bangkok','2026-11-10T09:00','2026-11-10T09:00',100,7,'draft',2,'maker','maker',0,5);\
         INSERT INTO maintenance_work_orders(id,company_id,branch_id,department_id,asset_id,title,priority,status,owner_id,version,created_by,created_at,updated_at)VALUES\
         ('work','company','branch',NULL,'asset','Synthetic work','high','in_progress','owner',4,'maker',0,6),\
         ('foreign-work','other','branch',NULL,'foreign-asset','Foreign work','normal','open','owner',1,'maker',0,6),\
         ('oversize-work','company','branch',NULL,'limit-asset',repeat('x',2100000),'normal','open','owner',1,'maker',0,6);",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    // Plans require a matching retained event snapshot at commit.
    sqlx::query(
        "INSERT INTO maintenance_preventive_events(id,plan_id,version,action,actor_id,reason,snapshot_json,created_at)\
         SELECT 'event-'||p.id,p.id,p.version,'create','maker','Synthetic source',to_jsonb(p),0 \
         FROM maintenance_preventive_plans p",
    )
    .execute(&mut *tx)
    .await
    .unwrap();
    tx.commit().await.unwrap();
}

fn company() -> CompanyId {
    CompanyId::new("company").unwrap()
}

#[tokio::test]
async fn reads_plan_and_work_sources_scoped_to_the_company() {
    fixture::with_database(2, |holding| async move {
        migrate(&holding).await;
        seed(&holding).await;
        let mut tx = serializable(&holding).await;
        lock_record_sources(&mut tx).await.unwrap();

        let plan = read_record_source(&mut tx, &company(), SourceKind::MaintenancePlan, "plan")
            .await
            .unwrap()
            .unwrap();
        let data = plan.data();
        assert_eq!(data.kind, SourceKind::MaintenancePlan);
        assert_eq!(data.id, "plan");
        assert_eq!(data.company_id, "company");
        assert_eq!(data.branch_id.as_deref(), Some("branch"));
        assert_eq!(data.department_id, None);
        assert_eq!(data.version, 2);
        assert_eq!(data.title, "Synthetic preventive plan");
        assert_eq!(data.status, "draft");

        let work = read_record_source(&mut tx, &company(), SourceKind::MaintenanceWork, "work")
            .await
            .unwrap()
            .unwrap();
        let data = work.data();
        assert_eq!(data.kind, SourceKind::MaintenanceWork);
        assert_eq!(data.version, 4);
        assert_eq!(data.title, "Synthetic work");
        assert_eq!(data.status, "in_progress");

        assert!(
            read_record_source(
                &mut tx,
                &company(),
                SourceKind::MaintenancePlan,
                "foreign-plan"
            )
            .await
            .unwrap()
            .is_none(),
            "a foreign company row is not readable"
        );
        assert!(
            read_record_source(&mut tx, &company(), SourceKind::MaintenanceWork, "missing")
                .await
                .unwrap()
                .is_none(),
            "a missing row is not fabricated"
        );
        assert!(
            matches!(
                read_record_source(&mut tx, &company(), SourceKind::Asset, "asset").await,
                Err(PgTransactionError::Business(_))
            ),
            "another owner kind is rejected"
        );

        sqlx::query(
            "UPDATE maintenance_preventive_plans SET title='Changed after capture' WHERE id='plan'",
        )
        .execute(&mut *tx)
        .await
        .unwrap();
        assert!(
            plan.require_current(&mut tx).await.is_err(),
            "a changed owner row invalidates the captured witness"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn source_reads_require_a_serializable_transaction_and_respect_the_bound() {
    fixture::with_database(1, |holding| async move {
        migrate(&holding).await;
        seed(&holding).await;

        let mut plain = holding.raw().begin().await.unwrap();
        assert!(
            read_record_source(&mut plain, &company(), SourceKind::MaintenancePlan, "plan")
                .await
                .is_err(),
            "read-committed reads are refused"
        );
        plain.rollback().await.unwrap();

        let mut tx = serializable(&holding).await;
        match read_record_source(
            &mut tx,
            &company(),
            SourceKind::MaintenanceWork,
            "oversize-work",
        )
        .await
        {
            Err(PgTransactionError::Business(BusinessError {
                code: "RECORD_SOURCE_LIMIT",
                ..
            })) => {}
            _ => panic!("expected the bounded read to refuse a 2 MiB source"),
        }
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn source_locks_require_an_open_transaction() {
    fixture::with_database(1, |holding| async move {
        let mut connection = holding.raw().acquire().await.unwrap();
        assert!(
            lock_record_sources(&mut *connection).await.is_err(),
            "the source table lock is fenced to a caller transaction"
        );
    })
    .await;
}
