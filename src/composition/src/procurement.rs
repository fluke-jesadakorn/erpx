//! Native Procurement read assembly. Current Account/Holding authority remains
//! with the selected coordinator; owner ports supply the supplier promise
//! history facts.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
    PurchaseWitness, current_package_catalogue,
};
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    staff_notifications::supplier_promise_recipients,
    work_context::{ExpectedWorkSelection, HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_collaboration::public::notification_statement::{
    NotificationStatementInput, finance_supplier_bill_draft_void_statement,
    supplier_bill_notification_statement,
};
use erpx_server_commerce::public::owner_write::{PgOwnerWrite, execute_owner_write};
use erpx_server_operations::public::staff_tasks::notify_staff_record;
use erpx_server_procurement::public::supplier_bill_command::{
    BillHead, CaptureAttributes, CaptureLineFact, CaptureOrderFact, CapturePostingFact,
    CapturePreparedLine, CaptureReceiptFact, PgSupplierBillCommand, PolicyHead, PostingConfig,
    StoredSupplierBillReceipt, SupplierBillCapturePlan, SupplierBillCommandActor,
    SupplierBillCommandFuture, SupplierBillCommandRepository, SupplierBillCommandStaging,
    SupplierBillCommitPlan, SupplierBillDecidePlan, SupplierBillPolicyPlan, SupplierBillReadback,
    SupplierBillRecordReceipt, capture_supplier_bill, decide_supplier_bill, save_supplier_bill_policy,
};
use erpx_server_procurement::public::supplier_bill_command_http::{
    SupplierBillCaptureHttpRequest, SupplierBillCommandHttpBackend, SupplierBillCommandHttpFuture,
    SupplierBillCommandHttpResponse, SupplierBillCommandHttpTarget, SupplierBillDecideHttpRequest,
    SupplierBillPolicyHttpRequest, supplier_bill_command_http_router,
};
use erpx_server_procurement::public::supplier_promise_command::{
    RecordSupplierPromiseCommand, SupplierPromiseCommandActor,
};
use erpx_server_procurement::public::supplier_promise_command_http::{
    SupplierPromiseCommandHttpBackend, SupplierPromiseCommandHttpFuture,
    SupplierPromiseCommandHttpRequest, SupplierPromiseCommandHttpResponse,
    SupplierPromiseCommandHttpTarget, SupplierPromiseDetailHttpRequest,
    SupplierPromiseDetailHttpResponse, record_supplier_promise_in_transaction,
    supplier_promise_command_http_router,
};
use erpx_server_procurement::public::supplier_promise_history::{
    PgSupplierPromiseHistoryRead, SupplierPromisePurchaseAccess, SupplierPromiseReadActor,
    supplier_promise_detail,
};
use erpx_server_procurement::public::supplier_promise_history_http::{
    SupplierPromiseHistoryHttpBackend, SupplierPromiseHistoryHttpFuture,
    SupplierPromiseHistoryHttpRequest, SupplierPromiseHistoryHttpResponse,
    SupplierPromiseHistoryHttpTarget, supplier_promise_history_router,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{
    AssignmentId, BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, RecordId,
    TimestampMillis,
};
use serde_json::Value;
use sqlx::{PgConnection, Row};
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "PROCUREMENT_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PROCUREMENT_UNAVAILABLE",
        "The Procurement workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct ProcurementSources;

impl HoldingWorkSourcePreparation for ProcurementSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct ProcurementConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl ProcurementConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn procurement_actor(
    current: &AuthorizedWorkContext,
) -> Result<SupplierPromiseReadActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(SupplierPromiseReadActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    })
}

fn procurement_command_actor(
    current: &AuthorizedWorkContext,
) -> Result<SupplierPromiseCommandActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(SupplierPromiseCommandActor {
        user_id: current.user_id().as_str().to_owned(),
        assignment_id: assignment.id.as_str().to_owned(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        revision: selected.revision(),
        assignment_version: assignment.version,
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    })
}

fn bill_command_actor(
    current: &AuthorizedWorkContext,
) -> Result<SupplierBillCommandActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    Ok(SupplierBillCommandActor {
        user_id: current.user_id().as_str().to_owned(),
        assignment_id: assignment.id.as_str().to_owned(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace
            .branch_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        department_id: workspace
            .department_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        revision: selected.revision(),
        assignment_version: assignment.version,
        permissions: current.command_iam_metadata().permissions.clone(),
        expires_at: current.expires_at(),
    })
}

/// Retained bill foundations; capture/decide settle existing purchases, policy
/// authorizes new work. A recorded receipt replays as history.
const BILL_FOUNDATIONS: [&str; 3] = [
    "supply.purchasing",
    "supply.receiving",
    "finance.inventory-valuation",
];

async fn bill_purchase_purpose(
    connection: &mut PgConnection,
    key: &CommandKey,
    operation: &str,
    actor: &SupplierBillCommandActor,
    fallback: CapabilityPurpose,
) -> Result<CapabilityPurpose, Error> {
    let replay: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND \
         operation=$2 AND actor_id=$3 AND authority_json::jsonb->>'companyId'=$4)",
    )
    .bind(key.as_str())
    .bind(operation)
    .bind(&actor.user_id)
    .bind(actor.company_id.as_str())
    .fetch_one(&mut *connection)
    .await?;
    Ok(if replay {
        CapabilityPurpose::History
    } else {
        fallback
    })
}

/// Exact retained Supply notification copy. Composition must not paraphrase it.
const SUPPLIER_PROMISE_NOTIFICATION: &str =
    "Supplier delivery confirmation changed: review remaining quantities and dates";

struct OrderNotificationScope {
    company_id: String,
    branch_id: String,
    department_id: Option<String>,
    version: i64,
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

pub struct ProcurementService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: ProcurementConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> ProcurementService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: ProcurementConfiguration,
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
            module: Some("procurement".into()),
            destination: "procurement".into(),
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

    /// Native supplier-invoice policy read. The owner applies `finance.read`
    /// and reveals immutable events only under Company `finance.approve`
    /// authority; composition supplies the purchased-package overlay.
    pub async fn supplier_bill_policy(
        &self,
        host: &str,
        proof: DeviceProof,
        selection: HoldingWorkspaceRequest,
    ) -> Result<erpx_server_procurement::public::supplier_bill_policy_read::SupplierBillPolicyHttpResponse, Error> {
        use erpx_server_procurement::public::supplier_bill_policy_read::{
            PolicyPurchase, PolicyReadActor, invoice_policy_in_transaction,
        };
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(move || Ok(selection)),
                Self::access(),
                ProcurementSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = PolicyReadActor {
                            user_id: current.user_id().as_str().to_owned(),
                            company_id: company.clone(),
                            permissions: current.command_iam_metadata().permissions.clone(),
                            expires_at: current.expires_at(),
                        };
                        let workspace = current.selection().workspace();
                        let company_authority =
                            current.has_permission("finance.approve")
                                && workspace.branch_id.is_none()
                                && workspace.department_id.is_none();
                        let now = clock.now();
                        let catalogue = current_package_catalogue()?;
                        let request = |purpose| PurchaseRequest {
                            capabilities: vec!["supply.purchasing".into()],
                            purpose,
                            mode: PackageEnforcementMode::Enforced,
                        };
                        let purchase = {
                            let mut reader = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            let history = reader
                                .inspect(&request(CapabilityPurpose::History), now)
                                .await
                                .map_err(business)?;
                            let new_work = reader
                                .inspect(&request(CapabilityPurpose::NewWork), now)
                                .await
                                .map_err(business)?;
                            let settle = reader
                                .inspect(&request(CapabilityPurpose::SettleExisting), now)
                                .await
                                .map_err(business)?;
                            let allowed = |inspection: &erpx_server_commercial::public::purchases::PurchaseInspection| {
                                inspection
                                    .access
                                    .iter()
                                    .find(|access| access.capability == "supply.purchasing")
                                    .is_some_and(|access| access.allowed_by_mode)
                            };
                            PolicyPurchase {
                                new_work: allowed(&new_work),
                                settle_existing: allowed(&settle),
                                history: allowed(&history),
                                revision: history.revision.clone(),
                            }
                        };
                        let data = invoice_policy_in_transaction(
                            connection,
                            &actor,
                            company_authority,
                            Some(purchase),
                            now,
                        )
                        .await?;
                        let data = serde_json::to_value(data).map_err(|_| unavailable())?;
                        Ok(
                            erpx_server_procurement::public::supplier_bill_policy_read::SupplierBillPolicyHttpResponse {
                                data,
                                context: context(current),
                            },
                        )
                    })
                },
            )
            .await
    }

    async fn read_detail<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        order_id: String,
    ) -> Result<(Value, Value), Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let order_id = order_id.clone();
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                self.input(input),
                Self::access(),
                ProcurementSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let order_id = order_id.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        let actor = procurement_actor(current)?;
                        let now = clock.now();
                        let catalogue = current_package_catalogue()?;
                        let request = |purpose| PurchaseRequest {
                            capabilities: vec!["supply.purchasing".into()],
                            purpose,
                            mode: PackageEnforcementMode::Enforced,
                        };
                        let purchase = {
                            let mut reader = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            let history = reader
                                .inspect(&request(CapabilityPurpose::History), now)
                                .await
                                .map_err(business)?;
                            let new_work = reader
                                .inspect(&request(CapabilityPurpose::NewWork), now)
                                .await
                                .map_err(business)?;
                            let settle = reader
                                .inspect(&request(CapabilityPurpose::SettleExisting), now)
                                .await
                                .map_err(business)?;
                            let allowed = |inspection: &erpx_server_commercial::public::purchases::PurchaseInspection| {
                                inspection
                                    .access
                                    .iter()
                                    .find(|access| access.capability == "supply.purchasing")
                                    .is_some_and(|access| access.allowed_by_mode)
                            };
                            SupplierPromisePurchaseAccess {
                                new_work: allowed(&new_work),
                                settle_existing: allowed(&settle),
                                history: allowed(&history),
                                revision: history.revision.clone(),
                            }
                        };
                        let mut repository = PgSupplierPromiseHistoryRead::new(connection);
                        let data = supplier_promise_detail(
                            &actor,
                            &mut repository,
                            &order_id,
                            Some(purchase),
                            now,
                        )
                        .await?;
                        let data = serde_json::to_value(data).map_err(|_| unavailable())?;
                        Ok((data, context(current)))
                    })
                },
            )
            .await
    }

    pub async fn history<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        order_id: String,
    ) -> Result<SupplierPromiseHistoryHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let (data, context) = self.read_detail(host, proof, input, order_id).await?;
        Ok(SupplierPromiseHistoryHttpResponse { data, context })
    }

    pub async fn detail<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        order_id: String,
    ) -> Result<SupplierPromiseDetailHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let (data, context) = self.read_detail(host, proof, input, order_id).await?;
        Ok(SupplierPromiseDetailHttpResponse { data, context })
    }

    /// Record one supplier promise with the retained purchase admission and the
    /// retained recipient notification in the same serializable Holding unit.
    #[allow(clippy::too_many_arguments)]
    pub async fn record(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        key: CommandKey,
        order_id: String,
        command: RecordSupplierPromiseCommand,
    ) -> Result<SupplierPromiseCommandHttpResponse, Error> {
        if selection.company_id != self.configuration.company {
            return Err(scope().into());
        }
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
                ProcurementSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let order_id = order_id.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("commerce.read")?;
                        current.require_permission("commerce.purchase")?;
                        let actor = procurement_command_actor(current)?;
                        let now = clock.now();
                        actor.require_record(now)?;
                        let catalogue = current_package_catalogue()?;
                        let purpose = purchase_purpose(connection, &key, &actor).await?;
                        let request = |purpose| PurchaseRequest {
                            capabilities: vec!["supply.purchasing".into()],
                            purpose,
                            mode: PackageEnforcementMode::Enforced,
                        };
                        let witness: PurchaseWitness = {
                            let mut reader = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            reader
                                .inspect(&request(purpose), now)
                                .await
                                .map_err(business)?
                                .require_allowed()?
                                .clone()
                        };
                        let receipt = record_supplier_promise_in_transaction(
                            connection, &actor, &order_id, &command, &key, now,
                        )
                        .await?;
                        {
                            let mut reader = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            reader
                                .revalidate(&witness, now)
                                .await
                                .map_err(business)?
                                .require_allowed()?;
                        }
                        let scope =
                            order_notification_scope(connection, &company, &order_id).await?;
                        let recipients = supplier_promise_recipients(
                            connection,
                            &scope.company_id,
                            Some(&scope.branch_id),
                            scope.department_id.as_deref(),
                        )
                        .await
                        .map_err(PgTransactionError::Business)?;
                        for user_id in recipients {
                            notify_staff_record(
                                connection,
                                key.as_str(),
                                &scope.company_id,
                                &user_id,
                                "supplier_promise",
                                &order_id,
                                scope.version,
                                SUPPLIER_PROMISE_NOTIFICATION,
                                now.get(),
                            )
                            .await
                            .map_err(PgTransactionError::Business)?;
                        }
                        let data = serde_json::to_value(&receipt).map_err(|_| unavailable())?;
                        Ok(SupplierPromiseCommandHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    /// Native supplier-bill capture: retained `finance.write` + `commerce.read`
    /// permissions with settle-existing admission over the three bill
    /// foundations. Cross-owner receipt-version/links run in-process; the void
    /// and notify hooks execute owner-built statements in the same unit.
    pub async fn capture_bill(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        key: CommandKey,
        input: Value,
    ) -> Result<SupplierBillCommandHttpResponse, Error> {
        if selection.company_id != self.configuration.company {
            return Err(scope().into());
        }
        selection.mutation = true;
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                WorkAuthorizationRequest {
                    permission: "finance.write".into(),
                    module: Some("procurement".into()),
                    destination: "procurement.bill.capture".into(),
                    api: true,
                },
                ProcurementSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let input = input.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.write")?;
                        current.require_permission("commerce.read")?;
                        let actor = bill_command_actor(current)?;
                        let now = clock.now();
                        let catalogue = current_package_catalogue()?;
                        let purpose = bill_purchase_purpose(
                            connection,
                            &key,
                            "procurement.bill.capture",
                            &actor,
                            CapabilityPurpose::SettleExisting,
                        )
                        .await?;
                        let witness = {
                            let mut reader = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            reader
                                .inspect(
                                    &PurchaseRequest {
                                        capabilities: BILL_FOUNDATIONS
                                            .iter()
                                            .map(|value| (*value).to_owned())
                                            .collect(),
                                        purpose,
                                        mode: PackageEnforcementMode::Enforced,
                                    },
                                    now,
                                )
                                .await
                                .map_err(business)?
                                .require_allowed()?
                                .clone()
                        };
                        let mut unit = BillCommandUnit::new(connection);
                        let receipt = {
                            let (repository, intents) =
                                (&mut unit.repository, &mut unit.intents);
                            let mut staging = BillStaging { intents };
                            capture_supplier_bill(
                                &actor,
                                repository,
                                &mut staging,
                                &input,
                                key.as_str(),
                                now,
                            )
                            .await?
                        };
                        unit.settle(&actor, now).await?;
                        {
                            let mut reader = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                unit.connection(),
                                &catalogue,
                            );
                            reader
                                .revalidate(&witness, now)
                                .await
                                .map_err(business)?
                                .require_allowed()?;
                        }
                        let data = serde_json::to_value(&receipt).map_err(|_| unavailable())?;
                        Ok(SupplierBillCommandHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    /// Native supplier-bill decision: retained `finance.read` + `finance.write`
    /// + `finance.approve` permissions with settle-existing admission. A
    /// return of a draft invoice voids it and clears its links in-unit.
    pub async fn decide_bill(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        key: CommandKey,
        match_id: String,
        input: Value,
    ) -> Result<SupplierBillCommandHttpResponse, Error> {
        if selection.company_id != self.configuration.company {
            return Err(scope().into());
        }
        selection.mutation = true;
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                WorkAuthorizationRequest {
                    permission: "finance.approve".into(),
                    module: Some("procurement".into()),
                    destination: "procurement.bill.decision".into(),
                    api: true,
                },
                ProcurementSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let match_id = match_id.clone();
                    let input = input.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.read")?;
                        current.require_permission("finance.write")?;
                        current.require_permission("finance.approve")?;
                        let actor = bill_command_actor(current)?;
                        let now = clock.now();
                        let catalogue = current_package_catalogue()?;
                        // The retained operation varies by action; purpose is
                        // settle-existing for a fresh decision, history on replay.
                        let replay_approve: bool = sqlx::query_scalar(
                            "SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND \
                             operation='procurement.bill.approve' AND actor_id=$2 AND \
                             authority_json::jsonb->>'companyId'=$3)",
                        )
                        .bind(key.as_str())
                        .bind(&actor.user_id)
                        .bind(actor.company_id.as_str())
                        .fetch_one(&mut *connection)
                        .await?;
                        let replay_return: bool = sqlx::query_scalar(
                            "SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND \
                             operation='procurement.bill.return' AND actor_id=$2 AND \
                             authority_json::jsonb->>'companyId'=$3)",
                        )
                        .bind(key.as_str())
                        .bind(&actor.user_id)
                        .bind(actor.company_id.as_str())
                        .fetch_one(&mut *connection)
                        .await?;
                        let purpose = if replay_approve || replay_return {
                            CapabilityPurpose::History
                        } else {
                            CapabilityPurpose::SettleExisting
                        };
                        let witness = {
                            let mut reader = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            reader
                                .inspect(
                                    &PurchaseRequest {
                                        capabilities: BILL_FOUNDATIONS
                                            .iter()
                                            .map(|value| (*value).to_owned())
                                            .collect(),
                                        purpose,
                                        mode: PackageEnforcementMode::Enforced,
                                    },
                                    now,
                                )
                                .await
                                .map_err(business)?
                                .require_allowed()?
                                .clone()
                        };
                        let mut unit = BillCommandUnit::new(connection);
                        let receipt = {
                            let (repository, intents) =
                                (&mut unit.repository, &mut unit.intents);
                            let mut staging = BillStaging { intents };
                            decide_supplier_bill(
                                &actor,
                                repository,
                                &mut staging,
                                &match_id,
                                &input,
                                key.as_str(),
                                now,
                            )
                            .await?
                        };
                        unit.settle(&actor, now).await?;
                        {
                            let mut reader = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                unit.connection(),
                                &catalogue,
                            );
                            reader
                                .revalidate(&witness, now)
                                .await
                                .map_err(business)?
                                .require_allowed()?;
                        }
                        let data = serde_json::to_value(&receipt).map_err(|_| unavailable())?;
                        Ok(SupplierBillCommandHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    /// Native supplier-invoice policy save: retained `finance.approve`
    /// permission with new-work admission over the bill foundations.
    pub async fn save_policy_bill(
        &self,
        host: &str,
        proof: DeviceProof,
        mut selection: HoldingWorkspaceRequest,
        key: CommandKey,
        input: Value,
    ) -> Result<SupplierBillCommandHttpResponse, Error> {
        if selection.company_id != self.configuration.company {
            return Err(scope().into());
        }
        selection.mutation = true;
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        self.authorizer
            .execute_with_sources(
                host,
                proof,
                selection,
                WorkAuthorizationRequest {
                    permission: "finance.approve".into(),
                    module: Some("procurement".into()),
                    destination: "procurement.invoice-policy.update".into(),
                    api: true,
                },
                ProcurementSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let key = key.clone();
                    let input = input.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("finance.approve")?;
                        let actor = bill_command_actor(current)?;
                        let now = clock.now();
                        let catalogue = current_package_catalogue()?;
                        let purpose = bill_purchase_purpose(
                            connection,
                            &key,
                            "procurement.invoice-policy.update",
                            &actor,
                            CapabilityPurpose::NewWork,
                        )
                        .await?;
                        let witness = {
                            let mut reader = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            reader
                                .inspect(
                                    &PurchaseRequest {
                                        capabilities: BILL_FOUNDATIONS
                                            .iter()
                                            .map(|value| (*value).to_owned())
                                            .collect(),
                                        purpose,
                                        mode: PackageEnforcementMode::Enforced,
                                    },
                                    now,
                                )
                                .await
                                .map_err(business)?
                                .require_allowed()?
                                .clone()
                        };
                        let mut unit = BillCommandUnit::new(connection);
                        let receipt = save_supplier_bill_policy(
                            &actor,
                            &mut unit.repository,
                            &input,
                            key.as_str(),
                            now,
                        )
                        .await?;
                        unit.settle(&actor, now).await?;
                        {
                            let mut reader = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                unit.connection(),
                                &catalogue,
                            );
                            reader
                                .revalidate(&witness, now)
                                .await
                                .map_err(business)?
                                .require_allowed()?;
                        }
                        let data = serde_json::to_value(&receipt).map_err(|_| unavailable())?;
                        Ok(SupplierBillCommandHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

/// Mirror the retained `withProcurementPurchasedCommand` purpose exactly: a
/// replay receipt is history; a fresh supplier confirmation settles an existing
/// purchase. `NewWork` would be stricter than the retained command, so it is not
/// substituted here.
async fn purchase_purpose(
    connection: &mut PgConnection,
    key: &CommandKey,
    actor: &SupplierPromiseCommandActor,
) -> Result<CapabilityPurpose, Error> {
    let replay: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND \
         operation='procurement.promise.record' AND actor_id=$2 AND \
         authority_json::jsonb->>'companyId'=$3)",
    )
    .bind(key.as_str())
    .bind(&actor.user_id)
    .bind(actor.company_id.as_str())
    .fetch_one(&mut *connection)
    .await?;
    Ok(if replay {
        CapabilityPurpose::History
    } else {
        CapabilityPurpose::SettleExisting
    })
}

/// Read the owner row after the command for the retained notification scope.
async fn order_notification_scope(
    connection: &mut PgConnection,
    company: &CompanyId,
    order_id: &str,
) -> Result<OrderNotificationScope, Error> {
    let row = sqlx::query(
        "SELECT company_id,branch_id,department_id,version FROM commerce_orders \
         WHERE id=$1 AND company_id=$2",
    )
    .bind(order_id)
    .bind(company.as_str())
    .fetch_optional(&mut *connection)
    .await?;
    let Some(row) = row else {
        return Err(not_found().into());
    };
    Ok(OrderNotificationScope {
        company_id: row.try_get("company_id")?,
        branch_id: row.try_get("branch_id")?,
        department_id: row.try_get("department_id")?,
        version: row.try_get("version")?,
    })
}

pub struct ProcurementBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<ProcurementService<C>>,
}

impl<C: Clock + Send + Sync + 'static> ProcurementBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<ProcurementService<C>>,
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

    /// Mutating selection with the exact current-work CAS headers. A missing or
    /// duplicate header is a request error, never a stale-context fallback.
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
                    "PROCUREMENT_CONTEXT_REQUIRED",
                    "Use one current work context.",
                ));
            }
            first
                .and_then(|value| value.to_str().ok())
                .filter(|value| !value.is_empty())
                .ok_or_else(|| {
                    BusinessError::invalid(
                        "PROCUREMENT_CONTEXT_REQUIRED",
                        "Use the current work revision and assignment.",
                    )
                })
        };
        let revision = field("x-erpx-context-revision")?
            .parse::<i64>()
            .map_err(|_| {
                BusinessError::invalid(
                    "PROCUREMENT_CONTEXT_REQUIRED",
                    "Use the current work revision.",
                )
            })?;
        if revision < 0 {
            return Err(BusinessError::invalid(
                "PROCUREMENT_CONTEXT_REQUIRED",
                "Use the current work revision.",
            ));
        }
        let assignment_id = AssignmentId::new(field("x-erpx-context-assignment")?)?;
        let assignment_version = field("x-erpx-context-assignment-version")?
            .parse::<i64>()
            .map_err(|_| {
                BusinessError::invalid(
                    "PROCUREMENT_CONTEXT_REQUIRED",
                    "Use the current assignment version.",
                )
            })?;
        if assignment_version < 1 {
            return Err(BusinessError::invalid(
                "PROCUREMENT_CONTEXT_REQUIRED",
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
                mutation: true,
            },
        ))
    }
}

impl<C: Clock + Send + Sync + 'static> SupplierPromiseHistoryHttpBackend for ProcurementBackend<C> {
    fn read<'a>(
        &'a self,
        request: SupplierPromiseHistoryHttpRequest<'a>,
    ) -> SupplierPromiseHistoryHttpFuture<'a, SupplierPromiseHistoryHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let order_id = request.order_id.to_owned();
            self.service
                .history(request.host, proof, move || Ok(selection), order_id)
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> SupplierPromiseCommandHttpBackend for ProcurementBackend<C> {
    fn record<'a>(
        &'a self,
        request: SupplierPromiseCommandHttpRequest<'a>,
    ) -> SupplierPromiseCommandHttpFuture<'a, SupplierPromiseCommandHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let key = request.key.clone();
            let order_id = request.order_id.to_owned();
            let command = request.command.clone();
            self.service
                .record(request.host, proof, selection, key, order_id, command)
                .await
                .map_err(business)
        })
    }

    fn detail<'a>(
        &'a self,
        request: SupplierPromiseDetailHttpRequest<'a>,
    ) -> SupplierPromiseCommandHttpFuture<'a, SupplierPromiseDetailHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            let order_id = request.order_id.to_owned();
            self.service
                .detail(request.host, proof, move || Ok(selection), order_id)
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> SupplierBillCommandHttpBackend for ProcurementBackend<C> {
    fn capture<'a>(
        &'a self,
        request: SupplierBillCaptureHttpRequest<'a>,
    ) -> SupplierBillCommandHttpFuture<'a, SupplierBillCommandHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let key = request.key.clone();
            let input = request.input.clone();
            self.service
                .capture_bill(request.host, proof, selection, key, input)
                .await
                .map_err(business)
        })
    }

    fn decide<'a>(
        &'a self,
        request: SupplierBillDecideHttpRequest<'a>,
    ) -> SupplierBillCommandHttpFuture<'a, SupplierBillCommandHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let key = request.key.clone();
            let match_id = request.match_id.to_owned();
            let input = request.input.clone();
            self.service
                .decide_bill(request.host, proof, selection, key, match_id, input)
                .await
                .map_err(business)
        })
    }

    fn save_policy<'a>(
        &'a self,
        request: SupplierBillPolicyHttpRequest<'a>,
    ) -> SupplierBillCommandHttpFuture<'a, SupplierBillCommandHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.command_context(request.headers)?;
            let key = request.key.clone();
            let input = request.input.clone();
            self.service
                .save_policy_bill(request.host, proof, selection, key, input)
                .await
                .map_err(business)
        })
    }
}

/// Cross-owner staging intents recorded by the bill port and executed by the
/// service inside the same caller-owned serializable transaction, in order.
/// Commerce receipt-version and invoice-link hooks run through the in-process
/// owner write (stronger than the retained HTTP dispatch); the Finance
/// draft-void and Collaboration notification hooks execute owner-built
/// statements locally.
enum BillStagingIntent {
    OwnerWrite {
        kind: &'static str,
        values: Vec<Value>,
        record_id: String,
        key: String,
    },
    Links {
        match_id: String,
        invoice_id: Option<String>,
    },
    VoidDraft {
        match_id: String,
    },
    Notify {
        match_id: String,
        key: String,
    },
}

/// Composition-owned unit binding the procurement repository to the
/// cross-owner staging hooks over one caller transaction.
struct BillCommandUnit<'c> {
    repository: PgSupplierBillCommand<'c>,
    intents: Vec<BillStagingIntent>,
}

impl<'c> BillCommandUnit<'c> {
    fn new(connection: &'c mut PgConnection) -> Self {
        Self {
            repository: PgSupplierBillCommand::new(connection),
            intents: Vec::new(),
        }
    }

    fn connection(&mut self) -> &mut PgConnection {
        self.repository.connection()
    }
}

impl<'c> SupplierBillCommandRepository for BillCommandUnit<'c> {
    type Error = Error;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<StoredSupplierBillReceipt>, Self::Error> {
        Box::pin(async move { self.repository.receipt(key).await })
    }

    fn capture_receipt<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        fulfillment_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<CaptureReceiptFact>, Self::Error> {
        Box::pin(async move { self.repository.capture_receipt(actor, fulfillment_id).await })
    }

    fn capture_order<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<CaptureOrderFact>, Self::Error> {
        Box::pin(async move { self.repository.capture_order(order_id).await })
    }

    fn posting_config<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
    ) -> SupplierBillCommandFuture<'a, Option<PostingConfig>, Self::Error> {
        Box::pin(async move { self.repository.posting_config(actor).await })
    }

    fn receipt_postings<'a>(
        &'a mut self,
        journal_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Vec<CapturePostingFact>, Self::Error> {
        Box::pin(async move { self.repository.receipt_postings(journal_id).await })
    }

    fn receipt_lines<'a>(
        &'a mut self,
        fulfillment_ids: &'a [String],
    ) -> SupplierBillCommandFuture<'a, Vec<CaptureLineFact>, Self::Error> {
        Box::pin(async move { self.repository.receipt_lines(fulfillment_ids).await })
    }

    fn bill<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        match_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<BillHead>, Self::Error> {
        Box::pin(async move { self.repository.bill(actor, match_id).await })
    }

    fn bill_variance<'a>(
        &'a mut self,
        match_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.repository.bill_variance(match_id).await })
    }

    fn variance_account_ok<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        account_id: &'a str,
        chart_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.repository
                .variance_account_ok(actor, account_id, chart_id)
                .await
        })
    }

    fn policy<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
    ) -> SupplierBillCommandFuture<'a, Option<PolicyHead>, Self::Error> {
        Box::pin(async move { self.repository.policy(actor).await })
    }

    fn company_authority<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        now: TimestampMillis,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.repository.company_authority(actor, now).await })
    }

    fn bill_image<'a>(
        &'a mut self,
        actor: &'a SupplierBillCommandActor,
        match_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<serde_json::Value>, Self::Error> {
        Box::pin(async move { self.repository.bill_image(actor, match_id).await })
    }

    fn verify_capture_receipts<'a>(
        &'a mut self,
        receipts: &'a [(String, String, i64)],
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move { self.repository.verify_capture_receipts(receipts).await })
    }

    fn verify_decide_state<'a>(
        &'a mut self,
        plan: &'a SupplierBillDecidePlan,
        bill_status: &'a str,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.repository.verify_decide_state(plan, bill_status).await
        })
    }

    fn verify_policy_version<'a>(
        &'a mut self,
        plan: &'a SupplierBillPolicyPlan,
        current_version: i64,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.repository
                .verify_policy_version(plan, current_version)
                .await
        })
    }

    fn insert_match<'a>(
        &'a mut self,
        plan: &'a SupplierBillCapturePlan,
        attributes: &'a CaptureAttributes,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move { self.repository.insert_match(plan, attributes).await })
    }

    fn insert_match_receipt<'a>(
        &'a mut self,
        plan: &'a SupplierBillCapturePlan,
        index: usize,
        fulfillment_id: &'a str,
        expected_version: i64,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.repository
                .insert_match_receipt(plan, index, fulfillment_id, expected_version)
                .await
        })
    }

    fn insert_match_lines<'a>(
        &'a mut self,
        plan: &'a SupplierBillCapturePlan,
        lines: &'a [CapturePreparedLine],
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move { self.repository.insert_match_lines(plan, lines).await })
    }

    fn insert_event<'a>(
        &'a mut self,
        plan: &'a SupplierBillCommitPlan,
        match_id: &'a str,
        version: i64,
        action: &'a str,
        note: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.repository
                .insert_event(plan, match_id, version, action, note)
                .await
        })
    }

    fn apply_decision<'a>(
        &'a mut self,
        plan: &'a SupplierBillDecidePlan,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move { self.repository.apply_decision(plan).await })
    }

    fn deactivate_match_receipts<'a>(
        &'a mut self,
        plan: &'a SupplierBillDecidePlan,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move { self.repository.deactivate_match_receipts(plan).await })
    }

    fn upsert_policy<'a>(
        &'a mut self,
        plan: &'a SupplierBillPolicyPlan,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move { self.repository.upsert_policy(plan).await })
    }

    fn insert_policy_event<'a>(
        &'a mut self,
        plan: &'a SupplierBillPolicyPlan,
        previous: &'a str,
        reason: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.repository
                .insert_policy_event(plan, previous, reason)
                .await
        })
    }

    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        operation: &'a str,
        actor: &'a SupplierBillCommandActor,
        request_fingerprint: &'a str,
        record_id: &'a str,
        result_json: &'a str,
        authority_json: &'a str,
        now: TimestampMillis,
    ) -> SupplierBillCommandFuture<'a, bool, Self::Error> {
        Box::pin(async move {
            self.repository
                .insert_receipt(
                    key,
                    operation,
                    actor,
                    request_fingerprint,
                    record_id,
                    result_json,
                    authority_json,
                    now,
                )
                .await
        })
    }

    fn audit<'a>(
        &'a mut self,
        plan: &'a SupplierBillCommitPlan,
        receipt: &'a SupplierBillRecordReceipt,
        authority_json: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move { self.repository.audit(plan, receipt, authority_json).await })
    }

    fn outbox<'a>(
        &'a mut self,
        plan: &'a SupplierBillCommitPlan,
        receipt: &'a SupplierBillRecordReceipt,
        authority_json: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move { self.repository.outbox(plan, receipt, authority_json).await })
    }

    fn readback<'a>(
        &'a mut self,
        company: &'a CompanyId,
        match_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, Option<SupplierBillReadback>, Self::Error> {
        Box::pin(async move { self.repository.readback(company, match_id).await })
    }
}

/// Execute an owner-built notification statement on the caller transaction.
/// Mirrors the retained `?` positional placeholders to Postgres `$n`. The SQL
/// text is owner-built with bound values only; audited safe.
async fn execute_notification_statement(
    connection: &mut PgConnection,
    sql: &str,
    values: &[Value],
) -> Result<(), Error> {
    let mut rewritten = String::with_capacity(sql.len());
    let mut index = 0usize;
    for ch in sql.chars() {
        if ch == '?' {
            index += 1;
            rewritten.push('$');
            rewritten.push_str(&index.to_string());
        } else {
            rewritten.push(ch);
        }
    }
    let mut query = sqlx::query(sqlx::AssertSqlSafe(rewritten.as_str()));
    for value in values {
        query = match value {
            Value::String(text) => query.bind(text.as_str()),
            Value::Number(number) if number.is_i64() => {
                query.bind(number.as_i64().unwrap_or(0))
            }
            Value::Number(number) => query.bind(number.as_f64().unwrap_or(0.0)),
            Value::Bool(flag) => query.bind(*flag),
            Value::Null => query.bind(Option::<String>::None),
            Value::Array(_) | Value::Object(_) => query.bind(value.to_string()),
        };
    }
    query.execute(&mut *connection).await?;
    Ok(())
}

/// Borrowed staging sink with disjoint borrows from the repository so the
/// application use case can take `&mut repository` and `&mut staging`
/// without a double mutable borrow of the unit.
struct BillStaging<'a> {
    intents: &'a mut Vec<BillStagingIntent>,
}

impl SupplierBillCommandStaging for BillStaging<'_> {
    type Error = Error;

    fn stage_receipt_version<'a>(
        &'a mut self,
        fulfillment_id: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.intents.push(BillStagingIntent::OwnerWrite {
                kind: "supplier.fulfillment-version",
                values: vec![Value::String(fulfillment_id.to_owned())],
                record_id: fulfillment_id.to_owned(),
                key: format!("supplier:fulfillment-version:{fulfillment_id}"),
            });
            Ok(())
        })
    }

    fn stage_receipt_links<'a>(
        &'a mut self,
        match_id: &'a str,
        invoice_id: Option<&'a str>,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.intents.push(BillStagingIntent::Links {
                match_id: match_id.to_owned(),
                invoice_id: invoice_id.map(str::to_owned),
            });
            Ok(())
        })
    }

    fn stage_draft_void<'a>(
        &'a mut self,
        plan: &'a SupplierBillDecidePlan,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.intents.push(BillStagingIntent::VoidDraft {
                match_id: plan.match_id.clone(),
            });
            Ok(())
        })
    }

    fn notify_bill<'a>(
        &'a mut self,
        match_id: &'a str,
        key: &'a str,
    ) -> SupplierBillCommandFuture<'a, (), Self::Error> {
        Box::pin(async move {
            self.intents.push(BillStagingIntent::Notify {
                match_id: match_id.to_owned(),
                key: key.to_owned(),
            });
            Ok(())
        })
    }
}

impl<'c> BillCommandUnit<'c> {
    /// Run the recorded staging intents in order inside the caller
    /// transaction, after the application use case returns.
    async fn settle(&mut self, actor: &SupplierBillCommandActor, now: TimestampMillis) -> Result<(), Error> {
        let company = actor.company_id.clone();
        for intent in std::mem::take(&mut self.intents) {
            match intent {
                BillStagingIntent::OwnerWrite {
                    kind,
                    values,
                    record_id,
                    key,
                } => {
                    let raw = serde_json::json!({"kind": kind, "values": values, "recordId": record_id});
                    let key = CommandKey::new(key).map_err(Error::from)?;
                    let mut owner =
                        PgOwnerWrite::new(self.connection(), company.as_str());
                    execute_owner_write("system", &raw, Some(&key), &mut owner, now)
                        .await
                        .map_err(|error| match error {
                            PgTransactionError::Business(error) => {
                                PgTransactionError::Business(error)
                            }
                            PgTransactionError::Persistence(error) => {
                                PgTransactionError::Persistence(error)
                            }
                        })?;
                }
                BillStagingIntent::Links { match_id, invoice_id } => {
                    let rows: Vec<(String,)> = if invoice_id.is_some() {
                        sqlx::query_as(
                            "SELECT fulfillment_id FROM procurement_bill_match_receipts \
                             WHERE match_id=$1 AND active=TRUE",
                        )
                        .bind(&match_id)
                        .fetch_all(&mut *self.connection())
                        .await?
                    } else {
                        sqlx::query_as(
                            "SELECT fulfillment_id FROM procurement_bill_match_receipts \
                             WHERE match_id=$1",
                        )
                        .bind(&match_id)
                        .fetch_all(&mut *self.connection())
                        .await?
                    };
                    let ids: Vec<String> =
                        rows.into_iter().map(|row| row.0).collect();
                    let _ = invoice_id;
                    let raw = serde_json::json!({"kind": "supplier.fulfillment-links", "values": [invoice_id, serde_json::json!(serde_json::to_string(&ids).map_err(|_| unavailable())?)], "recordId": match_id});
                    let key = CommandKey::new(format!("supplier:fulfillment-links:{match_id}"))
                        .map_err(Error::from)?;
                    let mut owner =
                        PgOwnerWrite::new(self.connection(), company.as_str());
                    execute_owner_write("system", &raw, Some(&key), &mut owner, now)
                        .await
                        .map_err(|error| match error {
                            PgTransactionError::Business(error) => {
                                PgTransactionError::Business(error)
                            }
                            PgTransactionError::Persistence(error) => {
                                PgTransactionError::Persistence(error)
                            }
                        })?;
                }
                BillStagingIntent::VoidDraft { match_id } => {
                    let row: Option<(Option<String>, Option<i64>)> = sqlx::query_as(
                        "SELECT i.id,i.version FROM finance_invoices i \
                         JOIN procurement_bill_matches m ON m.invoice_id=i.id \
                         WHERE m.id=$1 AND m.company_id=$2",
                    )
                    .bind(&match_id)
                    .bind(company.as_str())
                    .fetch_optional(&mut *self.connection())
                    .await?;
                    if let Some((Some(invoice_id), Some(invoice_version))) = row {
                        let _ = invoice_version;
                        let statement = finance_supplier_bill_draft_void_statement(
                            self.connection(),
                            &company,
                            &NotificationStatementInput {
                                company_id: company.as_str().to_owned(),
                                user_id: String::new(),
                                record_type: String::new(),
                                record_id: String::new(),
                                record_version: 0,
                                message: String::new(),
                                now: now.get(),
                                idempotency_key: String::new(),
                                kind: Some(
                                    "finance_supplier_bill_draft_void".to_owned(),
                                ),
                                lead_id: None,
                                identifier: None,
                                approver_user_id: None,
                                item_id: None,
                                bill_id: None,
                                rfq_id: None,
                                audience: None,
                                event: None,
                                order_id: None,
                                change: None,
                                timestamp: None,
                                post_id: None,
                                source_hash: None,
                                ai_draft_id: None,
                                document_table: None,
                                recipient_sql: None,
                                node_id: None,
                                scope_node_id: None,
                                runtime_predicate: None,
                                table_name: None,
                                cte_prefix: None,
                                source_access_sql: None,
                                target_kind: None,
                                target_id: None,
                                values_json: Some(
                                    serde_json::json!([now.get(), invoice_id]).to_string(),
                                ),
                            },
                        )
                        .await
                        .map_err(Error::from)?;
                        execute_notification_statement(
                            self.connection(),
                            &statement.sql,
                            &statement.values,
                        )
                        .await?;
                    }
                }
                BillStagingIntent::Notify { match_id, key } => {
                    let statement = supplier_bill_notification_statement(
                        self.connection(),
                        &company,
                        &match_id,
                        &key,
                    )
                    .await
                    .map_err(Error::from)?;
                    execute_notification_statement(
                        self.connection(),
                        &statement.sql,
                        &statement.values,
                    )
                    .await?;
                }
            }
        }
        Ok(())
    }
}

#[allow(dead_code)]
fn _router(targets: Vec<SupplierPromiseHistoryHttpTarget>) -> Result<axum::Router, BusinessError> {
    supplier_promise_history_router(targets)
}

#[allow(dead_code)]
fn _command_router(
    targets: Vec<SupplierPromiseCommandHttpTarget>,
) -> Result<axum::Router, BusinessError> {
    supplier_promise_command_http_router(targets)
}

#[allow(dead_code)]
fn _bill_command_router(
    targets: Vec<SupplierBillCommandHttpTarget>,
) -> Result<axum::Router, BusinessError> {
    supplier_bill_command_http_router(targets)
}

impl<C: Clock + Send + Sync + 'static>
    erpx_server_procurement::public::supplier_bill_policy_read::SupplierBillPolicyHttpBackend
    for ProcurementBackend<C>
{
    fn read<'a>(
        &'a self,
        request: erpx_server_procurement::public::supplier_bill_policy_read::SupplierBillPolicyHttpRequest<'a>,
    ) -> erpx_server_procurement::public::supplier_bill_policy_read::SupplierBillPolicyHttpFuture<
        'a,
        erpx_server_procurement::public::supplier_bill_policy_read::SupplierBillPolicyHttpResponse,
    > {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .supplier_bill_policy(request.host, proof, selection)
                .await
                .map_err(business)
        })
    }
}
