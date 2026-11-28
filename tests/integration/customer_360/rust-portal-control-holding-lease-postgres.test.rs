//! Two leased databases and real owner data; test-local session bridge is not
//! production composition, TS wrapper parity or distributed atomicity.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_identity_access_server::public::portal_private_session_lease::AuthenticatedPortalLease;
use erpx_identity_access_server::public::portal_session::PortalSessionReadRequest;
use erpx_customer_authority_server::public::portal_session::{PgPortalEnrollment,PortalCustomerRequest,PortalSessionEvidence,PortalSessionVerifier,PortalSessionFuture,resolve_portal_customer};
use erpx_customer_360_server::public::customer_portal::{PgCustomerPortal,PortalCustomerActor,PortalCommandFacts,PortalCommandInput,PortalAction};
use erpx_platform_runtime_server::public::postgres::{DatabasePool,MigrationTrack,load_migrations,migrate_database,PgTransactionError};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::{Connection,PgConnection};
const NOW:i64=1_800_000_000_000;
async fn setup_control(pool: &DatabasePool) {
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

async fn setup_holding(pool: &DatabasePool) {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../migrations/postgres");
    let files = load_migrations(root, MigrationTrack::HoldingWrite)
        .await
        .unwrap();
    migrate_database(pool, MigrationTrack::HoldingWrite, &files)
        .await
        .unwrap();
    sqlx::raw_sql(
        r#"INSERT INTO organization_nodes(id,kind,slug,display_name,parent_id,company_id,branch_id,status,hierarchy_revision) VALUES
             ('company','company','company','Company',NULL,'company',NULL,'active',1);
           INSERT INTO finance_parties(id,company_id,kind,code,name,currency,status,version,created_at,updated_at) VALUES
             ('customer-main','company','customer','C-001','Synthetic Customer','THB','active',1,10,10);
           INSERT INTO commerce_orders(id,company_id,branch_id,department_id,kind,number,party_id,warehouse_id,currency,order_date,due_date,note,state,subtotal_minor,tax_minor,version,created_by,created_at,updated_at) VALUES
             ('order-main','company',NULL,NULL,'quote','QT-100','customer-main',NULL,'THB','2026-09-20','2026-09-25','Main','confirmed',100,0,3,'requester',100,100);
           INSERT INTO customer_access_bindings(id,company_id,order_id,customer_id,email,expires_at,status,reason,created_by,approved_by,reviewed_at,version,created_at,updated_at) VALUES
             ('binding-valid','company','order-main','customer-main','valid@example.test',1800000060000,'approved','Approved portal access','requester','reviewer',100,2,100,100),
             ('binding-other-holding','company','order-main','customer-main','wrong-holding@example.test',1800000060000,'approved','Approved portal access','requester','reviewer',100,2,100,100),
             ('binding-expired','company','order-main','customer-main','expired@example.test',101,'approved','Expired portal access','requester','reviewer',100,2,100,100),
             ('binding-revoked','company','order-main','customer-main','revoked@example.test',1800000060000,'revoked','Revoked portal access','requester','reviewer',100,2,100,100),
             ('binding-unenrolled','company','order-main','customer-main','unenrolled@example.test',1800000060000,'approved','No accepted invitation','requester','reviewer',100,2,100,100);
           INSERT INTO customer_access_identities(binding_id,user_id,email,bound_at) VALUES
             ('binding-valid','user-valid','valid@example.test',100),
             ('binding-other-holding','customer-wrong-holding','wrong-holding@example.test',100),
             ('binding-expired','customer-expired','expired@example.test',100),
             ('binding-revoked','customer-revoked','revoked@example.test',100),
             ('binding-unenrolled','customer-unenrolled','unenrolled@example.test',100);
           INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at) VALUES
             ('invite-valid','customer.portal.invite','requester',repeat('a',64),'binding-valid','{}',100),
             ('invite-other','customer.portal.invite','requester',repeat('b',64),'binding-other-holding','{}',100),
             ('invite-expired','customer.portal.invite','requester',repeat('c',64),'binding-expired','{}',100),
             ('invite-revoked','customer.portal.invite','requester',repeat('d',64),'binding-revoked','{}',100);
           INSERT INTO customer_portal_invitations(id,binding_id,holding_slug,generation,email,token_hash,token_key_version,token_expires_at,state,attempts,next_attempt_at,created_by,created_at,accepted_at,accepted_user_id,command_id) VALUES
             ('invitation-valid','binding-valid','acme',1,'valid@example.test',repeat('1',64),1,1800000060000,'accepted',1,100,'requester',100,150,'user-valid','invite-valid'),
             ('invitation-other','binding-other-holding','other-tenant',1,'wrong-holding@example.test',repeat('2',64),1,1800000060000,'accepted',1,100,'requester',100,150,'customer-wrong-holding','invite-other'),
             ('invitation-expired','binding-expired','acme',1,'expired@example.test',repeat('3',64),1,1800000060000,'accepted',1,100,'requester',100,150,'customer-expired','invite-expired'),
             ('invitation-revoked','binding-revoked','acme',1,'revoked@example.test',repeat('4',64),1,1800000060000,'accepted',1,100,'requester',100,150,'customer-revoked','invite-revoked');"#,
    )
    .execute(pool.raw())
    .await
    .unwrap();
}

// This bridge carries a genuine Control proof obtained by normal native
// verification. It does not itself provide production authentication.
struct VerifiedSession(PortalSessionEvidence);
impl PortalSessionVerifier for VerifiedSession {
 type Error=PgTransactionError<BusinessError>;
 fn verify_session<'a>(&'a mut self,_:&'a PortalCustomerRequest<'_>,_:i64)->PortalSessionFuture<'a,Option<PortalSessionEvidence>,Self::Error>{Box::pin(async move {Ok(Some(self.0.clone()))})}
}
async fn write_action(c:&mut PgConnection,session:PortalSessionEvidence,action:PortalAction,id:&str,key:&str)->Result<(),PgTransactionError<BusinessError>>{
 let request=PortalCustomerRequest{token:"child-valid-token",audience:Some("portal"),environment:"development",holding_id:"h001",holding_slug:"acme"};
 let mut verifier=VerifiedSession(session);let mut enrollment=PgPortalEnrollment::new(&mut *c);
 let resolved=resolve_portal_customer(&mut verifier,&mut enrollment,&request,NOW).await?;
 let actor=PortalCustomerActor::new(resolved.user_id,resolved.email,None)?;let mut repo=PgCustomerPortal::new(c);
 let binding=repo.binding("binding-valid",&actor.email).await?.expect("seeded actual binding");assert!(repo.identity("binding-valid",&actor).await?);
 let facts=PortalCommandFacts{input:PortalCommandInput{binding_id:"binding-valid".into(),expected_binding_version:2,action,note:"".into(),attachment_set_ids:vec![]},key:key.into(),id:id.into(),now:NOW,request_hash:"a".repeat(64),actor,binding};repo.record(&facts).await
}
#[tokio::test]
async fn control_lease_holds_session_writer_until_holding_commit() {
 fixture::with_database(4,|control| async move {fixture::with_database(3,move |holding| async move {
  setup_control(&control).await;setup_holding(&holding).await;let request=PortalSessionReadRequest{token:"child-valid-token",audience:Some("portal"),environment:"development"};
  let mut lease=AuthenticatedPortalLease::authenticate(&control,&request,NOW).await.unwrap();
  let view=lease.verified_identity(NOW).await.unwrap();let session=PortalSessionEvidence{session_id:view.session_id().into(),user_id:view.user_id().into(),email:view.email().into(),email_verified:view.email_verified(),authenticated_at:view.authenticated_at(),expires_at:view.expires_at()};
  let mut tx=holding.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();write_action(&mut tx,session,PortalAction::RequestSupport,"action-1","command-key").await.unwrap();
  let other=control.clone();let (send,recv)=tokio::sync::oneshot::channel();let writer=tokio::spawn(async move {let mut c=other.raw().acquire().await.unwrap();let pid:i32=sqlx::query_scalar("SELECT pg_backend_pid()").fetch_one(&mut *c).await.unwrap();send.send(pid).unwrap();sqlx::query("DELETE FROM session WHERE id='child-valid'").execute(&mut *c).await.unwrap();});
  let pid=recv.await.unwrap();let mut blocked=false;for _ in 0..200 {blocked=sqlx::query_scalar("SELECT cardinality(pg_blocking_pids($1))>0").bind(pid).fetch_one(control.raw()).await.unwrap();if blocked{break;}tokio::time::sleep(std::time::Duration::from_millis(10)).await;}assert!(blocked,"Control session writer must wait");
  lease.require_current(NOW+1).await.unwrap();tx.commit().await.unwrap();lease.commit().await.unwrap();tokio::time::timeout(std::time::Duration::from_secs(5),writer).await.unwrap().unwrap();
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM customer_portal_actions WHERE id='action-1'").fetch_one(holding.raw()).await.unwrap();assert_eq!(count,1);
 }).await;}).await;
}
#[tokio::test]
async fn expired_control_session_before_holding_commit_rolls_back_action() {
 fixture::with_database(4,|control| async move {fixture::with_database(3,move |holding| async move {
  setup_control(&control).await;setup_holding(&holding).await;let request=PortalSessionReadRequest{token:"child-valid-token",audience:Some("portal"),environment:"development"};let mut lease=AuthenticatedPortalLease::authenticate(&control,&request,NOW).await.unwrap();
  let view=lease.verified_identity(NOW).await.unwrap();let session=PortalSessionEvidence{session_id:view.session_id().into(),user_id:view.user_id().into(),email:view.email().into(),email_verified:view.email_verified(),authenticated_at:view.authenticated_at(),expires_at:view.expires_at()};
  let mut tx=holding.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();write_action(&mut tx,session,PortalAction::RequestSupport,"action-1","command-key").await.unwrap();
  assert!(lease.require_current(NOW+60_000).await.is_err());tx.rollback().await.unwrap();lease.rollback().await.unwrap();
  let count:i64=sqlx::query_scalar("SELECT count(*) FROM customer_portal_actions WHERE id='action-1'").fetch_one(holding.raw()).await.unwrap();assert_eq!(count,0);
 }).await;}).await;
}
