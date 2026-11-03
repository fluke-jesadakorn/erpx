//! Field-source Operations transitions on the actual selected Holding unit.
//! Candidate HTTP wiring shares this unit; native opening/policy publication,
//! frontend journeys and legacy retirement are still required for delivery.
use crate::field_collaboration_command::FieldCollaborationTarget;
use crate::field_service_records_source::PreparedRecordsSource;
use crate::field_service_records_source::{FieldCollaborationSources, FieldRecordsSourcePort};
use crate::field_service_source_record::SourceRecordContext;
use erpx_server_collaboration::public::operations_notification::{
    RetainedOperationsNotification, stage_operations_notification,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    command_metadata::project_command_iam,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    field_service_workspace::{
        CurrentServiceWorkspaceAuthority, ServiceWorkspaceAssignmentQuery,
        ServiceWorkspaceAuthorityRequest, read_service_workspace_authority,
    },
    selected_command_authority::require_selected_actor_current,
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_operations::public::field_exceptions::*;
use erpx_server_people_payroll::public::workforce_reference::{
    CurrentWorkforceReference, inspect_workforce_reference_raw,
};
use erpx_server_records::public::source_record::{SourceKind, SourceReference, prepare_source};
use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CommandKey, FailureKind, IdentifierSource};
use erpx_server_workflow::public::commands::*;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection};
type Error = PgTransactionError<BusinessError>;
fn changed() -> Error {
    BusinessError::new(
        FailureKind::Conflict,
        "OPERATIONS_STATE_CHANGED",
        "Review the current exception state.",
    )
    .into()
}
fn require_access(current: &AuthorizedWorkContext) -> Result<(), Error> {
    current.require_permission("records.read")?;
    current.require_permission("records.write")?;
    if !current.modules().iter().any(|m| m == "operations") {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "MODULE_FORBIDDEN",
            "Operations is unavailable to the selected role.",
        )
        .into());
    }
    Ok(())
}
#[derive(Clone)]
struct Sources;
impl HoldingWorkSourcePreparation for Sources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            // Own intent precedes selected authority; the shared participant order is unchanged.
            lock_field_exception_sources(c).await?;
            FieldCollaborationSources.prepare(c).await
        })
    }
}
fn ledger_actor(current: &AuthorizedWorkContext) -> Result<CommandActor, Error> {
    require_access(current)?;
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
            return Err(PersistenceError::new("OPERATIONS_TRANSACTION_REQUIRED").into());
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
    record: Option<CurrentFieldException>,
    actor: Actor,
    input: TransitionInput,
    plan: Option<Transition>,
    key: CommandKey,
    now: i64,
    retained: Option<RetainedFieldException>,
    notice: Option<RetainedOperationsNotification>,
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
        let row = self.record.as_ref().ok_or_else(changed)?.data();
        let plan = self.plan.as_ref().ok_or_else(changed)?;
        let field = |v: Option<&str>| {
            v.map(|v| CommandField::Value(v.to_owned()))
                .unwrap_or(CommandField::Null)
        };
        Ok(WorkflowOutput {
            version: plan.version,
            result: CommandJson::from_serializable(
                &json!({"id":row.id,"status":plan.status,"version":plan.version}),
            )?,
            event_scope: Some(CommandEventScope {
                company_id: erpx_server_shared::public::CompanyId::new(&row.company_id)?,
                branch_id: field(row.branch_id.as_deref()),
                department_id: field(row.department_id.as_deref()),
            }),
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        let record = self.record.as_ref().ok_or_else(changed)?;
        let plan = self.plan.as_ref().ok_or_else(changed)?;
        let row = record.data();
        self.retained = Some(
            apply_field_exception_transition(
                c,
                record,
                &self.actor,
                &self.input,
                plan,
                &self.key,
                self.now,
            )
            .await?,
        );
        let recipient = if self.input.action == Action::Escalate {
            plan.escalation_owner.as_deref().ok_or_else(changed)?
        } else {
            &plan.owner
        };
        self.notice = Some(
            stage_operations_notification(
                c,
                &self.key,
                &erpx_server_shared::public::CompanyId::new(&row.company_id)?,
                recipient,
                &row.id,
                plan.version,
                self.now,
            )
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
        self.retained
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await?;
        self.notice
            .as_ref()
            .ok_or_else(changed)?
            .require_current(c)
            .await
    }
}
struct Recipient {
    iam: CurrentServiceWorkspaceAuthority,
    people: CurrentWorkforceReference,
}
struct Unit<'a, C, I> {
    c: &'a mut PgConnection,
    current: &'a AuthorizedWorkContext,
    target: &'a FieldCollaborationTarget<C>,
    ids: &'a mut I,
    request: WorkflowRequest,
    record: Option<CurrentFieldException>,
    actor: Option<Actor>,
    actor_iam: Option<CurrentServiceWorkspaceAuthority>,
    retained: Option<Effects>,
}
impl<C: Clock + Sync + Send, I: IdentifierSource + Send> FieldExceptionUnit for Unit<'_, C, I> {
    type Error = Error;
    type SourceWitness = PreparedRecordsSource;
    type RecipientWitness = Recipient;
    async fn actor(&mut self, row: &Exception) -> Result<Actor, Error> {
        require_access(self.current)?;
        let iam = read_service_workspace_authority(
            self.c,
            ServiceWorkspaceAuthorityRequest {
                company: self.target.company.clone(),
                actor: self.current.user_id().as_str().into(),
                assignments: vec![],
                same_people: std::iter::once(row.owner_id.clone())
                    .chain(row.resolved_by.clone())
                    .collect(),
            },
            self.target.clock.now(),
        )
        .await?;
        let same = |user: &str| {
            iam.data()
                .same_people
                .iter()
                .any(|v| v.user_id == user && v.same_person)
        };
        let actor = Actor {
            user: self.current.user_id().as_str().into(),
            can_review: self.current.has_permission("records.review"),
            same_person_owner: same(&row.owner_id),
            same_person_resolver: row.resolved_by.as_deref().is_some_and(same),
        };
        self.actor_iam = Some(iam);
        self.actor = Some(actor.clone());
        Ok(actor)
    }
    async fn source(&mut self, row: &Exception) -> Result<PreparedRecordsSource, Error> {
        let context = SourceRecordContext {
            actor: self.current,
            holding: &self.target.holding,
            company: &self.target.company,
            catalogue: &self.target.catalogue,
            clock: self.target.clock.as_ref(),
        };
        let mut port = FieldRecordsSourcePort {
            connection: self.c,
            context,
        };
        // Original source version is historical evidence. Like legacy detail/guards,
        // transitions require current visibility rather than the old source version.
        Ok(prepare_source(
            &mut port,
            self.target.company.as_str(),
            &SourceReference {
                kind: SourceKind::FieldServiceOrder,
                id: row.source_id.clone(),
                expected_version: None,
            },
        )
        .await?
        .into_parts()
        .1)
    }
    async fn recipient(
        &mut self,
        row: &Exception,
        user: &str,
        review: bool,
    ) -> Result<Recipient, Error> {
        prepare_operations_recipient(
            self.c,
            self.current,
            self.target,
            row.branch_id.as_deref(),
            row.department_id.as_deref(),
            user,
            review,
        )
        .await
    }
    async fn apply(
        &mut self,
        _: &Exception,
        input: &TransitionInput,
        plan: &Transition,
    ) -> Result<(), Error> {
        let mut authority = Authority {
            current: self.current,
            clock: self.target.clock.as_ref(),
            expected: &self.request,
        };
        let mut effects = Effects {
            record: self.record.take(),
            actor: self.actor.clone().ok_or_else(changed)?,
            input: input.clone(),
            plan: Some(plan.clone()),
            key: CommandKey::new(&self.request.key)?,
            now: self.target.clock.now().get(),
            retained: None,
            notice: None,
        };
        let mut ledger = PgCommandLedger::new(
            self.target.company.clone(),
            self.c,
            &mut authority,
            self.target.clock.as_ref(),
            self.ids,
        );
        let outcome = ledger.execute(&self.request, &mut effects).await?;
        let expected = CommandJson::from_serializable(
            &json!({"id":self.request.record_id,"status":plan.status,"version":plan.version}),
        )?;
        if outcome.result != expected {
            return Err(changed());
        }
        self.retained = Some(effects);
        Ok(())
    }
    async fn flush(&mut self) -> Result<(), Error> {
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *self.c)
            .await?;
        Ok(())
    }
    async fn require_current(
        &mut self,
        source: &PreparedRecordsSource,
        recipients: &[Recipient],
    ) -> Result<(), Error> {
        source
            .require_current(
                self.c,
                &SourceRecordContext {
                    actor: self.current,
                    holding: &self.target.holding,
                    company: &self.target.company,
                    catalogue: &self.target.catalogue,
                    clock: self.target.clock.as_ref(),
                },
            )
            .await?;
        for r in recipients {
            r.iam
                .require_current(self.c, self.target.clock.now())
                .await?;
            r.people
                .require_current(self.c, self.target.clock.now())
                .await?;
        }
        self.actor_iam
            .as_ref()
            .ok_or_else(changed)?
            .require_current(self.c, self.target.clock.now())
            .await?;
        let retained = self.retained.as_ref().ok_or_else(changed)?;
        retained
            .retained
            .as_ref()
            .ok_or_else(changed)?
            .require_current(self.c)
            .await?;
        retained
            .notice
            .as_ref()
            .ok_or_else(changed)?
            .require_current(self.c)
            .await?;
        require_selected_actor_current(self.c, self.current, self.target.clock.now().get()).await?;
        Ok(())
    }
}
fn decode(raw: &Value) -> Result<(TransitionInput, Value), BusinessError> {
    let invalid = || {
        BusinessError::invalid(
            "OPERATIONS_INPUT_INVALID",
            "Provide a valid Operations value.",
        )
    };
    let o = raw.as_object().ok_or_else(invalid)?;
    let action = match o
        .get("action")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?
    {
        "acknowledge" => Action::Acknowledge,
        "start" => Action::Start,
        "resolve" => Action::Resolve,
        "approve" => Action::Approve,
        "reject" => Action::Reject,
        "close" => Action::Close,
        "reopen" => Action::Reopen,
        "assign" => Action::Assign,
        "prioritize" => Action::Prioritize,
        "escalate" => Action::Escalate,
        "cancel" => Action::Cancel,
        _ => return Err(invalid()),
    };
    let version = o
        .get("expectedVersion")
        .and_then(Value::as_f64)
        .filter(|v| v.fract() == 0.0 && *v >= 1.0 && *v <= 9_007_199_254_740_991.0)
        .ok_or_else(invalid)? as i64;
    let reason = o
        .get("reason")
        .and_then(Value::as_str)
        .ok_or_else(invalid)?;
    text(reason, 3, 4000)?;
    let mut normalized =
        json!({"action":action.as_str(),"expectedVersion":version,"reason":trimmed(reason)});
    let mut input = TransitionInput {
        action,
        expected_version: version,
        reason: trimmed(reason).into(),
        owner_id: None,
        escalation_owner_id: None,
        resolution: None,
        priority: None,
    };
    let field = match action {
        Action::Assign => Some("ownerId"),
        Action::Escalate => Some("escalationOwnerId"),
        Action::Resolve => Some("resolution"),
        Action::Prioritize => Some("priority"),
        _ => None,
    };
    if let Some(field) = field {
        let v = o.get(field).and_then(Value::as_str).ok_or_else(invalid)?;
        let v = if action == Action::Resolve {
            text(v, 3, 4000)?;
            trimmed(v)
        } else {
            v
        };
        if matches!(action, Action::Assign | Action::Escalate)
            && (v.is_empty() || v.encode_utf16().count() > 180)
        {
            return Err(invalid());
        }
        if action == Action::Prioritize && !matches!(v, "normal" | "high" | "critical") {
            return Err(invalid());
        }
        normalized[field] = json!(v);
        match action {
            Action::Assign => input.owner_id = Some(v.into()),
            Action::Escalate => input.escalation_owner_id = Some(v.into()),
            Action::Resolve => input.resolution = Some(v.into()),
            Action::Prioritize => input.priority = Some(v.into()),
            _ => {}
        }
    }
    Ok((input, normalized))
}
#[derive(Clone)]
pub struct FieldOperationsTransition {
    pub id: String,
    pub key: String,
    pub body: Value,
}
/// Trusted selected command entry; HTTP uses the same inner transaction.
pub async fn execute_selected_field_operations<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    workspace: Input,
    target: FieldCollaborationTarget<C>,
    command: FieldOperationsTransition,
) -> Result<Value, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let company = target.company.clone();
    authorizer
        .execute_with_sources_deferred(
            host,
            proof,
            move || {
                let mut w = workspace()?;
                w.mutation = true;
                if w.company_id != company {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "OPERATIONS_SCOPE_REQUIRED",
                        "Use the current Company.",
                    ));
                }
                Ok(w)
            },
            WorkAuthorizationRequest {
                permission: "records.write".into(),
                module: Some("operations".into()),
                destination: "operations".into(),
                api: true,
            },
            Sources,
            move |c, current| {
                let target = target.clone();
                let command = command.clone();
                Box::pin(async move { execute(c, current, &target, command).await })
            },
        )
        .await
}

async fn execute<C: Clock + Sync + Send>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    target: &FieldCollaborationTarget<C>,
    command: FieldOperationsTransition,
) -> Result<Value, Error> {
    if current.holding_id() != &target.holding
        || current.selection().workspace().company_id != target.company
    {
        return Err(changed());
    }
    require_access(current)?;
    let (input, normalized) = decode(&command.body)?;
    let key = CommandKey::new(&command.key)?;
    let request = WorkflowRequest {
        key: key.as_str().into(),
        operation: format!("operations.exception.{}", input.action.as_str()),
        record_id: command.id.clone(),
        input: CommandJson::from_serializable(&normalized)?,
    };
    let scope = current.selection().workspace();
    let record = read_field_exception(
        c,
        &target.company,
        &command.id,
        scope.branch_id.as_ref().map(|v| v.as_str()),
        scope.department_id.as_ref().map(|v| v.as_str()),
    )
    .await?
    .ok_or_else(|| {
        BusinessError::new(
            FailureKind::NotFound,
            "OPERATIONS_EXCEPTION_NOT_FOUND",
            "Exception not found in this scope.",
        )
    })?;
    let row = record.data().clone();
    let mut ids = Ids;
    let mut unit = Unit {
        c,
        current,
        target,
        ids: &mut ids,
        request,
        record: Some(record),
        actor: None,
        actor_iam: None,
        retained: None,
    };
    // Replay still requires current source visibility; it never executes effects.
    let source = unit.source(&row).await?;
    let mut authority = Authority {
        current,
        clock: target.clock.as_ref(),
        expected: &unit.request,
    };
    let mut effects = Effects {
        record: None,
        actor: Actor {
            user: current.user_id().as_str().into(),
            can_review: false,
            same_person_owner: false,
            same_person_resolver: false,
        },
        input: input.clone(),
        plan: None,
        key,
        now: target.clock.now().get(),
        retained: None,
        notice: None,
    };
    let prior = PgCommandLedger::new(
        target.company.clone(),
        unit.c,
        &mut authority,
        target.clock.as_ref(),
        unit.ids,
    )
    .replay(&unit.request, &mut effects)
    .await?;
    if let Some(prior) = prior {
        source
            .require_current(
                unit.c,
                &SourceRecordContext {
                    actor: current,
                    holding: &target.holding,
                    company: &target.company,
                    catalogue: &target.catalogue,
                    clock: target.clock.as_ref(),
                },
            )
            .await?;
        let value: Value = serde_json::from_str(prior.result.as_str()).map_err(|_| changed())?;
        if value["id"] != command.id || value["version"] != input.expected_version + 1 {
            return Err(changed());
        }
        let statuses: &[&str] = match input.action {
            Action::Acknowledge => &["acknowledged"],
            Action::Start | Action::Reject | Action::Reopen => &["in_progress"],
            Action::Resolve => &["awaiting_review"],
            Action::Approve => &["resolved"],
            Action::Close => &["closed"],
            Action::Cancel => &["cancelled"],
            Action::Escalate => &["escalated"],
            Action::Assign | Action::Prioritize => {
                &["open", "acknowledged", "in_progress", "escalated"]
            }
        };
        if !value["status"]
            .as_str()
            .is_some_and(|v| statuses.contains(&v))
        {
            return Err(changed());
        }
        unit.record
            .as_ref()
            .ok_or_else(changed)?
            .require_current(unit.c)
            .await?;
        return Ok(value);
    }
    let plan = apply_transition(&mut unit, &row, &input).await?;
    Ok(json!({"id":command.id,"status":plan.status,"version":plan.version}))
}

struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> erpx_server_shared::public::RecordId {
        erpx_server_shared::public::RecordId::new(uuid::Uuid::new_v4().to_string())
            .expect("generated UUID")
    }
}

#[allow(clippy::too_many_arguments)]
pub async fn execute_selected_field_operations_http<C, Input>(
    authorizer: &erpx_server_identity_access::public::effective_authority::HoldingWorkAuthorizer,
    host: &str,
    proof: erpx_server_identity_access::public::DeviceProof,
    workspace: Input,
    target: FieldCollaborationTarget<C>,
    key: Result<String, BusinessError>,
    id: Option<String>,
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
        })},|_,e|Err(e),move|c,current,(key,raw):(String,serde_json::Value)|{let target=target.clone();let id=id.clone();Box::pin(async move{
            if current.holding_id()!=&target.holding || current.selection().workspace().company_id!=target.company {return Err(BusinessError::new(FailureKind::Forbidden,"OPERATIONS_SCOPE_REQUIRED","Use the current Company.").into());}
            let result=if let Some(id)=id{execute(c,current,&target,FieldOperationsTransition{id,key,body:raw}).await?}else{
                let id=raw.get("id").map(|v|v.as_str().filter(|v|!v.is_empty()).map(str::to_owned).ok_or_else(||BusinessError::invalid("OPERATIONS_INPUT_INVALID","Provide a valid Operations id."))).transpose()?;
                if raw["entity"]=="policy"||id.is_none(){setup::execute_setup(c,current,&target,FieldOperationsSetup{key,id,body:raw}).await?}else{let id=id.ok_or_else(||BusinessError::invalid("OPERATIONS_INPUT_INVALID","Provide a valid Operations id."))?;execute(c,current,&target,FieldOperationsTransition{id,key,body:raw}).await?}
            };
            Ok(Ok(serde_json::json!({"data":result,"context":crate::field_service_reservation_read::context(current)?})))
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
                        "OPERATIONS_SCOPE_REQUIRED",
                        "Use the current Company.",
                    ));
                }
                Ok(w)
            },
            (
                WorkAuthorizationRequest {
                    permission: "records.write".into(),
                    module: Some("operations".into()),
                    destination: "operations".into(),
                    api: true,
                },
                Sources,
            ),
            callbacks,
        )
        .await?
        .map_err(Into::into)
}

pub async fn read_selected_field_operations<C, Input>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    workspace: Input,
    target: FieldCollaborationTarget<C>,
    id: String,
) -> Result<Value, Error>
where
    C: Clock + Send + Sync + 'static,
    Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let company = target.company.clone();
    authorizer.execute_with_sources_deferred(host,proof,move||{let mut w=workspace()?;w.mutation=false;if w.company_id!=company{return Err(BusinessError::new(FailureKind::Forbidden,"OPERATIONS_SCOPE_REQUIRED","Use the current Company."));}Ok(w)},WorkAuthorizationRequest{permission:"records.read".into(),module:Some("operations".into()),destination:"operations".into(),api:true},Sources,move|c,current|{let target=target.clone();let id=id.clone();Box::pin(async move{
  if current.holding_id()!=&target.holding||current.selection().workspace().company_id!=target.company{return Err(changed());}
  let scope=current.selection().workspace();let record=read_field_exception(c,&target.company,&id,scope.branch_id.as_ref().map(|v|v.as_str()),scope.department_id.as_ref().map(|v|v.as_str())).await?.ok_or_else(||BusinessError::new(FailureKind::NotFound,"OPERATIONS_EXCEPTION_NOT_FOUND","Exception not found in this scope."))?;
  let view=read_field_exception_view(c,record).await?;
  let mut port=FieldRecordsSourcePort{connection:c,context:SourceRecordContext{actor:current,holding:&target.holding,company:&target.company,catalogue:&target.catalogue,clock:target.clock.as_ref()}};
  let prepared=prepare_source(&mut port,target.company.as_str(),&SourceReference{kind:SourceKind::FieldServiceOrder,id:view.source_id().into(),expected_version:None}).await?;
  let (source,witness)=prepared.into_parts();
  view.require_current(c).await?;witness.require_current(c,&SourceRecordContext{actor:current,holding:&target.holding,company:&target.company,catalogue:&target.catalogue,clock:target.clock.as_ref()}).await?;require_selected_actor_current(c,current,target.clock.now().get()).await?;
  Ok(json!({"data":{"row":view.data()["row"],"history":view.data()["history"],"source":{"kind":source.kind.as_str(),"id":source.id,"companyId":source.company_id,"branchId":source.branch_id,"departmentId":source.department_id,"version":source.version,"title":source.title,"status":source.status},"canWrite":current.has_permission("records.write"),"canReview":current.has_permission("records.review")},"context":crate::field_service_reservation_read::context(current)?}))
 })}).await
}

mod setup;
pub use setup::{
    FieldOperationsSetup, execute_selected_field_operations_setup,
    read_selected_field_operations_workspace,
};

async fn prepare_operations_recipient<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    target: &FieldCollaborationTarget<C>,
    branch: Option<&str>,
    department: Option<&str>,
    user: &str,
    review: bool,
) -> Result<Recipient, Error> {
    let branch = branch.unwrap_or_default().to_owned();
    let iam = read_service_workspace_authority(
        c,
        ServiceWorkspaceAuthorityRequest {
            company: target.company.clone(),
            actor: current.user_id().as_str().into(),
            assignments: vec![ServiceWorkspaceAssignmentQuery {
                user_id: user.into(),
                branch_id: branch,
                department_id: department.map(str::to_owned),
            }],
            same_people: vec![],
        },
        target.clock.now(),
    )
    .await?;
    let people =
        inspect_workforce_reference_raw(c, &target.company, user, target.clock.now()).await?;
    if !people.is_eligible()
        || !iam
            .data()
            .assignments
            .iter()
            .any(|v| v.user_id == user && if review { v.review } else { v.write })
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "OPERATIONS_RECIPIENT_UNAVAILABLE",
            "Choose an active eligible recipient in this scope.",
        )
        .into());
    }
    Ok(Recipient { iam, people })
}
