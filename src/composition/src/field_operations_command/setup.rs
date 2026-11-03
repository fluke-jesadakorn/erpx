//! Policy and opening variants share the selected authority and Workflow ledger
//! with native Operations transitions; all business persistence stays owned.
use super::*;
use erpx_server_operations::public::field_policy::*;
use erpx_server_shared::public::record_source_contract::SourceRecord;
#[derive(Clone)]
pub struct FieldOperationsSetup {
    pub key: String,
    pub id: Option<String>,
    pub body: Value,
}
enum Plan {
    Draft {
        before: CurrentPolicyArea,
        input: DraftPolicy,
    },
    Review {
        before: CurrentPolicyArea,
        id: String,
        action: PolicyAction,
        version: i64,
        same_creator: bool,
        reason: String,
    },
    Open {
        input: OpenException,
        source: SourceRecord,
        source_json: Value,
        policy: QueuePolicy,
    },
}
struct SetupEffects {
    plan: Option<Plan>,
    key: CommandKey,
    user: String,
    company: erpx_server_shared::public::CompanyId,
    now: i64,
    output: Value,
    scope: Option<CommandEventScope>,
    policy: Option<CurrentPolicyArea>,
    record: Option<RetainedFieldException>,
    notice: Option<RetainedOperationsNotification>,
}
impl PgOwnerCommandEffects for SetupEffects {
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
        if self.plan.is_none() {
            return Err(changed());
        }
        Ok(WorkflowOutput {
            version: self.output["version"].as_i64().ok_or_else(changed)?,
            result: CommandJson::from_serializable(&self.output)?,
            event_scope: self.scope.clone(),
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        match self.plan.as_ref().ok_or_else(changed)? {
            Plan::Draft { before, input } => {
                self.policy = Some(
                    draft_policy(
                        c,
                        before,
                        input,
                        self.output["id"].as_str().ok_or_else(changed)?,
                        &self.user,
                        self.now,
                    )
                    .await?,
                )
            }
            Plan::Review {
                before,
                id,
                action,
                version,
                same_creator,
                reason,
            } => {
                self.policy = Some(
                    review_policy_area(
                        c,
                        before,
                        id,
                        *action,
                        *version,
                        *same_creator,
                        reason,
                        &self.user,
                        self.now,
                    )
                    .await?,
                )
            }
            Plan::Open {
                input,
                source,
                source_json,
                policy,
            } => {
                self.record = Some(
                    open_field_exception(
                        c,
                        &self.company,
                        &self.key,
                        &self.user,
                        input,
                        source,
                        source_json,
                        policy,
                        self.now,
                    )
                    .await?,
                );
                self.notice = Some(
                    stage_operations_notification(
                        c,
                        &self.key,
                        &self.company,
                        &input.owner_id,
                        self.output["id"].as_str().ok_or_else(changed)?,
                        1,
                        self.now,
                    )
                    .await?,
                );
            }
        }
        Ok(())
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        if let Some(v) = &self.policy {
            v.require_current(c).await?;
        }
        if let Some(v) = &self.record {
            v.require_current(c).await?;
        }
        if let Some(v) = &self.notice {
            v.require_current(c).await?;
        }
        Ok(())
    }
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "OPERATIONS_INPUT_INVALID",
        "Provide a valid Operations value.",
    )
}
fn string(v: &Value, k: &str) -> Result<String, BusinessError> {
    v[k].as_str().map(str::to_owned).ok_or_else(invalid)
}
fn integer(v: &Value, k: &str) -> Result<i64, BusinessError> {
    v[k].as_f64()
        .filter(|v| v.fract() == 0.0 && *v >= 1.0 && *v <= 9_007_199_254_740_991.0)
        .map(|v| v as i64)
        .ok_or_else(invalid)
}
enum Input {
    Draft(DraftPolicy),
    Review(PolicyAction, i64, String),
    Open(OpenException),
}
fn decode(command: &FieldOperationsSetup) -> Result<(Input, Value, String, String), BusinessError> {
    let raw = &command.body;
    if !raw.is_object() {
        return Err(invalid());
    }
    let entity = raw["entity"].as_str().unwrap_or("exception");
    if entity == "policy" {
        if let Some(id) = &command.id {
            identifier(id)?;
            let action = match raw["action"].as_str() {
                Some("publish") => PolicyAction::Publish,
                Some("reject") => PolicyAction::Reject,
                Some("retire") => PolicyAction::Retire,
                _ => return Err(invalid()),
            };
            let version = integer(raw, "expectedVersion")?;
            let reason = string(raw, "reason")?;
            text(&reason, 3, 4000)?;
            let reason = trimmed(&reason).to_owned();
            let normalized =
                json!({"action":action.as_str(),"expectedVersion":version,"reason":reason});
            return Ok((
                Input::Review(action, version, reason),
                normalized,
                format!("operations.policy.{}", action.as_str()),
                id.clone(),
            ));
        }
        let input = DraftPolicy {
            area: trimmed(&string(raw, "area")?).into(),
            name: trimmed(&string(raw, "name")?).into(),
            target_minutes: integer(raw, "targetMinutes")?,
            source_kinds: raw["sourceKinds"]
                .as_array()
                .ok_or_else(invalid)?
                .iter()
                .map(|v| v.as_str().map(str::to_owned).ok_or_else(invalid))
                .collect::<Result<_, _>>()?,
        };
        input.validate()?;
        let normalized = json!({"area":input.area,"name":input.name,"targetMinutes":input.target_minutes,"sourceKinds":input.source_kinds});
        return Ok((
            Input::Draft(input),
            normalized,
            "operations.policy.draft".into(),
            format!("queue-policy-{}", command.key),
        ));
    }
    if entity != "exception"
        || command.id.is_some()
        || raw["source"]["kind"] != "field_service_order"
    {
        return Err(invalid());
    }
    let input = OpenException {
        source_id: string(&raw["source"], "id")?,
        source_version: integer(&raw["source"], "version")?,
        policy_id: string(raw, "policyId")?,
        policy_version: integer(raw, "policyVersion")?,
        title: trimmed(&string(raw, "title")?).into(),
        issue: trimmed(&string(raw, "issue")?).into(),
        priority: raw
            .get("priority")
            .map(|v| v.as_str().map(str::to_owned).ok_or_else(invalid))
            .transpose()?
            .unwrap_or("normal".into()),
        owner_id: string(raw, "ownerId")?,
    };
    input.validate()?;
    let normalized = json!({"source":{"kind":"field_service_order","id":input.source_id,"version":input.source_version},"policyId":input.policy_id,"policyVersion":input.policy_version,"title":input.title,"issue":input.issue,"priority":input.priority,"ownerId":input.owner_id});
    Ok((
        Input::Open(input),
        normalized,
        "operations.exception.open".into(),
        format!("operations-exception-{}", command.key),
    ))
}
fn policy_authority(current: &AuthorizedWorkContext) -> Result<(), Error> {
    current.require_permission("records.review")?;
    if current.selection().assignment().scope_kind != WorkScopeKind::Holding {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CONFLICT",
            "Use Company-wide policy review authority.",
        )
        .into());
    }
    Ok(())
}
async fn prepare_source_ref<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    target: &FieldCollaborationTarget<C>,
    id: &str,
    version: Option<i64>,
) -> Result<(SourceRecord, PreparedRecordsSource), Error> {
    let mut port = FieldRecordsSourcePort {
        connection: c,
        context: SourceRecordContext {
            actor: current,
            holding: &target.holding,
            company: &target.company,
            catalogue: &target.catalogue,
            clock: target.clock.as_ref(),
        },
    };
    Ok(prepare_source(
        &mut port,
        target.company.as_str(),
        &SourceReference {
            kind: SourceKind::FieldServiceOrder,
            id: id.into(),
            expected_version: version,
        },
    )
    .await?
    .into_parts())
}
fn component(value: &str) -> String {
    value
        .bytes()
        .map(|b| {
            if b.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&b) {
                (b as char).to_string()
            } else {
                format!("%{b:02X}")
            }
        })
        .collect()
}
pub(super) fn source_json(s: &SourceRecord) -> Value {
    let id = component(&s.id);
    json!({"kind":"field_service_order","id":s.id,"version":s.version,"companyId":s.company_id,"branchId":s.branch_id,"departmentId":s.department_id,"title":s.title,"label":format!("Field service order · {}",s.title),"status":s.status,"href":format!("/field-service/{id}")})
}
pub(super) async fn execute_setup<C: Clock + Send + Sync>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    target: &FieldCollaborationTarget<C>,
    command: FieldOperationsSetup,
) -> Result<Value, Error> {
    require_access(current)?;
    if current.holding_id() != &target.holding
        || current.selection().workspace().company_id != target.company
    {
        return Err(changed());
    }
    // Policy permission precedes entity validation, matching the owner command.
    if command.body["entity"] == "policy" {
        current.require_permission("records.review")?;
    }
    let key = CommandKey::new(&command.key)?;
    let (input, normalized, operation, id) = decode(&command)?;
    if matches!(input, Input::Draft(_) | Input::Review(..)) {
        policy_authority(current)?;
    }
    let request = WorkflowRequest {
        key: key.as_str().into(),
        operation,
        record_id: id.clone(),
        input: CommandJson::from_serializable(&normalized)?,
    };
    let mut effects = SetupEffects {
        plan: None,
        key,
        user: current.user_id().as_str().into(),
        company: target.company.clone(),
        now: target.clock.now().get(),
        output: Value::Null,
        scope: None,
        policy: None,
        record: None,
        notice: None,
    };
    let mut ids = Ids;
    let mut authority = Authority {
        current,
        clock: target.clock.as_ref(),
        expected: &request,
    };
    let prior = PgCommandLedger::new(
        target.company.clone(),
        c,
        &mut authority,
        target.clock.as_ref(),
        &mut ids,
    )
    .replay(&request, &mut effects)
    .await?;
    if let Some(prior) = prior {
        let value: Value = serde_json::from_str(prior.result.as_str()).map_err(|_| changed())?;
        if value["id"] != id {
            return Err(changed());
        }
        match &input {
            Input::Open(_) => {
                let w = current.selection().workspace();
                let row = read_field_exception(
                    c,
                    &target.company,
                    &id,
                    w.branch_id.as_ref().map(|v| v.as_str()),
                    w.department_id.as_ref().map(|v| v.as_str()),
                )
                .await?
                .ok_or_else(changed)?;
                let (_, source) =
                    prepare_source_ref(c, current, target, &row.data().source_id, None).await?;
                row.require_current(c).await?;
                source
                    .require_current(
                        c,
                        &SourceRecordContext {
                            actor: current,
                            holding: &target.holding,
                            company: &target.company,
                            catalogue: &target.catalogue,
                            clock: target.clock.as_ref(),
                        },
                    )
                    .await?;
                if value["version"] != 1 || value["status"] != "open" {
                    return Err(changed());
                }
            }
            Input::Draft(_) => {
                if value["version"] != 1 || value["status"] != "draft" {
                    return Err(changed());
                }
                read_policy(c, &target.company, &id)
                    .await?
                    .ok_or_else(changed)?
                    .require_current(c)
                    .await?;
            }
            Input::Review(action, version, _) => {
                if value["version"] != version + 1 || value["status"] != action.status() {
                    return Err(changed());
                }
                read_policy(c, &target.company, &id)
                    .await?
                    .ok_or_else(changed)?
                    .require_current(c)
                    .await?;
            }
        }
        require_selected_actor_current(c, current, target.clock.now().get()).await?;
        return Ok(value);
    }
    let mut source_witness = None;
    let mut recipient = None;
    let mut policy_witness = None;
    let mut reviewer_witness = None;
    effects.plan = Some(match input {
        Input::Draft(input) => {
            let before = read_policy_area(c, &target.company, &input.area).await?;
            effects.output = json!({"id":id,"status":"draft","version":1});
            Plan::Draft { before, input }
        }
        Input::Review(action, version, reason) => {
            let before = read_policy(c, &target.company, &id).await?.ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "OPERATIONS_POLICY_UNAVAILABLE",
                    "Choose a current queue policy.",
                )
            })?;
            let row = before.policy(&id).ok_or_else(changed)?;
            let iam = read_service_workspace_authority(
                c,
                ServiceWorkspaceAuthorityRequest {
                    company: target.company.clone(),
                    actor: current.user_id().as_str().into(),
                    assignments: vec![],
                    same_people: vec![row.created_by.clone()],
                },
                target.clock.now(),
            )
            .await?;
            let same_creator = iam
                .data()
                .same_people
                .iter()
                .any(|v| v.user_id == row.created_by && v.same_person);
            let next = review_policy(row, action, version, same_creator, &reason)?;
            reviewer_witness = Some(iam);
            effects.output = json!({"id":id,"status":action.status(),"version":next});
            Plan::Review {
                before,
                id: id.clone(),
                action,
                version,
                same_creator,
                reason,
            }
        }
        Input::Open(input) => {
            let (source, witness) = prepare_source_ref(
                c,
                current,
                target,
                &input.source_id,
                Some(input.source_version),
            )
            .await?;
            let area = read_policy(c, &target.company, &input.policy_id)
                .await?
                .ok_or_else(|| {
                    BusinessError::new(
                        FailureKind::Conflict,
                        "OPERATIONS_POLICY_UNAVAILABLE",
                        "Choose a published policy that permits this source kind.",
                    )
                })?;
            let policy = area.policy(&input.policy_id).ok_or_else(changed)?.clone();
            input.due_at(&policy, target.clock.now().get())?;
            // Recipient uses the stored source scope and Operations' dated workforce rule.
            recipient = Some(
                prepare_operations_recipient(
                    c,
                    current,
                    target,
                    source.branch_id.as_deref(),
                    source.department_id.as_deref(),
                    &input.owner_id,
                    false,
                )
                .await?,
            );
            let field = |v: Option<&str>| {
                v.map(|v| CommandField::Value(v.into()))
                    .unwrap_or(CommandField::Null)
            };
            effects.scope = Some(CommandEventScope {
                company_id: target.company.clone(),
                branch_id: field(source.branch_id.as_deref()),
                department_id: field(source.department_id.as_deref()),
            });
            effects.output = json!({"id":id,"status":"open","version":1});
            let source_json = source_json(&source);
            source_witness = Some(witness);
            policy_witness = Some(area);
            Plan::Open {
                input,
                source,
                source_json,
                policy,
            }
        }
    });
    let expected = CommandJson::from_serializable(&effects.output)?;
    let outcome = PgCommandLedger::new(
        target.company.clone(),
        c,
        &mut authority,
        target.clock.as_ref(),
        &mut ids,
    )
    .execute(&request, &mut effects)
    .await?;
    if outcome.result != expected {
        return Err(changed());
    }
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *c)
        .await?;
    effects.retain(c, &ledger_actor(current)?, &request).await?;
    if let Some(v) = source_witness {
        v.require_current(
            c,
            &SourceRecordContext {
                actor: current,
                holding: &target.holding,
                company: &target.company,
                catalogue: &target.catalogue,
                clock: target.clock.as_ref(),
            },
        )
        .await?;
    }
    if let Some(v) = recipient {
        v.iam.require_current(c, target.clock.now()).await?;
        v.people.require_current(c, target.clock.now()).await?;
    }
    if let Some(v) = policy_witness {
        v.require_current(c).await?;
    }
    if let Some(v) = reviewer_witness {
        v.require_current(c, target.clock.now()).await?;
    }
    require_selected_actor_current(c, current, target.clock.now().get()).await?;
    Ok(effects.output)
}
pub async fn execute_selected_field_operations_setup<C, InputFn>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    workspace: InputFn,
    target: FieldCollaborationTarget<C>,
    command: FieldOperationsSetup,
) -> Result<Value, Error>
where
    C: Clock + Send + Sync + 'static,
    InputFn: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
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
                Box::pin(async move { execute_setup(c, current, &target, command).await })
            },
        )
        .await
}

pub async fn read_selected_field_operations_workspace<C, F>(
    authorizer: &HoldingWorkAuthorizer,
    host: &str,
    proof: DeviceProof,
    workspace: F,
    target: FieldCollaborationTarget<C>,
) -> Result<Value, Error>
where
    C: Clock + Send + Sync + 'static,
    F: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
{
    let company = target.company.clone();
    authorizer.execute_with_sources_deferred(host,proof,move||{let mut w=workspace()?;w.mutation=false;if w.company_id!=company{return Err(BusinessError::new(FailureKind::Forbidden,"OPERATIONS_SCOPE_REQUIRED","Use the current Company."));}Ok(w)},WorkAuthorizationRequest{permission:"records.read".into(),module:Some("operations".into()),destination:"operations".into(),api:true},Sources,move|c,current|{let target=target.clone();Box::pin(async move{
 if current.holding_id()!=&target.holding||current.selection().workspace().company_id!=target.company{return Err(changed());}
 let w=current.selection().workspace();let rows=read_field_operations_workspace(c,&target.company,w.branch_id.as_ref().map(|v|v.as_str()),w.department_id.as_ref().map(|v|v.as_str())).await?;
 let mut exceptions=vec![];let mut guards=vec![];
 for row in rows.data()["exceptions"].as_array().ok_or_else(changed)?{
  let id=row["source_id"].as_str().ok_or_else(changed)?;
  match prepare_source_ref(c,current,&target,id,None).await{
   Ok((source,witness))=>{let mut item=row.clone();item["source"]=source_json(&source);exceptions.push(item);guards.push(witness);},
   Err(PgTransactionError::Business(e))if matches!(e.kind,FailureKind::Forbidden|FailureKind::NotFound)=>{},Err(e)=>return Err(e),
  }
 }
 let people=erpx_server_people_payroll::public::field_service_workspace::read_service_workspace_people(c,&target.company,vec![],true).await?;
 let members=erpx_server_identity_access::public::collaboration_candidates::read_operations_candidates(c,&target.company,people.data().iter().filter(|p|p.eligible).map(|p|p.user_id.clone()).collect(),target.clock.now()).await?;
 let picker:Vec<Value>=people.data().iter().filter(|p|p.eligible&&members.users().contains(&p.user_id)).map(|p|json!({"id":p.user_id,"name":p.name})).collect();
 let typed=exceptions.iter().cloned().map(serde_json::from_value::<Exception>).collect::<Result<Vec<_>,_>>().map_err(|_|changed())?;
 let same_people=read_service_workspace_authority(c,ServiceWorkspaceAuthorityRequest{company:target.company.clone(),actor:current.user_id().as_str().into(),assignments:vec![],same_people:typed.iter().flat_map(|r|std::iter::once(r.owner_id.clone()).chain(r.resolved_by.clone())).collect()},target.clock.now()).await?;
 let same=|user:&str|same_people.data().same_people.iter().any(|v|v.user_id==user&&v.same_person);
 let work:Vec<Value>=typed.iter().zip(&exceptions).map(|(row,source)|{
  let actor=Actor{user:current.user_id().as_str().into(),can_review:current.has_permission("records.review"),same_person_owner:same(&row.owner_id),same_person_resolver:row.resolved_by.as_deref().is_some_and(same)};
  let mut item=serde_json::to_value(erpx_server_operations::public::field_exceptions::exception_work(row,&actor,current.has_permission("records.write"))).map_err(|_|changed())?;
  item["href"]=json!(format!("/field-service/operations/{}",component(&row.id)));
  item["source"]=source["source"].clone();Ok(item)
 }).collect::<Result<_,Error>>()?;
 let summary=summarize(&typed,target.clock.now().get());
 let data=json!({"exceptions":exceptions,"policies":rows.data()["policies"],"people":picker,"actorId":current.user_id().as_str(),"canWrite":current.has_permission("records.write"),"canReview":current.has_permission("records.review"),"summary":summary,"work":work});
 if serde_json::to_vec(&data).map_err(|_|changed())?.len()>2*1024*1024-8192{return Err(BusinessError::new(FailureKind::Unavailable,"OPERATIONS_READ_LIMIT","The Operations workspace exceeds this bounded read.").into());}
 same_people.require_current(c,target.clock.now()).await?;rows.require_current(c).await?;people.require_current(c).await?;members.require_current(c,target.clock.now()).await?;for g in guards{g.require_current(c,&SourceRecordContext{actor:current,holding:&target.holding,company:&target.company,catalogue:&target.catalogue,clock:target.clock.as_ref()}).await?;}require_selected_actor_current(c,current,target.clock.now().get()).await?;
 Ok(json!({"data":data,"context":crate::field_service_reservation_read::context(current)?}))
 })}).await
}
