//! Native Field notification contribution. Other owners remain outside this
//! partition; no legacy source/permission decision can authorize these rows.
use crate::{
    field_collaboration_command::FieldCollaborationTarget,
    field_collaboration_read::{component, visible},
    field_service_records_source::{
        FieldCollaborationSources, FieldRecordsSourcePort, PreparedRecordsSource,
    },
    field_service_reservation_read::{
        FieldServiceRenderedContext, ReplyBudget, context, selected_read_context,
    },
    field_service_source_record::SourceRecordContext,
};
use erpx_server_collaboration::public::field_inbox::*;
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        SelectedIngressFuture, SelectedTransportCallbacks, SelectedWorkIngress,
        WorkAuthorizationRequest, WorkPermissionRequirement,
    },
    selected_command_authority::require_selected_actor_current,
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_records::public::source_record::{SourceKind, SourceReference, prepare_source};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, FailureKind};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::collections::BTreeMap;
type Error = PgTransactionError<BusinessError>;
#[derive(Clone)]
pub struct InboxSources {
    pub initial_count:
        Option<crate::service_initial_physical_stock::ServiceInitialPhysicalStockEnvironment>,
}
impl HoldingWorkSourcePreparation for InboxSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            if let Some(environment) = &self.initial_count {
                environment.sources().prepare(c).await?;
            }
            erpx_server_operations::public::field_exceptions::lock_field_exception_sources(c)
                .await?;
            FieldCollaborationSources.prepare(c).await?;
            crate::finance_service_invoice_history::lock_service_invoice_history_sources(c).await?;
            crate::finance_field_journal_history::lock_field_journal_history_sources(c).await?;
            lock_field_inbox_sources(c).await
        })
    }
}
#[derive(Serialize)]
pub struct FieldInboxReadResponse {
    pub data: Vec<Value>,
    pub context: FieldServiceRenderedContext,
}
enum ReadIssue {
    Invalid(BusinessError),
    NoPermission(FieldServiceRenderedContext),
}
fn limited() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INBOX_SOURCE_LIMIT",
        "The Inbox source exceeds this bounded read.",
    )
}
/// Current source witnesses for the finite Field notification partition. The
/// outer composition calls owner interfaces; Collaboration never writes them.
enum InboxOwnerWitness {
    Direct,
    Operations(erpx_server_operations::public::field_exceptions::CurrentFieldException),
    Collaboration(erpx_server_collaboration::public::collaboration_work::CurrentCollaborationRow),
}
enum InboxSourceWitness<'a> {
    Record(PreparedRecordsSource),
    Invoice(crate::finance_service_invoice_history::CurrentServiceInvoiceHistory),
    Journal(Box<crate::finance_field_journal_history::CurrentFieldJournalHistory>),
    Count {
        record: crate::service_initial_physical_stock_record::ServiceCountRecordSource<'a>,
        modules: erpx_server_pms::public::initial_count_modules::CurrentInitialCountModules,
    },
}
pub(crate) struct PreparedInboxSource<'a> {
    href: String,
    reference: Option<Value>,
    owner: InboxOwnerWitness,
    source: InboxSourceWitness<'a>,
}
impl PreparedInboxSource<'_> {
    pub(crate) async fn require_current<C: Clock>(
        &self,
        c: &mut PgConnection,
        ctx: &SourceRecordContext<'_, C>,
    ) -> Result<(), Error> {
        match &self.owner {
            InboxOwnerWitness::Direct => {}
            InboxOwnerWitness::Operations(row) => row.require_current(c).await?,
            InboxOwnerWitness::Collaboration(row) => row.require_current(c).await?,
        }
        match &self.source {
            InboxSourceWitness::Record(source) => source.require_current(c, ctx).await,
            InboxSourceWitness::Invoice(source) => source.require_current(c,ctx).await,
            InboxSourceWitness::Journal(source) => Box::pin(source.require_current(c,ctx)).await,
            InboxSourceWitness::Count { record, modules } => {
                record.require_current(c, ctx.clock).await?;
                record.flow.require_current(c, ctx.actor, ctx.clock).await?;
                modules.require_current(c).await
            }
        }
    }
}
pub(crate) async fn prepare_inbox_source<'a, C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &'a AuthorizedWorkContext,
    target: &FieldCollaborationTarget<C>,
    row: &FieldInboxCandidate,
    initial_count: Option<
        &crate::service_initial_physical_stock::ServiceInitialPhysicalStockEnvironment,
    >,
) -> Result<Option<PreparedInboxSource<'a>>, Error> {
    let scope = current.selection().workspace();
    let read=async{
        if row.source_type=="journal" {
            let ctx=SourceRecordContext{actor:current,holding:&target.holding,company:&target.company,catalogue:&target.catalogue,clock:target.clock.as_ref()};
            let source=Box::pin(crate::finance_field_journal_history::read_field_journal_history(c,&ctx,&row.source_id)).await?;
            return Ok(Some(PreparedInboxSource{href:format!("/finance/journals/{}",component(&row.source_id)),reference:None,owner:InboxOwnerWitness::Direct,source:InboxSourceWitness::Journal(Box::new(source))}));
        }
        if row.source_type=="invoice" {
            let ctx=SourceRecordContext{actor:current,holding:&target.holding,company:&target.company,catalogue:&target.catalogue,clock:target.clock.as_ref()};
            let source=crate::finance_service_invoice_history::read_service_invoice_history(c,&ctx,&row.source_id).await?;
            return Ok(Some(PreparedInboxSource{href:format!("/finance/invoices/{}",component(&row.source_id)),reference:None,owner:InboxOwnerWitness::Direct,source:InboxSourceWitness::Invoice(source)}));
        }
        if row.source_type=="stock_count" {
            if !current.has_permission("inventory.read"){return Ok(None);}
            let environment=initial_count.ok_or_else(||BusinessError::new(FailureKind::Unavailable,"INBOX_COUNT_RUNTIME_UNAVAILABLE","The native Service Count runtime is unavailable."))?;
            let modules=crate::service_initial_physical_stock_sources::require_count_modules(c,current,environment).await?;
            let record=crate::service_initial_physical_stock_record::read_service_count_record(c,current,target.clock.as_ref(),&row.source_id,erpx_server_identity_access::public::service_initial_count_authority::ServiceCountAuthorityPurpose::Read).await?;
            return Ok(Some(PreparedInboxSource{href:format!("/services/field-service/settings/initial-stock/{}",component(&row.source_id)),reference:None,owner:InboxOwnerWitness::Direct,source:InboxSourceWitness::Count{record,modules}}));
        }
        if row.source_type=="support_ticket" {
            let mut port=FieldRecordsSourcePort{connection:c,context:SourceRecordContext{actor:current,holding:&target.holding,company:&target.company,catalogue:&target.catalogue,clock:target.clock.as_ref()}};
            let (_,source)=prepare_source(&mut port,target.company.as_str(),&SourceReference{kind:SourceKind::SupportTicket,id:row.source_id.clone(),expected_version:None}).await?.into_parts();
            if source.support_data().and_then(|d|d["row"]["source_kind"].as_str())!=Some("field_service"){return Ok(None);}
            return Ok(Some(PreparedInboxSource{href:format!("/field-service/support/{}",component(&row.source_id)),reference:None,owner:InboxOwnerWitness::Direct,source:InboxSourceWitness::Record(source)}));
        }
        let (source_id,href,owner)=match row.source_type.as_str(){
            "field_service_order"=>(row.source_id.clone(),format!("/field-service/{}",component(&row.source_id)),InboxOwnerWitness::Direct),
            "operational_exception"=>{
                if !current.modules().iter().any(|v|v=="operations"){return Ok(None);}
                let record=erpx_server_operations::public::field_exceptions::read_field_exception(c,&target.company,&row.source_id,scope.branch_id.as_ref().map(|v|v.as_str()),scope.department_id.as_ref().map(|v|v.as_str())).await?;
                let Some(record)=record else{return Ok(None)};
                (record.data().source_id.clone(),format!("/operations/{}",component(&row.source_id)),InboxOwnerWitness::Operations(record))
            },
            "collaboration_action"=>{
                if !current.modules().iter().any(|v|v=="collaboration"){return Ok(None);}
                let (card,proof)=erpx_server_collaboration::public::collaboration_work::read_card(c,&target.company,&row.source_id,scope.branch_id.as_ref().map(|v|v.as_str()),scope.department_id.as_ref().map(|v|v.as_str())).await?;
                if card.source.kind!=SourceKind::FieldServiceOrder{return Ok(None);}
                (card.source.id,"/collaboration".to_string(),InboxOwnerWitness::Collaboration(proof))
            },
            _=>return Ok(None),
        };
        let Some((_,field))=visible(c,current,target,SourceKind::FieldServiceOrder,&source_id).await? else{return Ok(None)};
        let reference=if row.source_type=="field_service_order"{Some(json!({"kind":"field_service_order","id":row.source_id,"href":href,"label":"Field service order"}))}else{None};
        Ok(Some(PreparedInboxSource{href,reference,owner,source:InboxSourceWitness::Record(field)}))
    }.await;
    match read {
        Err(PgTransactionError::Business(e))
            if matches!(e.kind, FailureKind::Forbidden | FailureKind::NotFound) =>
        {
            Ok(None)
        }
        other => other,
    }
}
async fn read<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    target: &FieldCollaborationTarget<C>,
    query: &FieldInboxQuery,
    initial_count: Option<
        &crate::service_initial_physical_stock::ServiceInitialPhysicalStockEnvironment,
    >,
) -> Result<FieldInboxReadResponse, Error> {
    let mut data = Vec::new();
    let mut prepared: BTreeMap<(String, String), Option<PreparedInboxSource<'_>>> = BTreeMap::new();
    let mut scan = query.clone();
    scan.limit = 100;
    let mut scanned = 0;
    let mut budget = ReplyBudget(0);
    'pages: loop {
        let rows =
            read_field_inbox_candidates(c, &target.company, current.user_id(), &scan).await?;
        let exhausted = rows.len() < 100;
        if rows.is_empty() {
            break;
        }
        for row in rows {
            scanned += 1;
            if scanned > 1000 {
                return Err(limited().into());
            }
            scan.before = Some(row.cursor());
            let source_key = (row.source_type.clone(), row.source_id.clone());
            if !prepared.contains_key(&source_key) {
                let guard = Box::pin(prepare_inbox_source(c, current, target, &row, initial_count)).await?;
                prepared.insert(source_key.clone(), guard);
            }
            if let Some(Some(source)) = prepared.get(&source_key) {
                let item = json!({"id":row.id,"eventId":row.event_id,"category":row.category,"message":row.preview,"href":source.href,"createdAt":row.delivered_at,"readAt":row.read_at,"archivedAt":row.archived_at,"sourceType":row.source_type,"sourceId":row.source_id,"source":source.reference});
                let mut value = serde_json::to_value(&row).map_err(|_| limited())?;
                value["native_field_item"] = item;
                serde_json::to_writer(&mut budget, &value).map_err(|_| limited())?;
                data.push(value);
                if data.len() >= query.limit {
                    break 'pages;
                }
            }
        }
        if exhausted {
            break;
        }
    }
    let source_context = SourceRecordContext {
        actor: current,
        holding: &target.holding,
        company: &target.company,
        catalogue: &target.catalogue,
        clock: target.clock.as_ref(),
    };
    for guard in prepared.into_values().flatten() {
        guard.require_current(c, &source_context).await?;
    }
    require_selected_actor_current(c, current, target.clock.now().get()).await?;
    Ok(FieldInboxReadResponse {
        data,
        context: context(current)?,
    })
}
pub async fn read_selected_field_inbox<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    workspace: Input,
    target: FieldCollaborationTarget<C>,
    sources: InboxSources,
    query: FieldInboxQuery,
) -> Result<FieldInboxReadResponse, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let expected = target.company.clone();
    let initial_count = sources.initial_count.clone();
    let callbacks = SelectedTransportCallbacks::<
        _,
        (FieldInboxQuery, FieldServiceRenderedContext),
        ReadIssue,
        _,
    >::new::<FieldInboxReadResponse>(
        move |selected: &SelectedWorkIngress<'_>| -> SelectedIngressFuture<
            (FieldInboxQuery, FieldServiceRenderedContext),
            ReadIssue,
        > {
            let context = selected
                .read_context()
                .and_then(|r| selected_read_context(&r));
            let query = query.clone();
            Box::pin(async move {
                let context = context.map_err(ReadIssue::Invalid)?;
                query.validate().map_err(ReadIssue::Invalid)?;
                Ok((query, context))
            })
        },
        |(_, context), error| {
            if error.code == "PERMISSION_REQUIRED" {
                Ok(ReadIssue::NoPermission(context.clone()))
            } else {
                Err(error)
            }
        },
        move |c, current, (query, _): (FieldInboxQuery, FieldServiceRenderedContext)| {
            let target = target.clone();
            let initial_count = initial_count.clone();
            Box::pin(async move {
                Ok(Ok(Box::pin(read(
                    c,
                    current,
                    &target,
                    &query,
                    initial_count.as_ref(),
                ))
                .await?))
            })
        },
    );
    let result = authorizer
        .execute_with_selected_transport_ingress_deferred(
            host,
            proof,
            move || {
                let mut scope = workspace()?;
                if scope.company_id != expected {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "INBOX_SCOPE_REQUIRED",
                        "Use the current Company.",
                    ));
                }
                scope.mutation = false;
                Ok(scope)
            },
            (
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::AnyOf(vec![
                        "records.read".into(),
                        "support.read".into(),
                        "inventory.read".into(),
                        "finance.read".into(),
                    ]),
                    module: None,
                    destination: "staff/notifications".into(),
                    api: true,
                },
                sources,
            ),
            callbacks,
        )
        .await?;
    match result {
        Ok(value) => Ok(value),
        Err(ReadIssue::Invalid(error)) => Err(error.into()),
        Err(ReadIssue::NoPermission(context)) => Ok(FieldInboxReadResponse {
            data: Vec::new(),
            context,
        }),
    }
}
