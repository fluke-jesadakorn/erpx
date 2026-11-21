//! Commerce owner record-source reads on migrated PostgreSQL: Company fencing,
//! serializable-unit fencing, bounded payloads and change detection.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;

use erpx_server_commerce::public::record_source::{lock_record_sources, read_record_source};
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, load_migrations, migrate_database,
};
use erpx_server_shared::public::record_source_contract::SourceKind;
use erpx_server_shared::public::{BusinessError, CompanyId};

fn persistence_code(error: PgTransactionError<BusinessError>) -> String {
    match error {
        PgTransactionError::Persistence(error) => error.code.into(),
        other => panic!("expected a persistence failure, got {other:?}"),
    }
}

fn business_code(error: PgTransactionError<BusinessError>) -> &'static str {
    match error {
        PgTransactionError::Business(error) => error.code,
        other => panic!("expected an owner business failure, got {other:?}"),
    }
}

async fn serializable<'c>(
    pool: &'c DatabasePool,
) -> sqlx::Transaction<'c, sqlx::Postgres> {
    let mut tx = pool.raw().begin().await.unwrap();
    sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
        .execute(&mut *tx)
        .await
        .unwrap();
    tx
}

async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        "INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES \
         ('company','company','company','Company',NULL,'company',NULL,'active',1),\
         ('foreign','company','foreign','Foreign company',NULL,'foreign',NULL,'active',1),\
         ('branch','branch','branch','Branch',NULL,'company',NULL,'active',1),\
         ('foreign-branch','branch','foreign-branch','Foreign branch',NULL,'foreign',NULL,'active',1);\
         INSERT INTO finance_parties(id,company_id,kind,code,name,currency,payment_terms_days,credit_limit_minor,status,version,created_at,updated_at) VALUES \
         ('party-1','company','customer','C1','Customer','THB',30,0,'active',1,1,1),\
         ('party-f','foreign','customer','CF','Foreign customer','THB',30,0,'active',1,1,1);\
         INSERT INTO commerce_warehouses(id,company_id,branch_id,code,name,status,version) VALUES \
         ('warehouse-1','company','branch','WH1','Main warehouse','active',1);\
         INSERT INTO commerce_orders\
         (id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,\
         order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES \
         ('order-1','company','branch','department-1','sales','SO-1','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','confirmed',10000,700,3,'creator',100,300),\
         ('quote-1','company','branch',NULL,'quote','QT-1','party-1','warehouse-1','THB',\
          '2026-11-01','2026-11-08','','draft',10000,700,1,'creator',100,100),\
         ('foreign-order','foreign','foreign-branch',NULL,'sales','SO-F','party-f',NULL,'THB',\
          '2026-11-01','2026-11-08','','draft',10000,700,1,'foreign-user',100,100);",
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

#[tokio::test]
async fn order_facts_are_company_fenced_bounded_and_change_detected() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let company = CompanyId::new("company").unwrap();
        let mut tx = serializable(&pool).await;
        lock_record_sources(&mut tx).await.unwrap();

        let source = read_record_source(&mut tx, &company, "order-1")
            .await
            .unwrap()
            .expect("owner order");
        let data = source.data();
        assert_eq!(data.kind, SourceKind::CommerceOrder);
        assert_eq!(data.id, "order-1");
        assert_eq!(data.company_id, "company");
        assert_eq!(data.branch_id.as_deref(), Some("branch"));
        assert_eq!(data.department_id.as_deref(), Some("department-1"));
        assert_eq!(data.version, 3);
        assert_eq!(data.title, "SO-1");
        assert_eq!(data.status, "confirmed");
        source.require_current(&mut tx).await.unwrap();

        let quote = read_record_source(&mut tx, &company, "quote-1")
            .await
            .unwrap()
            .expect("owner quote");
        assert_eq!(quote.data().kind, SourceKind::CommerceOrder);
        assert_eq!(quote.data().title, "QT-1");
        assert_eq!(quote.data().department_id, None);
        quote.require_current(&mut tx).await.unwrap();

        assert!(read_record_source(&mut tx, &company, "missing")
            .await
            .unwrap()
            .is_none());
        assert!(read_record_source(&mut tx, &company, "foreign-order")
            .await
            .unwrap()
            .is_none());
        assert!(read_record_source(
            &mut tx,
            &CompanyId::new("foreign").unwrap(),
            "order-1"
        )
        .await
        .unwrap()
        .is_none());

        sqlx::query("UPDATE commerce_orders SET version=version+1,state='fulfilled' WHERE id='order-1'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert_eq!(
            business_code(source.require_current(&mut tx).await.unwrap_err()),
            "SOURCE_RECORD_CHANGED"
        );
        let updated = read_record_source(&mut tx, &company, "order-1")
            .await
            .unwrap()
            .expect("updated order");
        assert_eq!(updated.data().version, 4);
        assert_eq!(updated.data().status, "fulfilled");
        updated.require_current(&mut tx).await.unwrap();

        sqlx::query("UPDATE commerce_orders SET note=repeat('x',2097153) WHERE id='order-1'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert_eq!(
            business_code(
                read_record_source(&mut tx, &company, "order-1")
                    .await
                    .err()
                    .expect("expected a source read failure")
            ),
            "RECORD_SOURCE_LIMIT"
        );
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn order_source_reads_require_a_serializable_unit() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let company = CompanyId::new("company").unwrap();

        let mut connection = pool.raw().acquire().await.unwrap();
        assert_eq!(
            persistence_code(
                read_record_source(&mut *connection, &company, "order-1")
                    .await
                    .err()
                    .expect("expected a source read failure")
            ),
            "RECORD_SOURCE_SERIALIZABLE_REQUIRED"
        );
        assert_eq!(
            persistence_code(lock_record_sources(&mut *connection).await.unwrap_err()),
            "RECORD_SOURCE_TRANSACTION_REQUIRED"
        );
        drop(connection);

        let mut plain = pool.raw().begin().await.unwrap();
        assert_eq!(
            persistence_code(
                read_record_source(&mut plain, &company, "order-1")
                    .await
                    .err()
                    .expect("expected a source read failure")
            ),
            "RECORD_SOURCE_SERIALIZABLE_REQUIRED"
        );
        plain.rollback().await.unwrap();
    })
    .await;
}
