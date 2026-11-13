//! Native portal-support commands and owner read-audit port.
use crate::domain::portal_support_commands::{
    AcknowledgeInput, CloseInput, CustomerActor, Receipt, RespondInput, SupportActor,
    access_active, access_changed, already_acknowledged, feature_unavailable, identity_required,
    idempotency_conflict, independent_closer, independent_responder, not_found, readback_invalid,
    request_closed, response_changed, source_changed,
};
use erpx_server_commercial::public::capability_decision::PgCapabilityDecisionReader;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, current_package_catalogue,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PersistenceError};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use sqlx::{Connection, PgConnection, Row};

type Error = PgTransactionError<BusinessError>;

const AUTHORITY: &str = "SELECT EXISTS(SELECT 1 FROM assignments g WHERE g.id=$1 AND g.user_id=$2 AND g.role=$3 AND g.company_id=$4 \
 AND iam_assignment_active(g.id,$5) AND iam_principal_active(g.user_id,g.company_id,$5) \
 AND iam_role_allows(g.role,g.company_id,'commerce.read') AND iam_role_allows(g.role,g.company_id,'commerce.sales') \
 AND iam_role_allows(g.role,g.company_id,'commerce.approve') \
 AND (g.scope_kind='holding' OR g.scope_kind='branch' AND g.scope_node_id=$6 OR g.scope_kind='department' AND g.scope_node_id=$7) \
 AND NOT EXISTS(SELECT 1 FROM iam_revoked_sessions r WHERE r.company_id=$4 AND r.user_id=$2 AND r.session_id=$8) \
 AND ($9::bigint IS NULL OR EXISTS(SELECT 1 FROM assignments a WHERE a.id=$1 AND a.version=$9)))";
const FEATURE: &str = "SELECT (NOT $2::boolean OR EXISTS(SELECT 1 FROM engineering_verification_contexts e WHERE e.company_id=$1 AND e.enabled AND e.expires_at>floor(extract(epoch FROM clock_timestamp())*1000)::bigint))";
const SOURCE: &str = "SELECT request.id request_id,request.company_id,request.binding_id,request.order_id,request.customer_id,request.user_id,request.source_version,request.note, \
 binding.version binding_version,binding.created_by binding_prepared_by,binding.email,binding.status,binding.expires_at,original.state order_state,original.branch_id,original.department_id, \
 jsonb_build_array(to_jsonb(request),to_jsonb(binding),to_jsonb(identity),to_jsonb(original))::text image \
 FROM customer_portal_actions request JOIN customer_access_bindings binding ON binding.id=request.binding_id AND binding.company_id=request.company_id AND binding.order_id=request.order_id AND binding.customer_id=request.customer_id \
 JOIN customer_access_identities identity ON identity.binding_id=binding.id AND identity.user_id=request.user_id AND identity.email=binding.email \
 JOIN commerce_orders original ON original.id=request.order_id AND original.company_id=request.company_id AND original.party_id=request.customer_id AND original.kind IN ('quote','sales') \
 WHERE request.id=$1 AND request.action='request_support'";
const THREAD_IMAGE: &str = "SELECT jsonb_build_array((SELECT jsonb_agg(to_jsonb(response) ORDER BY response.response_version) FROM customer_portal_support_responses response WHERE response.request_id=$1),(SELECT to_jsonb(ack) FROM customer_portal_support_acknowledgements ack WHERE ack.request_id=$1),(SELECT to_jsonb(closure) FROM customer_portal_support_closures closure WHERE closure.request_id=$1))::text";

#[derive(Clone,Debug)]
pub struct SourceRow { pub request_id:String,pub company_id:String,pub binding_id:String,pub order_id:String,pub customer_id:String,pub user_id:String,pub source_version:i64,pub note:String,pub binding_version:i64,pub prepared_by:String,pub email:String,pub status:String,pub expires_at:i64,pub order_state:String,pub branch_id:String,pub department_id:Option<String>,pub image:String }
#[derive(Clone,Debug)]
pub struct ResponseRow { pub id:String,pub response_version:i64,pub message:String,pub created_at:i64,pub actor_id:String }

pub struct PgPortalSupportCommands<'c> { connection:&'c mut PgConnection, mode:PackageEnforcementMode, production:bool }
impl<'c> PgPortalSupportCommands<'c> {
    pub fn new(connection:&'c mut PgConnection, mode:PackageEnforcementMode, production:bool)->Self{Self{connection,mode,production}}
    fn ready(&self)->Result<(),Error>{if !self.connection.is_in_transaction(){return Err(PersistenceError::new("PORTAL_SUPPORT_TRANSACTION_REQUIRED").into())} Ok(())}
    pub async fn source(&mut self,request_id:&str)->Result<Option<SourceRow>,Error>{
        let row=sqlx::query(sqlx::AssertSqlSafe(SOURCE.to_owned())).bind(request_id).fetch_optional(&mut *self.connection).await?;
        row.map(|r|Ok::<_,sqlx::Error>(SourceRow{request_id:r.try_get("request_id")?,company_id:r.try_get("company_id")?,binding_id:r.try_get("binding_id")?,order_id:r.try_get("order_id")?,customer_id:r.try_get("customer_id")?,user_id:r.try_get("user_id")?,source_version:r.try_get("source_version")?,note:r.try_get("note")?,binding_version:r.try_get("binding_version")?,prepared_by:r.try_get("binding_prepared_by")?,email:r.try_get("email")?,status:r.try_get("status")?,expires_at:r.try_get("expires_at")?,order_state:r.try_get("order_state")?,branch_id:r.try_get("branch_id")?,department_id:r.try_get("department_id")?,image:r.try_get("image")?})).transpose().map_err(Into::into)
    }
    async fn response(&mut self,request_id:&str)->Result<Option<ResponseRow>,Error>{
        let row=sqlx::query("SELECT id,response_version,message,created_at,actor_id FROM customer_portal_support_responses WHERE request_id=$1 ORDER BY response_version DESC LIMIT 1").bind(request_id).fetch_optional(&mut *self.connection).await?;
        row.map(|r|Ok::<_,sqlx::Error>(ResponseRow{id:r.try_get("id")?,response_version:r.try_get("response_version")?,message:r.try_get("message")?,created_at:r.try_get("created_at")?,actor_id:r.try_get("actor_id")?})).transpose().map_err(Into::into)
    }
    async fn image(&mut self,request_id:&str)->Result<String,Error>{Ok(sqlx::query_scalar(THREAD_IMAGE).bind(request_id).fetch_one(&mut *self.connection).await?)}
    async fn feature(&mut self,company:&str)->Result<bool,Error>{Ok(sqlx::query_scalar(FEATURE).bind(company).bind(self.production).fetch_one(&mut *self.connection).await?)}
    async fn authority(&mut self,actor:&SupportActor,now:i64)->Result<bool,Error>{Ok(sqlx::query_scalar(AUTHORITY).bind(&actor.assignment_id).bind(&actor.user_id).bind(&actor.role).bind(&actor.company_id).bind(now).bind(actor.branch_id.as_deref()).bind(actor.department_id.as_deref()).bind(actor.session_id.as_deref()).bind(actor.assignment_version).fetch_one(&mut *self.connection).await?)}
    async fn capability(&mut self,company:&str,purpose:CapabilityPurpose,now:i64)->Result<(),Error>{if self.mode==PackageEnforcementMode::Disabled{return Ok(())}let catalogue=current_package_catalogue()?;let company=CompanyId::new(company)?;PgCapabilityDecisionReader::new(company,self.connection,&catalogue).require("sales.portal",purpose,self.mode,TimestampMillis::new(now)).await?;Ok(())}
    async fn flush(&mut self)->Result<(),Error>{sqlx::raw_sql("SET CONSTRAINTS ALL IMMEDIATE").execute(&mut *self.connection).await?;Ok(())}
    async fn scoped(&mut self,actor:&SupportActor,request:&str)->Result<SourceRow,Error>{let s=self.source(request).await?.ok_or_else(not_found)?;if s.company_id!=actor.company_id||actor.branch_id.as_ref().is_some_and(|b|b!=&s.branch_id)||actor.department_id.as_ref().is_some_and(|d|Some(d)!=s.department_id.as_ref()){return Err(not_found().into())} Ok(s)}
    async fn prior(&mut self,key:&str)->Result<Option<(String,String,String,String,String)>,Error>{let row=sqlx::query("SELECT request_hash,result_json,operation,actor_id,record_id FROM command_receipts WHERE command_id=$1").bind(key).fetch_optional(&mut *self.connection).await?;row.map(|r|Ok::<_,sqlx::Error>((r.try_get("request_hash")?,r.try_get("result_json")?,r.try_get("operation")?,r.try_get("actor_id")?,r.try_get("record_id")?))).transpose().map_err(Into::into)}
    fn digest(value:&str)->String{use sha2::{Digest,Sha256};format!("{:x}",Sha256::digest(value.as_bytes()))}
    fn valid_hash(hash:&str)->bool{hash.len()==64&&hash.bytes().all(|b|b.is_ascii_hexdigit())}
    async fn ledger(&mut self,actor:&SupportActor,key:&str,operation:&str,hash:&str,record:&str,receipt:&Receipt,purpose:&str,now:i64)->Result<(),Error>{
        let result=receipt.wire().to_string();
        let authority=actor.authority_json();
        sqlx::query("INSERT INTO command_receipts(command_id,operation,actor_id,request_hash,record_id,result_json,created_at,authority_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8)").bind(key).bind(operation).bind(&actor.user_id).bind(hash).bind(record).bind(&result).bind(now).bind(&authority).execute(&mut *self.connection).await?;
        sqlx::query("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES($1,$2,$3,$4,$5,'customer_portal_request',$6,$7,$8,$9,$10)").bind(format!("audit-portal-support-{key}")).bind(key).bind(&actor.user_id).bind(&actor.assignment_id).bind(operation).bind(record).bind(receipt.version).bind(purpose).bind(now).bind(&authority).execute(&mut *self.connection).await?;
        sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$2,$3,'customer_portal_request',$4,$5,$6,$7)").bind(format!("outbox-portal-support-{key}")).bind(key).bind(operation).bind(record).bind(receipt.version).bind(&result).bind(now).execute(&mut *self.connection).await?;
        Ok(())
    }
    pub async fn publish(&mut self,actor:&SupportActor,request:&str,input:RespondInput,key:&str,now:i64)->Result<Receipt,Error>{
        self.ready()?;let input=input.validate()?;sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))").bind(format!("portal-support:{key}")).execute(&mut *self.connection).await?;
        let hash=Self::digest(&input.identity(actor,request,key));if !Self::valid_hash(&hash){return Err(readback_invalid().into())}
        let source=self.scoped(actor,request).await?;if source.prepared_by==actor.user_id{return Err(independent_responder().into())}
        if let Some((prior_hash,result,operation,prior_actor,record))=self.prior(key).await?{if prior_hash!=hash||operation!=crate::domain::portal_support_commands::RESPOND||record!=request||prior_actor!=actor.user_id{return Err(idempotency_conflict().into())}self.capability(&source.company_id,CapabilityPurpose::History,now).await?;if !self.authority(actor,now).await?{return Err(access_changed().into())}return Ok(crate::domain::portal_support_commands::replay(&result)?)}
        if !self.feature(&source.company_id).await?{return Err(feature_unavailable().into())}
        self.capability(&source.company_id,CapabilityPurpose::SettleExisting,now).await?;
        if source.status!="approved"||source.expires_at<=now||!matches!(source.order_state.as_str(),"confirmed"|"converted"|"partially_fulfilled"|"fulfilled"){return Err(source_changed().into())}
        let current=self.response(request).await?;if self.closure_exists(request).await?{return Err(request_closed().into())}
        if source.binding_version!=input.expected_binding_version||current.as_ref().map(|r|r.response_version).unwrap_or(0)!=input.expected_response_version{return Err(response_changed().into())}
        if self.ack_exists(request).await?{return Err(already_acknowledged().into())}
        let version=input.expected_response_version+1;let id=format!("portal-support-response-{key}");let receipt=Receipt{id:id.clone(),request_id:request.into(),binding_id:source.binding_id.clone(),version,status:"published".into(),published_at:now};sqlx::query("INSERT INTO customer_portal_support_responses(id,company_id,request_id,binding_id,order_id,customer_id,customer_user_id,source_version,binding_version,response_version,message,reason,actor_id,command_id,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)").bind(&id).bind(&source.company_id).bind(request).bind(&source.binding_id).bind(&source.order_id).bind(&source.customer_id).bind(&source.user_id).bind(source.source_version).bind(source.binding_version).bind(version).bind(&input.message).bind(&input.reason).bind(&actor.user_id).bind(key).bind(now).execute(&mut *self.connection).await?;
        let before=self.image(request).await?;
        self.ledger(actor,key,crate::domain::portal_support_commands::RESPOND,&hash,request,&receipt,&input.reason,now).await?;self.flush().await?;
        if !self.authority(actor,now).await?{return Err(access_changed().into())}self.capability(&source.company_id,CapabilityPurpose::SettleExisting,now).await?;if self.image(request).await?!=before{return Err(source_changed().into())} if self.source(request).await?.map(|s|s.image).unwrap_or_default()!=source.image{return Err(source_changed().into())}
        let read=self.response(request).await?.ok_or_else(readback_invalid)?;if read.id!=receipt.id||read.response_version!=version{return Err(source_changed().into())}Ok(receipt)
    }
    pub async fn acknowledge(&mut self,actor:&CustomerActor,request:&str,input:AcknowledgeInput,key:&str,now:i64)->Result<Receipt,Error>{
        self.ready()?;let input=input.validate()?;let hash=Self::digest(&input.hash(actor));if !Self::valid_hash(&hash){return Err(readback_invalid().into())}
        let source=self.source(request).await?.ok_or_else(not_found)?;if !(source.status=="approved"&&source.expires_at>now&&matches!(source.order_state.as_str(),"confirmed"|"converted"|"partially_fulfilled"|"fulfilled")){return Err(source_changed().into())}
        if source.binding_id!=input.binding_id||source.user_id!=actor.user_id||source.email.to_lowercase()!=actor.email.to_lowercase(){return Err(identity_required().into())}
        let prior:Option<(String,String,i64,String,String,String,String)>=sqlx::query_as("SELECT request_hash,result_json,response_version,binding_id,id,user_id,request_key FROM customer_portal_support_acknowledgements WHERE request_key=$1").bind(key).fetch_optional(&mut *self.connection).await.map_err(Error::from)?;
        self.capability(&source.company_id,CapabilityPurpose::SettleExisting,now).await?;
        if let Some((prior_hash,result,version,binding,id,user,_))=&prior{if prior_hash!=&hash||binding!=&source.binding_id||user!=&actor.user_id{return Err(idempotency_conflict().into())}return Ok(Receipt{id:id.clone(),request_id:request.into(),binding_id:binding.clone(),version:*version,status:"acknowledged".into(),published_at:now})}
        let response=self.response(request).await?.ok_or_else(response_changed)?;if self.closure_exists(request).await?{return Err(request_closed().into())}
        if source.binding_version!=input.expected_binding_version||response.id!=input.response_id||response.response_version!=input.response_version{return Err(response_changed().into())}
        if self.ack_exists(request).await?{return Err(already_acknowledged().into())}
        let id=format!("portal-support-ack-{}",uuid_like());
        sqlx::query("INSERT INTO customer_portal_support_acknowledgements(id,company_id,request_id,response_id,response_version,binding_id,order_id,customer_id,user_id,binding_version,request_key,request_hash,note,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14)").bind(&id).bind(&source.company_id).bind(request).bind(&response.id).bind(response.response_version).bind(&source.binding_id).bind(&source.order_id).bind(&source.customer_id).bind(&actor.user_id).bind(source.binding_version).bind(key).bind(&hash).bind(&input.note).bind(now).execute(&mut *self.connection).await?;
        let before=self.image(request).await?;
        let authority=serde_json::json!({"kind":"external-customer","bindingId":source.binding_id,"email":actor.email.to_lowercase()}).to_string();
        sqlx::query("INSERT INTO audit_events(id,command_id,actor_id,assignment_id,action,record_type,record_id,record_version,purpose,created_at,authority_json) VALUES($1,$2,$3,'external-customer','customer.portal.support.acknowledge','customer_portal_request',$4,$5,'Verified customer acknowledged exact published Portal response',$6,$7)").bind(format!("audit-ack-{id}")).bind(key).bind(&actor.user_id).bind(request).bind(response.response_version).bind(now).bind(&authority).execute(&mut *self.connection).await?;
        sqlx::query("INSERT INTO outbox_events(id,command_id,event_type,aggregate_type,aggregate_id,aggregate_version,payload_json,created_at) VALUES($1,$2,'customer.portal.support.acknowledge','customer_portal_request',$3,$4,$5,$6)").bind(format!("outbox-ack-{id}")).bind(key).bind(request).bind(response.response_version).bind(serde_json::json!({"requestId":request,"bindingId":source.binding_id,"responseId":response.id,"customerId":source.customer_id,"userId":actor.user_id}).to_string()).bind(now).execute(&mut *self.connection).await?;
        self.flush().await?;if self.image(request).await?!=before{return Err(source_changed().into())}
        Ok(Receipt{id,request_id:request.into(),binding_id:source.binding_id,version:response.response_version,status:"acknowledged".into(),published_at:now})
    }
    pub async fn close(&mut self,actor:&SupportActor,request:&str,input:CloseInput,key:&str,now:i64)->Result<Receipt,Error>{
        self.ready()?;let input=input.validate()?;sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))").bind(format!("portal-support:{key}")).execute(&mut *self.connection).await?;
        let hash=Self::digest(&input.identity(actor,request,key));if !Self::valid_hash(&hash){return Err(readback_invalid().into())}
        let source=self.scoped(actor,request).await?;let current=self.response(request).await?;
        if source.prepared_by==actor.user_id||current.as_ref().map(|r|r.actor_id.as_str())==Some(actor.user_id.as_str()){return Err(independent_closer().into())}
        if let Some((prior_hash,result,operation,prior_actor,record))=self.prior(key).await?{if prior_hash!=hash||operation!=crate::domain::portal_support_commands::CLOSE||record!=request||prior_actor!=actor.user_id{return Err(idempotency_conflict().into())}self.capability(&source.company_id,CapabilityPurpose::History,now).await?;if !self.authority(actor,now).await?{return Err(access_changed().into())}return Ok(crate::domain::portal_support_commands::replay(&result)?)}
        if !self.feature(&source.company_id).await?{return Err(feature_unavailable().into())}
        self.capability(&source.company_id,CapabilityPurpose::SettleExisting,now).await?;
        if !matches!(source.status.as_str(),"revoked"|"rejected")&&source.expires_at>now{return Err(access_active().into())}
        if source.binding_version!=input.expected_binding_version||current.as_ref().map(|r|r.response_version).unwrap_or(0)!=input.expected_response_version{return Err(response_changed().into())}
        if self.ack_exists(request).await?||self.closure_exists(request).await?{return Err(request_closed().into())}
        let receipt=Receipt{id:format!("portal-support-closure-{key}"),request_id:request.into(),binding_id:source.binding_id.clone(),version:input.expected_response_version+1,status:"closed_without_customer_acknowledgement".into(),published_at:now};
        sqlx::query("INSERT INTO customer_portal_support_closures(id,company_id,request_id,binding_id,binding_version,response_version,outcome,reason,actor_id,command_id,created_at) VALUES($1,$2,$3,$4,$5,$6,'closed_without_customer_acknowledgement',$7,$8,$9,$10)").bind(&receipt.id).bind(&source.company_id).bind(request).bind(&source.binding_id).bind(source.binding_version).bind(input.expected_response_version).bind(&input.reason).bind(&actor.user_id).bind(key).bind(now).execute(&mut *self.connection).await?;
        let before=self.image(request).await?;
        self.ledger(actor,key,crate::domain::portal_support_commands::CLOSE,&hash,request,&receipt,&input.reason,now).await?;self.flush().await?;
        if !self.authority(actor,now).await?{return Err(access_changed().into())}self.capability(&source.company_id,CapabilityPurpose::SettleExisting,now).await?;if self.image(request).await?!=before{return Err(source_changed().into())}Ok(receipt)
    }
    async fn ack_exists(&mut self,request:&str)->Result<bool,Error>{Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM customer_portal_support_acknowledgements WHERE request_id=$1)").bind(request).fetch_one(&mut *self.connection).await?)}
    async fn closure_exists(&mut self,request:&str)->Result<bool,Error>{Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM customer_portal_support_closures WHERE request_id=$1)").bind(request).fetch_one(&mut *self.connection).await?)}
}
fn uuid_like()->String{use std::time::{SystemTime,UNIX_EPOCH};let n=SystemTime::now().duration_since(UNIX_EPOCH).map(|d|d.as_nanos()).unwrap_or_default();format!("{n:x}")}
