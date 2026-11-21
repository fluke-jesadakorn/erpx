//! Unmounted owner interaction using two leased databases; test-local bridge is not
//! production composition, TS wrapper parity or distributed atomicity.
#![allow(dead_code)]
#[path = "../../support/rust-postgres.rs"] mod fixture;
use erpx_server_identity_access::public::portal_private_session_lease::AuthenticatedPortalLease;
use erpx_server_identity_access::public::portal_session::PortalSessionReadRequest;
use erpx_server_customer_authority::public::portal_session::{PgPortalEnrollment,PortalCustomerRequest,PortalSessionEvidence,PortalSessionVerifier,PortalSessionFuture,resolve_portal_customer};
use erpx_server_customer_360::public::customer_portal::{PgCustomerPortal,PortalCustomerActor,PortalCommandFacts,PortalCommandInput,PortalAction};
use erpx_server_runtime::public::postgres::{DatabasePool,MigrationTrack,load_migrations,migrate_database,PgTransactionError};
use erpx_server_shared::public::BusinessError;
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

use erpx_server_customer_authority::public::portal_evidence::{prepare_portal_evidence_change,record_portal_evidence_change};
use erpx_server_customer_360::public::portal_action_source::*;
use erpx_server_commerce::public::portal_notification_source::*;
use erpx_server_files::public::portal_support_evidence::*;
use erpx_server_collaboration::public::customer_portal_notification::*;
use erpx_server_workflow::public::customer_portal_events::*;
use erpx_server_identity_access::public::staff_notifications::customer_portal_recipients;
use erpx_server_shared::public::{CompanyId,FailureKind};
struct Owners {action:PortalActionWitness,order:PortalOrderScopeWitness, notification:PortalNotificationFacts,event:PortalEventFacts}
impl Owners { async fn check(&self,c:&mut PgConnection)->Result<(),PgTransactionError<BusinessError>>{
 self.action.require_current(c).await?;self.order.require_current(c).await?;let a=self.action.facts();let o=self.order.scope();
 if a.id!=self.notification.action_id||a.id!=self.event.action_id||a.company_id!=o.company_id||a.order_id!=o.id||a.binding_id!=self.event.binding_id||a.customer_id!=self.event.customer_id||a.user_id!=self.event.actor_id||a.request_key!=self.event.command_key||a.action!="request_support"||o.version!=self.notification.order_version||o.version!=self.event.order_version {return Err(BusinessError::new(FailureKind::Conflict,"PORTAL_OWNER_SOURCE_CHANGED","Original Portal source changed.").into());}Ok(())
} }
impl PortalNotificationSource for Owners {
 fn facts(&self)->&PortalNotificationFacts {&self.notification}
 async fn require_current(&self,c:&mut PgConnection)->Result<(),PgTransactionError<BusinessError>>{self.check(c).await}
 async fn recipients(&self,c:&mut PgConnection)->Result<Vec<String>,PgTransactionError<BusinessError>>{let s=self.order.scope();customer_portal_recipients(c,&s.company_id,s.branch_id.as_deref(),s.department_id.as_deref()).await.map_err(Into::into)}
}
impl PortalEventSource for Owners {
 fn facts(&self)->&PortalEventFacts{&self.event}
 async fn require_current(&self,c:&mut PgConnection)->Result<(),PgTransactionError<BusinessError>>{self.check(c).await}
}
#[tokio::test]
async fn authenticated_disabled_mode_owner_effects_share_one_unit() {
 fixture::with_database(4,|control| async move {fixture::with_database(4,move |holding| async move {
  setup_control(&control).await;setup_holding(&holding).await;
  sqlx::raw_sql("INSERT INTO iam_roles(company_id,id,name,built_in,published_revision,status) VALUES('*','portal_reader','Portal reader',true,1,'active');\
  INSERT INTO iam_role_revisions(company_id,role_id,revision,permissions_json,grantable_json,scopes_json,created_by,created_at) VALUES('*','portal_reader',1,'[\"commerce.read\"]','[]','[\"holding\"]','fixture',0);\
  INSERT INTO assignments(id,user_id,role,modules_json,company_id,valid_from,valid_until,version,scope_kind) VALUES('reader-assignment','staff','portal_reader','[]','company',0,9000000000000,1,'holding');\
  INSERT INTO upload_intents(id,owner_type,owner_id,purpose,original_name,media_type,declared_bytes,staging_key,status,created_by,expires_at,created_at,updated_at) VALUES('upload','customer_access_binding','binding-valid','customer_support_evidence','proof.png','image/png',10,'staging','ready','user-valid',4102444800000,0,0);\
  INSERT INTO attachments(id,upload_id,owner_type,owner_id,purpose,object_key,revision,original_name,media_type,bytes,checksum_sha256,created_by,created_at,company_id,detected_media_type,source_state,scanned_at,scanner_definition_at) VALUES('attachment','upload','customer_access_binding','binding-valid','customer_support_evidence','object',1,'proof.png','image/png',10,repeat('a',64),'user-valid',0,'company','image/png','available',1,1);\
  INSERT INTO attachment_sets(id,company_id,target_type,target_id,component_kind,component_ref,purpose_key,expected_target_version,policy_version,classification,audience_json,required_count,max_files,state,created_by,created_at,updated_at) VALUES('set','company','customer_access_binding','binding-valid','action','request_support','customer_support_evidence',2,1,'confidential','[]',0,10,'draft','user-valid',0,0);\
  INSERT INTO attachment_links(id,set_id,attachment_id,state,created_by,created_at,expires_at) VALUES('link','set','attachment','draft','user-valid',0,4102444800000)").execute(holding.raw()).await.unwrap();
  let request=PortalSessionReadRequest{token:"child-valid-token",audience:Some("portal"),environment:"development"};let mut lease=AuthenticatedPortalLease::authenticate(&control,&request,NOW).await.unwrap();
  let view=lease.verified_identity(NOW).await.unwrap();let session=PortalSessionEvidence{session_id:view.session_id().into(),user_id:view.user_id().into(),email:view.email().into(),email_verified:view.email_verified(),authenticated_at:view.authenticated_at(),expires_at:view.expires_at()};
  let mut tx=holding.raw().begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE").await.unwrap();
  // Owner-local fences only: this is not the complete TS global lock chain.
  lock_portal_order_sources(&mut tx).await.unwrap();lock_portal_action_sources(&mut tx).await.unwrap();sqlx::query("LOCK TABLE attachments,upload_intents,attachment_sets,attachment_links,attachment_events IN SHARE ROW EXCLUSIVE MODE").execute(&mut *tx).await.unwrap();lock_customer_portal_notifications(&mut tx).await.unwrap();lock_customer_portal_events(&mut tx).await.unwrap();
  let req=PortalCustomerRequest{token:"child-valid-token",audience:Some("portal"),environment:"development",holding_id:"h001",holding_slug:"acme"};let mut verifier=VerifiedSession(session);let mut enrollment=PgPortalEnrollment::new(&mut tx);let customer=resolve_portal_customer(&mut verifier,&mut enrollment,&req,NOW).await.unwrap();
  let actor=PortalCustomerActor::new(customer.user_id.clone(),customer.email.clone(),None).unwrap();let mut repo=PgCustomerPortal::new(&mut tx);let binding=repo.binding("binding-valid",&actor.email).await.unwrap().unwrap();assert!(repo.identity("binding-valid",&actor).await.unwrap());drop(repo);
  let authority=prepare_portal_evidence_change(&mut tx,&customer,"binding-valid",2,NOW).await.unwrap();let company=CompanyId::new("company").unwrap();let files=prepare_portal_support_evidence(&mut tx,&company,"binding-valid",2,&["set".into()],NOW).await.unwrap();
  let facts=PortalCommandFacts{input:PortalCommandInput{binding_id:"binding-valid".into(),expected_binding_version:2,action:PortalAction::RequestSupport,note:"help".into(),attachment_set_ids:vec!["set".into()]},key:"command-key".into(),id:"action-1".into(),now:NOW,request_hash:"a".repeat(64),actor:actor.clone(),binding};
  PgCustomerPortal::new(&mut tx).record(&facts).await.unwrap();record_portal_evidence_change(&mut tx,&authority,"action-1","command-key",NOW+1).await.unwrap();bind_portal_support_evidence(&mut tx,&files,&actor.user_id,"command-key","action-1").await.unwrap();
  let action=read_portal_action_source(&mut tx,"action-1").await.unwrap().unwrap();let order=read_portal_order_scope(&mut tx,&company,"order-main").await.unwrap().unwrap();let scope=order.scope();
  let notification=PortalNotificationFacts{action_id:action.facts().id.clone(),company_id:scope.company_id.clone(),order_id:scope.id.clone(),order_version:scope.version,branch_id:scope.branch_id.clone(),department_id:scope.department_id.clone(),action:PortalNotificationAction::RequestSupport,command_now:NOW};
  let event=PortalEventFacts{command_key:"command-key".into(),action_id:"action-1".into(),actor_id:actor.user_id.clone(),actor_email:actor.email.clone(),binding_id:"binding-valid".into(),customer_id:"customer-main".into(),order_id:"order-main".into(),order_version:3,action:PortalEventAction::RequestSupport,note:"help".into(),command_now:NOW};
  let owners=Owners{action,order,notification,event};notify_customer_portal_staff(&mut tx,&owners).await.unwrap();let audit_id=uuid::Uuid::new_v4().to_string();let outbox_id=uuid::Uuid::new_v4().to_string();let audit=write_customer_portal_audit(&mut tx,&owners,&audit_id).await.unwrap();let manifest:Vec<serde_json::Value>=files.manifest().into_iter().map(|item|serde_json::to_value(item).unwrap()).collect();write_customer_portal_outbox(&mut tx,&owners,&audit,&outbox_id,&manifest).await.unwrap();
  lease.require_current(NOW+1).await.unwrap();tx.commit().await.unwrap();lease.commit().await.unwrap();
  let counts:(i64,i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM customer_portal_actions WHERE id='action-1'),(SELECT count(*) FROM attachment_links WHERE id='link' AND state='bound'),(SELECT count(*) FROM audit_events WHERE id=$1),(SELECT count(*) FROM outbox_events WHERE id=$2)").bind(audit_id).bind(outbox_id).fetch_one(holding.raw()).await.unwrap();assert_eq!(counts,(1,1,1,1));
  let notice:i64=sqlx::query_scalar("SELECT count(*) FROM staff_notifications WHERE id='customer-portal-action-1-staff' AND record_id='order-main' AND record_version=3").fetch_one(holding.raw()).await.unwrap();assert_eq!(notice,1);
  let version:i64=sqlx::query_scalar("SELECT version FROM customer_access_bindings WHERE id='binding-valid'").fetch_one(holding.raw()).await.unwrap();assert_eq!(version,3);
 }).await;}).await;
}
