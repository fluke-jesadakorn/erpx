#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_field_service::public::service_parts_posting::*;
use erpx_server_runtime::public::postgres::{
    MigrationTrack, PgUnitOfWork, TransactionOptions, load_migrations, migrate_database,
};
use erpx_server_shared::public::{BusinessError, CompanyId};
use std::sync::Arc;
#[tokio::test]
async fn field_service_history_is_company_scoped_and_bound_to_original_transaction() {
    fixture::with_database(2,|pool|async move {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
        let current=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_service_parts_field_history(c).await?;
            let source=read_service_parts_field_history(c,&CompanyId::new("company").unwrap()).await?;
            assert_eq!(source.data().service_json,"null");assert!(!source.data().has_history);
            source.require_current(c).await?;Ok(source)
        })).await.unwrap();
        let current=Arc::new(current);
        let changed=PgUnitOfWork::new(pool.clone()).run::<_,BusinessError,_>(TransactionOptions::default(),|c|{let current=current.clone();Box::pin(async move{current.require_current(c).await})}).await;
        assert!(matches!(changed,Err(erpx_server_runtime::public::postgres::PgTransactionError::Business(e)) if e.code=="SERVICE_PARTS_POSTING_SOURCE_CHANGED"));
    }).await;
}

#[tokio::test]
async fn field_history_lock_waits_before_serializable_snapshot() {
    fixture::with_database(3, |pool| async move {
        let root=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();
        migrate_database(&pool,MigrationTrack::HoldingWrite,&files).await.unwrap();
        sqlx::query("INSERT INTO organization_nodes(id,kind,slug,display_name,company_id) VALUES('company','company','company','Before','company')").execute(pool.raw()).await.unwrap();
        let mut writer=pool.raw().begin().await.unwrap();
        sqlx::query("LOCK TABLE field_service_orders IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();
        sqlx::query("UPDATE organization_nodes SET display_name='After' WHERE id='company'").execute(&mut *writer).await.unwrap();
        let read_pool=pool.clone();
        let task=tokio::spawn(async move { PgUnitOfWork::new(read_pool).run::<_,BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move {
            lock_service_parts_field_history(c).await?;
            let source=read_service_parts_field_history(c,&CompanyId::new("company").unwrap()).await?;
            source.require_current(c).await?;
            Ok(sqlx::query_scalar::<_,String>("SELECT display_name FROM organization_nodes WHERE id='company'").fetch_one(c).await?)
        })).await });
        tokio::time::timeout(std::time::Duration::from_secs(10),async {loop{
            let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_locks WHERE NOT granted AND mode='ShareLock' AND relation='field_service_orders'::regclass AND database=(SELECT oid FROM pg_database WHERE datname=current_database()))").fetch_one(pool.raw()).await.unwrap();
            if waiting{break}tokio::task::yield_now().await;
        }}).await.unwrap();
        writer.commit().await.unwrap();
        assert_eq!(task.await.unwrap().unwrap(),"After");
    }).await;
}
