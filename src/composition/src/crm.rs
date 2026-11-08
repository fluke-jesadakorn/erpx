//! Native CRM pipeline read assembly. Current Account/Holding authority remains
//! with the selected coordinator; owner ports supply the bounded pipeline,
//! staff and customer labels and package entitlement facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_collaboration::public::crm_activity_notification::{
    CrmLeadNotificationFacts, PgCrmLeadNotificationSource, RetainedCrmLeadNotification,
    lock_crm_activity_notifications, prepare_crm_lead_notification,
};
use erpx_server_commerce::public::order::{
    CommerceOrderActor, CommerceOrderReadRepository, PgCommerceOrders, SalesOrderRow,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, CommercialCatalogue, PackageEnforcementMode, PgCompanyPurchaseReader,
    PurchaseRequest, PurchaseWitness, current_package_catalogue,
    lock_commercial_authority_sources,
};
use erpx_server_crm::public::activity_source::{
    CrmReviewedAccountLinkRequest, inspect_crm_reviewed_account_link,
    lock_crm_reviewed_account_link_sources,
};
use erpx_server_crm::public::leads_http::{
    CrmLeadCommandHttpBackend, CrmLeadCommandHttpFuture, CrmLeadCommandHttpRequest,
    CrmLeadCommandHttpTarget, crm_lead_command_http_router,
};
use erpx_server_crm::public::leads_native::{
    ChangedCrmLeadFacts, ChangedCrmOpportunityFacts, LeadActor, LeadChangeInput, LeadChangeReceipt,
    OpportunityChangeInput, OpportunityChangeReceipt, PgCrmLeadCommands, PreparedLeadChange,
    PreparedOpportunityChange, RetainedCrmLeadChange, RetainedCrmOpportunityChange,
    lead_operation, native_lead_action, native_opportunity_action, not_native,
    opportunity_operation,
};
use erpx_server_crm::public::pipeline::{
    CrmPipelineActor, CrmPipelineCustomerDirectory, CrmPipelineEntitlement,
    CrmPipelineEntitlementPort, CrmPipelinePage, CrmPipelinePeopleDirectory, CrmPipelineQuery,
    PgCrmPipelineRead, crm_pipeline,
};
use erpx_server_crm::public::pipeline_http::{
    CrmPipelineHttpBackend, CrmPipelineHttpFuture, CrmPipelineHttpReadRequest,
    CrmPipelineHttpResponse,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_sales::public::sales_recipient::{
    CurrentSalesRecipient, SalesRecipientRequest, SalesRecipientSourceLocks,
    inspect_sales_recipient, prepare_sales_recipient_sources,
};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId,
    IdentifierSource, RecordId, TimestampMillis,
};
use erpx_server_workflow::public::commands::{
    CommandActor, CommandDisposition, CommandJson, CommandOutcome, PgCommandAuthority,
    PgCommandLedger, PgOwnerCommandEffects, WorkflowOutput, WorkflowRequest,
};
use serde_json::Value;
use sqlx::PgConnection;
use std::collections::BTreeMap;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CRM_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CRM_UNAVAILABLE",
        "The CRM pipeline is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct CrmSources;

impl HoldingWorkSourcePreparation for CrmSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

struct CrmPeopleDirectoryImpl;

impl CrmPipelinePeopleDirectory for CrmPeopleDirectoryImpl {
    type Error = BusinessError;

    fn names<'a>(
        &'a mut self,
        _company_id: &'a str,
        _user_ids: &'a [String],
    ) -> erpx_server_crm::public::pipeline::CrmPipelineFuture<
        'a,
        BTreeMap<String, String>,
        Self::Error,
    > {
        Box::pin(async { Ok(BTreeMap::new()) })
    }
}

struct CrmCustomerDirectoryImpl;

impl CrmPipelineCustomerDirectory for CrmCustomerDirectoryImpl {
    type Error = BusinessError;

    fn names<'a>(
        &'a mut self,
        _company_id: &'a str,
        _user_ids: &'a [String],
    ) -> erpx_server_crm::public::pipeline::CrmPipelineFuture<
        'a,
        BTreeMap<String, String>,
        Self::Error,
    > {
        Box::pin(async { Ok(BTreeMap::new()) })
    }
}

struct CrmEntitlementImpl;

impl CrmPipelineEntitlementPort for CrmEntitlementImpl {
    type Error = BusinessError;

    fn entitlement<'a>(
        &'a mut self,
        _company_id: &'a str,
    ) -> erpx_server_crm::public::pipeline::CrmPipelineFuture<'a, CrmPipelineEntitlement, Self::Error>
    {
        Box::pin(async {
            Ok(CrmPipelineEntitlement {
                enforced: false,
                history_allowed: false,
                new_work: false,
                settle_existing: false,
                revision: String::new(),
            })
        })
    }
}

#[derive(Clone)]
pub struct CrmConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl CrmConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn crm_actor(current: &AuthorizedWorkContext) -> Result<CrmPipelineActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(CrmPipelineActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    })
}

fn context(current: &AuthorizedWorkContext) -> Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    serde_json::json!({
        "userId": current.user_id().as_str(),
        "holdingId": current.holding_id().as_str(),
        "companyId": workspace.company_id.as_str(),
        "branchId": workspace.branch_id.as_ref().map(|value| value.as_str()),
        "departmentId": workspace.department_id.as_ref().map(|value| value.as_str()),
        "branchSlug": workspace.branch_slug,
        "departmentSlug": workspace.department_slug,
        "assignmentId": assignment.id.as_str(),
        "assignmentVersion": assignment.version,
        "revision": selected.revision(),
        "role": assignment.role,
        "assignmentScopeKind": match assignment.scope_kind {
            WorkScopeKind::Holding => "holding",
            WorkScopeKind::Branch => "branch",
            WorkScopeKind::Department => "department",
        },
        "modules": current.modules(),
        "permissions": current.command_iam_metadata().permissions,
        "expiresAt": current.expires_at(),
    })
}

pub struct CrmService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: CrmConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> CrmService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: CrmConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "commerce.read".into(),
            module: Some("crm".into()),
            destination: "crm".into(),
            api: true,
        }
    }

    fn input<Input>(
        &self,
        input: Input,
    ) -> impl FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let company = self.configuration.company.clone();
        move || {
            let mut request = input()?;
            if request.company_id != company {
                return Err(scope());
            }
            request.mutation = false;
            Ok(request)
        }
    }

    pub async fn read<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        query: CrmPipelineQuery,
    ) -> Result<CrmPipelineHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                CrmSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let query = query.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = crm_actor(current)?;
                        let mut repository = PgCrmPipelineRead::new(connection);
                        let mut people = CrmPeopleDirectoryImpl;
                        let mut customers = CrmCustomerDirectoryImpl;
                        let mut entitlement = CrmEntitlementImpl;
                        let now = clock.now();
                        let page: CrmPipelinePage = crm_pipeline(
                            &actor,
                            &query,
                            &mut repository,
                            &mut people,
                            &mut customers,
                            &mut entitlement,
                            now,
                        )
                        .await?;
                        let data = serde_json::to_value(page).map_err(|_| unavailable())?;
                        Ok(CrmPipelineHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

pub struct CrmBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<CrmService<C>>,
}

impl<C: Clock + Send + Sync + 'static> CrmBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<CrmService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: None,
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> CrmPipelineHttpBackend for CrmBackend<C> {
    fn read<'a>(
        &'a self,
        request: CrmPipelineHttpReadRequest<'a>,
    ) -> CrmPipelineHttpFuture<'a, CrmPipelineHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let query = request.query.clone();
            self.service
                .read(request.host, proof, move || Ok(selection), query)
                .await
                .map_err(business)
        })
    }
}

#[allow(dead_code)]
fn _router(
    targets: Vec<erpx_server_crm::public::pipeline_http::CrmPipelineHttpTarget>,
) -> Result<axum::Router, BusinessError> {
    erpx_server_crm::public::pipeline_http::crm_pipeline_http_router(targets)
}

// ---------------------------------------------------------------------------
// Native lead / opportunity commands
// ---------------------------------------------------------------------------

const CRM_LEAD_NOTICE: &str = "CRM handoff changed: review the source and next action";

fn command_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "CRM_COMMAND_CHANGED",
        "The CRM command changed. Reload and review before submitting.",
    )
}

fn recipient_required() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "CRM_RECIPIENT_REQUIRED",
        "The selected salesperson is not an eligible Sales recipient.",
    )
}

fn lead_scope_required() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "CRM_SOURCE_NOT_FOUND",
        "The original CRM source is unavailable in this workspace.",
    )
}

#[derive(Clone)]
pub enum CrmLeadCommand {
    Lead {
        key: CommandKey,
        lead_id: String,
        input: LeadChangeInput,
    },
    Opportunity {
        key: CommandKey,
        lead_id: String,
        input: OpportunityChangeInput,
    },
}

impl CrmLeadCommand {
    fn key(&self) -> &CommandKey {
        match self {
            Self::Lead { key, .. } | Self::Opportunity { key, .. } => key,
        }
    }
    fn lead_id(&self) -> &str {
        match self {
            Self::Lead { lead_id, .. } | Self::Opportunity { lead_id, .. } => lead_id,
        }
    }
    fn operation(&self) -> String {
        match self {
            Self::Lead { input, .. } => lead_operation(&input.action),
            Self::Opportunity { input, .. } => opportunity_operation(&input.action),
        }
    }
    fn purpose(&self) -> CapabilityPurpose {
        match self {
            Self::Lead { input, .. } => match input.action.as_str() {
                "reopen" => CapabilityPurpose::NewWork,
                _ => CapabilityPurpose::SettleExisting,
            },
            Self::Opportunity { input, .. } => match input.action.as_str() {
                "reopen" => CapabilityPurpose::NewWork,
                _ => CapabilityPurpose::SettleExisting,
            },
        }
    }
    fn extra_permission(&self) -> Option<&'static str> {
        match self {
            Self::Lead { input, .. } if matches!(input.action.as_str(), "reassign" | "reopen") => {
                Some("commerce.approve")
            }
            Self::Opportunity { input, .. } if input.action == "reopen" => Some("commerce.approve"),
            _ => None,
        }
    }
    fn request(&self) -> Result<WorkflowRequest, BusinessError> {
        let input = match self {
            Self::Lead { input, .. } => CommandJson::from_serializable(input)?,
            Self::Opportunity { input, .. } => CommandJson::from_serializable(input)?,
        };
        Ok(WorkflowRequest {
            key: self.key().as_str().to_owned(),
            operation: self.operation(),
            record_id: self.lead_id().to_owned(),
            input,
        })
    }
}

fn actor_for(current: &AuthorizedWorkContext) -> Result<LeadActor, BusinessError> {
    let workspace = current.selection().workspace();
    Ok(LeadActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.as_str().to_owned(),
        can_read: current.has_permission("commerce.read"),
        can_sales: current.has_permission("commerce.sales"),
        can_manage: current.has_permission("commerce.approve"),
        can_pms: current.has_permission("pms.read"),
        can_support: current.has_permission("support.read"),
        same_person_ids: Vec::new(),
    })
}

fn require_lead_scope(
    current: &AuthorizedWorkContext,
    branch_id: &str,
    department_id: Option<&str>,
) -> Result<(), BusinessError> {
    let workspace = current.selection().workspace();
    if workspace
        .branch_id
        .as_ref()
        .is_some_and(|value| value.as_str() != branch_id)
        || workspace
            .department_id
            .as_ref()
            .is_some_and(|value| Some(value.as_str()) != department_id)
    {
        return Err(lead_scope_required());
    }
    Ok(())
}


fn commerce_actor(current: &AuthorizedWorkContext) -> CommerceOrderActor {
    let selection = current.selection();
    let workspace = selection.workspace();
    let assignment = selection.assignment();
    CommerceOrderActor {
        user_id: current.user_id().clone(),
        person_user_ids: current.command_iam_metadata().person_user_ids.clone(),
        holding_id: current.holding_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        assignment_id: assignment.id.clone(),
        role: Some(assignment.role.clone()),
        authority_revision: Some(selection.revision()),
        assignment_version: Some(assignment.version),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    }
}

async fn current_commerce_order(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    id: &str,
    now: i64,
) -> Result<SalesOrderRow, Error> {
    let actor = commerce_actor(current);
    let mut orders = PgCommerceOrders::new(c);
    if !orders
        .current_authority(&actor, false, false, TimestampMillis::new(now))
        .await?
    {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "CRM_COMMERCE_AUTHORITY_REQUIRED",
            "Your current role cannot read the linked Commerce document.",
        )
        .into());
    }
    orders
        .order(&actor, id)
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::NotFound,
                "CRM_SOURCE_NOT_FOUND",
                "The linked customer document is unavailable in this workspace.",
            )
            .into()
        })
}

#[allow(clippy::too_many_arguments)]
fn require_commerce_order(
    order: &SalesOrderRow,
    kind: &str,
    states: &[&str],
    customer: &str,
    branch: &str,
    department: Option<&str>,
    currency: &str,
    version: Option<i64>,
) -> Result<(), BusinessError> {
    if order.kind != kind
        || !states.contains(&order.state.as_str())
        || order.party_id != customer
        || order.branch_id.as_deref() != Some(branch)
        || order.department_id.as_deref() != department
        || order.currency != currency
        || version.is_some_and(|value| order.version != value)
    {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "CRM_SOURCE_CHANGED",
            "The linked customer document changed. Reload and review before submitting.",
        ));
    }
    Ok(())
}

struct Authority<'a, G> {
    current: &'a AuthorizedWorkContext,
    command: &'a CrmLeadCommand,
    mode: PackageEnforcementMode,
    clock: &'a G,
    catalogue: CommercialCatalogue,
    purchase: Option<PurchaseWitness>,
}

impl<G: Clock + Sync> Authority<'_, G> {
    fn bind(&self, request: &WorkflowRequest) -> Result<CommandActor, BusinessError> {
        self.current.require_permission("commerce.sales")?;
        if let Some(permission) = self.command.extra_permission() {
            self.current.require_permission(permission)?;
        }
        let expected = self.command.request()?;
        if !self
            .current
            .modules()
            .iter()
            .any(|module| module == "crm")
            || self.clock.now().get() >= self.current.expires_at()
            || request.key != expected.key
            || request.operation != expected.operation
            || request.record_id != expected.record_id
            || request.input.as_str() != expected.input.as_str()
        {
            return Err(command_changed());
        }
        crate::maintenance_parts::command_actor(self.current)
    }
}

impl<G: Clock + Sync> PgCommandAuthority for Authority<'_, G> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        lock_commercial_authority_sources(c).await
    }

    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        request: &WorkflowRequest,
    ) -> Result<CommandActor, Error> {
        Ok(self.bind(request)?)
    }

    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        request: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), Error> {
        if &self.bind(request)? != actor {
            return Err(command_changed().into());
        }
        let now = self.clock.now();
        let mut purchases = PgCompanyPurchaseReader::new(
            self.current.holding_id().clone(),
            actor.company_id.clone(),
            c,
            &self.catalogue,
        );
        let purpose = match disposition {
            CommandDisposition::Execute => self.command.purpose(),
            CommandDisposition::Replay => CapabilityPurpose::History,
        };
        if let Some(witness) = &self.purchase {
            purchases.revalidate(witness, now).await?.require_allowed()?;
        } else {
            self.purchase = Some(
                purchases
                    .inspect(
                        &PurchaseRequest {
                            capabilities: vec!["sales.crm".into()],
                            purpose,
                            mode: self.mode,
                        },
                        now,
                    )
                    .await?
                    .require_allowed()?
                    .clone(),
            );
        }
        Ok(())
    }
}

struct LeadNotificationSource {
    facts: CrmLeadNotificationFacts,
}

impl PgCrmLeadNotificationSource for LeadNotificationSource {
    fn facts(&self) -> &CrmLeadNotificationFacts {
        &self.facts
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM staff_notifications WHERE id=$1 OR (user_id=$2 AND record_type='crm_lead' AND record_id=$3 AND record_version=$4 AND message=$5))",
        )
        .bind(format!("crm-notice-{}", self.facts.command_key))
        .bind(&self.facts.recipient)
        .bind(&self.facts.lead_id)
        .bind(self.facts.version)
        .bind(CRM_LEAD_NOTICE)
        .fetch_one(&mut *c)
        .await?;
        if exists {
            return Err(command_changed().into());
        }
        Ok(())
    }
    async fn require_after(&self, c: &mut PgConnection) -> Result<(), Error> {
        let row: Option<(String, String, i64, i64)> = sqlx::query_as(
            "SELECT owner_id,status,version,updated_at FROM crm_leads WHERE id=$1 AND company_id=$2",
        )
        .bind(&self.facts.lead_id)
        .bind(&self.facts.company_id)
        .fetch_optional(&mut *c)
        .await?;
        if row.as_ref().is_none_or(|(owner, _, version, updated_at)| {
            owner != &self.facts.recipient
                || *version != self.facts.version
                || *updated_at != self.facts.updated_at
        }) {
            return Err(command_changed().into());
        }
        Ok(())
    }
}

fn lead_notification_facts(facts: &ChangedCrmLeadFacts) -> CrmLeadNotificationFacts {
    CrmLeadNotificationFacts {
        command_key: facts.command_key.clone(),
        company_id: facts.company_id.clone(),
        recipient: facts.owner_id.clone(),
        lead_id: facts.lead_id.clone(),
        version: facts.version,
        updated_at: facts.updated_at,
    }
}

fn opportunity_notification_facts(
    facts: &ChangedCrmOpportunityFacts,
) -> CrmLeadNotificationFacts {
    CrmLeadNotificationFacts {
        command_key: facts.command_key.clone(),
        company_id: facts.company_id.clone(),
        recipient: facts.owner_id.clone(),
        lead_id: facts.lead_id.clone(),
        version: facts.lead_version,
        updated_at: facts.updated_at,
    }
}

enum Prepared {
    Lead(PreparedLeadChange),
    Opportunity(PreparedOpportunityChange),
}

enum Written {
    Lead {
        retained: RetainedCrmLeadChange,
        notification: RetainedCrmLeadNotification,
    },
    Opportunity {
        retained: RetainedCrmOpportunityChange,
        notification: RetainedCrmLeadNotification,
    },
}

struct Effects<'a, W> {
    current: &'a AuthorizedWorkContext,
    command: &'a CrmLeadCommand,
    write_clock: &'a W,
    recipient_locks: Option<SalesRecipientSourceLocks>,
    recipient: Option<CurrentSalesRecipient>,
    prepared: Option<Prepared>,
    written: Option<Written>,
}

impl<W: Clock + Sync> Effects<'_, W> {
    async fn inspect_recipient(
        &mut self,
        c: &mut PgConnection,
        user_id: &str,
        branch_id: &str,
        department_id: Option<&str>,
    ) -> Result<(), Error> {
        let locks = self
            .recipient_locks
            .as_ref()
            .ok_or_else(|| command_changed())?;
        let recipient = inspect_sales_recipient(
            c,
            locks,
            SalesRecipientRequest {
                company_id: self.current.selection().workspace().company_id.clone(),
                user_id: user_id.to_owned(),
                branch_id: branch_id.to_owned(),
                department_id: department_id.map(str::to_owned),
            },
        )
        .await?;
        if !recipient.is_eligible() {
            return Err(recipient_required().into());
        }
        self.recipient = Some(recipient);
        Ok(())
    }
}

impl<W: Clock + Sync> PgOwnerCommandEffects for Effects<'_, W> {
    async fn lock_sources(
        &mut self,
        c: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), Error> {
        lock_commercial_authority_sources(c).await?;
        let mut preparation = prepare_sales_recipient_sources(c).await?;
        erpx_server_crm::public::activity_source::lock_crm_activity_sources(
            preparation.connection(),
        )
        .await?;
        lock_crm_activity_notifications(preparation.connection()).await?;
        lock_crm_reviewed_account_link_sources(preparation.connection()).await?;
        self.recipient_locks = Some(preparation.bind().await?);
        Ok(())
    }

    async fn prepare(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, Error> {
        let actor = actor_for(self.current)?;
        let now = self.write_clock.now().get();
        match self.command {
            CrmLeadCommand::Lead {
                key: _,
                lead_id,
                input,
            } => {
                if !native_lead_action(&input.action) {
                    return Err(not_native(&input.action).into());
                }
                let mut commands = PgCrmLeadCommands::new(c);
                let prepared = commands.prepare_lead_change(&actor, lead_id, input).await?;
                require_lead_scope(
                    self.current,
                    &prepared.record.branch_id,
                    prepared.record.department_id.as_deref(),
                )?;
                let recipient = match input.action.as_str() {
                    "reassign" => prepared
                        .plan
                        .owner_id
                        .as_deref()
                        .ok_or_else(recipient_required)?,
                    "disqualify" => prepared.record.owner_id.as_str(),
                    _ => "",
                };
                if !recipient.is_empty() {
                    self.inspect_recipient(
                        c,
                        recipient,
                        &prepared.record.branch_id,
                        prepared.record.department_id.as_deref(),
                    )
                    .await?;
                }
                let receipt = LeadChangeReceipt {
                    id: lead_id.clone(),
                    version: prepared.plan.expected_version + 1,
                    opportunity_id: None,
                };
                let result = CommandJson::from_serializable(&receipt)?;
                let version = receipt.version;
                self.prepared = Some(Prepared::Lead(prepared));
                Ok(WorkflowOutput {
                    version,
                    result,
                    event_scope: None,
                })
            }
            CrmLeadCommand::Opportunity {
                key: _,
                lead_id,
                input,
            } => {
                if !native_opportunity_action(&input.action) {
                    return Err(not_native(&input.action).into());
                }
                let mut commands = PgCrmLeadCommands::new(c);
                let mut prepared = commands
                    .prepare_opportunity_change(&actor, lead_id, input)
                    .await?;
                require_lead_scope(
                    self.current,
                    &prepared.lead.branch_id,
                    prepared.lead.department_id.as_deref(),
                )?;
                if matches!(input.action.as_str(), "link-quote" | "negotiate" | "win") {
                    self.inspect_recipient(
                        c,
                        &prepared.lead.owner_id,
                        &prepared.lead.branch_id,
                        prepared.lead.department_id.as_deref(),
                    )
                    .await?;
                    let workspace = self.current.selection().workspace();
                    let link = inspect_crm_reviewed_account_link(
                        c,
                        CrmReviewedAccountLinkRequest {
                            company_id: workspace.company_id.clone(),
                            branch_id: Some(prepared.lead.branch_id.clone()),
                            department_id: prepared.lead.department_id.clone(),
                            lead_id: lead_id.clone(),
                            finance_party_id: prepared.opportunity.customer_id.clone(),
                        },
                    )
                    .await?;
                    link.require_current(c).await?;
                    match input.action.as_str() {
                        "link-quote" => {
                            let quote_id = prepared.plan.quote_id.as_deref().ok_or_else(|| {
                                BusinessError::invalid(
                                    "CRM_OPPORTUNITY_INPUT_INVALID",
                                    "Use a quote.",
                                )
                            })?;
                            let quote =
                                current_commerce_order(c, self.current, quote_id, now).await?;
                            require_commerce_order(
                                &quote,
                                "quote",
                                &["confirmed"],
                                &prepared.opportunity.customer_id,
                                &prepared.lead.branch_id,
                                prepared.lead.department_id.as_deref(),
                                &prepared.opportunity.currency,
                                prepared.plan.quote_version,
                            )?;
                            prepared.reference_minor = Some(quote.subtotal_minor);
                        }
                        "negotiate" => {
                            let quote_id =
                                prepared.opportunity.quote_id.as_deref().ok_or_else(|| {
                                    BusinessError::new(
                                        FailureKind::Conflict,
                                        "CRM_QUOTATION_REQUIRED",
                                        "Link an approved quotation first.",
                                    )
                                })?;
                            let quote =
                                current_commerce_order(c, self.current, quote_id, now).await?;
                            require_commerce_order(
                                &quote,
                                "quote",
                                &["confirmed"],
                                &prepared.opportunity.customer_id,
                                &prepared.lead.branch_id,
                                prepared.lead.department_id.as_deref(),
                                &prepared.opportunity.currency,
                                prepared.opportunity.quote_version,
                            )?;
                        }
                        "win" => {
                            let sales_id =
                                prepared.plan.sales_id.as_deref().ok_or_else(|| {
                                    BusinessError::invalid(
                                        "CRM_OPPORTUNITY_INPUT_INVALID",
                                        "Use a sales order.",
                                    )
                                })?;
                            let sale =
                                current_commerce_order(c, self.current, sales_id, now).await?;
                            require_commerce_order(
                                &sale,
                                "sales",
                                &["confirmed", "partially_fulfilled", "fulfilled"],
                                &prepared.opportunity.customer_id,
                                &prepared.lead.branch_id,
                                prepared.lead.department_id.as_deref(),
                                &prepared.opportunity.currency,
                                prepared.plan.sales_version,
                            )?;
                            if sale.source_id.as_deref()
                                != prepared.opportunity.quote_id.as_deref()
                            {
                                return Err(BusinessError::new(
                                    FailureKind::Conflict,
                                    "CRM_SOURCE_CHANGED",
                                    "The confirmed sale is not converted from the linked quotation.",
                                )
                                .into());
                            }
                            let quote_id =
                                prepared.opportunity.quote_id.as_deref().ok_or_else(|| {
                                    BusinessError::new(
                                        FailureKind::Conflict,
                                        "CRM_QUOTATION_REQUIRED",
                                        "Link an approved quotation first.",
                                    )
                                })?;
                            let quote =
                                current_commerce_order(c, self.current, quote_id, now).await?;
                            require_commerce_order(
                                &quote,
                                "quote",
                                &["converted"],
                                &prepared.opportunity.customer_id,
                                &prepared.lead.branch_id,
                                prepared.lead.department_id.as_deref(),
                                &prepared.opportunity.currency,
                                None,
                            )?;
                            prepared.reference_minor = Some(sale.subtotal_minor);
                        }
                        _ => {}
                    }
                } else if input.action == "lose" {
                    self.inspect_recipient(
                        c,
                        &prepared.lead.owner_id,
                        &prepared.lead.branch_id,
                        prepared.lead.department_id.as_deref(),
                    )
                    .await?;
                }
                let receipt = OpportunityChangeReceipt {
                    id: lead_id.clone(),
                    version: prepared.plan.expected_version + 1,
                    lead_version: prepared.plan.lead_version + 1,
                    stage: prepared.plan.stage.clone(),
                };
                let result = CommandJson::from_serializable(&receipt)?;
                let version = receipt.version;
                self.prepared = Some(Prepared::Opportunity(prepared));
                Ok(WorkflowOutput {
                    version,
                    result,
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
        let now = self.write_clock.now().get();
        match self.prepared.take().ok_or_else(command_changed)? {
            Prepared::Lead(prepared) => {
                let command_key = self.command.key().as_str().to_owned();
                let actor = actor_for(self.current)?;
                let retained = PgCrmLeadCommands::new(c)
                    .apply_lead_change(&actor, self.command.lead_id(), &command_key, prepared, now)
                    .await?;
                let facts = lead_notification_facts(retained.facts());
                let source = LeadNotificationSource { facts };
                let notification = prepare_crm_lead_notification(c, &source)
                    .await?
                    .persist(c)
                    .await?;
                self.written = Some(Written::Lead {
                    retained,
                    notification,
                });
            }
            Prepared::Opportunity(prepared) => {
                let command_key = self.command.key().as_str().to_owned();
                let actor = actor_for(self.current)?;
                let retained = PgCrmLeadCommands::new(c)
                    .apply_opportunity_change(
                        &actor,
                        self.command.lead_id(),
                        &command_key,
                        prepared,
                        now,
                    )
                    .await?;
                let facts = opportunity_notification_facts(retained.facts());
                let source = LeadNotificationSource { facts };
                let notification = prepare_crm_lead_notification(c, &source)
                    .await?
                    .persist(c)
                    .await?;
                self.written = Some(Written::Opportunity {
                    retained,
                    notification,
                });
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
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *c)
            .await?;
        match self.written.as_ref().ok_or_else(command_changed)? {
            Written::Lead {
                retained,
                notification,
            } => {
                retained.require_current(c).await?;
                let source = LeadNotificationSource {
                    facts: lead_notification_facts(retained.facts()),
                };
                notification.require_current(c, &source).await?;
            }
            Written::Opportunity {
                retained,
                notification,
            } => {
                retained.require_current(c).await?;
                let source = LeadNotificationSource {
                    facts: opportunity_notification_facts(retained.facts()),
                };
                notification.require_current(c, &source).await?;
            }
        }
        Ok(())
    }
}


/// Execute one native CRM owner command in the caller's retained unit.
pub async fn execute_crm_command<W: Clock + Sync, G: Clock + Sync, I: IdentifierSource>(
    c: &mut PgConnection,
    current: &AuthorizedWorkContext,
    command: &CrmLeadCommand,
    mode: PackageEnforcementMode,
    write_clock: &W,
    guard_clock: &G,
    identifiers: &mut I,
) -> Result<CommandOutcome, Error> {
    let request = command.request()?;
    let mut authority = Authority {
        current,
        command,
        mode,
        clock: guard_clock,
        catalogue: current_package_catalogue()?,
        purchase: None,
    };
    let mut effects = Effects {
        current,
        command,
        write_clock,
        recipient_locks: None,
        recipient: None,
        prepared: None,
        written: None,
    };
    PgCommandLedger::new(
        current.selection().workspace().company_id.clone(),
        c,
        &mut authority,
        write_clock,
        identifiers,
    )
    .execute(&request, &mut effects)
    .await
}

#[derive(Clone, Copy)]
struct CrmCommandSources;

impl HoldingWorkSourcePreparation for CrmCommandSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c).await?;
            let mut preparation = prepare_sales_recipient_sources(c).await?;
            erpx_server_crm::public::activity_source::lock_crm_activity_sources(
                preparation.connection(),
            )
            .await?;
            lock_crm_activity_notifications(preparation.connection()).await?;
            lock_crm_reviewed_account_link_sources(preparation.connection()).await?;
            preparation.bind().await?;
            Ok(())
        })
    }
}

struct CrmCommandIds;
impl IdentifierSource for CrmCommandIds {
    fn next(&mut self) -> RecordId {
        RecordId::new(uuid::Uuid::new_v4().to_string()).expect("generated UUID")
    }
}

pub struct CrmCommandService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: CrmConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> CrmCommandService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: CrmConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    pub fn company(&self) -> &CompanyId {
        self.configuration.company()
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "commerce.sales".into(),
            module: Some("crm".into()),
            destination: "crm".into(),
            api: true,
        }
    }

    fn verify(&self, selection: &HoldingWorkspaceRequest) -> Result<(), BusinessError> {
        if selection.company_id != self.configuration.company {
            return Err(scope());
        }
        Ok(())
    }

    pub async fn change_lead(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        key: CommandKey,
        lead_id: String,
        input: LeadChangeInput,
    ) -> Result<Value, Error> {
        self.verify(&selection)?;
        selection.mutation = true;
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                Self::access(),
                CrmCommandSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let lead_id = lead_id.clone();
                    let input = input.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let command = CrmLeadCommand::Lead {
                            key,
                            lead_id,
                            input,
                        };
                        let mut identifiers = CrmCommandIds;
                        let outcome = execute_crm_command(
                            connection,
                            current,
                            &command,
                            PackageEnforcementMode::Enforced,
                            clock.as_ref(),
                            clock.as_ref(),
                            &mut identifiers,
                        )
                        .await?;
                        serde_json::from_str(outcome.result.as_str()).map_err(|_| {
                            BusinessError::new(
                                FailureKind::Unavailable,
                                "CRM_COMMAND_RESULT_INVALID",
                                "The CRM command result is unavailable.",
                            )
                            .into()
                        })
                    })
                },
            )
            .await
    }

    pub async fn change_opportunity(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        key: CommandKey,
        lead_id: String,
        input: OpportunityChangeInput,
    ) -> Result<Value, Error> {
        self.verify(&selection)?;
        selection.mutation = true;
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                Self::access(),
                CrmCommandSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let lead_id = lead_id.clone();
                    let input = input.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let command = CrmLeadCommand::Opportunity {
                            key,
                            lead_id,
                            input,
                        };
                        let mut identifiers = CrmCommandIds;
                        let outcome = execute_crm_command(
                            connection,
                            current,
                            &command,
                            PackageEnforcementMode::Enforced,
                            clock.as_ref(),
                            clock.as_ref(),
                            &mut identifiers,
                        )
                        .await?;
                        serde_json::from_str(outcome.result.as_str()).map_err(|_| {
                            BusinessError::new(
                                FailureKind::Unavailable,
                                "CRM_COMMAND_RESULT_INVALID",
                                "The CRM command result is unavailable.",
                            )
                            .into()
                        })
                    })
                },
            )
            .await
    }
}

pub struct CrmCommandBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<CrmCommandService<C>>,
}

impl<C: Clock + Send + Sync + 'static> CrmCommandBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<CrmCommandService<C>>,
    ) -> Self {
        Self {
            policy,
            scopes,
            service,
        }
    }

    fn command_context(
        &self,
        headers: &HeaderMap,
    ) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
        let inbound = parse_inbound_account_request(&self.policy, headers)?;
        if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
            return Err(not_found());
        }
        let (branch, department) = self.scopes.slugs(headers)?;
        let field = |name: &'static str| -> Result<&str, BusinessError> {
            let mut values = headers.get_all(name).iter();
            let first = values.next();
            if values.next().is_some() {
                return Err(BusinessError::invalid(
                    "CRM_CONTEXT_REQUIRED",
                    "Use one current work context.",
                ));
            }
            first
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    BusinessError::invalid(
                        "CRM_CONTEXT_REQUIRED",
                        "Use the current work revision and assignment.",
                    )
                })
        };
        let revision = field("x-erpx-context-revision")?
            .parse::<i64>()
            .map_err(|_| {
                BusinessError::invalid(
                    "CRM_CONTEXT_REQUIRED",
                    "Use the current work revision.",
                )
            })?;
        if revision < 0 {
            return Err(BusinessError::invalid(
                "CRM_CONTEXT_REQUIRED",
                "Use the current work revision.",
            ));
        }
        let assignment_id = AssignmentId::new(field("x-erpx-context-assignment")?)?;
        let assignment_version = field("x-erpx-context-assignment-version")?
            .parse::<i64>()
            .map_err(|_| {
                BusinessError::invalid(
                    "CRM_CONTEXT_REQUIRED",
                    "Use the current assignment version.",
                )
            })?;
        if assignment_version < 1 {
            return Err(BusinessError::invalid(
                "CRM_CONTEXT_REQUIRED",
                "Use the current assignment version.",
            ));
        }
        Ok((
            inbound.into_proof(),
            HoldingWorkspaceRequest {
                company_id: self.service.company().clone(),
                branch_slug: branch,
                department_slug: department,
                expected: Some(ExpectedWorkSelection {
                    revision,
                    assignment_id,
                    assignment_version,
                }),
                mutation: false,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> CrmLeadCommandHttpBackend for CrmCommandBackend<C> {
    fn change_lead<'a>(
        &'a self,
        request: CrmLeadCommandHttpRequest<'a>,
    ) -> CrmLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let input: LeadChangeInput = serde_json::from_value(request.body.clone())
                .map_err(|_| crate::crm::command_request_invalid())?;
            if !native_lead_action(&input.action) {
                return Err(not_native(&input.action));
            }
            self.service
                .change_lead(
                    request.host,
                    proof,
                    selection,
                    request.key.clone(),
                    request.lead_id.to_owned(),
                    input,
                )
                .await
                .map_err(business)
        })
    }

    fn change_opportunity<'a>(
        &'a self,
        request: CrmLeadCommandHttpRequest<'a>,
    ) -> CrmLeadCommandHttpFuture<'a> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let input: OpportunityChangeInput =
                serde_json::from_value(request.body.clone())
                    .map_err(|_| crate::crm::command_request_invalid())?;
            if !native_opportunity_action(&input.action) {
                return Err(not_native(&input.action));
            }
            self.service
                .change_opportunity(
                    request.host,
                    proof,
                    selection,
                    request.key.clone(),
                    request.lead_id.to_owned(),
                    input,
                )
                .await
                .map_err(business)
        })
    }
}

fn command_request_invalid() -> BusinessError {
    BusinessError::invalid(
        "CRM_COMMAND_REQUEST_INVALID",
        "Use one bounded CRM command.",
    )
}

#[allow(dead_code)]
fn _command_router(
    targets: Vec<CrmLeadCommandHttpTarget>,
) -> Result<axum::Router, BusinessError> {
    crm_lead_command_http_router(targets)
}
