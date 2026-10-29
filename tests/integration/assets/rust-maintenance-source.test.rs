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

fn physical_change(action: MaintenancePhysicalAction) -> MaintenancePhysicalChange {
    MaintenancePhysicalChange {
        action,
        expected_asset_version: 1,
        actor_id: erpx_server_shared::public::UserId::new("actor").unwrap(),
        command_key: erpx_server_shared::public::CommandKey::new("physical-command").unwrap(),
        reason: "Synthetic reason".into(),
        at: 300,
    }
}
#[tokio::test]
async fn physical_effects_match_b0_sql_null_and_truthy_paths_with_precomputed_full_images() {
    fixture::with_database(2,|pool|async move {
  setup(&pool).await;
  let cases=[
   (MaintenancePhysicalAction::Start,None,"maintenance","good",true),
   (MaintenancePhysicalAction::Complete {condition:MaintenanceAssetCondition::Unsafe},Some(""),"maintenance","unsafe",true),
   (MaintenancePhysicalAction::Complete {condition:MaintenanceAssetCondition::Degraded},None,"registered","degraded",true),
   (MaintenancePhysicalAction::Complete {condition:MaintenanceAssetCondition::Good},Some(""),"in_service","good",true),
   (MaintenancePhysicalAction::Cancel {downtime_started_at:None},None,"registered","good",false),
   (MaintenancePhysicalAction::Cancel {downtime_started_at:Some(0)},None,"registered","good",false),
   (MaintenancePhysicalAction::Cancel {downtime_started_at:Some(-100)},Some(""),"in_service","good",true),
   (MaintenancePhysicalAction::Cancel {downtime_started_at:Some(100)},None,"registered","good",true),
  ];
  for(action,placement,lifecycle,condition,writes)in cases{
   let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
   sqlx::query("UPDATE operational_assets SET placement=$1 WHERE id='asset'").bind(placement).execute(&mut *tx).await.unwrap();
   let(h,co,id)=scope();let source=read_current_maintenance_asset(&mut tx,&h,&co,&id).await.unwrap().unwrap();
   let receipt=source.prepare_physical_change(&mut tx,physical_change(action)).await.unwrap().apply(&mut tx,&source).await.unwrap();
   let actual:(i64,String,String,Option<String>,i64)=sqlx::query_as("SELECT version,lifecycle_status,condition,placement,updated_at FROM operational_assets WHERE id='asset'").fetch_one(&mut *tx).await.unwrap();
   assert_eq!(actual,(if writes{2}else{1},lifecycle.into(),condition.into(),placement.map(str::to_string),if writes{300}else{0}));
   let events:i64=sqlx::query_scalar("SELECT count(*)FROM operational_asset_events").fetch_one(&mut *tx).await.unwrap();assert_eq!(events,i64::from(writes));
   if writes{let event:(String,String,String,i64)=sqlx::query_as("SELECT actor_id,reason,lifecycle_status,version FROM operational_asset_events").fetch_one(&mut *tx).await.unwrap();assert_eq!(event,("actor".into(),"Synthetic reason".into(),lifecycle.into(),2));}
   receipt.require_current(&mut tx).await.unwrap();
   // A post-effect source rewrite cannot become the final expected image.
   sqlx::query("UPDATE operational_assets SET name='tampered private display' WHERE id='asset'").execute(&mut *tx).await.unwrap();
   assert!(matches!(receipt.require_current(&mut tx).await,Err(PgTransactionError::Business(ref e))if e.code=="ASSET_MAINTENANCE_SOURCE_CHANGED"));
   tx.rollback().await.unwrap();
  }
 }).await;
}
#[tokio::test]
async fn physical_start_guard_is_late_and_preserves_original_generic_conflict() {
    fixture::with_database(2,|pool|async move{
  setup(&pool).await;
  for(stale,lifecycle)in [(true,"registered"),(false,"retirement_pending")]{
   let mut tx=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
   sqlx::query("UPDATE operational_assets SET lifecycle_status=$1,retirement_reason='Synthetic retirement' WHERE id='asset'").bind(lifecycle).execute(&mut *tx).await.unwrap();
   let(h,co,id)=scope();let source=read_current_maintenance_asset(&mut tx,&h,&co,&id).await.unwrap().unwrap();let mut input=physical_change(MaintenancePhysicalAction::Start);if stale{input.expected_asset_version=2;}
   // Pure preparation does not move this legacy refusal before the Work CAS.
   let prepared=source.prepare_physical_change(&mut tx,input).await.unwrap();
   assert!(matches!(prepared.apply(&mut tx,&source).await,Err(PgTransactionError::Business(ref e))if e.code=="CONFLICT"));
   assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM operational_asset_events").fetch_one(&mut *tx).await.unwrap(),0);
   tx.rollback().await.unwrap();
  }
 }).await;
}
#[tokio::test]
async fn physical_receipt_detects_deferred_trigger_rewrite_and_rolls_back_all_owner_effects() {
    fixture::with_database(2,|pool|async move{
  setup(&pool).await;
  sqlx::raw_sql("CREATE FUNCTION synthetic_asset_rewrite()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN UPDATE operational_assets SET name='deferred rewrite' WHERE id=NEW.asset_id;RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER synthetic_asset_rewrite AFTER INSERT ON operational_asset_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_asset_rewrite()").execute(pool.raw()).await.unwrap();
  let result=PgUnitOfWork::new(pool.clone()).run::<(),BusinessError,_>(TransactionOptions::default(),|c|Box::pin(async move{
   let(h,co,id)=scope();let source=read_current_maintenance_asset(c,&h,&co,&id).await?.unwrap();let receipt=source.prepare_physical_change(c,physical_change(MaintenancePhysicalAction::Start)).await?.apply(c,&source).await?;
   sqlx::query("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *c).await?;receipt.require_current(c).await?;Ok(())
  })).await;
  assert!(matches!(result,Err(PgTransactionError::Business(ref e))if e.code=="ASSET_MAINTENANCE_SOURCE_CHANGED"));
  assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM operational_assets WHERE id='asset'").fetch_one(pool.raw()).await.unwrap(),1);
  assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*)FROM operational_asset_events").fetch_one(pool.raw()).await.unwrap(),0);
 }).await;
}
#[tokio::test]
async fn physical_preparation_cannot_escape_into_a_second_transaction() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        let (h, co, id) = scope();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let source = read_current_maintenance_asset(&mut tx, &h, &co, &id)
            .await
            .unwrap()
            .unwrap();
        let prepared = source
            .prepare_physical_change(&mut tx, physical_change(MaintenancePhysicalAction::Start))
            .await
            .unwrap();
        tx.commit().await.unwrap();
        let mut tx = pool.raw().begin().await.unwrap();
        sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await
            .unwrap();
        let current = read_current_maintenance_asset(&mut tx, &h, &co, &id)
            .await
            .unwrap()
            .unwrap();
        assert!(prepared.apply(&mut tx, &current).await.is_err());
        tx.rollback().await.unwrap();
    })
    .await;
}

#[tokio::test]
async fn nul_reason_waits_for_physical_apply_and_late_start_guard() {
    fixture::with_database(2, |pool| async move {
        setup(&pool).await;
        for action in [
            MaintenancePhysicalAction::Start,
            MaintenancePhysicalAction::Complete { condition: MaintenanceAssetCondition::Good },
            MaintenancePhysicalAction::Cancel { downtime_started_at: Some(100) },
            MaintenancePhysicalAction::Cancel { downtime_started_at: None },
            MaintenancePhysicalAction::Cancel { downtime_started_at: Some(0) },
        ] {
            let no_event = matches!(action, MaintenancePhysicalAction::Cancel { downtime_started_at: None | Some(0) });
            let mut tx = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            let (h, co, id) = scope();
            let source = read_current_maintenance_asset(&mut tx, &h, &co, &id).await.unwrap().unwrap();
            let mut change = physical_change(action);
            change.reason = "reason\0fixture".into();
            let prepared = source.prepare_physical_change(&mut tx, change).await.unwrap();
            let result = prepared.apply(&mut tx, &source).await;
            if no_event {
                result.unwrap().require_current(&mut tx).await.unwrap();
            } else {
                assert!(matches!(result, Err(PgTransactionError::Persistence(_))));
            }
            tx.rollback().await.unwrap();
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM operational_assets WHERE id='asset'").fetch_one(pool.raw()).await.unwrap(), 1);
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM operational_asset_events").fetch_one(pool.raw()).await.unwrap(), 0);
        }
        for (stale, lifecycle) in [(true,"registered"), (false,"retirement_pending")] {
            let mut tx = pool.raw().begin().await.unwrap();
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *tx).await.unwrap();
            sqlx::query("UPDATE operational_assets SET lifecycle_status=$1,retirement_reason='Synthetic retirement' WHERE id='asset'").bind(lifecycle).execute(&mut *tx).await.unwrap();
            let (h, co, id) = scope();
            let source = read_current_maintenance_asset(&mut tx, &h, &co, &id).await.unwrap().unwrap();
            let mut change = physical_change(MaintenancePhysicalAction::Start);
            change.reason = "reason\0fixture".into();
            if stale { change.expected_asset_version = 2; }
            let prepared = source.prepare_physical_change(&mut tx, change).await.unwrap();
            assert!(matches!(prepared.apply(&mut tx, &source).await, Err(PgTransactionError::Business(ref e)) if e.code=="CONFLICT"));
            assert_eq!(sqlx::query_scalar::<_,i64>("SELECT count(*) FROM operational_asset_events").fetch_one(&mut *tx).await.unwrap(), 0);
            tx.rollback().await.unwrap();
        }
    }).await;
}
