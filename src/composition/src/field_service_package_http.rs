//! Service-only package lifecycle transport. One exact provider-entry Holding,
//! native Account/IAM/MFA and declared target resources; no legacy dispatch.
use crate::field_service_package_settlement::{
    apply_service_package_batch, overview_service_packages, preview_service_package_batch,
};
use axum::{
    Json, Router,
    body::to_bytes,
    extract::{Request, State},
    http::{HeaderMap, HeaderValue, StatusCode, header},
    response::{IntoResponse, Response},
    routing::{get, post},
};
use erpx_server_commercial::public::{
    package_review::PackageLifecycleReviewSigner, purchases::GrantBatch,
};
use erpx_server_identity_access::public::{
    AccountPolicy, RequestAudience,
    account_holding_bindings::ConfiguredHoldingRegistry,
    account_inbound_http::{parse_inbound_account_request, unique_request_header},
    platform_commercial_authority::*,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PersistenceError, PgRequestLease, PgTransactionError,
};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind, HoldingId};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};
type Error = PgTransactionError<BusinessError>;

pub struct ServicePackageHttpTarget {
    pub holding_id: HoldingId,
    pub audience: String,
    pub write: DatabasePool,
}
struct PackageState<C> {
    control: DatabasePool,
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    clock: Arc<C>,
    provider_host: String,
    provider_holding: HoldingId,
    targets: BTreeMap<HoldingId, ServicePackageHttpTarget>,
    signer: Arc<PackageLifecycleReviewSigner>,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct NativeServiceCompany {
    pub id: String,
    pub slug: String,
    pub name: String,
    pub holding_id: String,
    pub holding: String,
}
#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct NativeServiceCompanies {
    pub companies: Vec<NativeServiceCompany>,
    pub can_apply: bool,
}
#[allow(clippy::too_many_arguments)]
pub fn service_package_router<C: Clock + Send + Sync + 'static>(
    control: DatabasePool,
    policy: Arc<AccountPolicy>,
    registry: Arc<ConfiguredHoldingRegistry>,
    clock: Arc<C>,
    provider_host: String,
    provider_holding: HoldingId,
    targets: Vec<ServicePackageHttpTarget>,
    signer: Arc<PackageLifecycleReviewSigner>,
) -> Result<Router, BusinessError> {
    policy.validate()?;
    if !policy
        .audience(&provider_host)
        .and_then(|a| registry.expected_for_audience(&a).ok())
        .is_some_and(|h| h.holding_id == provider_holding)
    {
        return Err(configuration());
    }
    let mut configured = BTreeMap::new();
    for target in targets {
        if !registry
            .expected_for_audience(&RequestAudience::Holding {
                audience: target.audience.clone(),
            })
            .is_ok_and(|h| h.holding_id == target.holding_id)
            || configured
                .insert(target.holding_id.clone(), target)
                .is_some()
        {
            return Err(configuration());
        }
    }
    if configured.is_empty() {
        return Err(configuration());
    }
    Ok(Router::new()
        .route("/api/v1/platform/commercial", get(companies::<C>))
        .route("/api/v1/platform/commercial/packages", get(overview::<C>))
        .route(
            "/api/v1/platform/commercial/packages/preview",
            post(preview::<C>),
        )
        .route(
            "/api/v1/platform/commercial/packages/apply",
            post(apply::<C>),
        )
        .with_state(Arc::new(PackageState {
            control,
            policy,
            registry,
            clock,
            provider_host,
            provider_holding,
            targets: configured,
            signer,
        })))
}
fn configuration() -> BusinessError {
    BusinessError::invalid(
        "SERVICE_PACKAGE_CONFIGURATION_INVALID",
        "Use exact registered provider and native Service Holding resources.",
    )
}
fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn invalid() -> BusinessError {
    BusinessError::invalid(
        "VALIDATION_FAILED",
        "Use the reviewed Company package request.",
    )
}
async fn companies<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<PackageState<C>>>,
    request: Request,
) -> Response {
    let result=async move{
        if unique_request_header(request.headers(),"host")?!=Some(s.provider_host.as_str()){return Err(missing().into());}
        let inbound=parse_inbound_account_request(&s.policy,request.headers())?;
        let hint=assignment_hint(request.headers())?;let proof=inbound.into_proof();
        if proof.active.is_none(){return Err(BusinessError::new(FailureKind::Unauthenticated,"UNAUTHENTICATED","Sign in to continue.").into());}
        PgRequestLease::new(s.control.clone()).run(move|control|Box::pin(async move{
            let authority=verify_current_platform_commercial_authority(control,&s.provider_host,proof,(*s.policy).clone(),s.registry.clone(),&s.provider_holding,hint.as_deref(),PlatformCommercialOperation::Read,s.clock.now().get()).await?;
            let mut companies=Vec::new();
            for target in s.targets.values(){
                let binding=s.registry.read_current(control,&RequestAudience::Holding{audience:target.audience.clone()}).await?;
                let name=binding.display_name(control).await?;
                let mut unit=target.write.raw().begin().await?;
                sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unit).await?;
                erpx_server_organization::public::lock_hierarchy_sources(&mut unit).await?;
                let rows=erpx_server_organization::public::commercial_companies::read_commercial_companies(&mut unit).await?;
                binding.require_current(control).await?;authority.require_current(control,s.clock.now().get()).await?;
                unit.commit().await?;
                for row in rows{companies.push(NativeServiceCompany{id:row.id,slug:row.slug,name:row.name,holding_id:target.holding_id.as_str().into(),holding:name.clone()});}
            }
            authority.require_current(control,s.clock.now().get()).await?;
            let data=serde_json::to_value(NativeServiceCompanies{companies,can_apply:authority.can_manage()}).map_err(|_|PersistenceError::new("SERVICE_PACKAGE_RESPONSE_UNAVAILABLE"))?;
            if serde_json::to_vec(&data).map_err(|_|PersistenceError::new("SERVICE_PACKAGE_RESPONSE_UNAVAILABLE"))?.len()>2*1024*1024{return Err(BusinessError::new(FailureKind::Unavailable,"COMMERCIAL_COMPANY_LABEL_LIMIT","Company labels exceed this bounded read.").into());}Ok(data)
        })).await
    }.await;
    respond(result)
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServicePackagePreviewRequest {
    pub holding_id: String,
    pub request: GrantBatch,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
pub struct ServicePackageApplyRequest {
    pub holding_id: String,
    pub request: GrantBatch,
    pub token: String,
}
enum Action {
    Overview { holding: String, company: String },
    Preview(ServicePackagePreviewRequest),
    Apply(ServicePackageApplyRequest),
}
enum InboundAction {
    Overview {
        holding: String,
        company: String,
    },
    Mutation {
        apply: bool,
        value: serde_json::Value,
        key: Option<String>,
    },
}
impl InboundAction {
    fn operation(&self) -> PlatformCommercialOperation {
        if matches!(self, Self::Mutation { apply: true, .. }) {
            PlatformCommercialOperation::Apply
        } else {
            PlatformCommercialOperation::Read
        }
    }
    fn parse(self) -> Result<Action, BusinessError> {
        use erpx_server_commercial::public::purchases::normalize_lifecycle_batch;
        match self {
            Self::Overview { holding, company } => Ok(Action::Overview { holding, company }),
            Self::Mutation {
                apply: true,
                value,
                key,
            } => {
                let mut input: ServicePackageApplyRequest =
                    serde_json::from_value(value).map_err(|_| invalid())?;
                input.request = normalize_lifecycle_batch(input.request)?;
                if input.token.is_empty() || input.token.len() > 8192 {
                    return Err(invalid());
                }
                if key.as_deref() != Some(input.request.idempotency_key.as_str()) {
                    return Err(BusinessError::invalid(
                        "IDEMPOTENCY_KEY_REQUIRED",
                        "Use the reviewed Idempotency-Key.",
                    ));
                }
                Ok(Action::Apply(input))
            }
            Self::Mutation {
                apply: false,
                value,
                ..
            } => {
                let mut input: ServicePackagePreviewRequest =
                    serde_json::from_value(value).map_err(|_| invalid())?;
                input.request = normalize_lifecycle_batch(input.request)?;
                Ok(Action::Preview(input))
            }
        }
    }
}
impl Action {
    fn holding(&self) -> &str {
        match self {
            Self::Overview { holding, .. } => holding,
            Self::Preview(v) => &v.holding_id,
            Self::Apply(v) => &v.holding_id,
        }
    }
    fn company(&self) -> &str {
        match self {
            Self::Overview { company, .. } => company,
            Self::Preview(v) => &v.request.company_id,
            Self::Apply(v) => &v.request.company_id,
        }
    }
}
async fn overview<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<PackageState<C>>>,
    request: Request,
) -> Response {
    let result = async move {
        if unique_request_header(request.headers(), "host")? != Some(s.provider_host.as_str()) {
            return Err(missing().into());
        }
        let query: Vec<_> =
            url::form_urlencoded::parse(request.uri().query().unwrap_or("").as_bytes())
                .into_owned()
                .collect();
        if query.iter().filter(|(k, _)| k == "holdingId").count() != 1
            || query.iter().filter(|(k, _)| k == "companyId").count() != 1
        {
            return Err(BusinessError::invalid(
                "COMPANY_REQUIRED",
                "Select one Company and Holding.",
            )
            .into());
        }
        if query.len() != 2 {
            return Err(invalid().into());
        }
        let holding = query
            .iter()
            .find(|(k, _)| k == "holdingId")
            .unwrap()
            .1
            .clone();
        let company = query
            .iter()
            .find(|(k, _)| k == "companyId")
            .unwrap()
            .1
            .clone();
        if [&holding, &company]
            .iter()
            .any(|s| s.is_empty() || s.encode_utf16().count() > 100)
        {
            return Err(invalid().into());
        }
        run(
            s,
            request.headers().clone(),
            InboundAction::Overview { holding, company },
        )
        .await
    }
    .await;
    respond(result)
}
async fn preview<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<PackageState<C>>>,
    request: Request,
) -> Response {
    submit(s, request, false).await
}
async fn apply<C: Clock + Send + Sync + 'static>(
    State(s): State<Arc<PackageState<C>>>,
    request: Request,
) -> Response {
    submit(s, request, true).await
}
async fn submit<C: Clock + Send + Sync + 'static>(
    s: Arc<PackageState<C>>,
    request: Request,
    is_apply: bool,
) -> Response {
    let result = async {
        let (parts, body) = request.into_parts();
        if unique_request_header(&parts.headers, "host")? != Some(s.provider_host.as_str()) {
            return Err(missing().into());
        }
        let inbound = parse_inbound_account_request(&s.policy, &parts.headers)?;
        if unique_request_header(&parts.headers, "origin")? != Some(inbound.expected_origin()) {
            return Err(BusinessError::new(
                FailureKind::Forbidden,
                "ORIGIN_FORBIDDEN",
                "Use ERPX Administration to submit this request.",
            )
            .into());
        }
        if unique_request_header(&parts.headers, "content-type")?
            .and_then(|h| h.split(';').next())
            .is_none_or(|h| !h.trim().eq_ignore_ascii_case("application/json"))
        {
            return Err(BusinessError::invalid("JSON_REQUIRED", "Submit a JSON request.").into());
        }
        let bytes = to_bytes(body, 800000).await.map_err(|_| {
            BusinessError::invalid("REQUEST_TOO_LARGE", "This request is too large.")
        })?;
        let text = String::from_utf8_lossy(&bytes);
        if text.encode_utf16().count() > 200000 {
            return Err(
                BusinessError::invalid("REQUEST_TOO_LARGE", "This request is too large.").into(),
            );
        }
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|_| BusinessError::invalid("INVALID_JSON", "Submit a valid JSON request."))?;
        let action = InboundAction::Mutation {
            apply: is_apply,
            value,
            key: unique_request_header(&parts.headers, "idempotency-key")?.map(str::to_owned),
        };
        run(s, parts.headers, action).await
    }
    .await;
    respond(result)
}
fn assignment_hint(headers: &HeaderMap) -> Result<Option<String>, BusinessError> {
    if let Some(hint) = unique_request_header(headers, "x-erpx-platform-assignment")? {
        return Ok(Some(hint.into()));
    }
    let mut found = None;
    for item in unique_request_header(headers, "cookie")?
        .unwrap_or("")
        .split(';')
    {
        if let Some((key, value)) = item.trim().split_once('=')
            && key == "erpx_assignment_platform"
        {
            if found.is_some() {
                return Err(invalid());
            }
            let encoded = format!("v={}", value.replace('+', "%2B"));
            found = Some(
                url::form_urlencoded::parse(encoded.as_bytes())
                    .next()
                    .ok_or_else(invalid)?
                    .1
                    .into_owned(),
            );
        }
    }
    Ok(found)
}
async fn run<C: Clock + Send + Sync + 'static>(
    s: Arc<PackageState<C>>,
    headers: HeaderMap,
    action: InboundAction,
) -> Result<serde_json::Value, Error> {
    if unique_request_header(&headers, "host")? != Some(s.provider_host.as_str()) {
        return Err(missing().into());
    }
    let inbound = parse_inbound_account_request(&s.policy, &headers)?;
    if !matches!(inbound.audience(), RequestAudience::Holding { .. }) {
        return Err(missing().into());
    }
    let hint = assignment_hint(&headers)?;
    let proof = inbound.into_proof();
    if proof.active.is_none() {
        return Err(BusinessError::new(
            FailureKind::Unauthenticated,
            "UNAUTHENTICATED",
            "Sign in to continue.",
        )
        .into());
    }
    PgRequestLease::new(s.control.clone()).run(move|control|Box::pin(async move{
        let authority=verify_current_platform_commercial_authority(control,&s.provider_host,proof,(*s.policy).clone(),s.registry.clone(),&s.provider_holding,hint.as_deref(),action.operation(),s.clock.now().get()).await?;
        let action=action.parse()?;
        if [action.holding(),action.company()].iter().any(|v|v.is_empty()||v.encode_utf16().count()>100){return Err(invalid().into());}
        let holding=HoldingId::new(action.holding())?;let company=CompanyId::new(action.company())?;
        let target=s.targets.get(&holding).ok_or_else(||BusinessError::new(FailureKind::NotFound,"HOLDING_NOT_FOUND","Select an active native Service Holding."))?;
        let binding=s.registry.read_current(control,&RequestAudience::Holding{audience:target.audience.clone()}).await?;
        if binding.holding_id()!=&holding{return Err(missing().into());}
        // Only the Holding unit retries; never replay the outer Control lease
        // around a committed cross-database side effect.
        for attempt in 0..4{
            let mut unit=target.write.raw().begin().await?;
            sqlx::query("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE").execute(&mut *unit).await?;
            sqlx::query("SET LOCAL lock_timeout='5s'").execute(&mut *unit).await?;
            let outcome:Result<serde_json::Value,Error>=async{
                let data=match &action{
                    Action::Overview{..}=>serde_json::to_value(overview_service_packages(&mut unit,control,&authority,s.clock.as_ref(),holding.clone(),company.clone()).await?),
                    Action::Preview(input)=>serde_json::to_value(preview_service_package_batch(&mut unit,control,&authority,s.clock.as_ref(),holding.clone(),&input.request,&s.signer).await?),
                    Action::Apply(input)=>Ok(serde_json::json!({"receipt":apply_service_package_batch(&mut unit,control,&authority,s.clock.as_ref(),holding.clone(),&input.request,&s.signer,&input.token).await?})),
                }.map_err(|_|PersistenceError::new("SERVICE_PACKAGE_RESPONSE_UNAVAILABLE"))?;
                binding.require_current(control).await?;
                authority.require_current(control,s.clock.now().get()).await?;Ok(data)
            }.await;
            let result=match outcome{Ok(data)=>unit.commit().await.map(|_|data).map_err(Error::from),Err(error)=>{unit.rollback().await?;Err(error)}};
            match result{Err(Error::Persistence(ref error))if error.retryable()&&attempt<3=>continue,other=>return other}
        }
        Err(PersistenceError::new("SERVICE_PACKAGE_RETRY_EXHAUSTED").into())
    })).await
}
fn respond(result: Result<serde_json::Value, Error>) -> Response {
    let mut response = match result {
        Ok(data) => Json(serde_json::json!({"data":data})).into_response(),
        Err(Error::Business(error)) => {
            let status = match error.code {
                "JSON_REQUIRED" => StatusCode::UNSUPPORTED_MEDIA_TYPE,
                "REQUEST_TOO_LARGE" => StatusCode::PAYLOAD_TOO_LARGE,
                "STALE_PACKAGE_BATCH"
                | "STALE_PACKAGE_GRANT"
                | "IDEMPOTENCY_CONFLICT"
                | "PACKAGE_DEPENDENCY_REQUIRED"
                | "PACKAGE_DEPENDENCY_PERIOD_REQUIRED"
                | "PACKAGE_DEPENDENCY_IN_USE"
                | "PACKAGE_DEPENDENCY_PERIOD_CONFLICT"
                | "DRAIN_PACKAGE_FIRST"
                | "PACKAGE_OBLIGATIONS_REMAIN"
                | "PACKAGE_OBLIGATION_SOURCES_REQUIRED" => StatusCode::CONFLICT,
                _ => match error.kind {
                    FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
                    FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
                    FailureKind::Forbidden => StatusCode::FORBIDDEN,
                    FailureKind::NotFound => StatusCode::NOT_FOUND,
                    FailureKind::Conflict => StatusCode::CONFLICT,
                    FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
                },
            };
            (
                status,
                Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
            )
                .into_response()
        }
        Err(Error::Persistence(error)) => {
            let (status, code, message) =
                if matches!(error.sql_state(), Some("55P03" | "40P01" | "40001")) {
                    (
                        StatusCode::CONFLICT,
                        "AUTHORITY_BUSY",
                        "Access is changing. Refresh and review again.",
                    )
                } else {
                    (
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "SERVICE_PACKAGE_UNAVAILABLE",
                        "The Company package operation is unavailable.",
                    )
                };
            (
                status,
                Json(serde_json::json!({"error":{"code":code,"message":message}})),
            )
                .into_response()
        }
    };
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
