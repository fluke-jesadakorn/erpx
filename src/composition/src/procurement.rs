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
use erpx_server_operations::public::staff_tasks::notify_staff_record;
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
