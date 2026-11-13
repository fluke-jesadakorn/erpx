//! Native Customer Authority portal-support command validation.
use erpx_server_shared::public::{BusinessError, FailureKind};
pub const RESPOND: &str = "customer.portal.support.respond";
pub const ACKNOWLEDGE: &str = "customer.portal.support.acknowledge";
pub const CLOSE: &str = "customer.portal.support.close";
pub fn request_invalid() -> BusinessError { BusinessError::invalid("PORTAL_SUPPORT_REQUEST_INVALID", "Review the portal support request.") }
fn bounded(value: &str, min: usize, max: usize) -> bool { let t=value.trim(); t.chars().count()>=min && t.chars().count()<=max }
pub fn not_found() -> BusinessError { BusinessError::new(FailureKind::NotFound,"PORTAL_SUPPORT_REQUEST_NOT_FOUND","The actual customer support request is unavailable.") }
pub fn access_changed() -> BusinessError { BusinessError::new(FailureKind::Forbidden,"PORTAL_SUPPORT_ACCESS_CHANGED","The original Sales approval authority is no longer current.") }
pub fn source_changed() -> BusinessError { BusinessError::new(FailureKind::Conflict,"PORTAL_SUPPORT_SOURCE_CHANGED","The original customer request changed before commit.") }
pub fn response_changed() -> BusinessError { BusinessError::new(FailureKind::Conflict,"PORTAL_SUPPORT_RESPONSE_CHANGED","Reload the current request and response before retrying.") }
pub fn request_closed() -> BusinessError { BusinessError::new(FailureKind::Conflict,"PORTAL_SUPPORT_REQUEST_CLOSED","The actual support request already has an immutable conclusion.") }
pub fn already_acknowledged() -> BusinessError { BusinessError::new(FailureKind::Conflict,"PORTAL_SUPPORT_ALREADY_ACKNOWLEDGED","The customer already acknowledged this request response.") }
pub fn access_active() -> BusinessError { BusinessError::new(FailureKind::Conflict,"PORTAL_SUPPORT_CUSTOMER_ACCESS_ACTIVE","The verified customer can still read and acknowledge the response.") }
pub fn independent_responder() -> BusinessError { BusinessError::new(FailureKind::Forbidden,"INDEPENDENT_PORTAL_RESPONDER_REQUIRED","An independent authorized sales manager must publish the customer response.") }
pub fn independent_closer() -> BusinessError { BusinessError::new(FailureKind::Forbidden,"INDEPENDENT_PORTAL_CLOSER_REQUIRED","Another authorized manager must independently close the unacknowledged request.") }
pub fn identity_required() -> BusinessError { BusinessError::new(FailureKind::Forbidden,"CUSTOMER_PORTAL_IDENTITY_REQUIRED","Use the verified account that made the original request.") }
pub fn binding_unavailable() -> BusinessError { BusinessError::new(FailureKind::Forbidden,"CUSTOMER_PORTAL_BINDING_UNAVAILABLE","The original customer binding or document is no longer available.") }
pub fn idempotency_conflict() -> BusinessError { BusinessError::new(FailureKind::Conflict,"IDEMPOTENCY_CONFLICT","This command key belongs to another portal support command.") }
pub fn readback_invalid() -> BusinessError { BusinessError::new(FailureKind::Conflict,"PORTAL_SUPPORT_READBACK_INVALID","The original portal support result is unavailable.") }
pub fn feature_unavailable() -> BusinessError { BusinessError::new(FailureKind::Forbidden,"FEATURE_COMING_SOON","Customer access is not released for this Company.") }

#[derive(Clone,Debug,PartialEq,Eq)]
pub struct SupportActor { pub user_id:String,pub company_id:String,pub branch_id:Option<String>,pub department_id:Option<String>,pub assignment_id:String,pub role:String,pub session_id:Option<String>,pub assignment_version:Option<i64>,pub permissions:Vec<String> }
impl SupportActor { pub fn can(&self,p:&str)->bool{self.permissions.iter().any(|v|v==p)} pub fn authority_json(&self)->String{serde_json::json!({"userId":self.user_id,"assignmentId":self.assignment_id,"role":self.role,"companyId":self.company_id,"branchId":self.branch_id,"departmentId":self.department_id,"iam":{"sessionId":self.session_id,"queuedAssignmentVersion":self.assignment_version}}).to_string()} }
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct CustomerActor { pub user_id:String, pub email:String }
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct RespondInput { pub expected_binding_version:i64,pub expected_response_version:i64,pub message:String,pub reason:String }
impl RespondInput { pub fn validate(self)->Result<Self,BusinessError>{if self.expected_binding_version<1||self.expected_response_version<0||!bounded(&self.message,3,4000)||!bounded(&self.reason,3,2000){return Err(request_invalid())} Ok(Self{message:self.message.trim().to_owned(),reason:self.reason.trim().to_owned(),..self})} pub fn identity(&self,actor:&SupportActor,request:&str,key:&str)->String{serde_json::json!({"operation":RESPOND,"recordId":request,"actorId":actor.user_id,"companyId":actor.company_id,"idempotencyKey":key,"input":{"expectedBindingVersion":self.expected_binding_version,"expectedResponseVersion":self.expected_response_version,"message":self.message,"reason":self.reason}}).to_string()} }
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct AcknowledgeInput { pub binding_id:String,pub expected_binding_version:i64,pub response_id:String,pub response_version:i64,pub note:String }
impl AcknowledgeInput {
    pub fn validate(self)->Result<Self,BusinessError>{
        if self.binding_id.is_empty()||self.binding_id.chars().count()>200||self.expected_binding_version<1||self.response_id.is_empty()||self.response_id.chars().count()>200||self.response_version<1||self.note.trim().chars().count()>500{return Err(request_invalid())}
        Ok(Self{note:self.note.trim().to_owned(),..self})
    }
    /// The exact JSON digest retained by the original Customer Authority
    /// acknowledgement writer. Field order is part of the replay contract.
    pub fn hash(&self,actor:&CustomerActor)->String{
        let q=|value:&str|serde_json::to_string(value).unwrap_or_else(|_|"\"\"".to_owned());
        format!(
            "{{\"userId\":{},\"email\":{},\"input\":{{\"bindingId\":{},\"expectedBindingVersion\":{},\"responseId\":{},\"responseVersion\":{},\"note\":{}}}}}",
            q(&actor.user_id),q(&actor.email.to_lowercase()),q(&self.binding_id),self.expected_binding_version,q(&self.response_id),self.response_version,q(&self.note)
        )
    }
}
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct CloseInput { pub expected_binding_version:i64,pub expected_response_version:i64,pub reason:String }
impl CloseInput { pub fn validate(self)->Result<Self,BusinessError>{if self.expected_binding_version<1||self.expected_response_version<0||!bounded(&self.reason,3,2000){return Err(request_invalid())} Ok(Self{reason:self.reason.trim().to_owned(),..self})} pub fn identity(&self,actor:&SupportActor,request:&str,key:&str)->String{serde_json::json!({"operation":CLOSE,"recordId":request,"actorId":actor.user_id,"companyId":actor.company_id,"idempotencyKey":key,"input":{"expectedBindingVersion":self.expected_binding_version,"expectedResponseVersion":self.expected_response_version,"reason":self.reason}}).to_string()} }
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct Receipt { pub id:String,pub request_id:String,pub binding_id:String,pub version:i64,pub status:String,pub published_at:i64 }
impl Receipt { pub fn wire(&self)->serde_json::Value{serde_json::json!({"id":self.id,"requestId":self.request_id,"bindingId":self.binding_id,"version":self.version,"status":self.status,"publishedAt":self.published_at})} }
/// The customer acknowledgement keeps the acknowledgement-specific receipt
/// retained by the original verified Portal contract.
#[derive(Clone,Debug,PartialEq,Eq)]
pub struct AcknowledgementReceipt { pub id:String,pub request_id:String,pub binding_id:String,pub response_id:String,pub response_version:i64,pub created_at:i64 }
impl AcknowledgementReceipt {
    pub fn wire(&self)->serde_json::Value{serde_json::json!({"id":self.id,"requestId":self.request_id,"bindingId":self.binding_id,"responseId":self.response_id,"responseVersion":self.response_version,"status":"acknowledged","createdAt":self.created_at})}
}
pub fn replay_acknowledgement(raw:&str)->Result<AcknowledgementReceipt,BusinessError>{
    let value:serde_json::Value=serde_json::from_str(raw).map_err(|_|readback_invalid())?;let o=value.as_object().ok_or_else(readback_invalid)?;
    let id=o.get("id").and_then(serde_json::Value::as_str);let req=o.get("requestId").and_then(serde_json::Value::as_str);let bind=o.get("bindingId").and_then(serde_json::Value::as_str);let response=o.get("responseId").and_then(serde_json::Value::as_str);let version=o.get("responseVersion").and_then(serde_json::Value::as_i64);let status=o.get("status").and_then(serde_json::Value::as_str);let at=o.get("createdAt").and_then(serde_json::Value::as_i64);
    match(id,req,bind,response,version,status,at){(Some(id),Some(req),Some(b),Some(r),Some(v),Some("acknowledged"),Some(at))if !id.is_empty()&&!req.is_empty()&&!b.is_empty()&&!r.is_empty()&&v>0=>Ok(AcknowledgementReceipt{id:id.into(),request_id:req.into(),binding_id:b.into(),response_id:r.into(),response_version:v,created_at:at}),_=>Err(readback_invalid())}
}
pub fn replay(raw:&str)->Result<Receipt,BusinessError>{let value:serde_json::Value=serde_json::from_str(raw).map_err(|_|readback_invalid())?;let o=value.as_object().ok_or_else(readback_invalid)?;let id=o.get("id").and_then(serde_json::Value::as_str);let req=o.get("requestId").and_then(serde_json::Value::as_str);let bind=o.get("bindingId").and_then(serde_json::Value::as_str);let ver=o.get("version").and_then(serde_json::Value::as_i64);let status=o.get("status").and_then(serde_json::Value::as_str);let at=o.get("publishedAt").and_then(serde_json::Value::as_i64);match(id,req,bind,ver,status,at){(Some(id),Some(req),Some(b),Some(v),Some(s),Some(at))if !id.is_empty()&&!req.is_empty()&&!b.is_empty()&&v>=0&&matches!(s,"published"|"acknowledged"|"closed_without_customer_acknowledgement")=>Ok(Receipt{id:id.into(),request_id:req.into(),binding_id:b.into(),version:v,status:s.into(),published_at:at}),_=>Err(readback_invalid())}}
