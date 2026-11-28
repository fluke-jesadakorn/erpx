//! Real Control rows; lease requires a proof obtained by normal authentication.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_identity_access_server::public::portal_private_session_lease::{AuthenticatedPortalLease,PortalPrivateSessionLease};
use erpx_identity_access_server::public::portal_session::{verify_portal_session,PortalSessionReadRequest};
use erpx_platform_runtime_server::public::postgres::{DatabasePool,MigrationTrack,load_migrations,migrate_database};
const NOW:i64=1_800_000_000_000;
async fn setup(pool: &DatabasePool) {
        let root =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
        let files = load_migrations(root, MigrationTrack::Control)
            .await
            .unwrap();
        migrate_database(pool, MigrationTrack::Control, &files)
            .await
            .unwrap();
        sqlx::raw_sql(
            r#"
            INSERT INTO "user"(id,name,email,"emailVerified","createdAt","updatedAt") VALUES
              ('user-valid','Valid','valid@example.test',1,100,100),
              ('user-unverified','Unverified','unverified@example.test',0,100,100),
              ('user-suspended','Suspended','suspended@example.test',1,100,100),
              ('user-expired','Expired','expired@example.test',1,100,100),
              ('user-nodevice','No Device','nodevice@example.test',1,100,100),
              ('user-wrong-parent','Wrong Parent','wrong-parent@example.test',1,100,100),
              ('user-direct','Direct','direct@example.test',1,100,100);
            INSERT INTO principals(id,user_id,display_name,status,created_at) VALUES
              ('principal-suspended','user-suspended','Suspended','suspended',100);
            INSERT INTO session(id,"expiresAt",token,"createdAt","updatedAt","ipAddress","userAgent","userId") VALUES
              ('parent-valid',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'parent-valid-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-valid'),
              ('child-valid',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'child-valid-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-valid'),
              ('parent-unverified',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'parent-unverified-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-unverified'),
              ('child-unverified',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'child-unverified-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-unverified'),
              ('parent-suspended',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'parent-suspended-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-suspended'),
              ('child-suspended',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'child-suspended-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-suspended'),
              ('parent-expired',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'parent-expired-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-expired'),
              ('child-expired',to_char(to_timestamp(1799999999.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'child-expired-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-expired'),
              ('parent-nodevice',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'parent-nodevice-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-nodevice'),
              ('child-nodevice',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'child-nodevice-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-nodevice'),
              ('parent-wrong-parent',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'parent-wrong-parent-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-wrong-parent'),
              ('child-wrong-parent',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'child-wrong-parent-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-wrong-parent'),
              ('direct',to_char(to_timestamp(1800000060.0) AT TIME ZONE 'UTC','YYYY-MM-DD"T"HH24:MI:SS.MS"Z"'),'direct-token','2026-01-01T00:00:00.000Z','2026-01-01T00:00:00.000Z',NULL,NULL,'user-direct');
            INSERT INTO sso_session_bindings(session_id,audience,environment,parent_session_id,authenticated_at,absolute_expires_at) VALUES
              ('parent-valid','account','development',NULL,1799999999000,1800000060000),
              ('child-valid','portal','development','parent-valid',1799999999000,1800000060000),
              ('parent-unverified','account','development',NULL,1799999999000,1800000060000),
              ('child-unverified','portal','development','parent-unverified',1799999999000,1800000060000),
              ('parent-suspended','account','development',NULL,1799999999000,1800000060000),
              ('child-suspended','portal','development','parent-suspended',1799999999000,1800000060000),
              ('parent-expired','account','development',NULL,1799999999000,1800000060000),
              ('child-expired','portal','development','parent-expired',1799999999000,1800000060000),
              ('parent-nodevice','account','development',NULL,1799999999000,1800000060000),
              ('child-nodevice','portal','development','parent-nodevice',1799999999000,1800000060000),
              ('parent-wrong-parent','portal','development',NULL,1799999999000,1800000060000),
              ('child-wrong-parent','portal','development','parent-wrong-parent',1799999999000,1800000060000);
            INSERT INTO account_device_browsers(id,active_session_id,revision,updated_at) VALUES
              ('browser-valid','parent-valid',1,100),
              ('browser-unverified','parent-unverified',1,100),
              ('browser-suspended','parent-suspended',1,100),
              ('browser-expired','parent-expired',1,100),
              ('browser-nodevice',NULL,1,100),
              ('browser-wrong-parent','parent-wrong-parent',1,100);
            INSERT INTO account_device_accounts(session_id,browser_id,handle) VALUES
              ('parent-valid','browser-valid','handle-valid'),
              ('parent-unverified','browser-unverified','handle-unverified'),
              ('parent-suspended','browser-suspended','handle-suspended'),
              ('parent-expired','browser-expired','handle-expired'),
              ('parent-nodevice','browser-nodevice','handle-nodevice'),
              ('parent-wrong-parent','browser-wrong-parent','handle-wrong-parent');
            "#,
        )
        .execute(pool.raw())
        .await
        .unwrap();
    }

#[tokio::test]
async fn lease_fences_real_session_writer_and_rechecks_current_data() {
 fixture::with_database(4,|pool| async move {
  setup(&pool).await;let mut c=pool.raw().acquire().await.unwrap();
  let request=PortalSessionReadRequest{token:"child-valid-token",audience:Some("portal"),environment:"development"};
  let proof=verify_portal_session(&mut c,&request,NOW).await.unwrap().expect("real session");drop(c);
  let mut lease=PortalPrivateSessionLease::begin(&pool,proof.clone(),Some("portal".into()),NOW).await.unwrap();
  lease.require_current(NOW+1).await.unwrap();
  let other=pool.clone();let (send,recv)=tokio::sync::oneshot::channel();
  let writer=tokio::spawn(async move {let mut c=other.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(pid).unwrap();sqlx::query("DELETE FROM session WHERE id='child-valid'").execute(&mut *c).await.unwrap();});
  let pid=recv.await.unwrap();let mut blocked=false;for _ in 0..200 {blocked=sqlx::query_scalar("SELECT cardinality(pg_blocking_pids($1))>0").bind(pid).fetch_one(pool.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}assert!(blocked,"session writer must wait for lease");
  lease.commit().await.unwrap();tokio::time::timeout(std::time::Duration::from_secs(5),writer).await.unwrap().unwrap();
  assert!(PortalPrivateSessionLease::begin(&pool,proof,Some("portal".into()),NOW).await.is_err());
 }).await;
}
#[tokio::test]
async fn lease_rechecks_fresh_expiry_and_verified_email() {
 fixture::with_database(4,|pool| async move {setup(&pool).await;let mut c=pool.raw().acquire().await.unwrap();let request=PortalSessionReadRequest{token:"child-valid-token",audience:Some("portal"),environment:"development"};let proof=verify_portal_session(&mut c,&request,NOW).await.unwrap().unwrap();drop(c);
 let mut lease=PortalPrivateSessionLease::begin(&pool,proof,Some("portal".into()),NOW).await.unwrap();
 assert!(lease.require_current(NOW+60_000).await.is_err());
 sqlx::query("UPDATE \"user\" SET \"emailVerified\"=0 WHERE id='user-valid'").execute(pool.raw()).await.unwrap();
 assert!(lease.require_current(NOW+1).await.is_err());lease.rollback().await.unwrap();}).await;
}

#[tokio::test]
async fn authenticated_lease_requires_actual_token_and_borrows_current_identity() {
 fixture::with_database(3,|pool| async move {setup(&pool).await;
  let bad=PortalSessionReadRequest{token:"wrong-token",audience:Some("portal"),environment:"development"};
  assert!(AuthenticatedPortalLease::authenticate(&pool,&bad,NOW).await.is_err());
  let good=PortalSessionReadRequest{token:"child-valid-token",audience:Some("portal"),environment:"development"};
  let mut c=pool.raw().acquire().await.unwrap();let mut forged=verify_portal_session(&mut c,&good,NOW).await.unwrap().unwrap();drop(c);
  forged.user_id="attacker".into();assert!(PortalPrivateSessionLease::begin(&pool,forged,Some("portal".into()),NOW).await.is_err());
  let mut lease=AuthenticatedPortalLease::authenticate(&pool,&good,NOW).await.unwrap();
  let view=lease.verified_identity(NOW+1).await.unwrap();
  assert_eq!((view.session_id(),view.user_id(),view.email()),("child-valid","user-valid","valid@example.test"));
  assert!(view.email_verified());
  assert!(lease.verified_identity(NOW+60_000).await.is_err());lease.rollback().await.unwrap();
 }).await;
}

#[tokio::test]
async fn direct_session_and_missing_device_match_private_assertion_rules() {
 fixture::with_database(4,|pool| async move {
  setup(&pool).await;let mut c=pool.raw().acquire().await.unwrap();
  let direct=PortalSessionReadRequest{token:"direct-token",audience:None,environment:"development"};
  let direct_proof=verify_portal_session(&mut c,&direct,NOW).await.unwrap().unwrap();
  let portal=PortalSessionReadRequest{token:"child-valid-token",audience:Some("portal"),environment:"development"};
  let proof=verify_portal_session(&mut c,&portal,NOW).await.unwrap().unwrap();drop(c);
  let mut direct_lease=PortalPrivateSessionLease::begin(&pool,direct_proof,None,NOW).await.unwrap();direct_lease.require_current(NOW+1).await.unwrap();direct_lease.commit().await.unwrap();
  // Proof was obtained while a selected device existed; remove the device row
  // after normal authentication to test the TS assertion's optional behavior.
  sqlx::query("DELETE FROM account_device_accounts WHERE session_id='parent-valid'").execute(pool.raw()).await.unwrap();
  let mut lease=PortalPrivateSessionLease::begin(&pool,proof,Some("portal".into()),NOW).await.unwrap();lease.require_current(NOW+1).await.unwrap();lease.rollback().await.unwrap();
 }).await;
}
#[tokio::test]
async fn lease_lock_timeout_returns_busy_and_rolls_back_control_transaction() {
 fixture::with_database(4,|pool| async move {
  setup(&pool).await;let mut c=pool.raw().acquire().await.unwrap();let request=PortalSessionReadRequest{token:"child-valid-token",audience:Some("portal"),environment:"development"};let proof=verify_portal_session(&mut c,&request,NOW).await.unwrap().unwrap();drop(c);
  let mut writer=pool.raw().begin().await.unwrap();sqlx::query("LOCK TABLE session IN ROW EXCLUSIVE MODE").execute(&mut *writer).await.unwrap();
  let error=match tokio::time::timeout(std::time::Duration::from_secs(8),PortalPrivateSessionLease::begin(&pool,proof.clone(),Some("portal".into()),NOW)).await.unwrap(){Ok(_)=>panic!("lease unexpectedly acquired"),Err(error)=>error};
  assert!(matches!(error,erpx_platform_runtime_server::public::postgres::PgTransactionError::Business(error) if error.code=="SESSION_AUTHORITY_BUSY"));
  writer.rollback().await.unwrap();let lease=PortalPrivateSessionLease::begin(&pool,proof,Some("portal".into()),NOW).await.unwrap();lease.rollback().await.unwrap();
 }).await;
}
#[tokio::test]
async fn removed_child_parent_or_binding_rejects_previously_authenticated_session() {
 for mutation in ["DELETE FROM session WHERE id='child-valid'", "DELETE FROM session WHERE id='parent-valid'", "DELETE FROM sso_session_bindings WHERE session_id='child-valid'"] {
  fixture::with_database(3,move |pool| async move {
   setup(&pool).await;let mut c=pool.raw().acquire().await.unwrap();let request=PortalSessionReadRequest{token:"child-valid-token",audience:Some("portal"),environment:"development"};let proof=verify_portal_session(&mut c,&request,NOW).await.unwrap().unwrap();drop(c);
   sqlx::query(mutation).execute(pool.raw()).await.unwrap();
   let error=match PortalPrivateSessionLease::begin(&pool,proof,Some("portal".into()),NOW).await {Ok(_)=>panic!("revoked session accepted"),Err(error)=>error};
   assert!(matches!(error,erpx_platform_runtime_server::public::postgres::PgTransactionError::Business(error) if error.code=="SESSION_REVOKED"));
  }).await;
 }
}

// Temporary TS assertion oracle; its lease and role checks run before source
// queries. It does not run the global runtime wrapper or normal authentication.
fn legacy_assertion(database:&str,proof:&erpx_identity_access_server::public::portal_session::PortalSessionProof,audience:Option<&str>,now:i64)->serde_json::Value {
 use std::process::{Command,Stdio};
 let token=database.strip_prefix("erpx_dev_rust_").expect("leased database");
 let node=std::env::var("ERPX_NODE_BIN").expect("pinned Node required");
 let helper=std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../scripts/testing/fixtures/portal-private-session-legacy-assert.ts");
 let mut child=Command::new(node).arg("--conditions=react-server").arg("--import=tsx").arg(helper).current_dir(std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().expect("start isolated oracle");
 serde_json::to_writer(child.stdin.take().unwrap(),&serde_json::json!({"database":database,"token":token,"actor":{"userId":proof.user_id,"sessionId":proof.session_id,"email":proof.email,"audience":audience},"now":now})).unwrap();
 let out=child.wait_with_output().unwrap();let stderr=String::from_utf8_lossy(&out.stderr); let stage=["input","lease-check","transaction","begin","session-lock","sso-lock","assertion","rollback"].into_iter().find(|stage|stderr.contains(&format!("PORTAL_PRIVATE_SESSION_ORACLE_FAILED:{stage}"))).unwrap_or("startup"); assert!(out.status.success(),"legacy assertion oracle failed at {stage}");serde_json::from_slice(&out.stdout).expect("bounded oracle result")
}
async fn legacy_assertion_async(database:&str,proof:&erpx_identity_access_server::public::portal_session::PortalSessionProof,audience:Option<&str>,now:i64)->serde_json::Value {
 let db=database.to_owned();let proof=proof.clone();let audience=audience.map(str::to_owned);
 tokio::task::spawn_blocking(move||legacy_assertion(&db,&proof,audience.as_deref(),now)).await.unwrap()
}
#[tokio::test]
async fn actual_ts_assertion_matches_native_on_same_control_rows() {
 fixture::with_database(4,|pool| async move {
  setup(&pool).await;let mut c=pool.raw().acquire().await.unwrap();let database:String=sqlx::query_scalar("SELECT current_database()").fetch_one(&mut *c).await.unwrap();
  let portal=PortalSessionReadRequest{token:"child-valid-token",audience:Some("portal"),environment:"development"};let proof=verify_portal_session(&mut c,&portal,NOW).await.unwrap().unwrap();
  let direct=PortalSessionReadRequest{token:"direct-token",audience:None,environment:"development"};let direct_proof=verify_portal_session(&mut c,&direct,NOW).await.unwrap().unwrap();drop(c);
  for (p,a,t,expected) in [(&proof,Some("portal"),NOW,true),(&direct_proof,None,NOW,true),(&proof,Some("portal"),NOW+60_000,false),(&proof,Some("wrong"),NOW,false)] {
   let ts=legacy_assertion_async(&database,p,a,t).await;let native=PortalPrivateSessionLease::begin(&pool,p.clone(),a.map(str::to_owned),t).await;
   assert_eq!(ts.get("ok").and_then(|v|v.as_bool()).unwrap_or(false),expected);
   match native {Ok(lease)=>{assert!(expected);lease.rollback().await.unwrap();},Err(_)=>assert!(!expected)}
  }
  sqlx::query("DELETE FROM account_device_accounts WHERE session_id='parent-valid'").execute(pool.raw()).await.unwrap();
  assert_eq!(legacy_assertion_async(&database,&proof,Some("portal"),NOW).await["ok"],true);
  let lease=PortalPrivateSessionLease::begin(&pool,proof.clone(),Some("portal".into()),NOW).await.unwrap();lease.rollback().await.unwrap();
  sqlx::query("UPDATE \"user\" SET \"emailVerified\"=0 WHERE id='user-valid'").execute(pool.raw()).await.unwrap();
  assert_eq!(legacy_assertion_async(&database,&proof,Some("portal"),NOW).await["error"]["code"],"SESSION_REVOKED");
  assert!(PortalPrivateSessionLease::begin(&pool,proof,Some("portal".into()),NOW).await.is_err());
 }).await;
}
