#[allow(
    dead_code,
    reason = "Shared retained Account fixture serves multiple native owners"
)]
#[path = "../../support/rust-effective-authority.rs"]
mod auth;
#[allow(
    dead_code,
    reason = "Shared actual Maintenance/Commerce facts also serve parts commands"
)]
#[path = "../../support/maintenance/parts-facts.rs"]
mod facts;
#[path = "../../support/rust-postgres.rs"]
mod fixture;
use auth::*;
use erpx_server_commercial::public::purchases::PackageEnforcementMode;
use erpx_server_maintenance::public::{
    work_commands::MaintenanceWorkCommand, work_wire::normalize_maintenance_change,
};
use erpx_server_runtime::public::postgres::{DatabasePool, PgTransactionError};
use erpx_server_shared::public::{BusinessError, CommandKey, IdentifierSource, RecordId};
use erpx_service::maintenance_transitions::{
    MaintenanceTransitionsSourcePreparation, execute_maintenance_transition,
};
use facts::*;
struct Ids(u64);
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        self.0 += 1;
        RecordId::new(format!("synthetic-transition-event-{}", self.0)).unwrap()
    }
}
fn command(action: &str, version: i64) -> MaintenanceWorkCommand {
    let bytes=serde_json::to_vec(&serde_json::json!({"action":action,"expectedVersion":version,"reason":"Synthetic command evidence"})).unwrap();
    normalize_maintenance_change(&bytes).unwrap().into_command(
        CommandKey::new(format!("synthetic-command-{action}")).unwrap(),
        work_id(),
    )
}
async fn counts(pool: &DatabasePool) -> (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64) {
    sqlx::query_as("SELECT(SELECT count(*)FROM command_receipts),(SELECT count(*)FROM audit_events),(SELECT count(*)FROM outbox_events),(SELECT count(*)FROM maintenance_work_events),(SELECT count(*)FROM operational_asset_events),(SELECT count(*)FROM staff_notifications),(SELECT count(*)FROM inbox_events),(SELECT count(*)FROM inbox_deliveries),(SELECT count(*)FROM finance_journals),(SELECT count(*)FROM operational_inventory_consumptions)").fetch_one(pool.raw()).await.unwrap()
}
fn denied<T>(result: Result<T, PgTransactionError<BusinessError>>, code: &str) {
    assert!(
        matches!(result,Err(PgTransactionError::Business(ref e))if e.code==code),
        "expected {code}"
    );
}
async fn seed(pool: &DatabasePool) {
    seed_work(pool).await;
    seed_parts(pool).await;
    sqlx::query("UPDATE maintenance_work_orders SET status='open'WHERE id='work'")
        .execute(pool.raw())
        .await
        .unwrap();
}
#[tokio::test]
async fn actual_workflow_start_block_resume_have_one_receipt_each_and_exact_replay_without_extra_asset_or_finance_effects()
 {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
  let(authorizer,_)=setup(&control,&holding).await;seed(&holding).await;
  for(action,version,status)in [("start",1,"in_progress"),("block",2,"blocked"),("resume",3,"in_progress")]{
   authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance_request(),MaintenanceTransitionsSourcePreparation,move|c,current|Box::pin(async move{
    assert!(!current.has_permission("finance.write"));let command=command(action,version);let clock=PartsWriteClock(std::sync::atomic::AtomicI64::new(NOW));let mut ids=Ids(u64::try_from(version).unwrap()*10);
    let first=execute_maintenance_transition(c,current,&command,PackageEnforcementMode::Enforced,&clock,&PartsGuardClock,&mut ids).await?;
    assert!(!first.replayed);let result:serde_json::Value=serde_json::from_str(first.result.as_str()).unwrap();assert_eq!(result,serde_json::json!({"id":"work","version":version+1,"status":status}));let consumed_ids=ids.0;
    let replay=execute_maintenance_transition(c,current,&command,PackageEnforcementMode::Enforced,&clock,&PartsGuardClock,&mut ids).await?;assert!(replay.replayed);assert_eq!(replay.result,first.result);assert_eq!(ids.0,consumed_ids);assert_eq!(clock.0.load(std::sync::atomic::Ordering::SeqCst),NOW+2);Ok(())
   })).await.unwrap();
  }
  assert_eq!(counts(&holding).await,(3,3,3,3,1,3,3,3,0,0));
  let state:(String,i64,i64,String)=sqlx::query_as("SELECT w.status,w.version,a.version,a.lifecycle_status FROM maintenance_work_orders w JOIN operational_assets a ON a.id=w.asset_id").fetch_one(holding.raw()).await.unwrap();assert_eq!(state,("in_progress".into(),4,2,"maintenance".into()));
 }).await;}).await;
}
#[tokio::test]
async fn full_workflow_preserves_late_work_cas_before_asset_lifecycle_and_rolls_back_known_receipt()
{
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
  let(authorizer,_)=setup(&control,&holding).await;seed(&holding).await;
  sqlx::query("UPDATE operational_assets SET lifecycle_status='retirement_pending',retirement_reason='Synthetic retirement'WHERE id='asset'").execute(holding.raw()).await.unwrap();
  for(version,code)in [(9_007_199_254_740_991,"MAINTENANCE_STATE_CHANGED"),(1,"CONFLICT")]{
   denied(authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance_request(),MaintenanceTransitionsSourcePreparation,move|c,current|Box::pin(async move{
    execute_maintenance_transition(c,current,&command("start",version),PackageEnforcementMode::Enforced,&PartsWriteClock(std::sync::atomic::AtomicI64::new(NOW)),&PartsGuardClock,&mut Ids(0)).await
   })).await,code);
   assert_eq!(counts(&holding).await,(0,0,0,0,0,0,0,0,0,0));assert_eq!(sqlx::query_scalar::<_,i64>("SELECT version FROM maintenance_work_orders WHERE id='work'").fetch_one(holding.raw()).await.unwrap(),1);
  }
 }).await;}).await;
}
#[tokio::test]
async fn final_outbox_trigger_cannot_rebaseline_physical_asset_after_all_command_notification_and_inbox_effects()
 {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
  let(authorizer,_)=setup(&control,&holding).await;seed(&holding).await;
  sqlx::raw_sql("CREATE FUNCTION synthetic_transition_outbox_rewrite()RETURNS trigger LANGUAGE plpgsql AS $$BEGIN UPDATE operational_assets SET name='UNINTENDED'WHERE id=(SELECT asset_id FROM maintenance_work_orders WHERE id=NEW.aggregate_id);RETURN NEW;END$$;CREATE CONSTRAINT TRIGGER synthetic_transition_outbox_rewrite AFTER INSERT ON outbox_events DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION synthetic_transition_outbox_rewrite()").execute(holding.raw()).await.unwrap();
  denied(authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance_request(),MaintenanceTransitionsSourcePreparation,|c,current|Box::pin(async move{
   execute_maintenance_transition(c,current,&command("start",1),PackageEnforcementMode::Enforced,&PartsWriteClock(std::sync::atomic::AtomicI64::new(NOW)),&PartsGuardClock,&mut Ids(0)).await
  })).await,"ASSET_MAINTENANCE_SOURCE_CHANGED");
  assert_eq!(counts(&holding).await,(0,0,0,0,0,0,0,0,0,0));let state:(i64,i64,bool)=sqlx::query_as("SELECT w.version,a.version,a.name='PRIVATE_ASSET'FROM maintenance_work_orders w JOIN operational_assets a ON a.id=w.asset_id").fetch_one(holding.raw()).await.unwrap();assert_eq!(state,(1,1,true));
 }).await;}).await;
}
#[tokio::test]
async fn exact_command_history_still_requires_current_normal_iam_before_replay() {
    fixture::with_database(2,|control|async move{fixture::with_database(2,move|holding|async move{
  let(authorizer,_)=setup(&control,&holding).await;seed(&holding).await;
  authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance_request(),MaintenanceTransitionsSourcePreparation,|c,current|Box::pin(async move{
   execute_maintenance_transition(c,current,&command("start",1),PackageEnforcementMode::Enforced,&PartsWriteClock(std::sync::atomic::AtomicI64::new(NOW)),&PartsGuardClock,&mut Ids(0)).await
  })).await.unwrap();let before=counts(&holding).await;
  sqlx::raw_sql("INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at)VALUES('*','custom_native_authority',3,'[\"module.maintenance.read\"]','[]','[\"holding\",\"branch\",\"department\"]','fixture',0);UPDATE iam_roles SET published_revision=3 WHERE id='custom_native_authority'").execute(holding.raw()).await.unwrap();
  denied(authorizer.execute_with_sources("acme.example.test",proof(),input(),maintenance_request(),MaintenanceTransitionsSourcePreparation,|c,current|Box::pin(async move{
   execute_maintenance_transition(c,current,&command("start",1),PackageEnforcementMode::Enforced,&PartsWriteClock(std::sync::atomic::AtomicI64::new(NOW)),&PartsGuardClock,&mut Ids(100)).await
  })).await,"PERMISSION_REQUIRED");assert_eq!(counts(&holding).await,before);
 }).await;}).await;
}
