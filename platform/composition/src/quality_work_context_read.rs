//! Private WorkContext-header ingress for the native Quality work/detail_core
//! read owner port. Account/Holding cookies and the rendered WorkContext
//! headers are verified again inside the owner's serializable unit; this layer
//! supplies no session, Company, permission or purchase authority. The router
//! is mounted by the lead with exact registered Holding targets and returns a
//! validated WorkContext snapshot beside the native payload so the caller can
//! reject a context change before consuming the data.
use crate::business_http::BusinessScopeSlugs;
use axum::{
    Json, Router,
    extract::{Path, Request, State},
    http::{HeaderMap, HeaderName, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::get,
};
use erpx_commercial_server::public::purchases::{
    CapabilityPurpose, PackageEnforcementMode, PurchaseInspection, PgCompanyPurchaseReader,
    PurchaseRequest, current_package_catalogue, lock_commercial_authority_sources,
};
use erpx_identity_access_server::public::{
    AccountPolicy, DeviceProof, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::parse_inbound_account_request,
    effective_authority::{
        AuthorizedWorkContext, HoldingWorkAuthorizer, HoldingWorkSourcePreparation,
        WorkAuthorizationRequest,
    },
    work_context::HoldingWorkspaceRequest,
};
use erpx_quality_server::public::quality_http::{PgQualityRead, lock_quality_read_sources};
use erpx_quality_server::public::quality_read::{
    QualityActor, quality_detail_core_with_same_person,
};
use erpx_quality_server::public::quality_work::{
    FnSourceAccess, QualitySourceAccess, quality_work_with_same_person,
};
use erpx_platform_runtime_server::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_platform_shared_server::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, TimestampMillis, UserId,
};
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::{collections::BTreeMap, sync::Arc};

type Error = PgTransactionError<BusinessError>;

fn scope() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "QUALITY_SCOPE_REQUIRED",
        "Choose the current authorized Company workspace.",
    )
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "QUALITY_WORK_CONTEXT_REQUEST_INVALID",
        "Use one bounded Quality work-context read.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "QUALITY_WORK_CONTEXT_UNAVAILABLE",
        "The native Quality work-context reader is unavailable.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "QUALITY_WORK_CONTEXT_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn business(error: Error) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

#[derive(Clone, Copy)]
struct QualityWorkContextSources;

impl HoldingWorkSourcePreparation for QualityWorkContextSources {
    fn prepare<'c>(&'c self, c: &'c mut PgConnection) -> PgWorkFuture<'c, (), BusinessError> {
        Box::pin(async move {
            lock_commercial_authority_sources(c)
                .await
                .map_err(business)?;
            lock_quality_read_sources(c).await.map_err(business)?;
            Ok(())
        })
    }
}

fn purchase_mode() -> Result<PackageEnforcementMode, BusinessError> {
    let raw = std::env::var("ERPX_COMMERCE_PACKAGE_ENFORCEMENT")
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

async fn purchase_access(
    connection: &mut PgConnection,
    holding: &HoldingId,
    company: &CompanyId,
    now: TimestampMillis,
) -> Result<Option<QualitySourceAccess>, Error> {
    if purchase_mode().map_err(PgTransactionError::Business)? == PackageEnforcementMode::Disabled {
        return Ok(None);
    }
    let catalogue = current_package_catalogue().map_err(PgTransactionError::Business)?;
    let mut reader =
        PgCompanyPurchaseReader::new(holding.clone(), company.clone(), connection, &catalogue);
    let request = |purpose| PurchaseRequest {
        capabilities: vec!["supply.quality".into()],
        purpose,
        mode: PackageEnforcementMode::Enforced,
    };
    let allowed = |inspection: &PurchaseInspection| {
        inspection
            .access
            .iter()
            .find(|access| access.capability == "supply.quality")
            .is_some_and(|access| access.allowed_by_mode)
    };
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
    Ok(Some(QualitySourceAccess {
        new_work: allowed(&new_work),
        settle_existing: allowed(&settle),
        history: allowed(&history),
        revision: history.revision.clone(),
    }))
}

fn actor(current: &AuthorizedWorkContext) -> QualityActor {
    let selected = current.selection();
    let workspace = selected.workspace();
    QualityActor {
        user_id: current.user_id().clone(),
        company_id: workspace.company_id.clone(),
        branch_id: workspace.branch_id.clone(),
        department_id: workspace.department_id.clone(),
        permissions: current.command_iam_metadata().permissions.clone(),
    }
}

fn same_person(current: &AuthorizedWorkContext) -> impl Fn(&str) -> bool + '_ {
    move |user: &str| {
        UserId::new(user.to_owned()).is_ok_and(|id| current.same_person_user(&id))
    }
}

fn context(current: &AuthorizedWorkContext) -> Value {
    let selected = current.selection();
    let workspace = selected.workspace();
    let assignment = selected.assignment();
    json!({
        "holdingId": current.holding_id().as_str(),
        "userId": current.user_id().as_str(),
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
            erpx_identity_access_server::public::work_context::WorkScopeKind::Holding => "holding",
            erpx_identity_access_server::public::work_context::WorkScopeKind::Branch => "branch",
            erpx_identity_access_server::public::work_context::WorkScopeKind::Department => "department",
        },
        "modules": current.modules(),
        "permissions": current.command_iam_metadata().permissions,
        "expiresAt": current.expires_at(),
    })
}

fn decision_json(value: &erpx_quality_server::public::quality_read::QualityDecision) -> Value {
    json!({
        "canInspect": value.can_inspect,
        "canRelease": value.can_release,
        "canCancel": value.can_cancel,
        "canReviewCancellation": value.can_review_cancellation,
        "canDisposition": value.can_disposition,
    })
}

fn inspection_json(row: &erpx_quality_server::public::quality_read::QualityInspection) -> Value {
    json!({
        "id": row.id,
        "holdId": row.hold_id,
        "reviewedVersion": row.reviewed_version,
        "result": row.result.as_str(),
        "evidence": row.evidence,
        "inspectedBy": row.inspected_by,
        "createdAt": row.created_at,
        "specId": row.spec_id,
        "specVersion": row.spec_version,
    })
}

fn measurement_json(row: &erpx_quality_server::public::quality_read::QualityMeasurement) -> Value {
    json!({
        "id": row.id,
        "inspectionId": row.inspection_id,
        "metricId": row.metric_id,
        "code": row.code,
        "label": row.label,
        "unit": row.unit,
        "minimum": row.minimum,
        "maximum": row.maximum,
        "measuredValue": row.measured_value,
        "passed": row.passed,
        "ordinal": row.ordinal,
    })
}

fn disposition_json(row: &erpx_quality_server::public::quality_read::QualityDisposition) -> Value {
    json!({
        "id": row.id,
        "holdId": row.hold_id,
        "countId": row.count_id,
        "action": row.action,
        "actorId": row.actor_id,
        "reviewedVersion": row.reviewed_version,
        "reason": row.reason,
        "createdAt": row.created_at,
    })
}

fn metric_json(row: &erpx_quality_server::public::quality_read::QualityMetric) -> Value {
    json!({
        "id": row.id,
        "specId": row.spec_id,
        "code": row.code,
        "label": row.label,
        "unit": row.unit,
        "minimum": row.minimum,
        "maximum": row.maximum,
        "ordinal": row.ordinal,
    })
}

fn alert_json(row: &erpx_quality_server::public::quality_read::QualityAlert) -> Value {
    json!({
        "id": row.id,
        "holdId": row.hold_id,
        "phase": row.phase,
        "sourceVersion": row.source_version,
        "dueDate": row.due_date,
        "detectedAt": row.detected_at,
        "version": row.version,
        "acknowledgedBy": row.acknowledged_by,
        "acknowledgedAt": row.acknowledged_at,
        "reason": row.reason,
    })
}

fn hold_json(view: &erpx_quality_server::public::quality_read::QualityHoldView) -> Value {
    let h = &view.hold;
    let p = &view.product;
    let raw = json!({
        "id": h.id,
        "companyId": h.company_id,
        "branchId": h.branch_id,
        "warehouseId": h.warehouse_id,
        "productId": h.product_id,
        "lotId": h.lot_id,
        "reason": h.reason,
        "dueDate": h.due_date,
        "status": h.status.as_str(),
        "createdBy": h.created_by,
        "lastInspector": h.last_inspector,
        "releasedBy": h.released_by,
        "releaseReason": h.release_reason,
        "releasedAt": h.released_at,
        "version": h.version,
        "createdAt": h.created_at,
        "updatedAt": h.updated_at,
        "specId": h.spec_id,
        "specVersion": h.spec_version,
        "quantityScope": h.quantity_scope,
    });
    let source = json!({
        "heldQuantityMilli": h.held_quantity_milli,
        "sourceType": h.source_type,
        "sourceId": h.source_id,
        "cancelRequestedBy": h.cancel_requested_by,
        "cancelReason": h.cancel_reason,
        "cancelledBy": h.cancelled_by,
        "cancelledAt": h.cancelled_at,
        "dispositionKind": h.disposition_kind,
        "dispositionReason": h.disposition_reason,
        "dispositionRequestedBy": h.disposition_requested_by,
        "dispositionCountId": h.disposition_count_id,
        "dispositionQuantityMilli": h.disposition_quantity_milli,
        "disposedBy": h.disposed_by,
        "disposedAt": h.disposed_at,
        "inspectionCount": h.inspection_count,
    });
    let labels = json!({
        "productName": p.product_name,
        "productUnit": p.product_unit,
        "trackingMode": p.tracking_mode,
        "warehouseName": p.warehouse_name,
        "lotCode": p.lot_code,
        "serialNumber": p.serial_number,
        "expiryDate": p.expiry_date,
        "eligibleCount": p.eligible_count,
        "lastResult": h.last_result.map(|value| value.as_str()),
        "decision": decision_json(&view.decision),
        "nextAction": view.next_action,
    });
    let mut out = serde_json::Map::new();
    for part in [raw, source, labels] {
        if let Some(object) = part.as_object() {
            for (key, value) in object {
                out.insert(key.clone(), value.clone());
            }
        }
    }
    Value::Object(out)
}

#[derive(Clone, Debug)]
pub struct QualityWorkContextConfiguration {
    holding: HoldingId,
    company: CompanyId,
}

impl QualityWorkContextConfiguration {
    pub fn new(holding: HoldingId, company: CompanyId) -> Self {
        Self { holding, company }
    }
}

pub struct QualityWorkContextService<C> {
    authorizer: Arc<HoldingWorkAuthorizer>,
    configuration: QualityWorkContextConfiguration,
    clock: Arc<C>,
}

impl<C: Clock + Send + Sync + 'static> QualityWorkContextService<C> {
    pub fn new(
        authorizer: Arc<HoldingWorkAuthorizer>,
        configuration: QualityWorkContextConfiguration,
        clock: Arc<C>,
    ) -> Self {
        Self {
            authorizer,
            configuration,
            clock,
        }
    }

    fn access() -> WorkAuthorizationRequest {
        WorkAuthorizationRequest {
            permission: "quality.read".into(),
            module: Some("quality".into()),
            destination: "quality".into(),
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

    pub async fn work<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
    ) -> Result<Value, Error>
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
                QualityWorkContextSources,
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
                        current.require_permission("quality.read")?;
                        let now = clock.now();
                        let access = purchase_access(&mut *connection, &holding, &company, now)
                            .await?
                            .map(|access| {
                                FnSourceAccess::new(move |_hold_id: &str| Some(access.clone()))
                            });
                        let actor = actor(&current);
                        let mut repository = PgQualityRead { connection };
                        let work = match access {
                            Some(access) => {
                                quality_work_with_same_person(
                                    &actor,
                                    &mut repository,
                                    &access,
                                    same_person(&current),
                                )
                                .await?
                            }
                            None => {
                                let none = FnSourceAccess::new(|_hold_id: &str| None);
                                quality_work_with_same_person(
                                    &actor,
                                    &mut repository,
                                    &none,
                                    same_person(&current),
                                )
                                .await?
                            }
                        };
                        let snapshot = context(&current);
                        Ok(json!({"data": {"items": work.items}, "context": snapshot}))
                    })
                },
            )
            .await
    }

    pub async fn detail_core<Input>(
        &self,
        host: &str,
        proof: DeviceProof,
        input: Input,
        id: String,
    ) -> Result<Value, Error>
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
                QualityWorkContextSources,
                move |connection, current| {
                    let holding = holding.clone();
                    let company = company.clone();
                    let id = id.clone();
                    let clock = clock.clone();
                    Box::pin(async move {
                        if current.holding_id() != &holding
                            || current.selection().workspace().company_id != company
                        {
                            return Err(scope().into());
                        }
                        current.require_permission("quality.read")?;
                        let now = clock.now();
                        let witness = {
                            let catalogue = current_package_catalogue().map_err(PgTransactionError::Business)?;
                            let mut purchase = PgCompanyPurchaseReader::new(
                                holding.clone(),
                                company.clone(),
                                connection,
                                &catalogue,
                            );
                            purchase
                                .inspect(
                                    &PurchaseRequest {
                                        capabilities: vec!["supply.quality".into()],
                                        purpose: CapabilityPurpose::History,
                                        mode: PackageEnforcementMode::Enforced,
                                    },
                                    now,
                                )
                                .await
                                .map_err(business)?
                                .require_allowed()?
                                .clone()
                        };
                        let actor = actor(&current);
                        let Some(source) = erpx_quality_server::public::quality_http::read_quality_detail_core(
                            connection,
                            &actor.company_id,
                            actor.branch_id.as_ref(),
                            &id,
                        )
                        .await?
                        else {
                            return Err(BusinessError::new(
                                FailureKind::NotFound,
                                "QUALITY_HOLD_NOT_FOUND",
                                "Quality hold not found.",
                            )
                            .into());
                        };
                        let catalogue = current_package_catalogue().map_err(PgTransactionError::Business)?;
                        PgCompanyPurchaseReader::new(
                            holding.clone(),
                            company.clone(),
                            connection,
                            &catalogue,
                        )
                        .revalidate(&witness, now)
                        .await
                        .map_err(business)?
                        .require_allowed()?;
                        let core = quality_detail_core_with_same_person(
                            &actor,
                            source.data().clone(),
                            &same_person(&current),
                        )?;
                        let hold = hold_json(&core.hold);
                        let snapshot = context(&current);
                        Ok(json!({
                            "data": {
                                "hold": hold,
                                "inspections": core.inspections.iter().map(inspection_json).collect::<Vec<_>>(),
                                "measurements": core.measurements.iter().map(measurement_json).collect::<Vec<_>>(),
                                "dispositions": core.dispositions.iter().map(disposition_json).collect::<Vec<_>>(),
                                "specMetrics": core.spec_metrics.iter().map(metric_json).collect::<Vec<_>>(),
                                "alerts": core.alerts.iter().map(alert_json).collect::<Vec<_>>(),
                                "cancellationHistory": "paged",
                            },
                            "context": snapshot,
                        }))
                    })
                },
            )
            .await
    }
}

#[derive(Clone)]
pub struct QualityWorkContextTarget<C> {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub service: Arc<QualityWorkContextService<C>>,
}

struct QualityWorkContextState<C> {
    policy: Arc<AccountPolicy>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: BTreeMap<(String, String), QualityWorkContextTarget<C>>,
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && !value
            .bytes()
            .any(|byte| matches!(byte, b'\x00'..=b'\x1f' | b'\x7f'))
}

fn unique(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed()))
        .transpose()
}

fn status(kind: FailureKind) -> StatusCode {
    match kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    }
}

fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(json!({"error": {"code": error.code, "message": error.message}})),
    )
        .into_response()
}

fn bounded(body: Value) -> Response {
    let mut response = (StatusCode::OK, Json(body)).into_response();
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(value));
    }
    response
}

fn selection<C>(
    state: &QualityWorkContextState<C>,
    target: &QualityWorkContextTarget<C>,
    headers: &HeaderMap,
) -> Result<(DeviceProof, HoldingWorkspaceRequest), BusinessError> {
    let inbound = parse_inbound_account_request(&state.policy, headers)?;
    if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
        return Err(missing());
    }
    if unique(headers, header::ORIGIN)?.is_some_and(|origin| origin != target.origin) {
        return Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        ));
    }
    let (branch, department) = state.scopes.slugs(headers)?;
    Ok((
        inbound.into_proof(),
        HoldingWorkspaceRequest {
            company_id: target.service.configuration.company.clone(),
            branch_slug: branch,
            department_slug: department,
            expected: None,
            mutation: false,
        },
    ))
}

async fn work<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<QualityWorkContextState<C>>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        if !valid_slug(&slug) {
            return Err(malformed());
        }
        let host = unique(request.headers(), header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let target = state
            .targets
            .get(&(host.clone(), slug))
            .ok_or_else(missing)?;
        let (proof, input) = selection(&state, target, request.headers())?;
        target
            .service
            .work(&host, proof, move || Ok(input))
            .await
            .map_err(business)
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

async fn detail_core<C: Clock + Send + Sync + 'static>(
    State(state): State<Arc<QualityWorkContextState<C>>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if !valid_slug(&slug) || !valid_id(&id) {
            return Err(malformed());
        }
        let host = unique(request.headers(), header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let target = state
            .targets
            .get(&(host.clone(), slug))
            .ok_or_else(missing)?;
        let (proof, input) = selection(&state, target, request.headers())?;
        target
            .service
            .detail_core(&host, proof, move || Ok(input), id)
            .await
            .map_err(business)
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

/// Build the private GET router for the exact registered Holding targets.
pub fn quality_work_context_router<C: Clock + Send + Sync + 'static>(
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    scopes: Arc<dyn BusinessScopeSlugs>,
    targets: Vec<QualityWorkContextTarget<C>>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    let mut configured = BTreeMap::new();
    for target in targets {
        let valid = policy
            .audience(&target.host)
            .and_then(|audience| registry.expected_for_audience(&audience).ok())
            .is_some_and(|holding| holding.slug == target.holding_slug);
        if !valid
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    target,
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/quality/work-context/work",
            get(work::<C>),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/quality/work-context/holds/{id}/detail-core",
            get(detail_core::<C>),
        )
        .with_state(Arc::new(QualityWorkContextState {
            policy,
            scopes,
            targets: configured,
        })))
}
