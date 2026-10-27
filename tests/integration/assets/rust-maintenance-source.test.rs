#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_assets::public::maintenance::*;
use erpx_server_runtime::public::postgres::{
    DatabasePool, MigrationTrack, PgTransactionError, PgUnitOfWork, TransactionOptions,
    load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId, HoldingId, RecordId};
use std::{sync::Arc, time::Duration};
async fn setup(pool: &DatabasePool) {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(path, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::query("INSERT INTO operational_assets(id,company_id,branch_id,department_id,code,name,category,currency,cost_minor,created_by,created_at,updated_at)VALUES('asset','company','branch','department','SYNTHETIC',repeat('PRIVATE_NAME',200000),'synthetic','THB',987654321123,'fixture',0,0)").execute(pool.raw()).await.unwrap();
}
fn scope() -> (HoldingId, CompanyId, RecordId) {
    (
        HoldingId::new("h001").unwrap(),
        CompanyId::new("company").unwrap(),
        RecordId::new("asset").unwrap(),
    )
}
#[tokio::test]
async fn physical_asset_reference_is_minimal_company_scoped_and_not_lifecycle_admission() {
    fixture::with_database(2,|pool|async move{
        setup(&pool).await;let(holding,company,id)=scope();let mut outside=pool.raw().acquire().await.unwrap();assert!(read_current_maintenance_asset(&mut outside,&holding,&company,&id).await.is_err());drop(outside);
        PgUnitOfWork::new(pool).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
            lock_maintenance_asset_sources(c).await?;let(holding,company,id)=scope();let source=read_current_maintenance_asset(c,&holding,&company,&id).await?.unwrap();let f=source.facts();assert_eq!(f.asset_id,id);assert_eq!(f.company_id,company);assert_eq!(f.holding_id,holding);assert_eq!(f.branch_id.as_str(),"branch");assert_eq!(f.department_id.as_ref().unwrap().as_str(),"department");assert_eq!(f.version,1);let debug=format!("{f:?}");assert!(!debug.contains("PRIVATE_NAME"));assert!(!debug.contains("987654321123"));
            assert!(read_current_maintenance_asset(c,&holding,&CompanyId::new("foreign").unwrap(),&id).await?.is_none());assert!(read_current_maintenance_asset(c,&holding,&company,&RecordId::new("missing").unwrap()).await?.is_none());
            let full_source=source.retain_unchanged_image(c).await?;
            full_source.require_current(c).await?;
            sqlx::query("UPDATE operational_assets SET name='changed display',lifecycle_status='retirement_pending',retirement_reason='Synthetic retirement' WHERE id='asset'").execute(&mut *c).await?;source.require_current(c).await?;
            assert!(matches!(full_source.require_current(c).await,Err(PgTransactionError::Business(ref error))if error.code=="ASSET_MAINTENANCE_SOURCE_CHANGED"));
            // Presence/relationship is not permission to act on a retirement-pending
            // Asset. The owning action must apply its separate actual policy.
            Ok(())
        })).await.unwrap();
    }).await;
}
#[tokio::test]
async fn physical_asset_reference_refuses_local_version_rewrite_and_transaction_escape() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (holding, company, id) = scope();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        lock_maintenance_asset_sources(&mut tx).await.unwrap();
        let source = read_current_maintenance_asset(&mut tx, &holding, &company, &id)
            .await
            .unwrap()
            .unwrap();
        sqlx::query("UPDATE operational_assets SET version=version+1 WHERE id='asset'")
            .execute(&mut *tx)
            .await
            .unwrap();
        assert!(source.require_current(&mut tx).await.is_err());
        tx.rollback().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let source = read_current_maintenance_asset(&mut tx, &holding, &company, &id)
            .await
            .unwrap()
            .unwrap();
        tx.commit().await.unwrap();
        let mut other = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *other)
            .await
            .unwrap();
        assert!(source.require_current(&mut other).await.is_err());
        other.rollback().await.unwrap();
    })
    .await;
}
#[tokio::test]
async fn physical_asset_reference_retains_actual_tuple_until_the_owner_commit() {
    fixture::with_database(3,|pool|async move{
        setup(&pool).await;let ready=Arc::new(tokio::sync::Notify::new());let release=Arc::new(tokio::sync::Notify::new());let reader_pool=pool.clone();let reader_ready=ready.clone();let reader_release=release.clone();
        let reader=tokio::spawn(async move{PgUnitOfWork::new(reader_pool).run::<(),BusinessError,_>(TransactionOptions::default(),move|c|{let ready=reader_ready.clone();let release=reader_release.clone();Box::pin(async move{lock_maintenance_asset_sources(c).await?;let(holding,company,id)=scope();let source=read_current_maintenance_asset(c,&holding,&company,&id).await?.unwrap();ready.notify_one();release.notified().await;source.require_current(c).await?;Ok(())})}).await.unwrap()});
        if tokio::time::timeout(Duration::from_secs(3),ready.notified()).await.is_err(){reader.abort();let _=reader.await;panic!("owned physical source reader did not reach snapshot");}
        let writer_pool=pool.clone();let writer=tokio::spawn(async move{sqlx::query("UPDATE operational_assets SET version=version+1 WHERE id='asset'").execute(writer_pool.raw()).await.unwrap()});
        let observed=tokio::time::timeout(Duration::from_secs(3),async{loop{let wait:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND usename=current_user AND pid<>pg_backend_pid() AND wait_event_type='Lock')").fetch_one(pool.raw()).await.unwrap();if wait{break;}tokio::time::sleep(Duration::from_millis(10)).await;}}).await.is_ok();let pending=!writer.is_finished();release.notify_one();reader.await.unwrap();writer.await.unwrap();assert!(observed&&pending);
    }).await;
}
#[tokio::test]
async fn physical_asset_reference_preserves_old_snapshot_40001() {
    fixture::with_database(2,|pool|async move{
        setup(&pool).await;let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();sqlx::query("SELECT count(*)FROM operational_assets").fetch_one(&mut *tx).await.unwrap();sqlx::query("UPDATE operational_assets SET version=version+1 WHERE id='asset'").execute(pool.raw()).await.unwrap();let(holding,company,id)=scope();let result=read_current_maintenance_asset(&mut tx,&holding,&company,&id).await;assert!(matches!(result,Err(PgTransactionError::Persistence(error))if error.sql_state()==Some("40001")));tx.rollback().await.unwrap();
    }).await;
}
