//! Support ticket command composition. Support owns the ticket state, event and
//! notification rows; Identity supplies current authority, Workflow supplies the
//! command receipt/audit/outbox, Commercial supplies the purchase witness and the
//! Records/Assets/POS/PMS owners supply any typed source witness.
use crate::field_service_records_source::{PreparedLeafSource, prepare_support_origin};
use crate::field_service_source_record::SourceRecordContext;
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
use erpx_server_support::public::ticket::{
    SourceKind, TicketActor, TicketChange, TicketChangePlan, TicketCreate, TicketPermissions,
    TicketSnapshot, TicketStatus, plan_change, plan_create,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandField, CommandJson, PgCommandAuthority,
    PgCommandLedger, PgOwnerCommandEffects, WorkAuthority, WorkflowOutput, WorkflowRequest,
};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

const OWNER_ELIGIBLE: &str = "SELECT EXISTS(SELECT 1 FROM support_tickets t WHERE t.id=$1 AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=t.owner_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))))";


const TICKET_CREATED_NOTIFICATION: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT $1,company_id,owner_id,'support_ticket',id,version,'New customer support case: accept and review the issue',updated_at FROM support_tickets WHERE id=$2";
const TICKET_REOPENED_NOTIFICATION: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT DISTINCT $1||m.user_id,t.company_id,m.user_id,'support_ticket',t.id,t.version,'New issue needs Support triage',t.updated_at FROM support_tickets t JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m ON m.company_id=t.company_id WHERE t.id=$2 AND m.user_id<>t.created_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=m.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) ON CONFLICT DO NOTHING";
const TICKET_UPDATED_NOTIFICATION: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT $1,company_id,owner_id,'support_ticket',id,version,'Support case updated: review current status and next action',updated_at FROM support_tickets WHERE id=$2 AND owner_id IS NOT NULL";
const TICKET_REVIEWERS_NOTIFICATION: &str = "INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) SELECT DISTINCT $1||m.user_id,t.company_id,m.user_id,'support_ticket',t.id,t.version,'Review proposed issue resolution',t.updated_at FROM support_tickets t JOIN (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) m ON m.company_id=t.company_id WHERE t.id=$2 AND m.user_id<>t.owner_id AND m.user_id<>t.resolved_by AND iam_assignment_active(m.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(m.role,m.company_id,'support.manage') AND (m.scope_kind='holding' OR (m.scope_kind='branch' AND m.scope_node_id=t.branch_id) OR (m.scope_kind='department' AND m.scope_node_id=t.department_id)) AND EXISTS(SELECT 1 FROM (SELECT \"id\",\"user_id\",\"role\",\"modules_json\",\"company_id\",\"branch_id\",\"valid_from\",\"valid_until\",\"revoked_at\",\"version\",\"scope_kind\",\"scope_node_id\",\"parent_id\",\"source\",\"source_id\" FROM assignments) a JOIN (SELECT \"id\",\"user_id\",\"company_id\",\"legal_name\",\"preferred_name\",\"email\",\"phone\",\"country_code\",\"status\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"merged_into_id\",\"work_email\",\"work_phone\" FROM hr_people) p ON p.user_id=a.user_id AND p.company_id=a.company_id JOIN (SELECT \"id\",\"person_id\",\"company_id\",\"branch_id\",\"department_id\",\"worker_type\",\"employment_type\",\"status\",\"start_date\",\"end_date\",\"manager_person_id\",\"job_title\",\"currency\",\"version\",\"created_by\",\"created_at\",\"updated_at\",\"pay_group_id\" FROM hr_engagements) e ON e.person_id=p.id AND e.company_id=p.company_id WHERE a.company_id=t.company_id AND a.user_id=m.user_id AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'support.work') AND iam_role_allows(a.role,a.company_id,'support.read') AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=t.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=t.department_id)) AND p.status='active' AND p.merged_into_id IS NULL AND e.status='active' AND e.start_date<=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD') AND (e.end_date IS NULL OR e.end_date>=to_char(current_timestamp AT TIME ZONE 'UTC','YYYY-MM-DD'))) ON CONFLICT DO NOTHING";

#[derive(Clone, Debug)]
pub enum SupportTicketCreate {
    General { title: String, issue: String },
    Order {
        order_id: String,
        source_version: i64,
        title: String,
        issue: String,
        owner_id: String,
        due_date: String,
    },
    Source {
        kind: String,
        id: String,
        version: i64,
        title: String,
        issue: String,
    },
}

impl SupportTicketCreate {
    pub fn parse(value: &Value) -> Result<Self, BusinessError> {
        let text = |key: &str| {
            value[key]
                .as_str()
                .map(str::to_owned)
                .ok_or_else(|| invalid("SUPPORT_INPUT_INVALID", "Provide a valid ticket."))
        };
        match value["kind"].as_str() {
            Some("general") => Ok(Self::General {
                title: text("title")?,
                issue: text("issue")?,
            }),
            Some("source") => {
                let source = value["source"]
                    .as_object()
                    .ok_or_else(|| invalid("SUPPORT_INPUT_INVALID", "Provide a valid source."))?;
                let kind = source["kind"].as_str().ok_or_else(|| {
                    invalid("SUPPORT_SOURCE_INVALID", "Choose a supported source record.")
                })?;
                let id = source["id"].as_str().ok_or_else(|| {
                    invalid("SUPPORT_SOURCE_INVALID", "Choose a supported source record.")
                })?;
                let version = source["version"].as_i64().ok_or_else(|| {
                    invalid("SUPPORT_SOURCE_INVALID", "Choose a supported source record.")
                })?;
                Ok(Self::Source {
                    kind: kind.to_owned(),
                    id: id.to_owned(),
                    version,
                    title: text("title")?,
                    issue: text("issue")?,
                })
            }
            _ if value["orderId"].is_string() => Ok(Self::Order {
                order_id: text("orderId")?,
                source_version: value["sourceVersion"].as_i64().ok_or_else(|| {
                    invalid("SUPPORT_ORDER_INVALID", "Choose a current order source.")
                })?,
                title: text("title")?,
                issue: text("issue")?,
                owner_id: text("ownerId")?,
                due_date: text("dueDate")?,
            }),
            _ => Err(invalid(
                "SUPPORT_INPUT_INVALID",
                "Provide a general, order-backed or typed-source ticket.",
            )),
        }
    }
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TicketResult {
    pub id: String,
    pub status: String,
    pub version: i64,
}

#[derive(Clone)]
pub struct SupportTicketCommand<C> {
    pub authorizer: Arc<HoldingWorkAuthorizer>,
    pub holding: HoldingId,
    pub company: CompanyId,
    pub catalogue: Arc<CommercialCatalogue>,
    pub clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> SupportTicketCommand<C> {
    pub async fn create<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        input: SupportTicketCreate,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let expected_company = self.company.clone();
        let holding = self.holding.clone();
        let company = self.company.clone();
        let catalogue = self.catalogue.clone();
        let clock = self.clock.clone();
        let permission = match &input {
            SupportTicketCreate::Order { .. } => WorkPermissionRequirement::Exact("support.create".into()),
            _ => WorkPermissionRequirement::Exact("records.write".into()),
        };
        let expected = key.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                workspace()?,
                WorkAuthorizationRequest {
                    permission,
                    module: Some("support".into()),
                    destination: "support.ticket.create".into(),
                    api: true,
                },
                SupportTicketSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let holding = holding.clone();
                    let company = company.clone();
                    let catalogue = catalogue.clone();
                    let clock = clock.clone();
                    let expected = expected.clone();
                    let input = input.clone();
                    let wire = wire.clone();
                    Box::pin(async move {
                        if current.selection().workspace().company_id != expected_company {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "SUPPORT_SCOPE_REQUIRED",
                                "Use the current Company.",
                            )
                            .into());
                        }
                        let clock_ref = clock.as_ref();
                        let now = clock_ref.now();
                        let actor = support_actor(current, now)?;
                        if matches!(input, SupportTicketCreate::Order { .. }) {
                            current.require_permission("support.read")?;
                            current.require_permission("commerce.read")?;
                        }
                        let id = format!("ticket-{}", expected.as_str());
                        let request = WorkflowRequest {
                            key: expected.as_str().to_owned(),
                            operation: "support.create".into(),
                            record_id: id.clone(),
                            input: wire.clone(),
                        };
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock_ref,
                            purchases: None,
                        };
                        let mut effects = Effects {
                            current,
                            holding: &holding,
                            company: &company,
                            catalogue: &catalogue,
                            clock: clock_ref,
                            request: &request,
                            create: Some(&input),
                            change: None,
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

    #[allow(clippy::too_many_arguments)]
    pub async fn change<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        workspace: Input,
        id: String,
        input: TicketChange,
        wire: CommandJson,
        key: CommandKey,
    ) -> Result<CommandJson, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let expected_company = self.company.clone();
        let holding = self.holding.clone();
        let company = self.company.clone();
        let catalogue = self.catalogue.clone();
        let clock = self.clock.clone();
        let expected = key.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                workspace()?,
                WorkAuthorizationRequest {
                    permission: WorkPermissionRequirement::AnyOf(vec![
                        "support.read".into(),
                        "records.read".into(),
                    ]),
                    module: Some("support".into()),
                    destination: "support.ticket.change".into(),
                    api: true,
                },
                SupportTicketSources,
                move |c, current| {
                    let expected_company = expected_company.clone();
                    let holding = holding.clone();
                    let company = company.clone();
                    let catalogue = catalogue.clone();
                    let clock = clock.clone();
                    let expected = expected.clone();
                    let id = id.clone();
                    let input = input.clone();
                    let wire = wire.clone();
                    Box::pin(async move {
                        if current.selection().workspace().company_id != expected_company {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "SUPPORT_SCOPE_REQUIRED",
                                "Use the current Company.",
                            )
                            .into());
                        }
                        let clock_ref = clock.as_ref();
                        let now = clock_ref.now();
                        let actor = support_actor(current, now)?;
                        let request = WorkflowRequest {
                            key: expected.as_str().to_owned(),
                            operation: format!("support.{}", input.action()),
                            record_id: id.clone(),
                            input: wire.clone(),
                        };
                        let mut authority = Authority {
                            current,
                            request: &request,
                            clock: clock_ref,
                            purchases: None,
                        };
                        let mut effects = Effects {
                            current,
                            holding: &holding,
                            company: &company,
                            catalogue: &catalogue,
                            clock: clock_ref,
                            request: &request,
                            create: None,
                            change: Some((&id, &input)),
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

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
}

fn support_actor(
    current: &AuthorizedWorkContext,
    now: TimestampMillis,
) -> Result<TicketActor, BusinessError> {
    if now.get() >= current.expires_at() {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "SUPPORT_AUTHORITY_CHANGED",
            "Your current Support authority is unavailable.",
        ));
    }
    let workspace = current.selection().workspace();
    let actor = TicketActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        branch_id: workspace.branch_id.as_ref().map(|v| v.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|v| v.as_str().to_owned()),
        permissions: TicketPermissions {
            can_read: current.has_permission("support.read")
                || current.has_permission("records.read"),
            can_create: current.has_permission("support.create")
                || current.has_permission("records.write"),
            can_write: current.has_permission("records.write"),
            can_manage: current.has_permission("support.manage"),
            can_work: current.has_permission("support.work"),
        },
        same_person_ids: Vec::new(),
    };
    Ok(actor)
}

#[derive(Clone)]
struct SupportTicketSources;
impl erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation
    for SupportTicketSources
{
    fn prepare<'c>(
        &'c self,
        c: &'c mut PgConnection,
    ) -> erpx_server_runtime::public::postgres::PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            erpx_server_commercial::public::purchases::lock_commercial_authority_sources(c).await?;
            sqlx::query("LOCK TABLE support_tickets,support_ticket_events,staff_notifications IN SHARE MODE")
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
            &SupportTicketSources,
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
        reader.revalidate(&self.history, now).await?.require_allowed()?;
        if let Some(witness) = &self.new_work {
            reader.revalidate(witness, now).await?.require_allowed()?;
        }
        Ok(())
    }
}

struct Effects<'a, C> {
    current: &'a AuthorizedWorkContext,
    holding: &'a HoldingId,
    company: &'a CompanyId,
    catalogue: &'a CommercialCatalogue,
    clock: &'a C,
    request: &'a WorkflowRequest,
    create: Option<&'a SupportTicketCreate>,
    change: Option<(&'a str, &'a TicketChange)>,
    actor: TicketActor,
    prepared: Option<Prepared>,
}

#[allow(clippy::large_enum_variant)]
enum Prepared {
    Create {
        plan: erpx_server_support::public::ticket::TicketCreatePlan,
        source: Option<PreparedLeafSource>,
        order: Option<OrderFacts>,
    },
    Change {
        snapshot: TicketSnapshot,
        row: TicketRow,
        plan: TicketChangePlan,
        source: Option<PreparedLeafSource>,
    },
}

#[derive(Clone, Debug)]
struct OrderFacts {
    branch_id: String,
    department_id: Option<String>,
    customer_id: String,
}

#[derive(Clone, Debug)]
struct TicketRow {
    snapshot: TicketSnapshot,
    order_id: Option<String>,
    customer_id: Option<String>,
    resolved_at: Option<i64>,
    blocked_reason: Option<String>,
    resolution: Option<String>,
    confirmation_note_version: Option<i64>,
    confirmed_by: Option<String>,
    confirmed_at: Option<i64>,
    confirmation_reason: Option<String>,
    _sla_clock_json: Option<String>,
    source_record_json: Option<String>,
}

impl<C: Clock + Sync> PgOwnerCommandEffects for Effects<'_, C> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        erpx_server_identity_access::public::effective_authority::HoldingWorkSourcePreparation::prepare(
            &SupportTicketSources,
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
            let source = match input {
                SupportTicketCreate::Source {
                    kind,
                    id,
                    version,
                    ..
                } => {
                    let source_kind = match kind.as_str() {
                        "pos_sale" => erpx_server_records::public::source_record::SourceKind::PosSale,
                        "pms_record" => erpx_server_records::public::source_record::SourceKind::PmsRecord,
                        "asset" => erpx_server_records::public::source_record::SourceKind::Asset,
                        _ => {
                            return Err(invalid(
                                "SUPPORT_SOURCE_INVALID",
                                "Choose a supported source record.",
                            )
                            .into())
                        }
                    };
                    let context = SourceRecordContext {
                        actor: self.current,
                        holding: self.holding,
                        company: self.company,
                        catalogue: self.catalogue,
                        clock: self.clock,
                    };
                    Some(
                        prepare_support_origin(
                            c,
                            &context,
                            &erpx_server_records::public::source_record::SourceReference {
                                kind: source_kind,
                                id: id.clone(),
                                expected_version: Some(*version),
                            },
                        )
                        .await?,
                    )
                }
                _ => None,
            };
            let order = match input {
                SupportTicketCreate::Order {
                    order_id,
                    source_version,
                    ..
                } => {
                    let row:Option<(String,Option<String>,String)>=sqlx::query_as(
                        "SELECT o.branch_id,o.department_id,o.party_id FROM commerce_orders o JOIN finance_parties p ON p.id=o.party_id AND p.company_id=o.company_id WHERE o.id=$1 AND o.version=$2 AND o.company_id=$3 AND o.kind IN ('sales','quote') AND p.kind='customer' AND ($4::text IS NULL OR o.branch_id=$4) AND ($5::text IS NULL OR o.department_id=$5)",
                    )
                    .bind(order_id)
                    .bind(source_version)
                    .bind(self.company.as_str())
                    .bind(self.current.selection().workspace().branch_id.as_ref().map(|v| v.as_str()))
                    .bind(self.current.selection().workspace().department_id.as_ref().map(|v| v.as_str()))
                    .fetch_optional(&mut *c)
                    .await?;
                    let Some((branch_id, department_id, customer_id)) = row else {
                        return Err(BusinessError::new(
                            FailureKind::NotFound,
                            "SUPPORT_ORDER_SOURCE_UNAVAILABLE",
                            "The order source is unavailable in this scope.",
                        )
                        .into());
                    };
                    Some(OrderFacts {
                        branch_id,
                        department_id,
                        customer_id,
                    })
                }
                _ => None,
            };
            let domain = domain_create(input);
            let plan = match plan_create(
                &self.actor,
                self.request.key.as_str(),
                &domain,
                self.clock.now().get(),
            )? {
                erpx_server_support::public::ticket::TicketPlan::Create(plan) => plan,
                _ => return Err(changed().into()),
            };
            let status = if matches!(input, SupportTicketCreate::Order { .. }) {
                "open"
            } else {
                "triage"
            };
            self.prepared = Some(Prepared::Create {
                plan,
                source,
                order,
            });
            return Ok(create_output(self.request.key.as_str(), status));
        }
        let (id, input) = self.change.ok_or_else(changed)?;
        let row = load_ticket(c, self.company, id).await?;
        let source = match &row.snapshot.source_kind {
            SourceKind::PosSale | SourceKind::PmsRecord | SourceKind::Asset => {
                let raw = row
                    .source_record_json
                    .as_deref()
                    .ok_or_else(changed)?;
                let value: Value = serde_json::from_str(raw).map_err(|_| changed())?;
                let kind = match value["kind"].as_str() {
                    Some("pos_sale") => erpx_server_records::public::source_record::SourceKind::PosSale,
                    Some("pms_record") => erpx_server_records::public::source_record::SourceKind::PmsRecord,
                    Some("asset") => erpx_server_records::public::source_record::SourceKind::Asset,
                    _ => return Err(changed().into()),
                };
                let context = SourceRecordContext {
                    actor: self.current,
                    holding: self.holding,
                    company: self.company,
                    catalogue: self.catalogue,
                    clock: self.clock,
                };
                Some(
                    prepare_support_origin(
                        c,
                        &context,
                        &erpx_server_records::public::source_record::SourceReference {
                            kind,
                            id: value["id"].as_str().ok_or_else(changed)?.to_owned(),
                            expected_version: value["version"].as_i64(),
                        },
                    )
                    .await?,
                )
            }
            _ => None,
        };
        let plan = match plan_change(
            &self.actor,
            &row.snapshot,
            input,
            self.clock.now().get(),
        )? {
            erpx_server_support::public::ticket::TicketPlan::Change(plan) => plan,
            _ => return Err(changed().into()),
        };
        let result = TicketResult {
            id: id.to_owned(),
            status: plan.status.as_str().to_owned(),
            version: plan.version,
        };
        self.prepared = Some(Prepared::Change {
            snapshot: row.snapshot.clone(),
            row,
            plan,
            source,
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
            Prepared::Create {
                plan,
                source,
                order,
            } => {
                apply_create(c, self.request.key.as_str(), &plan, source.as_ref(), order.as_ref()).await?;
            }
            Prepared::Change {
                snapshot,
                row,
                plan,
                source,
            } => {
                if let Some(source) = source.as_ref() {
                    let context = SourceRecordContext {
                        actor: self.current,
                        holding: self.holding,
                        company: self.company,
                        catalogue: self.catalogue,
                        clock: self.clock,
                    };
                    source.require_current(c, &context).await?;
                }
                apply_change(
                    c,
                    self.request.key.as_str(),
                    &snapshot,
                    &row,
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
            Prepared::Create { plan, source, .. } => {
                let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_tickets t JOIN support_ticket_events e ON e.ticket_id=t.id WHERE t.id=$1 AND t.company_id=$2 AND t.version=1 AND e.action=$3)")
                    .bind(&plan.id)
                    .bind(self.company.as_str())
                    .bind(plan.event_action)
                    .fetch_one(&mut *c)
                    .await?;
                if !exists {
                    return Err(changed().into());
                }
                if let Some(source) = source {
                    let context = SourceRecordContext {
                        actor: self.current,
                        holding: self.holding,
                        company: self.company,
                        catalogue: self.catalogue,
                        clock: self.clock,
                    };
                    source.require_current(c, &context).await?;
                }
            }
            Prepared::Change { plan, source, .. } => {
                let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM support_tickets t JOIN support_ticket_events e ON e.ticket_id=t.id AND e.version=t.version WHERE t.id=$1 AND t.company_id=$2 AND t.version=$3 AND e.action=$4)")
                    .bind(&plan.id)
                    .bind(self.company.as_str())
                    .bind(plan.version)
                    .bind(plan.action)
                    .fetch_one(&mut *c)
                    .await?;
                if !exists {
                    return Err(changed().into());
                }
                if let Some(source) = source {
                    let context = SourceRecordContext {
                        actor: self.current,
                        holding: self.holding,
                        company: self.company,
                        catalogue: self.catalogue,
                        clock: self.clock,
                    };
                    source.require_current(c, &context).await?;
                }
            }
        }
        Ok(())
    }
}

fn domain_create(input: &SupportTicketCreate) -> TicketCreate {
    match input {
        SupportTicketCreate::General { title, issue } => TicketCreate::General {
            title: title.clone(),
            issue: issue.clone(),
        },
        SupportTicketCreate::Order {
            order_id,
            source_version,
            title,
            issue,
            owner_id,
            due_date,
        } => TicketCreate::Order {
            order_id: order_id.clone(),
            source_version: *source_version,
            title: title.clone(),
            issue: issue.clone(),
            owner_id: owner_id.clone(),
            due_date: due_date.clone(),
        },
        SupportTicketCreate::Source {
            kind,
            id,
            version,
            title,
            issue,
        } => TicketCreate::Source {
            source: erpx_server_support::public::ticket::TicketSourceReference {
                kind: kind.clone(),
                id: id.clone(),
                version: *version,
            },
            title: title.clone(),
            issue: issue.clone(),
        },
    }
}

fn create_output(key: &str, status: &str) -> WorkflowOutput {
    WorkflowOutput {
        version: 1,
        result: CommandJson::from_serializable(&TicketResult {
            id: format!("ticket-{key}"),
            status: status.into(),
            version: 1,
        })
        .expect("serializable result"),
        event_scope: None,
    }
}

async fn apply_create(
    c: &mut PgConnection,
    key: &str,
    plan: &erpx_server_support::public::ticket::TicketCreatePlan,
    source: Option<&PreparedLeafSource>,
    order: Option<&OrderFacts>,
) -> Result<(), Error> {
    let event_id = format!("ticket-event-{key}");
    match (&plan.source_kind, order) {
        (SourceKind::SalesOrder, Some(order)) => {
            sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13)")
                .bind(&plan.id)
                .bind(&plan.company_id)
                .bind(&order.branch_id)
                .bind(&order.department_id)
                .bind(plan.order_id.as_deref())
                .bind(&order.customer_id)
                .bind(&plan.title)
                .bind(&plan.issue)
                .bind(plan.owner_id.as_deref())
                .bind(plan.due_date.as_deref())
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
                    "SUPPORT_OWNER_INELIGIBLE",
                    "Choose a currently eligible Support owner.",
                )
                .into());
            }
            sqlx::query("INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) SELECT $1,id,version,'create',$2,$3,owner_id,due_date,status,updated_at,$4::jsonb FROM support_tickets WHERE id=$5")
                .bind(&event_id)
                .bind(&plan.created_by)
                .bind(&plan.issue)
                .bind(serde_json::to_string(&serde_json::json!({"kind":"order"})).map_err(|_| changed())?)
                .bind(&plan.id)
                .execute(&mut *c)
                .await?;
            sqlx::query(TICKET_CREATED_NOTIFICATION)
                .bind(format!("ticket-notice-{key}"))
                .bind(&plan.id)
                .execute(&mut *c)
                .await?;
        }
        (_, None) => {
            let source_json = source.map(|source| {
                let record = source.record();
                serde_json::json!({
                    "kind": record.kind.as_str(),
                    "id": record.id,
                    "version": record.version,
                    "companyId": record.company_id,
                    "branchId": record.branch_id,
                    "departmentId": record.department_id,
                    "title": record.title,
                    "status": record.status,
                })
                .to_string()
            });
            sqlx::query("INSERT INTO support_tickets(id,company_id,branch_id,department_id,order_id,customer_id,title,issue,owner_id,due_date,status,source_kind,created_by,created_at,updated_at,source_record_json) VALUES($1,$2,$3,$4,NULL,NULL,$5,$6,NULL,NULL,'triage',$7,$8,$9,$10,$11)")
                .bind(&plan.id)
                .bind(&plan.company_id)
                .bind(&plan.branch_id)
                .bind(&plan.department_id)
                .bind(&plan.title)
                .bind(&plan.issue)
                .bind(plan.source_kind.as_str())
                .bind(&plan.created_by)
                .bind(plan.created_at)
                .bind(plan.updated_at)
                .bind(source_json)
                .execute(&mut *c)
                .await?;
            sqlx::query("INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) SELECT $1,id,version,'report',$2,$3,owner_id,due_date,status,updated_at,$4::jsonb FROM support_tickets WHERE id=$5")
                .bind(&event_id)
                .bind(&plan.created_by)
                .bind(&plan.issue)
                .bind(serde_json::to_string(&serde_json::json!({"kind":"report"})).map_err(|_| changed())?)
                .bind(&plan.id)
                .execute(&mut *c)
                .await?;
            sqlx::query(TICKET_REOPENED_NOTIFICATION)
                .bind(format!("ticket-triage-{key}-"))
                .bind(&plan.id)
                .execute(&mut *c)
                .await?;
        }
        _ => return Err(changed().into()),
    }
    Ok(())
}

async fn apply_change(
    c: &mut PgConnection,
    key: &str,
    snapshot: &TicketSnapshot,
    row: &TicketRow,
    plan: &TicketChangePlan,
    actor_id: &str,
) -> Result<(), Error> {
    let resetting = matches!(plan.action, "reopen" | "reject-resolution");
    let blocked = if plan.status == TicketStatus::WaitingCustomer {
        if plan.action == "reassign" {
            row.blocked_reason.clone()
        } else {
            Some(plan.reason.clone())
        }
    } else {
        None
    };
    let resolution = if plan.action == "resolve" {
        Some(plan.reason.clone())
    } else if resetting {
        None
    } else {
        row.resolution.clone()
    };
    let resolved_by = if plan.action == "resolve" {
        Some(actor_id.to_owned())
    } else if resetting {
        None
    } else {
        snapshot.resolved_by.clone()
    };
    let resolved_at = if plan.action == "resolve" {
        Some(plan.event_at)
    } else if resetting {
        None
    } else {
        row.resolved_at
    };
    let confirmation_note_id = if plan.action == "confirm" {
        plan.confirmation_note_id.clone()
    } else if plan.action == "reopen" {
        None
    } else {
        snapshot.confirmation_note_id.clone()
    };
    let confirmation_note_version = if plan.action == "confirm" {
        plan.confirmation_note_version
    } else if plan.action == "reopen" {
        None
    } else {
        row.confirmation_note_version
    };
    let confirmed_by = if plan.action == "confirm-general" {
        Some(actor_id.to_owned())
    } else if plan.action == "reopen" {
        None
    } else {
        row.confirmed_by.clone()
    };
    let confirmed_at = if plan.action == "confirm-general" {
        Some(plan.event_at)
    } else if plan.action == "reopen" {
        None
    } else {
        row.confirmed_at
    };
    let confirmation_reason = if plan.action == "confirm-general" {
        Some(plan.reason.clone())
    } else if plan.action == "reopen" {
        None
    } else {
        row.confirmation_reason.clone()
    };
    if plan.action == "confirm" {
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM customer_contact_notes WHERE id=$1 AND version=$2 AND order_id=$3 AND company_id=$4 AND customer_id=$5 AND status='active' AND occurred_at>=$6 AND created_at>=$6)")
            .bind(plan.confirmation_note_id.as_deref())
            .bind(plan.confirmation_note_version)
            .bind(row.order_id.as_deref())
            .bind(&plan.company_id)
            .bind(row.customer_id.as_deref())
            .bind(row.resolved_at)
            .fetch_one(&mut *c)
            .await?;
        if !exists {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "SUPPORT_CONFIRMATION_NOTE_CHANGED",
                "The customer confirmation changed. Reload before confirming.",
            )
            .into());
        }
    }
    let updated = sqlx::query("UPDATE support_tickets SET status=$1,owner_id=$2,due_date=$3,blocked_reason=$4,resolution=$5,resolved_by=$6,resolved_at=$7,confirmation_note_id=$8,confirmation_note_version=$9,confirmed_by=$10,confirmed_at=$11,confirmation_reason=$12,version=version+1,updated_at=$13 WHERE id=$14 AND company_id=$15 AND version=$16")
        .bind(plan.status.as_str())
        .bind(plan.owner_id.as_deref())
        .bind(plan.due_date.as_deref())
        .bind(blocked)
        .bind(resolution)
        .bind(resolved_by)
        .bind(resolved_at)
        .bind(confirmation_note_id)
        .bind(confirmation_note_version)
        .bind(confirmed_by)
        .bind(confirmed_at)
        .bind(confirmation_reason)
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
    if plan.action == "edit-report" {
        sqlx::query("UPDATE support_tickets SET title=$1,issue=$2 WHERE id=$3")
            .bind(plan.title.as_deref())
            .bind(plan.issue.as_deref())
            .bind(&plan.id)
            .execute(&mut *c)
            .await?;
    }
    sqlx::query("INSERT INTO support_ticket_events(id,ticket_id,version,action,actor_id,reason,owner_id,due_date,status,created_at,evidence_json) SELECT $1,id,version,$2,$3,$4,owner_id,due_date,status,updated_at,$5::jsonb FROM support_tickets WHERE id=$6")
        .bind(format!("ticket-event-{key}"))
        .bind(plan.action)
        .bind(actor_id)
        .bind(&plan.reason)
        .bind(serde_json::to_string(&serde_json::json!({"action":plan.action})).map_err(|_| changed())?)
        .bind(&plan.id)
        .execute(&mut *c)
        .await?;
    if plan.action != "cancel" {
        sqlx::query(TICKET_UPDATED_NOTIFICATION)
            .bind(format!("ticket-notice-{key}"))
            .bind(&plan.id)
            .execute(&mut *c)
            .await?;
    }
    if plan.action == "resolve" {
        sqlx::query(TICKET_REVIEWERS_NOTIFICATION)
            .bind(format!("ticket-review-{key}-"))
            .bind(&plan.id)
            .execute(&mut *c)
            .await?;
    }
    Ok(())
}

async fn load_ticket(
    c: &mut PgConnection,
    company: &CompanyId,
    id: &str,
) -> Result<TicketRow, Error> {
    let row=sqlx::query(
        "SELECT id,company_id,branch_id,department_id,source_kind,status,owner_id,due_date,resolved_by,created_by,confirmation_note_id,version,order_id,customer_id,resolved_at,blocked_reason,resolution,confirmation_note_version,confirmed_by,confirmed_at,confirmation_reason,sla_clock_json::text,source_record_json FROM support_tickets WHERE id=$1 AND company_id=$2",
    )
    .bind(id)
    .bind(company.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some(row) = row else {
        return Err(BusinessError::new(
            FailureKind::NotFound,
            "SUPPORT_TICKET_NOT_FOUND",
            "Ticket not found in this scope.",
        )
        .into());
    };
    use sqlx::Row;
    let id:String=row.try_get("id")?;
    let company_id:String=row.try_get("company_id")?;
    let branch_id:Option<String>=row.try_get("branch_id")?;
    let department_id:Option<String>=row.try_get("department_id")?;
    let source_kind:String=row.try_get("source_kind")?;
    let status:String=row.try_get("status")?;
    let owner_id:Option<String>=row.try_get("owner_id")?;
    let due_date:Option<String>=row.try_get("due_date")?;
    let resolved_by:Option<String>=row.try_get("resolved_by")?;
    let created_by:String=row.try_get("created_by")?;
    let confirmation_note_id:Option<String>=row.try_get("confirmation_note_id")?;
    let version:i64=row.try_get("version")?;
    let order_id:Option<String>=row.try_get("order_id")?;
    let customer_id:Option<String>=row.try_get("customer_id")?;
    let resolved_at:Option<i64>=row.try_get("resolved_at")?;
    let blocked_reason:Option<String>=row.try_get("blocked_reason")?;
    let resolution:Option<String>=row.try_get("resolution")?;
    let confirmation_note_version:Option<i64>=row.try_get("confirmation_note_version")?;
    let confirmed_by:Option<String>=row.try_get("confirmed_by")?;
    let confirmed_at:Option<i64>=row.try_get("confirmed_at")?;
    let confirmation_reason:Option<String>=row.try_get("confirmation_reason")?;
    let sla_clock_json:Option<String>=row.try_get("sla_clock_json")?;
    let source_record_json:Option<String>=row.try_get("source_record_json")?;
    let source_kind = match source_kind.as_str() {
        "sales_order" => SourceKind::SalesOrder,
        "general" => SourceKind::General,
        "pos_sale" => SourceKind::PosSale,
        "pms_record" => SourceKind::PmsRecord,
        "asset" => SourceKind::Asset,
        "field_service" => SourceKind::FieldService,
        _ => return Err(changed().into()),
    };
    let status = match status.as_str() {
        "triage" => TicketStatus::Triage,
        "open" => TicketStatus::Open,
        "in_progress" => TicketStatus::InProgress,
        "waiting_customer" => TicketStatus::WaitingCustomer,
        "resolved" => TicketStatus::Resolved,
        "closed" => TicketStatus::Closed,
        "cancelled" => TicketStatus::Cancelled,
        _ => return Err(changed().into()),
    };
    Ok(TicketRow {
        snapshot: TicketSnapshot {
            id,
            company_id,
            branch_id,
            department_id,
            source_kind,
            status,
            owner_id,
            due_date,
            resolved_by,
            created_by,
            confirmation_note_id,
            version,
        },
        order_id,
        customer_id,
        resolved_at,
        blocked_reason,
        resolution,
        confirmation_note_version,
        confirmed_by,
        confirmed_at,
        confirmation_reason,
        _sla_clock_json: sla_clock_json,
        source_record_json,
    })
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "SUPPORT_TICKET_CHANGED",
        "The Support ticket changed. Reload before retrying.",
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

