#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_commerce::public::supplier_promise_source::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, load_migrations, migrate_database,
};
use erpx_server_shared::public::CompanyId;
use sqlx::Connection;
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
async fn source_modes_preserve_scope_and_unbounded_lines() {
    fixture::with_database(3, |pool| async move {
  setup(&pool).await;
  sqlx::raw_sql("INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps) SELECT 'p-'||n,'company','sku-'||n,'Product','each',1,0 FROM generate_series(1,10001) n; INSERT INTO commerce_order_lines(id,order_id,product_id,description,quantity_milli,fulfilled_milli,price_minor,tax_bps,subtotal_minor,tax_minor,ordinal) SELECT 'l-'||n,'order-1','p-'||n,'Line',1000,0,1,0,1,0,n FROM generate_series(1,10001) n;").execute(pool.raw()).await.unwrap();
  let company=CompanyId::new("company").unwrap();let mut c=pool.raw().acquire().await.unwrap();
  let mut tx=c.begin().await.unwrap();
  assert!(inspect_supplier_promise_source(&mut tx,SupplierPromiseSourceMode::Enabled,&company,None,None,"order-1").await.is_err());
  tx.rollback().await.unwrap();
  let mut tx=c.begin().await.unwrap();
  let source=inspect_supplier_promise_source(&mut tx,SupplierPromiseSourceMode::Disabled,&company,Some("branch"),Some("department-1"),"order-1").await.unwrap().unwrap();
  assert_eq!(source.lines().len(),10001);assert_eq!(source.lines()[0].id,"l-1");assert_eq!(source.lines()[10000].id,"l-10001");
  let heavy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE pid=pg_backend_pid() AND relation='public.commerce_order_lines'::regclass AND mode IN ('ShareRowExclusiveLock','ExclusiveLock','AccessExclusiveLock') AND granted)").fetch_one(&mut *tx).await.unwrap();assert!(!heavy);
  sqlx::query("UPDATE commerce_orders SET note='unrelated' WHERE id='order-1'").execute(&mut *tx).await.unwrap();source.require_source_guard(&mut tx).await.unwrap();
  sqlx::query("UPDATE commerce_orders SET version=version+1 WHERE id='order-1'").execute(&mut *tx).await.unwrap();assert!(source.require_source_guard(&mut tx).await.is_err());tx.rollback().await.unwrap();
  let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();sqlx::query("SAVEPOINT before_lock").execute(&mut *tx).await.unwrap();lock_supplier_promise_sources(&mut tx).await.unwrap();
  let source=inspect_supplier_promise_source(&mut tx,SupplierPromiseSourceMode::Enabled,&company,None,None,"order-1").await.unwrap().unwrap();
  assert!(inspect_supplier_promise_source(&mut tx,SupplierPromiseSourceMode::Enabled,&company,Some("other"),None,"order-1").await.unwrap().is_none());
  sqlx::query("ROLLBACK TO SAVEPOINT before_lock").execute(&mut *tx).await.unwrap();assert!(source.require_source_guard(&mut tx).await.is_err());tx.rollback().await.unwrap();
  let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_supplier_promise_sources(&mut tx).await.unwrap();assert!(source.require_source_guard(&mut tx).await.is_err());tx.rollback().await.unwrap();
 }).await;
}

#[tokio::test]
async fn enabled_fence_waits_before_snapshot_for_line_writer() {
    fixture::with_database(3, |pool| async move {
        setup(&pool).await;
        let mut writer = pool.raw().begin().await.unwrap();
        sqlx::query("LOCK TABLE commerce_order_lines IN ROW EXCLUSIVE MODE")
            .execute(&mut *writer)
            .await
            .unwrap();
        sqlx::query("UPDATE commerce_orders SET version=4 WHERE id='order-1'")
            .execute(&mut *writer)
            .await
            .unwrap();
        let writer_pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
            .fetch_one(&mut *writer)
            .await
            .unwrap();
        let (send, receive) = tokio::sync::oneshot::channel();
        let other = pool.clone();
        let reader = tokio::spawn(async move {
            let mut c = other.raw().acquire().await.unwrap();
            let pid: i32 = sqlx::query_scalar("SELECT pg_backend_pid()")
                .fetch_one(&mut *c)
                .await
                .unwrap();
            send.send(pid).unwrap();
            let mut tx = c
                .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
                .await
                .unwrap();
            lock_supplier_promise_sources(&mut tx).await.unwrap();
            let source = inspect_supplier_promise_source(
                &mut tx,
                SupplierPromiseSourceMode::Enabled,
                &CompanyId::new("company").unwrap(),
                None,
                None,
                "order-1",
            )
            .await
            .unwrap()
            .unwrap();
            let version = source.order().version;
            tx.rollback().await.unwrap();
            version
        });
        let pid = receive.await.unwrap();
        let blocked = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let yes: bool = sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))")
                    .bind(writer_pid)
                    .bind(pid)
                    .fetch_one(pool.raw())
                    .await
                    .unwrap();
                if yes {
                    break;
                }
                tokio::time::sleep(std::time::Duration::from_millis(10)).await;
            }
        })
        .await;
        assert!(blocked.is_ok());
        writer.commit().await.unwrap();
        assert_eq!(
            tokio::time::timeout(std::time::Duration::from_secs(5), reader)
                .await
                .unwrap()
                .unwrap(),
            4
        );
    })
    .await;
}
