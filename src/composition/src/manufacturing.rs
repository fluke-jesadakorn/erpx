//! Native Manufacturing BOM and Planning composition assembly.
use crate::business_http::BusinessScopeSlugs;
use erpx_server_identity_access::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::{HoldingWorkspaceRequest, WorkScopeKind},
};
use erpx_server_manufacturing::public::bom::{
    BomActor, BomReadRepository, BomRegister, PgBomRead, bom_register,
};
use erpx_server_manufacturing::public::bom_http::{
    BomHttpBackend, BomHttpFuture, BomHttpRequest, BomHttpResponse,
};
use erpx_server_planning::public::planning::{
    PgPlanningWorkspaceRead, PlanningActor, PlanningWorkspace, PlanningWorkspaceRepository,
    planning_workspace,
};
use erpx_server_planning::public::scenario::{
    ManufacturingOrderPlan, PlanningReleaseFactory, PlanningReleasePort, ReleaseFuture,
};
use erpx_server_planning::public::planning_command_http::{
    PlanningCommandHttpBackend, PlanningCommandHttpCommand, PlanningCommandHttpFuture,
    PlanningCommandHttpRequest, PlanningCommandHttpResponse, PlanningCommandHttpTarget,
};
use erpx_server_planning::public::scenario::{
    CalculateRequest, CancelRequest, ReviewRequest, calculate_scenario, cancel_scenario,
    review_recommendation, PgPlanningScenarios,
};
use erpx_server_planning::public::resources::{
    CalendarRequest, PgPlanningResources, ResourceRequest, save_calendar, save_resource,
};
use erpx_server_manufacturing::public::specification::{
    CreateRequest as SpecificationCreateRequest, PgSpecificationCommands,
    ReviewAction, ReviewRequest as SpecificationReviewRequest, SpecificationHttpBackend,
    SpecificationHttpCommand, SpecificationHttpFuture, SpecificationHttpRequest,
    SpecificationHttpResponse, create_specification, review_specification,
};
use erpx_server_commercial::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PgCompanyPurchaseReader, PurchaseRequest,
    PurchaseWitness, current_package_catalogue,
};
use erpx_server_planning::public::planning_http::{
    PlanningHttpBackend, PlanningHttpFuture, PlanningHttpRequest, PlanningHttpResponse,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId, TimestampMillis};
use serde_json::Value;
use sqlx::PgConnection;
use std::sync::Arc;

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "MANUFACTURING_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "MANUFACTURING_UNAVAILABLE",
        "The Manufacturing workspace is unavailable.",
    )
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct ManufacturingSources;

impl HoldingWorkSourcePreparation for ManufacturingSources {
    fn prepare<'c>(&'c self, _: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async { Ok(()) })
    }
}

#[derive(Clone)]
pub struct ManufacturingConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl ManufacturingConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }

    pub fn company(&self) -> &CompanyId {
        &self.company
    }
}

fn bom_actor(current: &AuthorizedWorkContext) -> Result<BomActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(BomActor {
        user_id: current.user_id().as_str().to_owned(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
        modules: current.modules().to_vec(),
        expires_at: current.expires_at(),
    })
}

fn planning_actor(current: &AuthorizedWorkContext) -> Result<PlanningActor, BusinessError> {
    let selected = current.selection();
    let workspace = selected.workspace();
    Ok(PlanningActor {
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

pub struct ManufacturingService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: ManufacturingConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> ManufacturingService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: ManufacturingConfiguration,
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

    fn bom_access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "records.read".into(),
            module: Some("manufacturing".into()),
            destination: "manufacturing".into(),
            api: true,
        }
    }

    fn planning_access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "records.read".into(),
            module: Some("planning".into()),
            destination: "planning".into(),
            api: true,
        }
    }

    fn specification_access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "records.write".into(),
            module: Some("manufacturing".into()),
            destination: "manufacturing".into(),
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

    pub async fn bom<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<BomHttpResponse, Error>
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
                Self::bom_access(),
                ManufacturingSources,
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
                        current.require_permission("records.read")?;
                        let actor = bom_actor(current)?;
                        let mut repository = PgBomRead::new(connection);
                        let data = bom_register(&actor, &mut repository, clock.now()).await?;
                        let data = serde_json::to_value(data).map_err(|_| unavailable())?;
                        Ok(BomHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn planning<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<PlanningHttpResponse, Error>
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
                Self::planning_access(),
                ManufacturingSources,
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
                        current.require_permission("records.read")?;
                        let actor = planning_actor(current)?;
                        let mut repository = PgPlanningWorkspaceRead::new(connection);
                        let data = planning_workspace(&actor, &mut repository, clock.now()).await?;
                        let data = serde_json::to_value(data).map_err(|_| unavailable())?;
                        Ok(PlanningHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn planning_command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: PlanningCommandHttpCommand,
    ) -> Result<PlanningCommandHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != expected {
                return Err(BusinessError::new(
                    FailureKind::Forbidden,
                    "PLANNING_SCOPE_REQUIRED",
                    "Choose the current authorized Company workspace.",
                ));
            }
            request.mutation = true;
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                Self::planning_access(),
                ManufacturingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(BusinessError::new(
                                FailureKind::Forbidden,
                                "PLANNING_SCOPE_REQUIRED",
                                "Choose the current authorized Company workspace.",
                            )
                            .into());
                        }
                        let actor = planning_actor(current)?;
                        let now = clock.now();
                        let key = match &command {
                            PlanningCommandHttpCommand::Calculate { idempotency_key, .. }
                            | PlanningCommandHttpCommand::Cancel { idempotency_key, .. }
                            | PlanningCommandHttpCommand::Review { idempotency_key, .. }
                            | PlanningCommandHttpCommand::CreateResource { idempotency_key, .. }
                            | PlanningCommandHttpCommand::UpdateResource { idempotency_key, .. }
                            | PlanningCommandHttpCommand::SaveCalendar { idempotency_key, .. } => {
                                erpx_server_shared::public::CommandKey::new(idempotency_key.clone())
                                    .map_err(PgTransactionError::Business)?
                            }
                        };
                        let data = match command {
                            PlanningCommandHttpCommand::Calculate { body, .. } => {
                                let request: CalculateRequest = serde_json::from_value(body)
                                    .map_err(|_| PgTransactionError::Business(BusinessError::invalid(
                                        "PLANNING_REQUEST_INVALID",
                                        "Use one unambiguous Planning command.",
                                    )))?;
                                let mut ports = PgPlanningScenarios::new(connection);
                                calculate_scenario(&mut ports, &actor, &key, request, now).await
                            }
                            PlanningCommandHttpCommand::Cancel { scenario_id, body, .. } => {
                                let request: CancelRequest = serde_json::from_value(body)
                                    .map_err(|_| PgTransactionError::Business(BusinessError::invalid(
                                        "PLANNING_REQUEST_INVALID",
                                        "Use one unambiguous Planning command.",
                                    )))?;
                                let mut ports = PgPlanningScenarios::new(connection);
                                cancel_scenario(&mut ports, &actor, &key, &scenario_id, request, now).await
                            }
                            PlanningCommandHttpCommand::Review { recommendation_id, body, .. } => {
                                let request: ReviewRequest = serde_json::from_value(body)
                                    .map_err(|_| PgTransactionError::Business(BusinessError::invalid(
                                        "PLANNING_REQUEST_INVALID",
                                        "Use one unambiguous Planning command.",
                                    )))?;
                                let factory = PlanningReleaseFactoryImpl;
                                let mut ports = PgPlanningScenarios::with_release(connection, &factory);
                                review_recommendation(&mut ports, &actor, &key, &recommendation_id, request, now).await
                            }
                            PlanningCommandHttpCommand::CreateResource { body, .. } => {
                                let request: ResourceRequest = serde_json::from_value(body)
                                    .map_err(|_| PgTransactionError::Business(BusinessError::invalid(
                                        "PLANNING_REQUEST_INVALID",
                                        "Use one unambiguous Planning command.",
                                    )))?;
                                let mut ports = PgPlanningResources::new(connection);
                                save_resource(&mut ports, &actor, &key, None, request, now).await
                            }
                            PlanningCommandHttpCommand::UpdateResource { resource_id, body, .. } => {
                                let request: ResourceRequest = serde_json::from_value(body)
                                    .map_err(|_| PgTransactionError::Business(BusinessError::invalid(
                                        "PLANNING_REQUEST_INVALID",
                                        "Use one unambiguous Planning command.",
                                    )))?;
                                let mut ports = PgPlanningResources::new(connection);
                                save_resource(&mut ports, &actor, &key, Some(&resource_id), request, now).await
                            }
                            PlanningCommandHttpCommand::SaveCalendar { resource_id, body, .. } => {
                                let request: CalendarRequest = serde_json::from_value(body)
                                    .map_err(|_| PgTransactionError::Business(BusinessError::invalid(
                                        "PLANNING_REQUEST_INVALID",
                                        "Use one unambiguous Planning command.",
                                    )))?;
                                let mut ports = PgPlanningResources::new(connection);
                                save_calendar(&mut ports, &actor, &key, &resource_id, request, now).await
                            }
                        }?;
                        Ok(PlanningCommandHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }

    pub async fn specification_command<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        command: SpecificationHttpCommand,
    ) -> Result<SpecificationHttpResponse, Error>
    where
        Input: FnOnce() -> Result<HoldingWorkspaceRequest, BusinessError> + Send + 'static,
    {
        let holding = self.configuration.holding.clone();
        let company = self.configuration.company.clone();
        let clock = self.clock.clone();
        let expected = company.clone();
        let input = move || {
            let mut request = input()?;
            if request.company_id != expected {
                return Err(scope());
            }
            request.mutation = true;
            Ok(request)
        };
        self.authorizer
            .execute_with_sources_deferred(
                host,
                proof,
                input,
                Self::specification_access(),
                ManufacturingSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let clock = clock.clone();
                    let command = command.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        let actor = bom_actor(current)?;
                        let key = match &command {
                            SpecificationHttpCommand::Create { idempotency_key, .. }
                            | SpecificationHttpCommand::Review { idempotency_key, .. } => {
                                erpx_server_shared::public::CommandKey::new(idempotency_key.clone())
                                    .map_err(PgTransactionError::Business)?
                            }
                        };
                        let now = clock.now();
                        let data = match command {
                            SpecificationHttpCommand::Create { body, .. } => {
                                let request: SpecificationCreateRequest = decode_specification(body)?;
                                let record_id = format!("mfg-spec-{}", key.as_str());
                                let witness = admit_specification(
                                    connection,
                                    &holding,
                                    &company,
                                    &actor,
                                    &key,
                                    erpx_server_manufacturing::public::specification::CREATE_OPERATION,
                                    &record_id,
                                    CapabilityPurpose::NewWork,
                                    now,
                                )
                                .await?;
                                let data = {
                                    let mut repository = PgSpecificationCommands::new(&mut *connection);
                                    create_specification(&mut repository, &actor, &key, request, now)
                                        .await?
                                };
                                if let Some(witness) = &witness {
                                    revalidate_specification(
                                        connection, &holding, &company, witness, now,
                                    )
                                    .await?;
                                }
                                Ok::<Value, PgTransactionError<BusinessError>>(data)
                            }
                            SpecificationHttpCommand::Review {
                                specification_id,
                                body,
                                ..
                            } => {
                                let request: SpecificationReviewRequest = decode_specification(body)?;
                                let action = request.action;
                                let purpose = match action {
                                    ReviewAction::Approve => CapabilityPurpose::NewWork,
                                    ReviewAction::Reject | ReviewAction::Retire => {
                                        CapabilityPurpose::SettleExisting
                                    }
                                };
                                let witness = admit_specification(
                                    connection,
                                    &holding,
                                    &company,
                                    &actor,
                                    &key,
                                    action.operation(),
                                    &specification_id,
                                    purpose,
                                    now,
                                )
                                .await?;
                                let data = {
                                    let mut repository = PgSpecificationCommands::new(&mut *connection);
                                    review_specification(
                                        &mut repository,
                                        &actor,
                                        &key,
                                        &specification_id,
                                        request,
                                        now,
                                    )
                                    .await?
                                };
                                if let Some(witness) = &witness {
                                    revalidate_specification(
                                        connection, &holding, &company, witness, now,
                                    )
                                    .await?;
                                }
                                Ok::<Value, PgTransactionError<BusinessError>>(data)
                            }
                        }?;
                        Ok(SpecificationHttpResponse {
                            data,
                            context: context(current),
                        })
                    })
                },
            )
            .await
    }
}

/// The retained route sends its `entity` discriminator and review `id`; the
/// extracted native bodies reject unknown fields, so remove only those two
/// transport selectors before the typed decode.
fn decode_specification<T: serde::de::DeserializeOwned>(body: Value) -> Result<T, BusinessError> {
    let Value::Object(mut object) = body else {
        return Err(BusinessError::invalid(
            "MANUFACTURING_REQUEST_INVALID",
            "Use one unambiguous Manufacturing specification command.",
        ));
    };
    object.remove("entity");
    object.remove("id");
    serde_json::from_value(Value::Object(object)).map_err(|_| {
        BusinessError::invalid(
            "MANUFACTURING_REQUEST_INVALID",
            "Use one unambiguous Manufacturing specification command.",
        )
    })
}

/// `ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT` mirrors the retained development
/// rollout switch. Invalid values fail closed; non-local environments may not
/// enable enforcement.
fn manufacturing_purchase_mode() -> Result<PackageEnforcementMode, BusinessError> {
    let raw = std::env::var("ERPX_MANUFACTURING_PACKAGE_ENFORCEMENT")
        .unwrap_or_else(|_| "disabled".to_owned());
    let mode = match raw.as_str() {
        "disabled" => PackageEnforcementMode::Disabled,
        "shadow" => PackageEnforcementMode::Shadow,
        "enforced" => PackageEnforcementMode::Enforced,
        _ => {
            return Err(BusinessError::new(
                FailureKind::Unavailable,
                "PACKAGE_ENFORCEMENT_CONFIGURATION",
                "Package enforcement is unavailable.",
            ));
        }
    };
    if mode != PackageEnforcementMode::Disabled
        && std::env::var("ERPX_ENVIRONMENT").as_deref() != Ok("local")
    {
        return Err(BusinessError::new(
            FailureKind::Unavailable,
            "PACKAGE_ENFORCEMENT_ENVIRONMENT",
            "Package enforcement is unavailable in this environment.",
        ));
    }
    Ok(mode)
}

/// Inspect the current Company purchase before the owner effect. A replay is
/// admitted under `history`, matching the retained command boundary.
#[allow(clippy::too_many_arguments)]
async fn admit_specification(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    actor: &BomActor,
    key: &erpx_server_shared::public::CommandKey,
    operation: &str,
    record_id: &str,
    purpose: CapabilityPurpose,
    now: TimestampMillis,
) -> Result<Option<PurchaseWitness>, Error> {
    let mode = manufacturing_purchase_mode()?;
    if mode == PackageEnforcementMode::Disabled {
        return Ok(None);
    }
    let replayed: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM command_receipts WHERE command_id=$1 AND operation=$2 \
         AND record_id=$3 AND actor_id=$4 AND authority_json::jsonb->>'companyId'=$5)",
    )
    .bind(key.as_str())
    .bind(operation)
    .bind(record_id)
    .bind(&actor.user_id)
    .bind(actor.company_id.as_str())
    .fetch_one(&mut *connection)
    .await?;
    let purpose = if replayed {
        CapabilityPurpose::History
    } else {
        purpose
    };
    let catalogue = current_package_catalogue()?;
    let mut reader =
        PgCompanyPurchaseReader::new(holding.clone(), company.clone(), connection, &catalogue);
    let inspection = reader
        .inspect(
            &PurchaseRequest {
                capabilities: vec!["manufacturing.bom".to_owned()],
                purpose,
                mode,
            },
            now,
        )
        .await?;
    Ok(Some(inspection.require_allowed()?.clone()))
}

/// Revalidate the admission witness before the owner unit commits.
async fn revalidate_specification(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    witness: &PurchaseWitness,
    now: TimestampMillis,
) -> Result<(), Error> {
    let catalogue = current_package_catalogue()?;
    let mut reader =
        PgCompanyPurchaseReader::new(holding.clone(), company.clone(), connection, &catalogue);
    reader.revalidate(witness, now).await?;
    Ok(())
}

struct PlanningReleasePortImpl<'a> {
    connection: &'a mut PgConnection,
}

impl PlanningReleasePort for PlanningReleasePortImpl<'_> {
    fn release<'a>(&'a mut self, plan: &'a ManufacturingOrderPlan) -> ReleaseFuture<'a, ()> {
        Box::pin(async move {
            sqlx::query(
                "INSERT INTO manufacturing_orders(id,company_id,branch_id,warehouse_id,recommendation_id,specification_id,specification_version,planned_quantity_milli,status,created_by,created_at,updated_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'planned',$9,$10,$10)",
            )
            .bind(&plan.order_id)
            .bind(&plan.company_id)
            .bind(&plan.branch_id)
            .bind(&plan.warehouse_id)
            .bind(&plan.recommendation_id)
            .bind(&plan.specification_id)
            .bind(plan.specification_version)
            .bind(plan.planned_quantity_milli)
            .bind(&plan.user_id)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await
            .map_err(|_| BusinessError::new(
                FailureKind::Unavailable,
                "PLANNING_RELEASE_UNAVAILABLE",
                "The manufacturing release could not be recorded.",
            ))?;
            sqlx::query(
                "INSERT INTO manufacturing_order_events VALUES($1,$2,1,'release',$3,$4,'planned','{}',$5)",
            )
            .bind(&plan.event_id)
            .bind(&plan.order_id)
            .bind(&plan.user_id)
            .bind(&plan.reason)
            .bind(plan.now)
            .execute(&mut *self.connection)
            .await
            .map_err(|_| BusinessError::new(
                FailureKind::Unavailable,
                "PLANNING_RELEASE_UNAVAILABLE",
                "The manufacturing release could not be recorded.",
            ))?;
            Ok(())
        })
    }
}

struct PlanningReleaseFactoryImpl;

impl PlanningReleaseFactory for PlanningReleaseFactoryImpl {
    fn create<'a>(&'a self, connection: &'a mut PgConnection) -> Box<dyn PlanningReleasePort + 'a> {
        Box::new(PlanningReleasePortImpl { connection })
    }
}

pub struct ManufacturingBackend<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    service: Arc<ManufacturingService<C>>,
}

impl<C: Clock + Send + Sync + 'static> ManufacturingBackend<C> {
    pub fn new(
        policy: Arc<AccountPolicy>,
        scopes: Arc<dyn BusinessScopeSlugs>,
        service: Arc<ManufacturingService<C>>,
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


impl<C: Clock + Send + Sync + 'static> PlanningCommandHttpBackend for ManufacturingBackend<C> {
    fn execute<'a>(
        &'a self,
        request: PlanningCommandHttpRequest<'a>,
    ) -> PlanningCommandHttpFuture<'a, PlanningCommandHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .planning_command(request.host, proof, move || Ok(selection), request.command)
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> BomHttpBackend for ManufacturingBackend<C> {
    fn read<'a>(&'a self, request: BomHttpRequest<'a>) -> BomHttpFuture<'a, BomHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .bom(request.host, proof, move || Ok(selection))
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> SpecificationHttpBackend for ManufacturingBackend<C> {
    fn execute<'a>(
        &'a self,
        request: SpecificationHttpRequest<'a>,
    ) -> SpecificationHttpFuture<'a, SpecificationHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .specification_command(
                    request.host,
                    proof,
                    move || Ok(selection),
                    request.command,
                )
                .await
                .map_err(business)
        })
    }
}

impl<C: Clock + Send + Sync + 'static> PlanningHttpBackend for ManufacturingBackend<C> {
    fn read<'a>(
        &'a self,
        request: PlanningHttpRequest<'a>,
    ) -> PlanningHttpFuture<'a, PlanningHttpResponse> {
        Box::pin(async move {
            let (proof, selection) = self.context(request.headers)?;
            self.service
                .planning(request.host, proof, move || Ok(selection))
                .await
                .map_err(business)
        })
    }
}

pub type BomConfiguration = ManufacturingConfiguration;
pub type BomService<C> = ManufacturingService<C>;
pub type BomBackend<C> = ManufacturingBackend<C>;

pub type PlanningConfiguration = ManufacturingConfiguration;
pub type PlanningService<C> = ManufacturingService<C>;
pub type PlanningBackend<C> = ManufacturingBackend<C>;

pub type ManufacturingBomConfiguration = ManufacturingConfiguration;
pub type ManufacturingBomService<C> = ManufacturingService<C>;
pub type ManufacturingBomBackend<C> = ManufacturingBackend<C>;

pub type ManufacturingPlanningConfiguration = ManufacturingConfiguration;
pub type ManufacturingPlanningService<C> = ManufacturingService<C>;
pub type ManufacturingPlanningBackend<C> = ManufacturingBackend<C>;
