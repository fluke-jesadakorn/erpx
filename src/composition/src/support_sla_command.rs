//! Support SLA command composition. Support owns the policy, clock and alert
//! rows; Identity supplies current authority, Workflow supplies the command
//! receipt/audit/outbox and Commercial supplies the purchase witness.
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
use erpx_server_support::public::sla::{
    AlertAcknowledge, AlertAcknowledgePlan, AlertSnapshot, ClockAttach, ClockAttachPlan, PolicyChange,
    PolicyChangePlan, PolicyCreate, PolicyCreatePlan, PolicySnapshot, PolicyStatus, SlaActor,
    SlaPermissions, SlaPolicy, TicketSnapshot, parse_clock, plan_alert_acknowledge, plan_attach,
    plan_policy_change, plan_policy_create,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandField, CommandJson, PgCommandAuthority,
    PgCommandLedger, PgOwnerCommandEffects, WorkAuthority, WorkflowOutput, WorkflowRequest,
};
use serde::Serialize;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const MANAGER_ELIGIBLE: &str = "SELECT EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=m.user_id AND p.company_id=m.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE m.company_id=$1 AND m.user_id=$2 AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(m.user_id,m.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=$3) OR (m.scope_kind='department' AND m.scope_node_id=$4)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')))";

const POLICY_REVIEWERS_NOTIFICATION: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT DISTINCT $1||m.user_id,p.company_id,m.user_id,'support_policy',p.id,p.version,'Review a proposed working-time resolution policy',p.updated_at FROM support_sla_policies p JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m ON m.company_id=p.company_id AND m.user_id<>p.created_by AND m.scope_kind='holding' AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p2 ON p2.user_id=m.user_id AND p2.company_id=m.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\" FROM hr_engagements) e ON e.person_id=p2.id AND e.company_id=p2.company_id WHERE p.id=$2 AND p2.status='active' AND p2.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ON CONFLICT DO NOTHING";

const POLICY_UPDATED_NOTIFICATION: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT $1,company_id,created_by,'support_policy',id,version,'SLA policy reviewed: check its current availability',updated_at FROM support_sla_policies WHERE id=$2 ON CONFLICT DO NOTHING";

const TICKET_CLOCK_NOTIFICATION: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT $1,company_id,owner_id,'support_ticket',id,version,'Approved working-time target attached: review the deadline',updated_at FROM support_tickets WHERE id=$2 AND owner_id IS NOT NULL ON CONFLICT DO NOTHING";

const SLA_BREACHED_NOTIFICATION: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT DISTINCT s.id||':'||m.user_id,t.company_id,m.user_id,'support_sla_alert',s.id,s.version,'Resolution target breached: take responsibility for follow-up',$1 FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m ON m.company_id=t.company_id AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=m.user_id AND p.company_id=m.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE s.detected_at=$1 AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD')) ON CONFLICT DO NOTHING";

#[derive(Clone, Debug)]
enum SlaWrite {
    CreatePolicy(PolicyCreate),
    ChangePolicy { id: String, input: PolicyChange },
    Attach { id: String, input: ClockAttach },
    Acknowledge { id: String, input: AlertAcknowledge },
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SlaResult {
    pub id: String,
    pub version: i64,
}

#[derive(Clone)]
pub struct SupportSlaCommand<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub catalogue: Arc<CommercialCatalogue>,
    pub clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> SupportSlaCommand<C> {
    pub async fn create_policy<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        input: PolicyCreate,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        self.run(host, proof, workspace, SlaWrite::CreatePolicy(input), wire, key)
            .await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn change_policy<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        input: PolicyChange,
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
            SlaWrite::ChangePolicy { id, input },
            wire,
            key,
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn attach_clock<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        input: ClockAttach,
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
            SlaWrite::Attach { id, input },
            wire,
            key,
        )
        .await
    }
    #[allow(clippy::too_many_arguments)]
    pub async fn acknowledge_alert<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        input: AlertAcknowledge,
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
            SlaWrite::Acknowledge { id, input },
            wire,
            key,
        )
        .await
    }
    async fn run<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        write: SlaWrite,
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
        let (operation, record_id) = match &write {
            SlaWrite::CreatePolicy(_) => (
                "support.sla.create".to_owned(),
                format!("support-policy-{}", expected.as_str()),
            ),
            SlaWrite::ChangePolicy { id, input } => (
                format!("support.sla.{}", input.action()),
                id.clone(),
            ),
            SlaWrite::Attach { id, .. } => ("support.sla.attach".to_owned(), id.clone()),
            SlaWrite::Acknowledge { id, .. } => ("support.sla.acknowledge".to_owned(), id.clone()),
        };
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                workspace()?,
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::Exact("support.manage".into()),
                    module: Some("support".into()),
                    destination: operation.clone(),
                    api: true,
                },
                SupportSlaSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let expected = expected.clone();
                    let operation = operation.clone();
                    let record_id = record_id.clone();
                    let write = write.clone();
                    let wire = wire.clone();
                    Box::pin(async move {
                        if current.selection().workspace().company_id != expected_company {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "SUPPORT_SLA_SCOPE_REQUIRED",
                                "Use the current Company.",
                            )
                            .into());
                        }
                        let clock_ref = clock.as_ref();
                        let now = clock_ref.now();
                        let actor = sla_actor(current, now)?;
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
                            write: &write,
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

fn sla_actor(
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
) -> Result<SlaActor, BusinessError> {
    if now.get() >= current.expires_at() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_SLA_AUTHORITY_CHANGED",
            "Your current Support authority is unavailable.",
        ));
    }
    let workspace = current.selection().workspace();
    Ok(SlaActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|v| v.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|v| v.as_str().to_owned()),
        permissions: SlaPermissions {
            can_read: current.has_permission("support.read")
                || current.has_permission("records.read"),
            can_manage: current.has_permission("support.manage"),
        },
        same_person_ids: Vec::new(),
    })
}

#[derive(Clone)]
struct SupportSlaSources;
impl erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation
    for SupportSlaSources
{
    fn prepare<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> erpx_server_runtime::public::postgres::PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            sqlx::query("LOCK TABLE support_sla_policies,support_sla_policy_events,support_sla_alerts,support_tickets,staff_notifications IN SHARE MODE")
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
            &SupportSlaSources,
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
            capabilities: vec!["support.sla".into()],
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
    write: &'a SlaWrite,
    actor: SlaActor,
    prepared: Option<Prepared>,
}

enum Prepared {
    Create(PolicyCreatePlan),
    Change(PolicyChangePlan),
    Attach(ClockAttachPlan),
    Acknowledge(AlertAcknowledgePlan),
}

type PolicyRow = (
    String,
    String,
    String,
    String,
    Option<String>,
    i64,
    String,
);
type TicketRow = (
    String,
    String,
    String,
    i64,
    Option<String>,
    Option<String>,
);
type AlertRow = (
    String,
    String,
    String,
    Option<String>,
    Option<String>,
    i64,
    Option<String>,
);

impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation::prepare(
            &SupportSlaSources,
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
        match self.write {
            SlaWrite::CreatePolicy(input) => {
                let plan = plan_policy_create(
                    &self.actor,
                    self.request.key.as_str(),
                    input,
                    self.clock.now().get(),
                )?;
                let id = plan.id.clone();
                self.prepared = Some(Prepared::Create(plan));
                Ok(WorkflowOutput {
                    version: 1,
                    result: CommandJson::from_serializable(&SlaResult { id, version: 1 })?,
                    event_scope: None,
                })
            }
            SlaWrite::ChangePolicy { id, input } => {
                let (snapshot, _) = load_policy(c, self.company, id).await?;
                let plan = plan_policy_change(
                    &self.actor,
                    &snapshot,
                    input,
                    self.clock.now().get(),
                )?;
                let result = SlaResult {
                    id: plan.id.clone(),
                    version: plan.version,
                };
                self.prepared = Some(Prepared::Change(plan));
                Ok(WorkflowOutput {
                    version: result.version,
                    result: CommandJson::from_serializable(&result)?,
                    event_scope: None,
                })
            }
            SlaWrite::Attach { id, input } => {
                let ticket = load_ticket(c, self.company, id).await?;
                let (policy, value) = load_policy(c, self.company, &input.policy_id).await?;
                let plan = plan_attach(
                    &self.actor,
                    &ticket,
                    &policy,
                    &value,
                    input,
                    self.clock.now().get(),
                )?;
                let result = SlaResult {
                    id: plan.ticket_id.clone(),
                    version: plan.ticket_version,
                };
                self.prepared = Some(Prepared::Attach(plan));
                Ok(WorkflowOutput {
                    version: result.version,
                    result: CommandJson::from_serializable(&result)?,
                    event_scope: None,
                })
            }
            SlaWrite::Acknowledge { id, input } => {
                let alert = load_alert(c, self.company, id).await?;
                let eligible: bool = sqlx::query_scalar(MANAGER_ELIGIBLE)
                    .bind(self.company.as_str())
                    .bind(&self.actor.user_id)
                    .bind(alert.branch_id.as_deref())
                    .bind(alert.department_id.as_deref())
                    .fetch_one(&mut *c)
                    .await?;
                if !eligible {
                    return Err(BusinessError::new(
                        FailureKind::Forbidden,
                        "SUPPORT_ALERT_RECIPIENT_REQUIRED",
                        "An eligible Support manager must acknowledge this escalation.",
                    )
                    .into());
                }
                let plan = plan_alert_acknowledge(
                    &self.actor,
                    &alert,
                    input,
                    self.clock.now().get(),
                )?;
                let result = SlaResult {
                    id: plan.id.clone(),
                    version: plan.version,
                };
                self.prepared = Some(Prepared::Acknowledge(plan));
                Ok(WorkflowOutput {
                    version: result.version,
                    result: CommandJson::from_serializable(&result)?,
                    event_scope: None,
                })
            }
        }
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        match self.prepared.take().ok_or_else(changed)? {
            Prepared::Create(plan) => {
                apply_create(c, self.request.key.as_str(), &plan).await?;
            }
            Prepared::Change(plan) => {
                apply_change(c, self.request.key.as_str(), &plan).await?;
            }
            Prepared::Attach(plan) => {
                apply_attach(c, self.request.key.as_str(), &plan).await?;
            }
            Prepared::Acknowledge(plan) => {
                apply_acknowledge(c, &plan).await?;
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
            Prepared::Create(plan) => {
                let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_sla_policies p JOIN support_sla_policy_events e ON e.policy_id=p.id AND e.version=1 AND e.action='create' WHERE p.id=$1 AND p.company_id=$2 AND p.version=1 AND p.status='draft')")
                    .bind(&plan.id)
                    .bind(self.company.as_str())
                    .fetch_one(&mut *c)
                    .await?;
                if !exists {
                    return Err(changed().into());
                }
            }
            Prepared::Change(plan) => {
                let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_sla_policies p JOIN support_sla_policy_events e ON e.policy_id=p.id AND e.version=p.version WHERE p.id=$1 AND p.company_id=$2 AND p.version=$3 AND e.action=$4)")
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
            Prepared::Attach(plan) => {
                let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_tickets t JOIN support_ticket_events e ON e.ticket_id=t.id AND e.version=t.version AND e.action='sla-attach' WHERE t.id=$1 AND t.company_id=$2 AND t.version=$3 AND t.sla_policy_id=$4 AND t.sla_policy_version=$5 AND t.sla_clock_json IS NOT NULL)")
                    .bind(&plan.ticket_id)
                    .bind(self.company.as_str())
                    .bind(plan.ticket_version)
                    .bind(&plan.policy_id)
                    .bind(plan.policy_version)
                    .fetch_one(&mut *c)
                    .await?;
                if !exists {
                    return Err(changed().into());
                }
            }
            Prepared::Acknowledge(plan) => {
                let exists: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_sla_alerts WHERE id=$1 AND version=$2 AND acknowledged_by IS NOT NULL)")
                    .bind(&plan.id)
                    .bind(plan.version)
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

async fn load_policy(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<(PolicySnapshot, SlaPolicy), Error> {
    let row: Option<PolicyRow> = sqlx::query_as(
        "SELECT id,company_id,status,created_by,approved_by,version,policy_json::text FROM support_sla_policies WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some((id, company_id, status, created_by, approved_by, version, policy_json)) = row else {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_POLICY_NOT_FOUND",
            "Policy not found in this Company.",
        )
        .into());
    };
    let status = match status.as_str() {
        "draft" => PolicyStatus::Draft,
        "approved" => PolicyStatus::Approved,
        "retired" => PolicyStatus::Retired,
        _ => return Err(changed().into()),
    };
    let value: SlaPolicy = serde_json::from_str(&policy_json)
        .map_err(|_| BusinessError::invalid("SUPPORT_POLICY_INVALID", "Provide a valid policy."))?;
    Ok((
        PolicySnapshot {
            id,
            company_id,
            status,
            created_by,
            approved_by,
            version,
        },
        value,
    ))
}

async fn load_ticket(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<TicketSnapshot, Error> {
    let row: Option<TicketRow> = sqlx::query_as(
        "SELECT id,company_id,status,version,sla_policy_id,sla_clock_json::text FROM support_tickets WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some((id, company_id, status, version, sla_policy_id, clock_json)) = row else {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_TICKET_NOT_FOUND",
            "Ticket not found in this scope.",
        )
        .into());
    };
    Ok(TicketSnapshot {
        id,
        company_id,
        status,
        version,
        sla_policy_id,
        clock: clock_json.as_deref().map(parse_clock).transpose()?,
    })
}

async fn load_alert(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<AlertSnapshot, Error> {
    let row: Option<AlertRow> = sqlx::query_as(
        "SELECT s.id,s.ticket_id,t.company_id,t.branch_id,t.department_id,s.version,s.acknowledged_by FROM support_sla_alerts s JOIN support_tickets t ON t.id=s.ticket_id WHERE s.id=$1 AND t.company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some((id, ticket_id, company_id, branch_id, department_id, version, acknowledged_by)) =
        row
    else {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_ALERT_NOT_FOUND",
            "Target escalation is not available in this scope.",
        )
        .into());
    };
    Ok(AlertSnapshot {
        id,
        ticket_id,
        company_id,
        branch_id,
        department_id,
        version,
        acknowledged_by,
    })
}

async fn apply_create(
    c: &mut PgConnection,
    key: &str,
    plan: &PolicyCreatePlan,
) -> Result<(), Error> {
    sqlx::query("INSERT INTO support_sla_policies(id,company_id,name,policy_json,status,created_by,created_at,updated_at) VALUES($1,$2,$3,$4::jsonb,'draft',$5,$6,$7)")
        .bind(&plan.id)
        .bind(&plan.company_id)
        .bind(&plan.name)
        .bind(&plan.policy_json)
        .bind(&plan.created_by)
        .bind(plan.created_at)
        .bind(plan.updated_at)
        .execute(&mut *c)
        .await?;
    sqlx::query("INSERT INTO support_sla_policy_events(id,policy_id,version,action,actor_id,reason,created_at) VALUES($1,$2,1,'create',$3,$4,$5)")
        .bind(format!("support-policy-event-{key}"))
        .bind(&plan.id)
        .bind(&plan.created_by)
        .bind(&plan.reason)
        .bind(plan.created_at)
        .execute(&mut *c)
        .await?;
    sqlx::query(POLICY_REVIEWERS_NOTIFICATION)
        .bind(format!("support-policy-notice-{key}-"))
        .bind(&plan.id)
        .execute(&mut *c)
        .await?;
    Ok(())
}

async fn apply_change(
    c: &mut PgConnection,
    key: &str,
    plan: &PolicyChangePlan,
) -> Result<(), Error> {
    let updated = sqlx::query("UPDATE support_sla_policies SET status=$1,approved_by=coalesce($2,approved_by),approved_at=coalesce($3,approved_at),version=version+1,updated_at=$4 WHERE id=$5 AND company_id=$6 AND version=$7")
        .bind(plan.status.as_str())
        .bind(plan.approved_by.as_deref())
        .bind(plan.approved_at)
        .bind(plan.event_at)
        .bind(&plan.id)
        .bind(&plan.company_id)
        .bind(plan.version - 1)
        .execute(&mut *c)
        .await?
        .rows_affected();
    if updated != 1 {
        return Err(changed().into());
    }
    sqlx::query("INSERT INTO support_sla_policy_events(id,policy_id,version,action,actor_id,reason,created_at) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(format!("support-policy-event-{key}"))
        .bind(&plan.id)
        .bind(plan.version)
        .bind(plan.action)
        .bind(&plan.actor_id)
        .bind(&plan.reason)
        .bind(plan.event_at)
        .execute(&mut *c)
        .await?;
    sqlx::query(POLICY_UPDATED_NOTIFICATION)
        .bind(format!("support-policy-notice-{key}"))
        .bind(&plan.id)
        .execute(&mut *c)
        .await?;
    Ok(())
}

async fn apply_attach(
    c: &mut PgConnection,
    key: &str,
    plan: &ClockAttachPlan,
) -> Result<(), Error> {
    let updated = sqlx::query("UPDATE support_tickets SET sla_policy_id=$1,sla_policy_version=$2,sla_clock_json=$3::jsonb,version=version+1,updated_at=$4 WHERE id=$5 AND company_id=$6 AND version=$7 AND status='open' AND sla_policy_id IS NULL")
        .bind(&plan.policy_id)
        .bind(plan.policy_version)
        .bind(&plan.clock_json)
        .bind(plan.event_at)
        .bind(&plan.ticket_id)
        .bind(&plan.company_id)
        .bind(plan.ticket_version - 1)
        .execute(&mut *c)
        .await?
        .rows_affected();
    if updated != 1 {
        return Err(changed().into());
    }
    sqlx::query("INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) SELECT $1,id,version,'sla-attach',$2,$3,owner_id,due_date,status,updated_at,$4::jsonb FROM support_tickets WHERE id=$5")
        .bind(format!("support-sla-event-{key}"))
        .bind(&plan.actor_id)
        .bind(&plan.reason)
        .bind(
            serde_json::to_string(&serde_json::json!({
                "policyId": plan.policy_id,
                "policyVersion": plan.policy_version,
                "clock": plan.clock_json,
            }))
            .map_err(|_| changed())?,
        )
        .bind(&plan.ticket_id)
        .execute(&mut *c)
        .await?;
    sqlx::query(TICKET_CLOCK_NOTIFICATION)
        .bind(format!("support-sla-notice-{key}"))
        .bind(&plan.ticket_id)
        .execute(&mut *c)
        .await?;
    Ok(())
}

async fn apply_acknowledge(
    c: &mut PgConnection,
    plan: &AlertAcknowledgePlan,
) -> Result<(), Error> {
    let updated = sqlx::query("UPDATE support_sla_alerts SET acknowledged_by=$1,acknowledged_at=$2,reason=$3,version=version+1 WHERE id=$4 AND version=$5 AND acknowledged_by IS NULL")
        .bind(&plan.actor_id)
        .bind(plan.event_at)
        .bind(&plan.reason)
        .bind(&plan.id)
        .bind(plan.version - 1)
        .execute(&mut *c)
        .await?
        .rows_affected();
    if updated != 1 {
        return Err(changed().into());
    }
    Ok(())
}

/// Drain breached Support clocks into idempotent alerts and eligible-manager
/// notifications. The caller supplies the existing transaction.
pub async fn drain_support_sla(
    c: &mut PgConnection,
    company: &CompanyId,
    now: i64,
) -> Result<i64, Error> {
    sqlx::query("LOCK TABLE support_tickets,support_sla_alerts,staff_notifications IN SHARE MODE")
        .execute(&mut *c)
        .await?;
    let inserted = sqlx::query("INSERT INTO support_sla_alerts(id,ticket_id,source_version,breached_at,detected_at) SELECT 'support-breach-'||t.id,t.id,t.version,COALESCE((t.sla_clock_json->>'breachedAt')::bigint,(t.sla_clock_json->>'dueAt')::bigint),$1 FROM support_tickets t WHERE t.company_id=$2 AND EXISTS(SELECT 1 FROM company_package_grants g WHERE g.company_id=t.company_id AND g.package_id='support' AND g.state='active' AND g.starts_at<=$1 AND (g.ends_at IS NULL OR g.ends_at>$1)) AND ((t.sla_clock_json->>'breachedAt')::bigint IS NOT NULL OR (t.sla_clock_json->>'state'='running' AND (t.sla_clock_json->>'dueAt')::bigint<$1)) ON CONFLICT DO NOTHING")
        .bind(now)
        .bind(company.as_str())
        .execute(&mut *c)
        .await?
        .rows_affected();
    sqlx::query(SLA_BREACHED_NOTIFICATION)
        .bind(now)
        .execute(&mut *c)
        .await?;
    Ok(inserted as i64)
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SUPPORT_SLA_CHANGED",
        "The Support SLA record changed. Reload before retrying.",
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
