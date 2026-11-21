//! Support session command composition. Support owns the session, capability
//! and event rows; Identity supplies current authority, Workflow supplies the
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
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, IdentifierSource,
    RecordId, TimestampMillis,
};
use erpx_server_support::public::session::{
    SessionAction, SessionActor, SessionChangePlan, SessionPermissions, SessionPlan,
    SessionRequest, SessionRequestPlan, SessionSnapshot, SessionStatus, plan_change, plan_request,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandField, CommandJson, PgCommandAuthority,
    PgCommandLedger, PgOwnerCommandEffects, WorkAuthority, WorkflowOutput, WorkflowRequest,
};
use serde::Serialize;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const TARGET_ELIGIBLE: &str = "SELECT EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=$1 AND a.user_id=$2 AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=$3) OR (a.scope_kind='department' AND a.scope_node_id=$4)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))";

const CAPABILITY_ACTIVE: &str = "SELECT EXISTS(SELECT 1 FROM support_capabilities WHERE id=$1 AND session_id=$2 AND version=$3 AND status='active')";

const SESSION_REVIEWERS_NOTIFICATION: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT DISTINCT $1||m.user_id,s.company_id,m.user_id,'support_session',s.id,s.version,'Review a temporary customer support session',s.updated_at FROM support_sessions s JOIN support_tickets t ON t.id=s.ticket_id JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m ON m.company_id=s.company_id AND m.user_id<>s.created_by AND m.user_id<>s.requested_for AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) WHERE s.id=$2 ON CONFLICT DO NOTHING";

const SESSION_UPDATED_NOTIFICATION: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT $1,company_id,requested_for,'support_session',id,version,'Temporary support access changed: review current grants',updated_at FROM support_sessions WHERE id=$2 ON CONFLICT DO NOTHING";

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionResult {
    pub id: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone)]
pub struct SupportSessionCommand<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub catalogue: Arc<CommercialCatalogue>,
    pub clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> SupportSessionCommand<C> {
    pub async fn request<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        input: SessionRequest,
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
        input: SessionAction,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        self.run(
            host,
            proof,
            workspace,
            Some(id),
            None,
            Some(input),
            wire,
            key,
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    async fn run<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        change_id: Option<String>,
        request_input: Option<SessionRequest>,
        action: Option<SessionAction>,
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
        let permission = if change {
            WorkPermissionRequirement::Exact("support.manage".into())
        } else {
            WorkPermissionRequirement::Exact("support.work".into())
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
                        "support.session.change".into()
                    } else {
                        "support.session.request".into()
                    },
                    api: true,
                },
                SupportSessionSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let expected = expected.clone();
                    let request_input = request_input.clone();
                    let action = action.clone();
                    let change_id = change_id.clone();
                    let wire = wire.clone();
                    Box::pin(async move {
                        if current.selection().workspace().company_id != expected_company {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "SUPPORT_SESSION_SCOPE_REQUIRED",
                                "Use the current Company.",
                            )
                            .into());
                        }
                        let clock_ref = clock.as_ref();
                        let now = clock_ref.now();
                        let actor = session_actor(current, now)?;
                        let (operation, record_id) = match (&request_input, &change_id) {
                            (Some(_), _) => (
                                "support.session.request".to_owned(),
                                format!("support-session-{}", expected.as_str()),
                            ),
                            (None, Some(id)) => (
                                format!(
                                    "support.session.{}",
                                    action
                                        .as_ref()
                                        .map(|value| value.action())
                                        .unwrap_or("change")
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
                            request_input: request_input.as_ref(),
                            action: action.as_ref(),
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

fn session_actor(
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
) -> Result<SessionActor, BusinessError> {
    if now.get() >= current.expires_at() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_SESSION_AUTHORITY_CHANGED",
            "Your current Support authority is unavailable.",
        ));
    }
    let workspace = current.selection().workspace();
    Ok(SessionActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|v| v.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|v| v.as_str().to_owned()),
        permissions: SessionPermissions {
            can_read: current.has_permission("support.read")
                || current.has_permission("records.read"),
            can_work: current.has_permission("support.work"),
            can_manage: current.has_permission("support.manage"),
        },
        same_person_ids: Vec::new(),
    })
}

#[derive(Clone)]
struct SupportSessionSources;
impl erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation
    for SupportSessionSources
{
    fn prepare<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> erpx_server_runtime::public::postgres::PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            sqlx::query("LOCK TABLE support_sessions,support_session_events,support_capabilities,support_capability_events,support_tickets,staff_notifications IN SHARE MODE")
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
            &SupportSessionSources,
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
                self.purchases =
                    Some(Purchases::read(c, self.current, disposition, self.clock.now()).await?)
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
            capabilities: vec!["support.tickets".into()],
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
        reader
            .revalidate(&self.history, now)
            .await?
            .require_allowed()?;
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
    request_input: Option<&'a SessionRequest>,
    action: Option<&'a SessionAction>,
    actor: SessionActor,
    prepared: Option<Prepared>,
}

enum Prepared {
    Request(Box<SessionRequestPlan>),
    Change {
        snapshot: Box<SessionSnapshot>,
        plan: Box<SessionChangePlan>,
    },
}

type TicketRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    Option<String>,
    String,
    i64,
);
type SessionRow = (
    String,
    String,
    Option<String>,
    Option<String>,
    String,
    i64,
    String,
    String,
    String,
    Option<String>,
    i64,
    i64,
);

impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation::prepare(
            &SupportSessionSources,
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
        if let Some(input) = self.request_input {
            let ticket = load_ticket(c, self.company, &input.ticket_id).await?;
            if ticket.company_id != self.company.as_str() {
                return Err(not_found().into());
            }
            if ticket.source_kind != "sales_order" {
                return Err(BusinessError::invalid(
                    "SUPPORT_SESSION_SOURCE_REQUIRED",
                    "Temporary customer context requires a linked customer order.",
                )
                .into());
            }
            if ticket.version != input.ticket_version {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SUPPORT_SESSION_SOURCE_CHANGED",
                    "The linked customer case changed. Reload before requesting access.",
                )
                .into());
            }
            if ticket.owner_id.as_deref() != Some(self.actor.user_id.as_str()) {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "SUPPORT_SESSION_TICKET_OWNER_REQUIRED",
                    "Request access only for a case assigned to you.",
                )
                .into());
            }
            if !matches!(
                ticket.status.as_str(),
                "open" | "in_progress" | "waiting_customer"
            ) {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SUPPORT_SESSION_TICKET_CLOSED",
                    "The linked customer case is no longer active.",
                )
                .into());
            }
            if !scope_matches(
                &self.actor,
                ticket.branch_id.as_deref(),
                ticket.department_id.as_deref(),
            ) {
                return Err(not_found().into());
            }
            if !target_eligible(c, &ticket, &input.requested_for).await? {
                return Err(BusinessError::new(
                    FailureKind::Conflict,
                    "SUPPORT_SESSION_TARGET_INELIGIBLE",
                    "Choose a currently eligible support colleague.",
                )
                .into());
            }
            let plan = match plan_request(
                &self.actor,
                self.request.key.as_str(),
                input,
                self.clock.now().get(),
            )? {
                SessionPlan::Request(plan) => plan,
                _ => return Err(changed().into()),
            };
            self.prepared = Some(Prepared::Request(Box::new(plan)));
            return Ok(WorkflowOutput {
                version: 1,
                result: CommandJson::from_serializable(&SessionResult {
                    id: format!("support-session-{}", self.request.key),
                    status: "pending".into(),
                    version: 1,
                })?,
                event_scope: None,
            });
        }
        let action = self.action.ok_or_else(changed)?;
        let snapshot = load_session(c, self.company, &self.request.record_id).await?;
        let plan = match plan_change(&self.actor, &snapshot, action, self.clock.now().get())? {
            SessionPlan::Change(plan) => plan,
            _ => return Err(changed().into()),
        };
        match action {
            SessionAction::Approve { .. }
            | SessionAction::Reject { .. }
            | SessionAction::IssueCapability { .. } => {
                let ticket = load_ticket(c, self.company, &snapshot.ticket_id).await?;
                if !matches!(
                    ticket.status.as_str(),
                    "open" | "in_progress" | "waiting_customer"
                ) {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "SUPPORT_SESSION_TICKET_CLOSED",
                        "The linked customer case is no longer active.",
                    )
                    .into());
                }
                if !target_eligible(c, &ticket, &snapshot.requested_for).await? {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "SUPPORT_SESSION_TARGET_INELIGIBLE",
                        "The requesting colleague is no longer eligible.",
                    )
                    .into());
                }
            }
            SessionAction::RevokeCapability {
                capability_id,
                expected_capability_version,
                ..
            } => {
                let active: bool = sqlx::query_scalar(CAPABILITY_ACTIVE)
                    .bind(capability_id)
                    .bind(&snapshot.id)
                    .bind(expected_capability_version)
                    .fetch_one(&mut *c)
                    .await?;
                if !active {
                    return Err(BusinessError::new(
                        FailureKind::Conflict,
                        "SUPPORT_SESSION_CAPABILITY_CHANGED",
                        "Review the current capability state.",
                    )
                    .into());
                }
            }
            SessionAction::Revoke { .. } => {}
        }
        let result = SessionResult {
            id: plan.id.clone(),
            status: plan.status.as_str().to_owned(),
            version: plan.version,
        };
        self.prepared = Some(Prepared::Change {
            snapshot: Box::new(snapshot),
            plan: Box::new(plan),
        });
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
            Prepared::Request(plan) => {
                apply_request(c, self.request.key.as_str(), &plan).await?;
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
            Prepared::Request(plan) => {
                let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_sessions s JOIN support_session_events e ON e.session_id=s.id AND e.version=1 AND e.action='request' WHERE s.id=$1 AND s.company_id=$2 AND s.version=1 AND s.status='pending')")
                    .bind(&plan.id)
                    .bind(self.company.as_str())
                    .fetch_one(&mut *c)
                    .await?;
                if !exists {
                    return Err(changed().into());
                }
            }
            Prepared::Change { plan, .. } => {
                if let Some(event) = plan.session_event() {
                    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_sessions s JOIN support_session_events e ON e.session_id=s.id AND e.version=s.version WHERE s.id=$1 AND s.company_id=$2 AND s.version=$3 AND e.action=$4)")
                        .bind(&plan.id)
                        .bind(self.company.as_str())
                        .bind(plan.version)
                        .bind(event)
                        .fetch_one(&mut *c)
                        .await?;
                    if !exists {
                        return Err(changed().into());
                    }
                } else {
                    let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_sessions s WHERE s.id=$1 AND s.company_id=$2 AND s.version=$3)")
                        .bind(&plan.id)
                        .bind(self.company.as_str())
                        .bind(plan.version)
                        .fetch_one(&mut *c)
                        .await?;
                    if !exists {
                        return Err(changed().into());
                    }
                }
            }
        }
        Ok(())
    }
}

fn scope_matches(
    actor: &SessionActor,
    branch_id: Option<&str>,
    department_id: Option<&str>,
) -> bool {
    if let Some(branch) = &actor.branch_id
        && branch_id != Some(branch.as_str())
    {
        return false;
    }
    if let Some(department) = &actor.department_id
        && department_id != Some(department.as_str())
    {
        return false;
    }
    true
}

async fn target_eligible(
    c: &mut PgConnection,
    ticket: &TicketFacts,
    user_id: &str,
) -> Result<bool, Error> {
    let eligible: bool = sqlx::query_scalar(TARGET_ELIGIBLE)
        .bind(&ticket.company_id)
        .bind(user_id)
        .bind(ticket.branch_id.as_deref())
        .bind(ticket.department_id.as_deref())
        .fetch_one(&mut *c)
        .await?;
    Ok(eligible)
}

#[derive(Clone, Debug)]
struct TicketFacts {
    company_id: String,
    branch_id: Option<String>,
    department_id: Option<String>,
    source_kind: String,
    owner_id: Option<String>,
    status: String,
    version: i64,
}

async fn load_ticket(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<TicketFacts, Error> {
    let row: Option<TicketRow> = sqlx::query_as(
        "SELECT id,company_id,branch_id,department_id,source_kind,owner_id,status,version FROM support_tickets WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some((_id, company_id, branch_id, department_id, source_kind, owner_id, status, version)) =
        row
    else {
        return Err(not_found().into());
    };
    Ok(TicketFacts {
        company_id,
        branch_id,
        department_id,
        source_kind,
        owner_id,
        status,
        version,
    })
}

async fn load_session(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<SessionSnapshot, Error> {
    let row: Option<SessionRow> = sqlx::query_as(
        "SELECT id,company_id,branch_id,department_id,ticket_id,ticket_version,requested_for,status,created_by,reviewed_by,expires_at,version FROM support_sessions WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some((
        id,
        company_id,
        branch_id,
        department_id,
        ticket_id,
        ticket_version,
        requested_for,
        status,
        created_by,
        reviewed_by,
        expires_at,
        version,
    )) = row
    else {
        return Err(not_found().into());
    };
    let status = match status.as_str() {
        "pending" => SessionStatus::Pending,
        "approved" => SessionStatus::Approved,
        "rejected" => SessionStatus::Rejected,
        "revoked" => SessionStatus::Revoked,
        _ => return Err(changed().into()),
    };
    Ok(SessionSnapshot {
        id,
        company_id,
        branch_id,
        department_id,
        ticket_id,
        ticket_version,
        requested_for,
        status,
        created_by,
        reviewed_by,
        expires_at,
        version,
    })
}

async fn apply_request(
    c: &mut PgConnection,
    key: &str,
    plan: &SessionRequestPlan,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO support_sessions(id,company_id,branch_id,department_id,ticket_id,ticket_version,requested_for,reason,expires_at,status,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,'pending',$10,$11,$12)")
        .bind(&plan.id)
        .bind(&plan.company_id)
        .bind(&plan.branch_id)
        .bind(plan.department_id.as_deref())
        .bind(&plan.ticket_id)
        .bind(plan.ticket_version)
        .bind(&plan.requested_for)
        .bind(&plan.reason)
        .bind(plan.expires_at)
        .bind(&plan.created_by)
        .bind(plan.created_at)
        .bind(plan.updated_at)
        .execute(&mut *c)
        .await?;
    sqlx::query("INSERT INTO support_session_events(id,session_id,version,action,actor_id,reason,status,expires_at,created_at) VALUES($1,$2,1,'request',$3,$4,'pending',$5,$6)")
        .bind(format!("support-session-event-{key}"))
        .bind(&plan.id)
        .bind(&plan.created_by)
        .bind(&plan.reason)
        .bind(plan.expires_at)
        .bind(plan.created_at)
        .execute(&mut *c)
        .await?;
    sqlx::query(SESSION_REVIEWERS_NOTIFICATION)
        .bind(format!("support-session-review-{key}-"))
        .bind(&plan.id)
        .execute(&mut *c)
        .await?;
    Ok(())
}

async fn apply_change(
    c: &mut PgConnection,
    key: &str,
    snapshot: &SessionSnapshot,
    plan: &SessionChangePlan,
    actor_id: &str,
) -> Result<(), Error> {
    match plan.action {
        "approve" | "reject" => {
            let updated = sqlx::query("UPDATE support_sessions SET status=$1,reviewed_by=$2,reviewed_at=$3,version=version+1,updated_at=$3 WHERE id=$4 AND company_id=$5 AND version=$6")
                .bind(plan.status.as_str())
                .bind(actor_id)
                .bind(plan.event_at)
                .bind(&plan.id)
                .bind(&plan.company_id)
                .bind(snapshot.version)
                .execute(&mut *c)
                .await?
                .rows_affected();
            if updated != 1 {
                return Err(changed().into());
            }
            insert_session_event(c, key, plan, actor_id).await?;
        }
        "revoke" => {
            let updated = sqlx::query("UPDATE support_sessions SET status='revoked',reviewed_by=coalesce(reviewed_by,$1),reviewed_at=coalesce(reviewed_at,$2),revoked_by=$1,revoked_at=$2,version=version+1,updated_at=$2 WHERE id=$3 AND company_id=$4 AND version=$5")
                .bind(actor_id)
                .bind(plan.event_at)
                .bind(&plan.id)
                .bind(&plan.company_id)
                .bind(snapshot.version)
                .execute(&mut *c)
                .await?
                .rows_affected();
            if updated != 1 {
                return Err(changed().into());
            }
            insert_session_event(c, key, plan, actor_id).await?;
            sqlx::query("INSERT INTO support_capability_events(id,capability_id,version,action,actor_id,reason,status,created_at) SELECT $1||id,id,version+1,'session-revoke',$2,$3,'revoked',$4 FROM support_capabilities WHERE session_id=$5 AND status='active'")
                .bind(format!("support-cap-event-{key}-"))
                .bind(actor_id)
                .bind(&plan.reason)
                .bind(plan.event_at)
                .bind(&plan.id)
                .execute(&mut *c)
                .await?;
            sqlx::query("UPDATE support_capabilities SET status='revoked',revoked_by=$1,revoked_at=$2,version=version+1,updated_at=$2 WHERE session_id=$3 AND status='active'")
                .bind(actor_id)
                .bind(plan.event_at)
                .bind(&plan.id)
                .execute(&mut *c)
                .await?;
        }
        "issue-capability" => {
            let code = plan.capability_code.ok_or_else(changed)?;
            sqlx::query("INSERT INTO support_capabilities(id,session_id,code,status,reason,issued_by,created_at,updated_at) VALUES($1,$2,$3,'active',$4,$5,$6,$6) ON CONFLICT(session_id,code) DO UPDATE SET status='active',reason=excluded.reason,revoked_by=NULL,revoked_at=NULL,version=support_capabilities.version+1,updated_at=excluded.updated_at")
                .bind(format!("support-capability-{key}"))
                .bind(&plan.id)
                .bind(code.as_str())
                .bind(&plan.reason)
                .bind(actor_id)
                .bind(plan.event_at)
                .execute(&mut *c)
                .await?;
            sqlx::query("INSERT INTO support_capability_events(id,capability_id,version,action,actor_id,reason,status,created_at) SELECT $1||id,id,version,CASE WHEN version=1 THEN 'issue' ELSE 'reissue' END,$2,$3,'active',$4 FROM support_capabilities WHERE session_id=$5 AND code=$6")
                .bind(format!("support-cap-event-{key}-"))
                .bind(actor_id)
                .bind(&plan.reason)
                .bind(plan.event_at)
                .bind(&plan.id)
                .bind(code.as_str())
                .execute(&mut *c)
                .await?;
            bump_session(c, snapshot, plan).await?;
        }
        "revoke-capability" => {
            let capability_id = plan.capability_id.as_deref().ok_or_else(changed)?;
            let expected = plan.expected_capability_version.ok_or_else(changed)?;
            let updated = sqlx::query("UPDATE support_capabilities SET status='revoked',revoked_by=$1,revoked_at=$2,version=version+1,updated_at=$2 WHERE id=$3 AND session_id=$4 AND version=$5 AND status='active'")
                .bind(actor_id)
                .bind(plan.event_at)
                .bind(capability_id)
                .bind(&plan.id)
                .bind(expected)
                .execute(&mut *c)
                .await?
                .rows_affected();
            if updated != 1 {
                return Err(changed().into());
            }
            sqlx::query("INSERT INTO support_capability_events(id,capability_id,version,action,actor_id,reason,status,created_at) SELECT $1,id,version,'revoke',$2,$3,'revoked',$4 FROM support_capabilities WHERE id=$5")
                .bind(format!("support-cap-event-{key}"))
                .bind(actor_id)
                .bind(&plan.reason)
                .bind(plan.event_at)
                .bind(capability_id)
                .execute(&mut *c)
                .await?;
            bump_session(c, snapshot, plan).await?;
        }
        _ => return Err(changed().into()),
    }
    sqlx::query(SESSION_UPDATED_NOTIFICATION)
        .bind(format!("support-session-notice-{key}"))
        .bind(&plan.id)
        .execute(&mut *c)
        .await?;
    Ok(())
}

async fn insert_session_event(
    c: &mut PgConnection,
    key: &str,
    plan: &SessionChangePlan,
    actor_id: &str,
) -> Result<(), Error> {
    let event = plan.session_event().ok_or_else(changed)?;
    sqlx::query("INSERT INTO support_session_events(id,session_id,version,action,actor_id,reason,status,expires_at,created_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(format!("support-session-event-{key}"))
        .bind(&plan.id)
        .bind(plan.version)
        .bind(event)
        .bind(actor_id)
        .bind(&plan.reason)
        .bind(plan.status.as_str())
        .bind(plan.expires_at)
        .bind(plan.event_at)
        .execute(&mut *c)
        .await?;
    Ok(())
}

async fn bump_session(
    c: &mut PgConnection,
    snapshot: &SessionSnapshot,
    plan: &SessionChangePlan,
) -> Result<(), Error> {
    let updated = sqlx::query("UPDATE support_sessions SET version=version+1,updated_at=$1 WHERE id=$2 AND company_id=$3 AND version=$4")
        .bind(plan.event_at)
        .bind(&plan.id)
        .bind(&plan.company_id)
        .bind(snapshot.version)
        .execute(&mut *c)
        .await?
        .rows_affected();
    if updated != 1 {
        return Err(changed().into());
    }
    Ok(())
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SUPPORT_SESSION_CHANGED",
        "The Support session changed. Reload before retrying.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "SUPPORT_SESSION_NOT_FOUND",
        "Support session not found in this scope.",
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
