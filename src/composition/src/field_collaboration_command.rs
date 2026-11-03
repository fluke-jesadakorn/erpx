//! Field-linked Collaboration commands in the existing selected Holding unit.
//! The caller must acquire FieldCollaborationSources before selecting IAM, and
//! propagate every error to the enclosing rollback. No route or commit here.
use crate::field_service_records_source::PreparedRecordsSource;
use crate::field_service_records_source::{
    FieldRecordsSourcePort, PreparedCollaborationAssignee, prepare_assignee,
};
use crate::field_service_source_record::SourceRecordContext;
use erpx_server_collaboration::public::collaboration_work::{self as work, *};
use erpx_server_commercial::public::purchases::CommercialCatalogue;
use erpx_server_identity_access::public::{
    command_metadata::project_command_iam, effective_authority::AuthorizedWorkContext,
    selected_command_authority::require_selected_actor_current, work_context::WorkScopeKind,
};
use erpx_server_records::public::source_record::{SourceRecord, SourceReference};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind, IdentifierSource};
use erpx_server_workflow::public::commands::*;
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "COLLABORATION_SOURCE_CHANGED",
        "Refresh the collaboration record.",
    )
    .into()
}
fn actor(current: &AuthorizedWorkContext) -> Result<AuthorizedActor, Error> {
    current.require_permission("records.read")?;
    current.require_permission("records.write")?;
    if !current.modules().iter().any(|m| m == "collaboration") {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MODULE_FORBIDDEN",
            "Your current role cannot access Collaboration.",
        )
        .into());
    }
    Ok(AuthorizedActor {
        user_id: current.user_id().as_str().into(),
        company_id: current.selection().workspace().company_id.as_str().into(),
        can_review: current.has_permission("records.review"),
    })
}
fn ledger_actor(current: &AuthorizedWorkContext) -> Result<CommandActor, Error> {
    actor(current)?;
    let s = current.selection();
    let a = s.assignment();
    let w = s.workspace();
    let field = |id: Option<&str>| {
        id.map(|v| CommandField::Value(v.into()))
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
    expected: &'a WorkflowRequest,
}
impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        if !c.is_in_transaction() {
            return Err(PersistenceError::new("COLLABORATION_TRANSACTION_REQUIRED").into());
        }
        Ok(())
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        request: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if request.key != self.expected.key
            || request.operation != self.expected.operation
            || request.record_id != self.expected.record_id
            || request.input != self.expected.input
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
        request: &WorkflowRequest,
        _: CommandDisposition,
    ) -> Result<(), Error> {
        self.authorize(c, request).await.map(|_| ())
    }
}
struct Effects {
    actor: AuthorizedActor,
    key: CommandKey,
    plan: Option<OwnerPlan>,
    now: i64,
    retained: Option<CurrentCollaborationEffects>,
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
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let result = plan_receipt(&self.key, self.plan.as_ref().ok_or_else(changed)?)?;
        Ok(WorkflowOutput {
            version: result.version,
            result: encode_receipt(&result)?,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let plan = self.plan.as_ref().ok_or_else(changed)?;
        let receipt = apply_plan(c, &self.actor, &self.key, plan, self.now).await?;
        self.retained =
            Some(retain_effects(c, &self.actor, &self.key, plan, &receipt, self.now).await?);
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        self.retained
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await
    }
}
fn encode_receipt(receipt: &Receipt) -> Result<CommandJson, BusinessError> {
    CommandJson::from_serializable(
        &serde_json::json!({"id":receipt.id,"version":receipt.version,"status":receipt.status}),
    )
}
fn decode_receipt(value: &CommandJson) -> Result<Receipt, Error> {
    let v: serde_json::Value = serde_json::from_str(value.as_str()).map_err(|_| changed())?;
    let status = match v["status"].as_str() {
        Some("active") => "active",
        Some("removed") => "removed",
        Some("open") => "open",
        Some("completed") => "completed",
        Some("cancelled") => "cancelled",
        _ => return Err(changed()),
    };
    let version = v["version"]
        .as_i64()
        .filter(|v| *v > 0 && *v <= 9_007_199_254_740_991)
        .ok_or_else(changed)?;
    Ok(Receipt {
        id: v["id"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(changed)?
            .into(),
        version,
        status,
    })
}
struct Unit<'a, C, I> {
    c: &'a mut PgConnection,
    current: &'a AuthorizedWorkContext,
    clock: &'a C,
    ids: &'a mut I,
    catalogue: &'a CommercialCatalogue,
    request: WorkflowRequest,
    expected: Receipt,
    eligibility: Vec<PreparedCollaborationAssignee>,
    retained: Vec<CurrentCollaborationEffects>,
    card_scope: Option<(String, Option<String>, Option<String>)>,
}
impl<C: Clock + Send + Sync, I: IdentifierSource + Send> Unit<'_, C, I> {
    async fn eligible(
        &mut self,
        user: &str,
        branch: Option<&str>,
        department: Option<&str>,
    ) -> Result<bool, Error> {
        let context = SourceRecordContext {
            actor: self.current,
            holding: self.current.holding_id(),
            company: &self.current.selection().workspace().company_id,
            catalogue: self.catalogue,
            clock: self.clock,
        };
        let assignee =
            prepare_assignee(self.c, &context, user, branch.unwrap_or(""), department).await?;
        let eligible = assignee.eligible();
        self.eligibility.push(assignee);
        Ok(eligible)
    }
    async fn ledger(&mut self, plan: Option<OwnerPlan>) -> Result<Option<Receipt>, Error> {
        let mut authority = Authority {
            current: self.current,
            clock: self.clock,
            expected: &self.request,
        };
        let mut effects = Effects {
            actor: actor(self.current)?,
            key: CommandKey::new(self.request.key.clone())?,
            plan,
            now: self.clock.now().get(),
            retained: None,
        };
        let mut ledger = PgCommandLedger::new(
            self.current.selection().workspace().company_id.clone(),
            self.c,
            &mut authority,
            self.clock,
            self.ids,
        );
        let outcome = if effects.plan.is_some() {
            Some(ledger.execute(&self.request, &mut effects).await?)
        } else {
            ledger.replay(&self.request, &mut effects).await?
        };
        if let Some(retained) = effects.retained {
            self.retained.push(retained);
        }
        let receipt = outcome.map(|v| decode_receipt(&v.result)).transpose()?;
        if receipt.as_ref().is_some_and(|r| r != &self.expected) {
            return Err(changed());
        }
        Ok(receipt)
    }
}
impl<C: Clock + Send + Sync, I: IdentifierSource + Send> CollaborationSourcePort
    for Unit<'_, C, I>
{
    type Error = Error;
    type Witness = PreparedRecordsSource;
    async fn prepare(
        &mut self,
        reference: &SourceReference,
    ) -> Result<(SourceRecord, Self::Witness), Error> {
        let context = SourceRecordContext {
            actor: self.current,
            holding: self.current.holding_id(),
            company: &self.current.selection().workspace().company_id,
            catalogue: self.catalogue,
            clock: self.clock,
        };
        let company = self.current.selection().workspace().company_id.as_str();
        let mut port = FieldRecordsSourcePort {
            connection: self.c,
            context,
        };
        Ok(erpx_server_records::public::source_record::prepare_source(
            &mut port, company, reference,
        )
        .await?
        .into_parts())
    }
    async fn require_current(&mut self, witness: &Self::Witness) -> Result<(), Error> {
        let context = SourceRecordContext {
            actor: self.current,
            holding: self.current.holding_id(),
            company: &self.current.selection().workspace().company_id,
            catalogue: self.catalogue,
            clock: self.clock,
        };
        witness.require_current(self.c, &context).await
    }
}
impl<C: Clock + Send + Sync, I: IdentifierSource + Send> CollaborationUnit for Unit<'_, C, I> {
    async fn authorize(&mut self) -> Result<AuthorizedActor, Error> {
        require_selected_actor_current(self.c, self.current, self.clock.now().get()).await?;
        actor(self.current)
    }
    async fn replay(&mut self, _: &Change) -> Result<Option<Receipt>, Error> {
        self.ledger(None).await
    }
    async fn relation(&mut self, id: &str) -> Result<CurrentRelation, Error> {
        read_relation(self.c, &self.current.selection().workspace().company_id, id)
            .await
            .map(|v| v.0)
    }
    async fn card(&mut self, id: &str) -> Result<CurrentCard, Error> {
        let scope = self.current.selection().workspace();
        let (row, _) = read_card(
            self.c,
            &scope.company_id,
            id,
            scope.branch_id.as_ref().map(|v| v.as_str()),
            scope.department_id.as_ref().map(|v| v.as_str()),
        )
        .await?;
        let eligible = self
            .eligible(
                &row.owner_id,
                row.branch_id.as_deref(),
                row.department_id.as_deref(),
            )
            .await?;
        self.card_scope = Some((
            row.id.clone(),
            row.branch_id.clone(),
            row.department_id.clone(),
        ));
        Ok(CurrentCard {
            id: row.id,
            version: row.version,
            open: row.open,
            owner_id: row.owner_id,
            due_at: row.due_at,
            owner_eligible: eligible,
            source: row.source,
        })
    }
    async fn apply(&mut self, _: &AuthorizedActor, plan: OwnerPlan) -> Result<Receipt, Error> {
        let eligibility = match &plan {
            OwnerPlan::CreateCard {
                source, owner_id, ..
            } => Some((
                owner_id.clone(),
                source.branch_id.clone(),
                source.department_id.clone(),
            )),
            OwnerPlan::ChangeCard {
                current,
                action: CardAction::Reassign,
                owner_id: Some(user),
                ..
            } => {
                let (id, branch, department) = self.card_scope.as_ref().ok_or_else(changed)?;
                if id != &current.id {
                    return Err(changed());
                }
                Some((user.clone(), branch.clone(), department.clone()))
            }
            _ => None,
        };
        if let Some((user, branch, department)) = eligibility
            && !self
                .eligible(&user, branch.as_deref(), department.as_deref())
                .await?
        {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "ACTION_CARD_OWNER_INELIGIBLE",
                "Choose an eligible action owner.",
            )
            .into());
        }
        self.ledger(Some(plan)).await?.ok_or_else(changed)
    }
    async fn flush_deferred(&mut self) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *self.c)
            .await?;
        Ok(())
    }
    async fn require_unit_current(&mut self) -> Result<(), Error> {
        for assignee in &self.eligibility {
            assignee.require_current(self.c, self.clock).await?;
        }
        for effect in &self.retained {
            effect.require_current(self.c).await?;
        }
        require_selected_actor_current(self.c, self.current, self.clock.now().get()).await?;
        actor(self.current)?;
        if self.ledger(None).await?.is_none() {
            return Err(changed());
        }
        Ok(())
    }
}
/// Normalization and receipt identity are constructed together, so HTTP callers
/// cannot pair one mutation with another input fingerprint. Still unmounted
/// until the native Field journey and retirement gates pass.
pub(crate) async fn execute<C: Clock + Sync + Send, I: IdentifierSource + Send>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    catalogue: &CommercialCatalogue,
    clock: &C,
    ids: &mut I,
    key: &str,
    operation: &str,
    id: Option<&str>,
    raw: &serde_json::Value,
) -> Result<Receipt, Error> {
    let key = CommandKey::new(key)?;
    let decoded = decode_work_command(operation, id, raw)?;
    let record_id = match &decoded.change {
        Change::CreateRelation { .. } => format!("record-relation-{}", key.as_str()),
        Change::CreateCard { .. } => format!("action-card-{}", key.as_str()),
        Change::RemoveRelation { id, .. } | Change::ChangeCard { id, .. } => id.clone(),
    };
    let version = match &decoded.change {
        Change::RemoveRelation {
            expected_version, ..
        }
        | Change::ChangeCard {
            expected_version, ..
        } => expected_version
            .checked_add(1)
            .filter(|v| *v <= 9_007_199_254_740_991)
            .ok_or_else(changed)?,
        _ => 1,
    };
    let status = match &decoded.change {
        Change::CreateRelation { .. } => "active",
        Change::RemoveRelation { .. } => "removed",
        Change::CreateCard { .. } => "open",
        Change::ChangeCard {
            action: CardAction::Complete,
            ..
        } => "completed",
        Change::ChangeCard {
            action: CardAction::Cancel,
            ..
        } => "cancelled",
        Change::ChangeCard {
            action: CardAction::Reassign,
            ..
        } => "open",
    };
    let expected = Receipt {
        id: record_id.clone(),
        version,
        status,
    };
    let request = WorkflowRequest {
        key: key.as_str().into(),
        operation: operation.into(),
        record_id,
        input: CommandJson::from_serializable(&decoded.input)?,
    };
    let mut unit = Unit {
        c,
        current,
        clock,
        ids,
        catalogue,
        request,
        expected,
        eligibility: vec![],
        retained: vec![],
        card_scope: None,
    };
    work::execute(&mut unit, decoded.change).await
}

/// Trusted composition target. Request data cannot supply its Holding, Company,
/// purchase catalogue or clock. This administration is not a mounted HTTP route.
pub struct FieldCollaborationTarget<C> {
    pub holding: erpx_server_shared::public::HoldingId,
    pub company: erpx_server_shared::public::CompanyId,
    pub clock: std::sync::Arc<C>,
    pub catalogue: std::sync::Arc<CommercialCatalogue>,
}
impl<C> Clone for FieldCollaborationTarget<C> {
    fn clone(&self) -> Self {
        Self {
            holding: self.holding.clone(),
            company: self.company.clone(),
            clock: self.clock.clone(),
            catalogue: self.catalogue.clone(),
        }
    }
}
#[derive(Clone)]
pub struct FieldCollaborationInput {
    pub key: String,
    pub operation: String,
    pub id: Option<String>,
    pub body: Vec<u8>,
}
struct CommandIds;
impl IdentifierSource for CommandIds {
    fn next(&mut self) -> erpx_server_shared::public::RecordId {
        erpx_server_shared::public::RecordId::new(uuid::Uuid::new_v4().to_string())
            .expect("UUID is a valid record identifier")
    }
}
/// Normal Account/session and selected-work authorization precede JSON parsing.
/// All participating source locks precede the first selected IAM snapshot.
pub async fn execute_selected_field_collaboration<C, Input>(
    authorizer: &erpx_server_identity_access::public::effective_authority::HoldingWorkAuthorizer,
    host: &str,
    proof: erpx_server_identity_access::public::DeviceProof,
    workspace: Input,
    target: FieldCollaborationTarget<C>,
    command: FieldCollaborationInput,
) -> Result<Receipt, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<
            erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest,
            BusinessError,
        > + Send
        + 'static,
{
    use erpx_server_identity_access::public::effective_authority::WorkAuthorizationRequest;
    let expected_company = target.company.clone();
    authorizer
        .execute_with_sources_deferred(
            host,
            proof,
            move || {
                let mut workspace = workspace()?;
                workspace.mutation = true;
                if workspace.company_id != expected_company {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "COLLABORATION_SCOPE_REQUIRED",
                        "Use the current Company.",
                    ));
                }
                Ok(workspace)
            },
            WorkAuthorizationRequest {
                permission: "records.write".into(),
                module: Some("collaboration".into()),
                destination: "collaboration".into(),
                api: true,
            },
            crate::field_service_records_source::FieldCollaborationSources,
            move |c, current| {
                let target = target.clone();
                let command = command.clone();
                Box::pin(async move {
                    if current.holding_id() != &target.holding
                        || current.selection().workspace().company_id != target.company
                    {
                        return Err(BusinessError::new(
                            FailureKind::Forbidden,
                            "COLLABORATION_SCOPE_REQUIRED",
                            "Use the current Company.",
                        )
                        .into());
                    }
                    if command.body.len() > 2 * 1024 * 1024 {
                        return Err(
                            BusinessError::invalid("INVALID_JSON", "Use valid JSON.").into()
                        );
                    }
                    let text = String::from_utf8_lossy(&command.body);
                    let text = text.strip_prefix('\u{feff}').unwrap_or(&text);
                    CommandJson::parse(text)
                        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Use valid JSON."))?;
                    let raw = serde_json::from_str(text)
                        .map_err(|_| BusinessError::invalid("INVALID_JSON", "Use valid JSON."))?;
                    execute(
                        c,
                        current,
                        &target.catalogue,
                        target.clock.as_ref(),
                        &mut CommandIds,
                        &command.key,
                        &command.operation,
                        command.id.as_deref(),
                        &raw,
                    )
                    .await
                })
            },
        )
        .await
}

/// Generic UI envelope is decoded only after normal selected authentication.
/// Cached bytes are reused by a serializable retry, never re-read from a stream.
pub async fn execute_selected_field_collaboration_http<C, Input>(
    authorizer: &erpx_server_identity_access::public::effective_authority::HoldingWorkAuthorizer,
    host: &str,
    proof: erpx_server_identity_access::public::DeviceProof,
    workspace: Input,
    target: FieldCollaborationTarget<C>,
    key: Result<String, BusinessError>,
    body: axum::body::Body,
) -> Result<serde_json::Value, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<
            erpx_server_identity_access::public::work_context::HoldingWorkspaceRequest,
            BusinessError,
        > + Send
        + 'static,
{
    use erpx_server_identity_access::public::effective_authority::{
        SelectedIngressFuture, SelectedTransportCallbacks, SelectedWorkIngress,
        WorkAuthorizationRequest,
    };
    use std::sync::Arc;
    let body = Arc::new(tokio::sync::Mutex::new(Some(body)));
    let cache = Arc::new(tokio::sync::OnceCell::new());
    let expected_company = target.company.clone();
    let callbacks=SelectedTransportCallbacks::<_,(String,serde_json::Value),BusinessError,_>::new::<serde_json::Value>(
        move|_:&SelectedWorkIngress<'_>|->SelectedIngressFuture<(String,serde_json::Value),BusinessError>{let body=body.clone();let cache=cache.clone();let key=key.clone();Box::pin(async move{
            let key=key?;let bytes=cache.get_or_init(||async{let body=body.lock().await.take()?;axum::body::to_bytes(body,2*1024*1024).await.ok().map(|v|v.to_vec())}).await.as_ref().ok_or_else(||BusinessError::invalid("INVALID_JSON","Use valid JSON."))?;
            let text=String::from_utf8_lossy(bytes);let text=text.strip_prefix('\u{feff}').unwrap_or(&text);
            CommandJson::parse(text).map_err(|_|BusinessError::invalid("INVALID_JSON","Use valid JSON."))?;
            let raw=serde_json::from_str(text).map_err(|_|BusinessError::invalid("INVALID_JSON","Use valid JSON."))?;
            Ok((key,raw))
        })},|_,e|Err(e),move|c,current,(key,raw):(String,serde_json::Value)|{let target=target.clone();Box::pin(async move{
            if current.holding_id()!=&target.holding || current.selection().workspace().company_id!=target.company {return Err(BusinessError::new(FailureKind::Forbidden,"COLLABORATION_SCOPE_REQUIRED","Use the current Company.").into());}
            let operation=if raw["entity"]=="relation" {if raw["action"]=="remove"{"collaboration.relation.remove"}else{"collaboration.relation.create"}}else if raw["id"].is_string(){match raw["action"].as_str(){Some("complete")=>"collaboration.action.complete",Some("reassign")=>"collaboration.action.reassign",Some("cancel")=>"collaboration.action.cancel",_=>return Err(BusinessError::invalid("COLLABORATION_INPUT_INVALID","Provide a valid collaboration value.").into())}}else{"collaboration.action.create"};
            let result=execute(c,current,&target.catalogue,target.clock.as_ref(),&mut CommandIds,&key,operation,raw["id"].as_str(),&raw).await?;
            Ok(Ok(serde_json::json!({"data":{"id":result.id,"version":result.version,"status":result.status},"context":crate::field_service_reservation_read::context(current)?})))
        })});
    authorizer
        .execute_with_selected_transport_ingress_deferred(
            host,
            proof,
            move || {
                let mut w = workspace()?;
                w.mutation = true;
                if w.company_id != expected_company {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "COLLABORATION_SCOPE_REQUIRED",
                        "Use the current Company.",
                    ));
                }
                Ok(w)
            },
            (
                WorkAuthorizationRequest {
                    permission: "records.write".into(),
                    module: Some("collaboration".into()),
                    destination: "collaboration".into(),
                    api: true,
                },
                crate::field_service_records_source::FieldCollaborationSources,
            ),
            callbacks,
        )
        .await?
        .map_err(Into::into)
}
