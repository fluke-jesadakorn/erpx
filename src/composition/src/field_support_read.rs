//! Native readback of Field-managed Support cases with actual owner authority.
use crate::field_collaboration_command::FieldCollaborationTarget;
use crate::field_service_records_source::FieldCollaborationSources;
use crate::field_service_records_source::FieldRecordsSourcePort;
use crate::field_service_source_record::SourceRecordContext;
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{HoldingWorkAuthorizer, WorkAuthorizationRequest, WorkPermissionRequirement},
    selected_command_authority::require_selected_actor_current,
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_records::public::source_record::{SourceKind, SourceReference, prepare_source};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use serde::Serialize;
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FieldSupportReadResponse {
    pub data: serde_json::Value,
    pub context: crate::field_service_reservation_read::FieldServiceRenderedContext,
}
/// Support owns visibility: support.read or records.read for the exact reporter.
/// A Field backlink never adds a Field module or parent-order visibility gate.
pub async fn read_selected_field_support<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    workspace: Input,
    target: FieldCollaborationTarget<C>,
    id: String,
) -> Result<FieldSupportReadResponse, PgTransactionError<BusinessError>>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let expected = target.company.clone();
    authorizer.execute_with_sources_deferred(host,proof,move||{let mut w=workspace()?;w.mutation=false;if w.company_id!=expected{return Err(BusinessError::new(FailureKind::Forbidden,"SUPPORT_SCOPE_REQUIRED","Use the current Company."));}Ok(w)},WorkAuthorizationRequest{permission:WorkPermissionRequirement::AnyOf(vec!["support.read".into(),"records.read".into()]),module:None,destination:"service/support".into(),api:true},FieldCollaborationSources,move|c,current|{let target=target.clone();let id=id.clone();Box::pin(async move{
  if current.holding_id()!=&target.holding||current.selection().workspace().company_id!=target.company{return Err(BusinessError::new(FailureKind::Forbidden,"SUPPORT_SCOPE_REQUIRED","Use the current Company.").into());}
  let mut port=FieldRecordsSourcePort{connection:c,context:SourceRecordContext{purchase_locks:None,actor:current,holding:&target.holding,company:&target.company,catalogue:&target.catalogue,clock:target.clock.as_ref()}};
  let prepared=prepare_source(&mut port,target.company.as_str(),&SourceReference{kind:SourceKind::SupportTicket,id,expected_version:None}).await.map_err(|error|match error{PgTransactionError::Business(e)if e.kind==FailureKind::NotFound&&e.code=="SOURCE_RECORD_NOT_FOUND"=>BusinessError::new(FailureKind::NotFound,"SUPPORT_TICKET_NOT_FOUND","Ticket not found in this scope.").into(),other=>other})?;
  let (_,witness)=prepared.into_parts();let ticket=witness.support_data().ok_or_else(||BusinessError::new(FailureKind::Unavailable,"SUPPORT_SOURCE_INVALID","The Support source is unavailable."))?;
  let data=serde_json::json!({"row":ticket["row"],"history":ticket["history"],"managedInFieldService":ticket["row"]["source_kind"]=="field_service","canWork":false,"canManage":false,"canReport":false,"canConfirm":false,"canReopen":false,"slaPolicies":[],"canChange":false,"canShare":false});
  witness.require_current(c,&SourceRecordContext{purchase_locks:None,actor:current,holding:&target.holding,company:&target.company,catalogue:&target.catalogue,clock:target.clock.as_ref()}).await?;
  require_selected_actor_current(c,current,target.clock.now().get()).await?;
  Ok(FieldSupportReadResponse{data,context:crate::field_service_reservation_read::context(current)?})
 })}).await
}
