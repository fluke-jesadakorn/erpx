#[path = "../../support/rust-postgres.rs"]
mod fixture;
use erpx_server_workflow::public::service_count_outbox::*;
use erpx_server_inventory::public::service_count_outbox::*;
use erpx_server_runtime::public::postgres::{load_migrations,migrate_database,MigrationTrack};
use std::path::Path;

#[tokio::test]
async fn malformed_source_is_quarantined_retried_and_dead_lettered_without_legacy_claim() {
 fixture::with_database(2,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
  let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();
  migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
  let db=pool.raw();
  // A syntactically valid receipt carrying an invalid marker cannot be handed
  // to the legacy worker. The valid-source predicate must fail closed.
  sqlx::query("INSERT INTO command_receipts(command_id,operation,record_id,actor_id,request_hash,result_json,created_at,authority_json) VALUES('create','inventory.count.create','bad','actor','hash','[]',1,'{}')").execute(db).await.unwrap();
  sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES('bad-event','update','inventory.count.update','inventory_count','bad',1,'{\"companyId\":\"wrong\"}',1),('other','x','inventory.count.update','inventory_count','ordinary',1,'{}',2)").execute(db).await.unwrap();
  let mut c=db.acquire().await.unwrap();
  let mut legacy=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT e.id FROM outbox_events e WHERE NOT ");
  legacy.push(SERVICE_COUNT_OUTBOX_QUARANTINE).push(" ORDER BY e.id");
  let legacy_ids:Vec<String>=legacy.build_query_scalar().fetch_all(&mut *c).await.unwrap();
  assert_eq!(legacy_ids,vec!["other"]);
  let event=claim(&mut c,SERVICE_COUNT_OUTBOX_QUARANTINE,"lease-1",10).await.unwrap().unwrap();
  assert_eq!(event.id,"bad-event");assert!(!eligible(&mut c,SERVICE_COUNT_OUTBOX_ELIGIBLE,&event).await.unwrap());
  assert!(!complete(&mut c,&ServiceCountEvent{lease_token:"stale".into(),..event.clone()},11).await.unwrap());
  assert!(fail(&mut c,&event,11,"SOURCE_INVALID").await.unwrap());
  let mut now=2012;
  for n in 2..=6 {let event=claim(&mut c,SERVICE_COUNT_OUTBOX_QUARANTINE,&format!("lease-{n}"),now).await.unwrap().unwrap();assert_eq!(event.attempts,n);assert!(fail(&mut c,&event,now,"SOURCE_INVALID").await.unwrap());now+=60_001;}
  let (attempts,dead,error):(i64,Option<i64>,Option<String>)=sqlx::query_as("SELECT attempts,dead_letter_at,last_error FROM outbox_events WHERE id='bad-event'").fetch_one(&mut *c).await.unwrap();assert_eq!(attempts,6);assert!(dead.is_some());assert_eq!(error.as_deref(),Some("SOURCE_INVALID"));
  assert!(claim(&mut c,SERVICE_COUNT_OUTBOX_QUARANTINE,"lease-final",now).await.unwrap().is_none());
 }).await;
}

#[tokio::test]
async fn projection_survives_crash_between_databases_and_lost_lease() {
 fixture::with_database(2,|write|async move{
  fixture::with_database(2,move|read|async move{
   let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
   let migrations=load_migrations(&root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&write,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
   let migrations=load_migrations(&root,MigrationTrack::HoldingRead).await.unwrap();migrate_database(&read,MigrationTrack::HoldingRead,&migrations).await.unwrap();
   sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES('event','command','inventory.count.update','inventory_count','count',2,'{}',1)").execute(write.raw()).await.unwrap();
   let mut w=write.raw().acquire().await.unwrap();let first=claim(&mut w,"TRUE","first",100).await.unwrap().unwrap();
   {let mut tx=read.raw().begin().await.unwrap();project(&mut tx,&first,101).await.unwrap();tx.commit().await.unwrap();}
   // Simulate process death after read commit and expiry before write ack.
   let second=claim(&mut w,"TRUE","second",90_101).await.unwrap().unwrap();
   assert!(!complete(&mut w,&first,90_102).await.unwrap());
   assert!(renew(&mut w,&second,90_102).await.unwrap());
   {let mut tx=read.raw().begin().await.unwrap();project(&mut tx,&second,90_103).await.unwrap();tx.commit().await.unwrap();}
   assert!(complete(&mut w,&second,90_104).await.unwrap());
   let receipts:i64=sqlx::query_scalar("SELECT count(*) FROM projection_receipts WHERE event_id='event'").fetch_one(read.raw()).await.unwrap();assert_eq!(receipts,1);
   let checkpoint:String=sqlx::query_scalar("SELECT last_event_id FROM projection_checkpoints WHERE projector='work-items'").fetch_one(read.raw()).await.unwrap();assert_eq!(checkpoint,"event");
  }).await;
 }).await;
}

#[tokio::test]
async fn simultaneous_claimants_cannot_acquire_the_same_event() {
 fixture::with_database(3,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
  let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
  sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES('only','command','inventory.count.update','inventory_count','count',1,'{}',1)").execute(pool.raw()).await.unwrap();
  let mut first=pool.raw().begin().await.unwrap();
  let event=claim(&mut first,"TRUE","first",100).await.unwrap().unwrap();
  let mut other=pool.raw().acquire().await.unwrap();
  assert!(claim(&mut other,"TRUE","second",100).await.unwrap().is_none());
  first.commit().await.unwrap();
  assert!(claim(&mut other,"TRUE","second",100).await.unwrap().is_none());
  assert!(complete(&mut other,&event,101).await.unwrap());
 }).await;
}

#[tokio::test]
async fn flow_identity_is_append_only_while_progression_remains_mutable() {
 fixture::with_database(2,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
  sqlx::query("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES('f','command','company','expense','same','draft','assign','actor','Actor','reason',1,1)").execute(pool.raw()).await.unwrap();
  for command in ["UPDATE flow_events SET kind='stock_count' WHERE id='f'", "UPDATE flow_events SET actor_name='forged' WHERE id='f'", "DELETE FROM flow_events WHERE id='f'"] {
   let error=sqlx::query(sqlx::AssertSqlSafe(command)).execute(pool.raw()).await.unwrap_err();
   assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("23514"));
  }
  sqlx::query("UPDATE flow_events SET next_step_key='submitted',next_user_id='reviewer' WHERE id='f'").execute(pool.raw()).await.unwrap();
  let row:(String,String)=sqlx::query_as("SELECT next_step_key,next_user_id FROM flow_events WHERE id='f'").fetch_one(pool.raw()).await.unwrap();assert_eq!(row,("submitted".into(),"reviewer".into()));
 }).await;
}

#[tokio::test]
async fn colliding_flow_kinds_are_classified_by_consistent_retained_evidence() {
 fixture::with_database(2,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();let before:Vec<_>=migrations.iter().filter(|m| !m.name().starts_with("1077_")).cloned().collect();migrate_database(&pool,MigrationTrack::HoldingWrite,&before).await.unwrap();
  // A non-object create receipt is an owned malformed source and provides a
  // collision without forging a valid Service source.
  sqlx::query("INSERT INTO command_receipts(command_id,operation,record_id,actor_id,request_hash,result_json,created_at,authority_json) VALUES('create','inventory.count.create','same','actor','hash','[]',1,'{}')").execute(pool.raw()).await.unwrap();
  for (id,kind) in [("expense","expense"),("stock","stock_count"),("unknown","unrecognized"),("conflict","stock_count")] {
   sqlx::query("INSERT INTO command_receipts(command_id,operation,record_id,actor_id,request_hash,result_json,created_at,authority_json) VALUES($1,'flow.assign','same','actor','hash','{}',1,'{\"companyId\":\"company\"}')").bind(id).execute(pool.raw()).await.unwrap();
   sqlx::query("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES($1,$1,'company',$2,'same','draft','assign','actor','Actor','reason',1,1)").bind(id).bind(kind).execute(pool.raw()).await.unwrap();
  }
  sqlx::query("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES('extra','conflict','company','expense','same','draft','assign','actor','Actor','reason',1,1)").execute(pool.raw()).await.unwrap();
  for id in ["expense","stock","unknown","conflict","missing"] {
   sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$1,'flow.assign','flow_assign','same',1,'{\"id\":\"same\",\"version\":1,\"companyId\":\"company\"}',1)").bind(id).execute(pool.raw()).await.unwrap();
  }
  // Preserve genuinely ambiguous historical rows before 1077 installs prospective fencing.
  migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
  let mut query=sqlx::QueryBuilder::<sqlx::Postgres>::new("SELECT e.id,");query.push(SERVICE_COUNT_OUTBOX_QUARANTINE).push(",").push(SERVICE_COUNT_OUTBOX_AMBIGUOUS).push(",").push(SERVICE_COUNT_OUTBOX_ELIGIBLE).push(" FROM outbox_events e ORDER BY e.id");
  let rows:Vec<(String,bool,bool,bool)>=query.build_query_as().fetch_all(pool.raw()).await.unwrap();
  assert_eq!(rows,vec![("conflict".into(),true,true,false),("expense".into(),false,false,false),("missing".into(),true,true,false),("stock".into(),true,false,false),("unknown".into(),true,true,false)]);
  // The ambiguous claimant does not contact the projection database.
  assert!(erpx_service::service_count_worker::run_once(pool.raw(),pool.raw(),"worker-token",10).await.unwrap());
  let (code,processed):(Option<String>,Option<i64>)=sqlx::query_as("SELECT last_error,processed_at FROM outbox_events WHERE id='conflict'").fetch_one(pool.raw()).await.unwrap();
  assert_eq!(code.as_deref(),Some("SOURCE_AMBIGUOUS"));assert_eq!(processed,None);

 }).await;
}

#[tokio::test]
async fn assignment_cardinality_fences_concurrent_and_later_inserts() {
 fixture::with_database(3,|pool|async move{
  let root=Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");let migrations=load_migrations(root,MigrationTrack::HoldingWrite).await.unwrap();migrate_database(&pool,MigrationTrack::HoldingWrite,&migrations).await.unwrap();
  let mut first=pool.raw().begin().await.unwrap();
  sqlx::query("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES('first','same-command','company','stock_count','count','draft','assign','actor','Actor','reason',1,1)").execute(&mut *first).await.unwrap();
  let db=pool.raw().clone();let second=tokio::spawn(async move {sqlx::query("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES('second','same-command','company','expense','other','draft','assign','actor','Actor','reason',1,1)").execute(&db).await});
  tokio::task::yield_now().await;first.commit().await.unwrap();let error=second.await.unwrap().unwrap_err();assert_eq!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("23514"));
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM flow_events WHERE command_id='same-command'").fetch_one(pool.raw()).await.unwrap();assert_eq!(count,1);
  // Establish an older SERIALIZABLE snapshot before a different command's
  // first event commits; the stale inserter may not publish a second event.
  let mut stale=pool.raw().begin().await.unwrap();sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *stale).await.unwrap();
  let absent:i64=sqlx::query_scalar("SELECT count(*) FROM flow_events WHERE command_id='stale-command'").fetch_one(&mut *stale).await.unwrap();assert_eq!(absent,0);
  sqlx::query("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES('fresh','stale-command','company','stock_count','count','draft','assign','actor','Actor','reason',1,1)").execute(pool.raw()).await.unwrap();
  let error=sqlx::query("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES('stale','stale-command','company','expense','other','draft','assign','actor','Actor','reason',1,1)").execute(&mut *stale).await.unwrap_err();
  assert!(matches!(error.as_database_error().and_then(|e|e.code()).as_deref(),Some("40001"|"23514")));stale.rollback().await.unwrap();

  sqlx::raw_sql("INSERT INTO flow_events(id,command_id,company_id,kind,record_id,step_key,action,actor_user_id,actor_name,reason,record_version,created_at) VALUES('multi1','multi','company','expense','one','draft','submit','actor','Actor','reason',1,1),('multi2','multi','company','expense','two','draft','submit','actor','Actor','reason',1,1)").execute(pool.raw()).await.unwrap();
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM flow_events WHERE command_id='multi'").fetch_one(pool.raw()).await.unwrap();assert_eq!(count,2);
 }).await;
}
