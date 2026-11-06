//! Field Inbox acknowledgement composition: current native Records authority,
//! Collaboration-owned state and Workflow receipt share the selected unit.
use crate::{
    field_collaboration_command::FieldCollaborationTarget,
    field_inbox_read::{InboxSources, prepare_inbox_source},
    field_service_source_record::SourceRecordContext,
};
use erpx_server_collaboration::public::field_inbox::*;
use erpx_server_identity_access::public::{
    DeviceProof,
    command_metadata::project_command_iam,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, SelectedIngressFuture,
        SelectedTransportCallbacks, SelectedWorkIngress, WorkAuthorizationRequest,
        WorkPermissionRequirement,
    },
    selected_command_authority::require_selected_actor_current,
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, FailureKind, IdentifierSource, RecordId,
};
use erpx_server_workflow::public::commands::*;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
use std::sync::Arc;
type Error = PgTransactionError<BusinessError>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "INBOX_SOURCE_CHANGED",
        "Refresh the Inbox and retry.",
    )
    .into()
}
fn missing() -> Error {
    BusinessError::new(
        FailureKind::NotFound,
        "INBOX_ITEM_NOT_FOUND",
        "This update is unavailable.",
    )
    .into()
}
fn ledger_actor(current: &AuthorizedWorkContext) -> Result<CommandActor, Error> {
    let s = current.selection();
    let a = s.assignment();
    let w = s.workspace();
    let field = |v: Option<&str>| {
        v.map(|s| CommandField::Value(s.into()))
            .unwrap_or(CommandField::Null)
    };
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: a.id.clone(),
        role: a.role.clone(),
        company_id: w.company_id.clone(),
        branch_id: field(w.branch_id.as_ref().map(|v| v.as_str())),
        department_id: field(w.department_id.as_ref().map(|v| v.as_str())),
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: s.revision(),
            assignment_version: a.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(&project_command_iam(
            current,
        )?)?),
        assignment_scope_kind: match a.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        }
        .into(),
        assignment_scope_node_id: a.scope_node_id.as_ref().map(|v| v.as_str().into()),
    })
}
struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    request: &'a WorkflowRequest,
}
impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        if !c.is_in_transaction() {
            return Err(PersistenceError::new("INBOX_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if r.key != self.request.key
            || r.operation != self.request.operation
            || r.record_id != self.request.record_id
            || r.input != self.request.input
        {
            return Err(changed());
        }
        require_selected_actor_current(c, self.current, self.clock.now().get()).await?;
        ledger_actor(self.current)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        r: &WorkflowRequest,
        _: CommandDisposition,
    ) -> Result<(), Error> {
        self.authorize(c, r).await.map(|_| ())
    }
}
struct Effects {
    before: Option<CurrentFieldInboxDelivery>,
    after: Option<CurrentFieldInboxDelivery>,
    state: FieldInboxState,
}
impl PgOwnerCommandEffects for Effects {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        Ok(())
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        self.before
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await?;
        Ok(WorkflowOutput {
            version: 1,
            result: CommandJson::from_serializable(
                &json!({"id":self.state.id,"version":1,"_nativeFieldInbox":true,"readAt":self.state.read_at,"archivedAt":self.state.archived_at}),
            )?,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.after = Some(
            apply_field_inbox_state(c, self.before.as_ref().ok_or_else(changed)?, &self.state)
                .await?,
        );
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.after
            .as_ref()
            .or(self.before.as_ref())
            .ok_or_else(changed)?
            .require_current(c)
            .await
    }
}
struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("UUID record id")
    }
}
async fn execute<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    target: &FieldCollaborationTarget<C>,
    id: &str,
    key: &str,
    raw: Value,
    initial_count: Option<
        &crate::service_initial_physical_stock::ServiceInitialPhysicalStockEnvironment,
    >,
) -> Result<Value, Error> {
    let invalid =
        || BusinessError::invalid("INVALID_INBOX_STATE", "Choose a read or archive state.");
    let object = raw.as_object().ok_or_else(invalid)?;
    if object.is_empty()
        || object
            .iter()
            .any(|(k, v)| !matches!(k.as_str(), "read" | "archived") || !v.is_boolean())
    {
        return Err(invalid().into());
    }
    let patch: FieldInboxPatch = serde_json::from_value(raw.clone()).map_err(|_| invalid())?;
    let key = CommandKey::new(key)?;
    let before = read_field_inbox_delivery(c, &target.company, current.user_id(), id).await?;
    let guard = prepare_inbox_source(c, current, target, before.candidate(), initial_count)
        .await?
        .ok_or_else(missing)?;
    let state = patch.plan(before.candidate(), target.clock.now().get())?;
    let request = WorkflowRequest {
        key: key.as_str().into(),
        operation: "inbox.delivery.state".into(),
        record_id: id.into(),
        input: CommandJson::from_serializable(&raw)?,
    };
    let mut authority = Authority {
        current,
        clock: target.clock.as_ref(),
        request: &request,
    };
    let mut ids = Ids;
    let mut effects = Effects {
        before: Some(before),
        after: None,
        state,
    };
    let outcome = PgCommandLedger::new(
        target.company.clone(),
        c,
        &mut authority,
        target.clock.as_ref(),
        &mut ids,
    )
    .execute(&request, &mut effects)
    .await?;
    let mut stored: Value = serde_json::from_str(outcome.result.as_str()).map_err(|_| changed())?;
    if stored["version"] != 1 || stored["_nativeFieldInbox"] != true {
        return Err(changed());
    }
    let object = stored.as_object_mut().ok_or_else(changed)?;
    object.remove("version");
    object.remove("_nativeFieldInbox");
    let result: FieldInboxState = serde_json::from_value(stored).map_err(|_| changed())?;
    if result.id != id {
        return Err(changed());
    }
    // Deferred triggers may alter any participating source; retain exact owner
    // images and current IAM after flushing, including the exact replay path.
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *c)
        .await?;
    effects
        .after
        .as_ref()
        .or(effects.before.as_ref())
        .ok_or_else(changed)?
        .require_current(c)
        .await?;
    guard
        .require_current(
            c,
            &SourceRecordContext {
                purchase_locks: None,
                actor: current,
                holding: &target.holding,
                company: &target.company,
                catalogue: &target.catalogue,
                clock: target.clock.as_ref(),
            },
        )
        .await?;
    require_selected_actor_current(c, current, target.clock.now().get()).await?;
    let retained = PgCommandLedger::new(
        target.company.clone(),
        c,
        &mut authority,
        target.clock.as_ref(),
        &mut ids,
    )
    .replay(&request, &mut effects)
    .await?
    .ok_or_else(changed)?;
    if retained.result != outcome.result {
        return Err(changed());
    }
    Ok(json!({"data":result,"context":crate::field_service_reservation_read::context(current)?}))
}
#[allow(clippy::too_many_arguments)]
pub async fn execute_selected_field_inbox_http<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    workspace: Input,
    target: FieldCollaborationTarget<C>,
    sources: InboxSources,
    id: String,
    key: Result<String, BusinessError>,
    body: axum::body::Body,
) -> Result<Value, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let body = Arc::new(tokio::sync::Mutex::new(Some(body)));
    let cache = Arc::new(tokio::sync::OnceCell::new());
    let company = target.company.clone();
    let initial_count = sources.initial_count.clone();
    let callbacks=SelectedTransportCallbacks::<_,(String,Value),BusinessError,_>::new::<Value>(move|_:&SelectedWorkIngress<'_>|->SelectedIngressFuture<(String,Value),BusinessError>{let body=body.clone();let cache=cache.clone();let key=key.clone();Box::pin(async move{let key=key?;let bytes=cache.get_or_init(||async{let body=body.lock().await.take()?;axum::body::to_bytes(body,8192).await.ok().map(|v|v.to_vec())}).await.as_ref().ok_or_else(||BusinessError::invalid("INVALID_JSON","Use valid JSON."))?;let text=std::str::from_utf8(bytes).map_err(|_|BusinessError::invalid("INVALID_JSON","Use valid JSON."))?;let text=text.strip_prefix('\u{feff}').unwrap_or(text);CommandJson::parse(text).map_err(|_|BusinessError::invalid("INVALID_JSON","Use valid JSON."))?;let raw=serde_json::from_str(text).map_err(|_|BusinessError::invalid("INVALID_JSON","Use valid JSON."))?;Ok((key,raw))})},|_,error|Err(error),move|c,current,(key,raw):(String,Value)|{let target=target.clone();let id=id.clone();let initial_count=initial_count.clone();Box::pin(async move{if current.holding_id()!=&target.holding{return Err(missing());}Ok(Ok(execute(c,current,&target,&id,&key,raw,initial_count.as_ref()).await?))})});
    authorizer
        .execute_with_selected_transport_ingress_deferred(
            host,
            proof,
            move || {
                let mut w = workspace()?;
                w.mutation = true;
                if w.company_id != company {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "INBOX_SCOPE_REQUIRED",
                        "Use the current Company.",
                    ));
                }
                Ok(w)
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
        .await?
        .map_err(Into::into)
}
