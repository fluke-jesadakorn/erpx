//! Support incident command composition. Support owns the incident, link and
//! event rows; Identity supplies current authority, Workflow supplies the
//! command receipt/audit/outbox and Commercial supplies the purchase witness.
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue,
};
use erpx_server_identity_access::public::{
    DeviceProof,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, WorkAuthorizationRequest,
        WorkPermissionRequirement,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
    TimestampMillis,
};
use erpx_server_support::public::incident::{
    IncidentActor, IncidentChange, IncidentChangePlan, IncidentCreate, IncidentCreatePlan,
    IncidentPermissions, IncidentSeverity, IncidentSnapshot, IncidentStatus, plan_change,
    plan_create,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandField, CommandJson, PgCommandAuthority,
    PgCommandLedger, PgOwnerCommandEffects, WorkAuthority, WorkflowOutput, WorkflowRequest,
};
use serde::Serialize;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const OWNER_ELIGIBLE: &str = "SELECT EXISTS(SELECT 1 FROM support_incidents t WHERE t.id=$1 AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=t.owner_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))))";

const REVIEWER: &str = "SELECT EXISTS(SELECT 1 FROM support_incidents t JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m ON m.company_id=t.company_id WHERE t.id=$1 AND m.user_id=$2 AND m.user_id<>t.owner_id AND m.user_id IS DISTINCT FROM t.mitigated_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)))";

const INCIDENT_UPDATED_NOTIFICATION: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT $1,company_id,owner_id,'support_incident',id,version,'Shared Support incident updated: review current status and next action',updated_at FROM support_incidents WHERE id=$2 AND owner_id IS NOT NULL ON CONFLICT DO NOTHING";

#[derive(Clone, Debug)]
pub enum IncidentWrite {
    Create(IncidentCreate),
    Change(IncidentChange),
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncidentResult {
    pub id: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone)]
pub struct SupportIncidentCommand<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub catalogue: Arc<CommercialCatalogue>,
    pub clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> SupportIncidentCommand<C> {
    pub async fn create<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        input: IncidentCreate,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        self.run(host, proof, workspace, None, Some(input), None, wire, key)
            .await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn change<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        input: IncidentChange,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        self.run(host, proof, workspace, Some(id), None, Some(input), wire, key)
            .await
    }
    #[allow(clippy::too_many_arguments)]
    async fn run<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        change_id: Option<String>,
        create: Option<IncidentCreate>,
        change_input: Option<IncidentChange>,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let expected_company = self.company.clone();
        let company = self.company.clone();
        let clock = self.clock.clone();
        let expected = key.clone();
        let change = change_id.is_some();
        let permission = match (&create, &change_id) {
            (Some(_), _) => WorkPermissionRequirement::Exact("support.manage".into()),
            (None, Some(_)) => WorkPermissionRequirement::AnyOf(vec![
                "support.manage".into(),
                "support.work".into(),
            ]),
            _ => WorkPermissionRequirement::AnyOf(vec![
                "support.manage".into(),
                "support.work".into(),
            ]),
        };
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                workspace()?,
                WorkAuthorizationRequest {
                    permission,
                    module: Some("support".into()),
                    destination: if change {
                        "support.incident.change".into()
                    } else {
                        "support.incident.create".into()
                    },
                    api: true,
                },
                SupportIncidentSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let expected = expected.clone();
                    let create = create.clone();
                    let change_id = change_id.clone();
                    let change_input = change_input.clone();
                    let wire = wire.clone();
                    Box::pin(async move {
                        if current.selection().workspace().company_id != expected_company {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "SUPPORT_INCIDENT_SCOPE_REQUIRED",
                                "Use the current Company.",
                            )
                            .into());
                        }
                        let clock_ref = clock.as_ref();
                        let now = clock_ref.now();
                        let actor = incident_actor(current, now)?;
                        let (operation, record_id) = match (&create, &change_id) {
                            (Some(_), _) => (
                                "support.incident.create".to_owned(),
                                format!("incident-{}", expected.as_str()),
                            ),
                            (None, Some(id)) => (
                                format!(
                                    "support.incident.{}",
                                    change_input.as_ref().map(|input| input.action()).unwrap_or("change")
                                ),
                                id.clone(),
                            ),
                            _ => return Err(changed().into()),
                        };
                        let request = WorkflowRequest {
                            key: expected.as_str().to_owned(),
                            operation,
                            record_id,
                            input: wire.clone(),
                        };
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock_ref,
                            purchases: None,
                        };
                        let mut effects = Effects {
                            company: &company,
                            clock: clock_ref,
                            request: &request,
                            create: create.as_ref(),
                            change: change_input.as_ref(),
                            actor,
                            prepared: None,
                        };
                        let mut ids = Ids;
                        let outcome = PgCommandLedger::new(
                            company.clone(),
                            c,
                            &mut authority,
                            clock_ref,
                            &mut ids,
                        )
                        .execute(&request, &mut effects)
                        .await?;
                        Ok(outcome.result)
                    })
                },
            )
            .await
    }
}



fn incident_actor(
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
) -> Result<IncidentActor, BusinessError> {
    if now.get() >= current.expires_at() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_INCIDENT_AUTHORITY_CHANGED",
            "Your current Support authority is unavailable.",
        ));
    }
    let workspace = current.selection().workspace();
    Ok(IncidentActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|v| v.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|v| v.as_str().to_owned()),
        permissions: IncidentPermissions {
            can_read: current.has_permission("support.read")
                || current.has_permission("records.read"),
            can_work: current.has_permission("support.work"),
            can_manage: current.has_permission("support.manage"),
        },
        same_person_ids: Vec::new(),
    })
}

#[derive(Clone)]
struct SupportIncidentSources;
impl erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation
    for SupportIncidentSources
{
    fn prepare<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> erpx_server_runtime::public::postgres::PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            sqlx::query("LOCK TABLE support_incidents,support_incident_tickets,support_incident_events,support_tickets,staff_notifications IN SHARE MODE")
                .execute(&mut *c)
                .await?;
            Ok(())
        })
    }
}

struct Authority<'a, C> {
    current: &'a AuthorizedWorkContext,
    request: &'a WorkflowRequest,
    clock: &'a C,
    purchases: Option<Purchases>,
}
impl<C: Clock + Sync> PgCommandAuthority for Authority<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation::prepare(
            &SupportIncidentSources,
            c,
        )
        .await
    }
    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        r: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        if r.key != self.request.key
            || r.operation != self.request.operation
            || r.record_id != self.request.record_id
            || r.input.as_str() != self.request.input.as_str()
        {
            return Err(changed().into());
        }
        Ok(actor(self.current, self.clock.now())?)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        a: &CommandActor,
        _: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        if &actor(self.current, self.clock.now())? != a {
            return Err(changed().into());
        }
        match &self.purchases {
            Some(p) => p.check(c, self.current, self.clock.now()).await?,
            None => {
                self.purchases = Some(
                    Purchases::read(c, self.current, disposition, self.clock.now()).await?,
                )
            }
        }
        Ok(())
    }
}

struct Purchases {
    catalogue: CommercialCatalogue,
    history: PurchaseWitness,
    new_work: Option<PurchaseWitness>,
}
impl Purchases {
    async fn read(
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        disposition: CommandDisposition,
        now: TimestampMillis,
    ) -> Result<Self, Error> {
        let catalogue = current_package_catalogue()?;
        let mut reader = PgCompanyPurchaseReader::new(
            current.holding_id().clone(),
            current.selection().workspace().company_id.clone(),
            c,
            &catalogue,
        );
        let inspect = |purpose| PurchaseRequest {
            capabilities: vec!["support.incidents".into()],
            purpose,
            mode: PackageEnforcementMode::Enforced,
        };
        let history = reader
            .inspect(&inspect(CapabilityPurpose::History), now)
            .await?
            .require_allowed()?
            .clone();
        let new_work = if disposition == CommandDisposition::Execute {
            Some(
                reader
                    .inspect(&inspect(CapabilityPurpose::NewWork), now)
                    .await?
                    .require_allowed()?
                    .clone(),
            )
        } else {
            None
        };
        Ok(Self {
            catalogue,
            history,
            new_work,
        })
    }
    async fn check(
        &self,
        c: &mut PgConnection,
        current: &AuthorizedWorkContext,
        now: TimestampMillis,
    ) -> Result<(), Error> {
        let mut reader = PgCompanyPurchaseReader::new(
            current.holding_id().clone(),
            current.selection().workspace().company_id.clone(),
            c,
            &self.catalogue,
        );
        reader.revalidate(&self.history, now).await?.require_allowed()?;
        if let Some(witness) = &self.new_work {
            reader.revalidate(witness, now).await?.require_allowed()?;
        }
        Ok(())
    }
}

struct Effects<'a, C> {
    company: &'a CompanyId,
    clock: &'a C,
    request: &'a WorkflowRequest,
    create: Option<&'a IncidentCreate>,
    change: Option<&'a IncidentChange>,
    actor: IncidentActor,
    prepared: Option<Prepared>,
}

enum Prepared {
    Create {
        plan: IncidentCreatePlan,
        source: IncidentTicketFacts,
    },
    Change {
        snapshot: IncidentSnapshot,
        plan: IncidentChangePlan,
    },
}

#[derive(Clone, Debug)]
struct IncidentTicketFacts {
    id: String,
    branch_id: Option<String>,
    department_id: Option<String>,
    source_kind: String,
    version: i64,
}

impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation::prepare(
            &SupportIncidentSources,
            c,
        )
        .await
    }
    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        if let Some(input) = self.create {
            let source = load_ticket(c, self.company, &input.ticket_id).await?;
            if source.source_kind != "sales_order" {
                return Err(BusinessError::invalid(
                    "SUPPORT_INCIDENT_SOURCE_REQUIRED",
                    "Shared customer incidents require a linked customer order.",
                )
                .into());
            }
            if source.version != input.ticket_version {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SUPPORT_INCIDENT_SOURCE_CHANGED",
                    "The linked customer case changed. Reload before declaring the incident.",
                )
                .into());
            }
            let mut plan = match plan_create(
                &self.actor,
                self.request.key.as_str(),
                input,
                self.clock.now().get(),
            )? {
                erpx_server_support::public::incident::IncidentPlan::Create(plan) => plan,
                _ => return Err(changed().into()),
            };
            plan.branch_id = source.branch_id.clone();
            plan.department_id = source.department_id.clone();
            self.prepared = Some(Prepared::Create { plan, source });
            return Ok(WorkflowOutput {
                version: 1,
                result: CommandJson::from_serializable(&IncidentResult {
                    id: format!("incident-{}", self.request.key),
                    status: "open".into(),
                    version: 1,
                })?,
                event_scope: None,
            });
        }
        let input = self.change.ok_or_else(changed)?;
        let snapshot = load_incident(c, self.company, &self.request.record_id).await?;
        let plan = match plan_change(&self.actor, &snapshot, input, self.clock.now().get())? {
            erpx_server_support::public::incident::IncidentPlan::Change(plan) => plan,
            _ => return Err(changed().into()),
        };
        let result = IncidentResult {
            id: plan.id.clone(),
            status: plan.status.as_str().to_owned(),
            version: plan.version,
        };
        self.prepared = Some(Prepared::Change { snapshot, plan });
        Ok(WorkflowOutput {
            version: result.version,
            result: CommandJson::from_serializable(&result)?,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        match self.prepared.take().ok_or_else(changed)? {
            Prepared::Create { plan, source } => {
                apply_create(c, self.request.key.as_str(), &plan, &source).await?;
            }
            Prepared::Change { snapshot, plan } => {
                apply_change(
                    c,
                    self.request.key.as_str(),
                    &snapshot,
                    &plan,
                    self.actor.user_id.as_str(),
                )
                .await?;
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
        let Some(prepared) = &self.prepared else {
            return Ok(());
        };
        match prepared {
            Prepared::Create { plan, source } => {
                let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_incidents t JOIN support_incident_events e ON e.incident_id=t.id WHERE t.id=$1 AND t.company_id=$2 AND t.version=1 AND e.action='create' AND EXISTS(SELECT 1 FROM support_incident_tickets l WHERE l.incident_id=t.id AND l.ticket_id=$3 AND l.active))")
                    .bind(&plan.id)
                    .bind(self.company.as_str())
                    .bind(&source.id)
                    .fetch_one(&mut *c)
                    .await?;
                if !exists {
                    return Err(changed().into());
                }
            }
            Prepared::Change { plan, .. } => {
                let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_incidents t JOIN support_incident_events e ON e.incident_id=t.id AND e.version=t.version WHERE t.id=$1 AND t.company_id=$2 AND t.version=$3 AND e.action=$4)")
                    .bind(&plan.id)
                    .bind(self.company.as_str())
                    .bind(plan.version)
                    .bind(plan.action)
                    .fetch_one(&mut *c)
                    .await?;
                if !exists {
                    return Err(changed().into());
                }
            }
        }
        Ok(())
    }
}

type TicketRow = (String, String, Option<String>, Option<String>, String, i64);
type IncidentRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    String,
    String,
    Option<String>,
    String,
    i64,
);

async fn load_ticket(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<IncidentTicketFacts, Error> {
    let row:Option<TicketRow>=sqlx::query_as(
        "SELECT id,company_id,branch_id,department_id,source_kind,version FROM support_tickets WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some((id, _company_id, branch_id, department_id, source_kind, version)) = row else {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_TICKET_NOT_FOUND",
            "Ticket not found in this scope.",
        )
        .into());
    };
    Ok(IncidentTicketFacts {
        id,
        branch_id,
        department_id,
        source_kind,
        version,
    })
}

async fn load_incident(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<IncidentSnapshot, Error> {
    let row:Option<IncidentRow>=sqlx::query_as(
        "SELECT id,company_id,branch_id,department_id,status,severity,owner_id,mitigated_by,created_by,version FROM support_incidents WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some((id, company_id, branch_id, department_id, status, severity, owner_id, mitigated_by, created_by, version)) = row else {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_INCIDENT_NOT_FOUND",
            "Incident not found in this scope.",
        )
        .into());
    };
    let status = match status.as_str() {
        "open" => IncidentStatus::Open,
        "investigating" => IncidentStatus::Investigating,
        "mitigated" => IncidentStatus::Mitigated,
        "closed" => IncidentStatus::Closed,
        "cancelled" => IncidentStatus::Cancelled,
        _ => return Err(changed().into()),
    };
    let severity = match severity.as_str() {
        "critical" => IncidentSeverity::Critical,
        "high" => IncidentSeverity::High,
        "normal" => IncidentSeverity::Normal,
        _ => return Err(changed().into()),
    };
    Ok(IncidentSnapshot {
        id,
        company_id,
        branch_id,
        department_id,
        status,
        severity,
        owner_id,
        mitigated_by,
        created_by,
        version,
    })
}

async fn apply_create(
    c: &mut PgConnection,
    key: &str,
    plan: &IncidentCreatePlan,
    source: &IncidentTicketFacts,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO support_incidents(id,company_id,branch_id,department_id,title,impact,severity,owner_id,due_date,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
        .bind(&plan.id)
        .bind(&plan.company_id)
        .bind(
            plan.branch_id
                .as_deref()
                .or(source.branch_id.as_deref())
                .ok_or_else(changed)?,
        )
        .bind(plan.department_id.as_deref())
        .bind(&plan.title)
        .bind(&plan.impact)
        .bind(plan.severity.as_str())
        .bind(&plan.owner_id)
        .bind(&plan.due_date)
        .bind(&plan.created_by)
        .bind(plan.created_at)
        .bind(plan.updated_at)
        .execute(&mut *c)
        .await?;
    let eligible:bool=sqlx::query_scalar(OWNER_ELIGIBLE)
        .bind(&plan.id)
        .fetch_one(&mut *c)
        .await?;
    if !eligible {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_INCIDENT_OWNER_INELIGIBLE",
            "Choose a currently eligible incident owner.",
        )
        .into());
    }
    sqlx::query("INSERT INTO support_incident_tickets(incident_id,ticket_id,linked_version,linked_by,linked_at) VALUES($1,$2,$3,$4,$5)")
        .bind(&plan.id)
        .bind(&source.id)
        .bind(source.version)
        .bind(&plan.created_by)
        .bind(plan.created_at)
        .execute(&mut *c)
        .await?;
    sqlx::query("INSERT INTO support_incident_events(id,incident_id,version,action,actor_id,reason,created_at,evidence_json) VALUES($1,$2,1,'create',$3,$4,$5,$6::jsonb)")
        .bind(format!("incident-event-{key}"))
        .bind(&plan.id)
        .bind(&plan.created_by)
        .bind(&plan.impact)
        .bind(plan.created_at)
        .bind(serde_json::to_string(&serde_json::json!({"action":"create","ticketId":source.id,"severity":plan.severity.as_str()})).map_err(|_| changed())?)
        .execute(&mut *c)
        .await?;
    sqlx::query(INCIDENT_UPDATED_NOTIFICATION)
        .bind(format!("incident-notice-{key}"))
        .bind(&plan.id)
        .execute(&mut *c)
        .await?;
    Ok(())
}

async fn apply_change(
    c: &mut PgConnection,
    key: &str,
    snapshot: &IncidentSnapshot,
    plan: &IncidentChangePlan,
    actor_id: &str,
) -> Result<(), Error> {
    if plan.action == "close" {
        let reviewer:bool=sqlx::query_scalar(REVIEWER)
            .bind(&plan.id)
            .bind(actor_id)
            .fetch_one(&mut *c)
            .await?;
        if !reviewer {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "INDEPENDENT_APPROVER_REQUIRED",
                "Another manager must review the incident mitigation.",
            )
            .into());
        }
    }
    let updated = match plan.action {
        "mitigate" => sqlx::query("UPDATE support_incidents SET status='mitigated',root_cause=$1,mitigation=$2,mitigated_by=$3,version=version+1,updated_at=$4 WHERE id=$5 AND company_id=$6 AND version=$7")
            .bind(plan.root_cause.as_deref())
            .bind(plan.mitigation.as_deref())
            .bind(plan.mitigated_by.as_deref())
            .bind(plan.event_at)
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(snapshot.version)
            .execute(&mut *c)
            .await?
            .rows_affected(),
        "reject" | "reopen" => sqlx::query("UPDATE support_incidents SET status='investigating',mitigated_by=NULL,version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4")
            .bind(plan.event_at)
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(snapshot.version)
            .execute(&mut *c)
            .await?
            .rows_affected(),
        "close" => sqlx::query("UPDATE support_incidents SET status='closed',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4")
            .bind(plan.event_at)
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(snapshot.version)
            .execute(&mut *c)
            .await?
            .rows_affected(),
        "cancel" => sqlx::query("UPDATE support_incidents SET status='cancelled',version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4")
            .bind(plan.event_at)
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(snapshot.version)
            .execute(&mut *c)
            .await?
            .rows_affected(),
        "reassign" => sqlx::query("UPDATE support_incidents SET owner_id=$1,due_date=$2,version=version+1,updated_at=$3 WHERE id=$4 AND company_id=$5 AND version=$6")
            .bind(&plan.owner_id)
            .bind(plan.due_date.as_deref())
            .bind(plan.event_at)
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(snapshot.version)
            .execute(&mut *c)
            .await?
            .rows_affected(),
        "link" | "unlink" => sqlx::query("UPDATE support_incidents SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4")
            .bind(plan.event_at)
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(snapshot.version)
            .execute(&mut *c)
            .await?
            .rows_affected(),
        _ => sqlx::query("UPDATE support_incidents SET status=$1,version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4 AND version=$5")
            .bind(plan.status.as_str())
            .bind(plan.event_at)
            .bind(&plan.id)
            .bind(&plan.company_id)
            .bind(snapshot.version)
            .execute(&mut *c)
            .await?
            .rows_affected(),
    };
    if updated != 1 {
        return Err(changed().into());
    }
    if plan.action == "link" {
        let ticket_id = plan.ticket_id.as_deref().ok_or_else(changed)?;
        let ticket_version = plan.ticket_version.ok_or_else(changed)?;
        sqlx::query("INSERT INTO support_incident_tickets(incident_id,ticket_id,linked_version,linked_by,linked_at,active) VALUES($1,$2,$3,$4,$5,true) ON CONFLICT(incident_id,ticket_id) DO UPDATE SET linked_version=EXCLUDED.linked_version,linked_by=EXCLUDED.linked_by,linked_at=EXCLUDED.linked_at,active=true")
            .bind(&plan.id)
            .bind(ticket_id)
            .bind(ticket_version)
            .bind(actor_id)
            .bind(plan.event_at)
            .execute(&mut *c)
            .await?;
    }
    if plan.action == "unlink" {
        let ticket_id = plan.ticket_id.as_deref().ok_or_else(changed)?;
        let count:i64=sqlx::query_scalar("SELECT COUNT(*) FROM support_incident_tickets WHERE incident_id=$1 AND active")
            .bind(&plan.id)
            .fetch_one(&mut *c)
            .await?;
        if count <= 1 {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SUPPORT_INCIDENT_LAST_LINK",
                "Keep at least one affected customer case linked to the incident.",
            )
            .into());
        }
        sqlx::query("UPDATE support_incident_tickets SET active=false WHERE incident_id=$1 AND ticket_id=$2 AND active")
            .bind(&plan.id)
            .bind(ticket_id)
            .execute(&mut *c)
            .await?;
    }
    sqlx::query("INSERT INTO support_incident_events(id,incident_id,version,action,actor_id,reason,created_at,evidence_json) VALUES($1,$2,$3,$4,$5,$6,$7,$8::jsonb)")
        .bind(format!("incident-event-{key}"))
        .bind(&plan.id)
        .bind(plan.version)
        .bind(plan.action)
        .bind(actor_id)
        .bind(&plan.reason)
        .bind(plan.event_at)
        .bind(serde_json::to_string(&serde_json::json!({"action":plan.action,"status":plan.status.as_str()})).map_err(|_| changed())?)
        .execute(&mut *c)
        .await?;
    if plan.action != "cancel" {
        sqlx::query(INCIDENT_UPDATED_NOTIFICATION)
            .bind(format!("incident-notice-{key}"))
            .bind(&plan.id)
            .execute(&mut *c)
            .await?;
    }
    Ok(())
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SUPPORT_INCIDENT_CHANGED",
        "The Support incident changed. Reload before retrying.",
    )
}

fn actor(
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
) -> Result<CommandActor, BusinessError> {
    current.require_permission("records.read")?;
    if now.get() >= current.expires_at() {
        return Err(changed());
    }
    let s = current.selection();
    let w = s.workspace();
    let a = s.assignment();
    Ok(CommandActor {
        user_id: current.user_id().clone(),
        assignment_id: a.id.clone(),
        role: a.role.clone(),
        company_id: w.company_id.clone(),
        branch_id: w
            .branch_id
            .as_ref()
            .map(|v| CommandField::Value(v.as_str().into()))
            .unwrap_or(CommandField::Null),
        department_id: w
            .department_id
            .as_ref()
            .map(|v| CommandField::Value(v.as_str().into()))
            .unwrap_or(CommandField::Null),
        authority: CommandField::Value(WorkAuthority {
            session_id: current.session_id().as_str().into(),
            revision: s.revision(),
            assignment_version: a.version,
        }),
        iam: CommandField::Value(CommandJson::from_serializable(
            &erpx_server_identity_access::public::command_metadata::project_command_iam(current)?,
        )?),
        assignment_scope_kind: match a.scope_kind {
            erpx_server_identity_access::public::work_context::WorkScopeKind::Holding => "holding",
            erpx_server_identity_access::public::work_context::WorkScopeKind::Branch => "branch",
            erpx_server_identity_access::public::work_context::WorkScopeKind::Department => {
                "department"
            }
        }
        .into(),
        assignment_scope_node_id: a.scope_node_id.as_ref().map(|v| v.as_str().into()),
    })
}

struct Ids;
impl IdentifierSource for Ids {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}
