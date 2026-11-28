//! Commerce-only lock fence; no purchase or Portal authority.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_commerce_server::public::portal_commerce_locks::lock_portal_commerce_sources;
use erpx_platform_runtime_server::public::postgres::{DatabasePool,MigrationTrack,load_migrations,migrate_database};
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
async fn commerce_locks_wait_for_writer_and_fence_reference_writes() {
 fixture::with_database(4,|pool| async move {
  setup(&pool).await;sqlx::query("INSERT INTO commerce_products(id,company_id,sku,name,unit,price_minor,tax_bps,status,version) VALUES('product','company','P1','Product','each',100,0,'active',1)").execute(pool.raw()).await.unwrap();
  let mut c=pool.raw().acquire().await.unwrap();assert!(lock_portal_commerce_sources(&mut c).await.is_err());drop(c);
  let mut writer=pool.raw().begin().await.unwrap();let wp:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
  sqlx::query("UPDATE commerce_orders SET note='committed' WHERE id='order-1'").execute(&mut *writer).await.unwrap();
  let other=pool.clone();let (send,recv)=tokio::sync::oneshot::channel();let reader=tokio::spawn(async move {let mut c=other.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(pid).unwrap();let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_portal_commerce_sources(&mut tx).await.unwrap();let note:String=sqlx::query_scalar("SELECT note FROM commerce_orders WHERE id='order-1'").fetch_one(&mut *tx).await.unwrap();tx.rollback().await.unwrap();note});
  let pid=recv.await.unwrap();let mut blocked=false;for _ in 0..200{blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(wp).bind(pid).fetch_one(pool.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}assert!(blocked);writer.commit().await.unwrap();assert_eq!(tokio::time::timeout(std::time::Duration::from_secs(5),reader).await.unwrap().unwrap(),"committed");
  let mut owner=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *owner).await.unwrap();lock_portal_commerce_sources(&mut owner).await.unwrap();let owner_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *owner).await.unwrap();
  let (mutable,reference):(i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE mode='ShareRowExclusiveLock' AND relation=ANY(ARRAY['commerce_orders'::regclass,'commerce_order_lines'::regclass,'commerce_fulfillments'::regclass,'commerce_fulfillment_lines'::regclass,'commerce_fulfillment_evidence'::regclass,'commerce_returns'::regclass,'commerce_return_lines'::regclass,'commerce_return_lot_allocations'::regclass,'commerce_return_stock_moves'::regclass,'commerce_goods_valuations'::regclass,'commerce_invoice_goods_carrying'::regclass,'commerce_stock'::regclass,'commerce_stock_moves'::regclass])),count(*) FILTER(WHERE mode='ShareLock' AND relation=ANY(ARRAY['commerce_settings'::regclass,'commerce_products'::regclass,'commerce_warehouses'::regclass])) FROM pg_locks WHERE pid=pg_backend_pid() AND granted").fetch_one(&mut *owner).await.unwrap();assert_eq!((mutable,reference),(13,3));
  let other=pool.clone();let (send,recv)=tokio::sync::oneshot::channel();let writer=tokio::spawn(async move {let mut c=other.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(pid).unwrap();sqlx::query("UPDATE commerce_products SET name='Changed' WHERE id='product'").execute(&mut *c).await.unwrap();});
  let pid=recv.await.unwrap();let mut blocked=false;for _ in 0..200{blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(owner_pid).bind(pid).fetch_one(pool.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}assert!(blocked,"reference writer must wait for SHARE lock");owner.rollback().await.unwrap();tokio::time::timeout(std::time::Duration::from_secs(5),writer).await.unwrap().unwrap();
 }).await;
}
