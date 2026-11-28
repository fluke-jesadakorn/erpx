#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_commerce_server::public::portal_notification_source::*;
use erpx_platform_runtime_server::public::postgres::{DatabasePool,MigrationTrack,load_migrations,migrate_database};
use erpx_platform_shared_server::public::CompanyId;
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
async fn portal_scope_ignores_unrelated_fields_but_retains_unit_and_version() {
 fixture::with_database(3,|pool| async move {
  setup(&pool).await;
  sqlx::query("INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,status,reason,created_by,approved_by,version,created_at,updated_at) VALUES('binding','company','order-1','party-1','user@example.test',9000000000000,'approved','approved','creator','reviewer',2,0,0)").execute(pool.raw()).await.unwrap();
  let company=CompanyId::new("company").unwrap();let mut c=pool.raw().acquire().await.unwrap();
  assert!(read_portal_order_scope(&mut c,&company,"order-1").await.is_err());
  let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
  sqlx::query("SAVEPOINT before_lock").execute(&mut *tx).await.unwrap();lock_portal_order_sources(&mut tx).await.unwrap();
  let w=read_portal_order_scope(&mut tx,&company,"order-1").await.unwrap().unwrap();
  assert_eq!((w.scope().company_id.as_str(),w.scope().branch_id.as_deref(),w.scope().department_id.as_deref(),w.scope().version),("company",Some("branch"),Some("department-1"),3));
  assert!(read_portal_order_scope(&mut tx,&company,"foreign-order").await.unwrap().is_none());
  assert!(read_portal_order_scope(&mut tx,&company,"missing").await.unwrap().is_none());
  sqlx::query("SAVEPOINT scope_guard").execute(&mut *tx).await.unwrap();
  assert!(sqlx::query("UPDATE commerce_orders SET branch_id=NULL WHERE id='order-1'").execute(&mut *tx).await.is_err());
  sqlx::query("ROLLBACK TO SAVEPOINT scope_guard").execute(&mut *tx).await.unwrap();
  sqlx::query("UPDATE commerce_orders SET note='unrelated',number='SO-new' WHERE id='order-1'").execute(&mut *tx).await.unwrap();w.require_current(&mut tx).await.unwrap();
  sqlx::query("UPDATE commerce_orders SET version=version+1 WHERE id='order-1'").execute(&mut *tx).await.unwrap();assert!(w.require_current(&mut tx).await.is_err());
  sqlx::query("ROLLBACK TO SAVEPOINT before_lock").execute(&mut *tx).await.unwrap();assert!(w.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
  let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_portal_order_sources(&mut tx).await.unwrap();assert!(w.require_current(&mut tx).await.is_err());tx.rollback().await.unwrap();
 }).await;
}

#[tokio::test]
async fn portal_scope_waits_for_committed_writer_before_snapshot() {
 fixture::with_database(4,|pool| async move {
  setup(&pool).await;
  let mut writer=pool.raw().begin().await.unwrap();
  let writer_pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *writer).await.unwrap();
  sqlx::query("UPDATE commerce_orders SET version=4 WHERE id='order-1'").execute(&mut *writer).await.unwrap();
  let other=pool.clone();let (send,receive)=tokio::sync::oneshot::channel();
  let worker=tokio::spawn(async move {
   let mut c=other.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(pid).unwrap();
   let mut tx=c.begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();lock_portal_order_sources(&mut tx).await.unwrap();
   let version=read_portal_order_scope(&mut tx,&CompanyId::new("company").unwrap(),"order-1").await.unwrap().unwrap().scope().version;tx.rollback().await.unwrap();version
  });
  let pid=receive.await.unwrap();let mut blocked=false;for _ in 0..200 {blocked=sqlx::query_scalar("SELECT $1=ANY(pg_blocking_pids($2))").bind(writer_pid).bind(pid).fetch_one(pool.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}
  assert!(blocked,"scope read must wait for writer before snapshot");writer.commit().await.unwrap();
  assert_eq!(tokio::time::timeout(std::time::Duration::from_secs(5),worker).await.unwrap().unwrap(),4);
 }).await;
}
