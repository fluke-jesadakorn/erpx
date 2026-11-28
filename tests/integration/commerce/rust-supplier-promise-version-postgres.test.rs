#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_commerce_server::public::supplier_promise_version::*;
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_platform_shared_server::public::CompanyId;
use sqlx::Connection;
async fn setup(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../migrations/postgres");
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
         ('order-1','company','branch','department-1','purchase','PO-1','party-1','warehouse-1','THB',\
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
async fn source_bound_version_is_atomic_and_never_grants_authority() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut c = pool.raw().acquire().await.unwrap();
        let company = CompanyId::new("company").unwrap();
        let mut tx = c
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        sqlx::query("SAVEPOINT before_fence")
            .execute(&mut *tx)
            .await
            .unwrap();
        lock_supplier_promise_order(&mut tx).await.unwrap();
        assert!(
            prepare_supplier_promise_order(&mut tx, &company, "foreign-order", 1)
                .await
                .is_err()
        );
        assert!(
            prepare_supplier_promise_order(&mut tx, &company, "order-1", 2)
                .await
                .is_err()
        );
        let before = prepare_supplier_promise_order(&mut tx, &company, "order-1", 3)
            .await
            .unwrap();
        sqlx::raw_sql("CREATE FUNCTION supplier_order_note_trigger() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN NEW.note='trigger note'; RETURN NEW; END $$; CREATE TRIGGER supplier_order_note_trigger BEFORE UPDATE ON commerce_orders FOR EACH ROW EXECUTE FUNCTION supplier_order_note_trigger()")
            .execute(&mut *tx).await.unwrap();
        let after = before.advance(&mut tx, 500).await.unwrap();
        let note: String = sqlx::query_scalar("SELECT note FROM commerce_orders WHERE id='order-1'").fetch_one(&mut *tx).await.unwrap();
        assert_eq!(note, "trigger note");
        after.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE commerce_orders SET note='tamper' WHERE id='order-1'")
            .execute(&mut *tx)
            .await
            .unwrap();
        after.require_current(&mut tx).await.unwrap();
        sqlx::query("UPDATE commerce_orders SET version=version+1 WHERE id='order-1'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(after.require_current(&mut tx).await.is_err());
        sqlx::query("ROLLBACK TO SAVEPOINT before_fence")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(after.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let version: i64 =
            sqlx::query_scalar("SELECT version FROM commerce_orders WHERE id='order-1'")
                .fetch_one(pool.raw())
                .await
                .unwrap();
        assert_eq!(version, 3);
        let mut tx = c
            .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
            .await
            .unwrap();
        lock_supplier_promise_order(&mut tx).await.unwrap();
        assert!(after.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
    })
    .await;
}
