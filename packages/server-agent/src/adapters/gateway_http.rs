//! Native Platform AI-gateway HTTP ingress.
//!
//! Transport facts only: one registered Host target, the retained
//! `/api/v1/platform/billing/ai-gateway` path, a bounded JSON body and the
//! exact application origin for mutations. Current Platform authority, the
//! target Holding database, policy/statement/cost effects and audit remain
//! composition and owner responsibilities behind the backend trait.
//!
//! Policy changes use the retained `expectedUpdatedAt` compare-and-set and
//! statement/cost writes are immutable inserts with exact-replay equality, so
//! no separate command-receipt namespace is introduced here.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes,
    header,
    routing::{get, post},
    serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;
const HOLDING_LIMIT: usize = 128;

pub type AiGatewayHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiGatewayStatementInput {
    pub id: String,
    pub provider: String,
    pub month: String,
    pub currency: String,
    pub amount_minor: i64,
    pub statement_digest: String,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct AiGatewayCostInput {
    pub request_id: String,
    pub provider_statement_id: String,
    pub currency: String,
    pub amount_minor: i64,
    pub thb_satang: i64,
    pub fx_evidence_ref: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum AiGatewayHttpCommand {
    SetEnabled(crate::domain::gateway::AiSetEnabledInput),
    UpdateLimits(crate::domain::gateway::AiPolicyChange),
    RecordStatement(AiGatewayStatementInput),
    RecordCost(AiGatewayCostInput),
    Reconcile(String),
}

pub struct AiGatewayHttpReadRequest<'a> {
    pub host: &'a str,
    pub headers: &'a HeaderMap,
    pub holding_id: HoldingId,
}

pub struct AiGatewayHttpCommandRequest<'a> {
    pub host: &'a str,
    pub headers: &'a HeaderMap,
    pub holding_id: HoldingId,
    pub command: AiGatewayHttpCommand,
}

pub trait AiGatewayHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: AiGatewayHttpReadRequest<'a>,
    ) -> AiGatewayHttpFuture<'a, serde_json::Value>;
    fn command<'a>(
        &'a self,
        request: AiGatewayHttpCommandRequest<'a>,
    ) -> AiGatewayHttpFuture<'a, serde_json::Value>;
}

pub struct AiGatewayHttpTarget {
    pub host: String,
    pub origin: String,
    pub backend: Arc<dyn AiGatewayHttpBackend>,
}

struct AiGatewayHttpState {
    targets: BTreeMap<String, AiGatewayHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "AI_GATEWAY_REQUEST_INVALID",
        "Use one bounded AI-gateway request.",
    )
}
fn holding_required() -> BusinessError {
    BusinessError::invalid("HOLDING_REQUIRED", "Select a Customer Holding.")
}
fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}
fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "AI_GATEWAY_CONFIGURATION_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}
fn valid_holding(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= HOLDING_LIMIT
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_'))
}
fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        let (host, port) = match rest.rsplit_once(':') {
            Some((host, port)) => (host, Some(port)),
            None => (rest, None),
        };
        !host.is_empty()
            && !host
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
            && port.is_none_or(|port| {
                (1..=5).contains(&port.len())
                    && port.bytes().all(|byte| byte.is_ascii_digit())
                    && port.parse::<u16>().is_ok_and(|value| value > 0)
            })
    })
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
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}
fn bounded(body: serde_json::Value, created: bool) -> Response {
    let mut response = (
        if created {
            StatusCode::CREATED
        } else {
            StatusCode::OK
        },
        Json(body),
    )
        .into_response();
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
fn respond(result: Result<serde_json::Value, BusinessError>, created: bool) -> Response {
    match result {
        Ok(data) => bounded(serde_json::json!({"data": data}), created),
        Err(error) => error_response(error),
    }
}
fn host(headers: &HeaderMap) -> Result<String, BusinessError> {
    let mut values = headers.get_all(header::HOST).iter();
    let value = values.next().ok_or_else(missing)?;
    if values.next().is_some() {
        return Err(malformed());
    }
    let value = value.to_str().map_err(|_| malformed())?;
    if value.is_empty() || value.len() > 255 {
        return Err(malformed());
    }
    Ok(value.to_owned())
}
fn target<'a>(
    state: &'a AiGatewayHttpState,
    headers: &HeaderMap,
) -> Result<&'a AiGatewayHttpTarget, BusinessError> {
    state.targets.get(&host(headers)?).ok_or_else(missing)
}
fn require_origin(
    target: &AiGatewayHttpTarget,
    headers: &HeaderMap,
    mutation: bool,
) -> Result<(), BusinessError> {
    let mut values = headers.get_all(header::ORIGIN).iter();
    let origin = values
        .next()
        .map(|value| value.to_str().map_err(|_| malformed()))
        .transpose()?;
    if values.next().is_some() {
        return Err(malformed());
    }
    if mutation && origin != Some(target.origin.as_str())
        || origin.is_some_and(|value| value != target.origin)
    {
        return Err(origin_forbidden());
    }
    Ok(())
}
fn json_body<'a, T: Deserialize<'a>>(body: &'a [u8]) -> Result<T, BusinessError> {
    serde_json::from_slice(body).map_err(|_| malformed())
}
fn required_holding(value: &str) -> Result<HoldingId, BusinessError> {
    if !valid_holding(value) {
        return Err(holding_required());
    }
    Ok(HoldingId::new(value)?)
}
fn query_holding(request: &Request) -> Result<HoldingId, BusinessError> {
    let Some(raw) = request.uri().query() else {
        return Err(holding_required());
    };
    if raw.len() > 512 {
        return Err(malformed());
    }
    let mut holding = None;
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name != "holdingId" || holding.is_some() {
            return Err(malformed());
        }
        holding = Some(value);
    }
    required_holding(holding.ok_or_else(holding_required)?)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct SetEnabledBody {
    holding_id: String,
    enabled: bool,
    expected_updated_at: i64,
    reason: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateLimitsBody {
    holding_id: String,
    enabled: bool,
    monthly_request_limit: i64,
    monthly_spend_cap_satang: i64,
    reservation_satang: i64,
    reason: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct StatementBody {
    holding_id: String,
    id: String,
    provider: String,
    month: String,
    currency: String,
    amount_minor: i64,
    statement_digest: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CostBody {
    holding_id: String,
    request_id: String,
    provider_statement_id: String,
    currency: String,
    amount_minor: i64,
    thb_satang: i64,
    fx_evidence_ref: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ReconcileBody {
    holding_id: String,
    statement_id: String,
}

fn statement_id(value: String) -> Result<String, BusinessError> {
    if value.is_empty() || value.len() > 200 || value.chars().any(char::is_control) {
        return Err(malformed());
    }
    Ok(value)
}

async fn read(State(state): State<Arc<AiGatewayHttpState>>, request: Request) -> Response {
    let headers = request.headers().clone();
    let result: Result<serde_json::Value, BusinessError> = async move {
        let target = target(&state, &headers)?;
        require_origin(target, &headers, false)?;
        let holding_id = query_holding(&request)?;
        target
            .backend
            .read(AiGatewayHttpReadRequest {
                host: &target.host,
                headers: &headers,
                holding_id,
            })
            .await
    }
    .await;
    respond(result, false)
}

async fn command(State(state): State<Arc<AiGatewayHttpState>>, request: Request) -> Response {
    dispatch(state, request, Vec::new()).await
}

async fn command_path(
    State(state): State<Arc<AiGatewayHttpState>>,
    Path(path): Path<String>,
    request: Request,
) -> Response {
    dispatch(
        state,
        request,
        path.split('/')
            .filter(|part| !part.is_empty())
            .map(str::to_owned)
            .collect(),
    )
    .await
}

async fn dispatch(state: Arc<AiGatewayHttpState>, request: Request, path: Vec<String>) -> Response {
    let headers = request.headers().clone();
    let body = match to_bytes(request.into_body(), BODY_LIMIT).await {
        Ok(body) => body,
        Err(_) => return error_response(malformed()),
    };
    let result: Result<serde_json::Value, BusinessError> = async move {
        let target = target(&state, &headers)?;
        require_origin(target, &headers, true)?;
        if headers
            .get(header::CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_none_or(|value| !value.starts_with("application/json"))
        {
            return Err(malformed());
        }
        let (holding_id, command) = match path.as_slice() {
            [] => {
                let body: SetEnabledBody = json_body(&body)?;
                (
                    required_holding(&body.holding_id)?,
                    AiGatewayHttpCommand::SetEnabled(crate::domain::gateway::AiSetEnabledInput {
                        enabled: body.enabled,
                        expected_updated_at: body.expected_updated_at,
                        reason: body.reason,
                    }),
                )
            }
            [limits] if limits == "limits" => {
                let body: UpdateLimitsBody = json_body(&body)?;
                (
                    required_holding(&body.holding_id)?,
                    AiGatewayHttpCommand::UpdateLimits(crate::domain::gateway::AiPolicyChange {
                        enabled: body.enabled,
                        monthly_request_limit: body.monthly_request_limit,
                        monthly_spend_cap_satang: body.monthly_spend_cap_satang,
                        reservation_satang: body.reservation_satang,
                        reason: body.reason,
                    }),
                )
            }
            [statements] if statements == "statements" => {
                let body: StatementBody = json_body(&body)?;
                (
                    required_holding(&body.holding_id)?,
                    AiGatewayHttpCommand::RecordStatement(AiGatewayStatementInput {
                        id: statement_id(body.id)?,
                        provider: body.provider,
                        month: body.month,
                        currency: body.currency,
                        amount_minor: body.amount_minor,
                        statement_digest: body.statement_digest,
                    }),
                )
            }
            [cost] if cost == "cost-evidence" => {
                let body: CostBody = json_body(&body)?;
                (
                    required_holding(&body.holding_id)?,
                    AiGatewayHttpCommand::RecordCost(AiGatewayCostInput {
                        request_id: statement_id(body.request_id)?,
                        provider_statement_id: statement_id(body.provider_statement_id)?,
                        currency: body.currency,
                        amount_minor: body.amount_minor,
                        thb_satang: body.thb_satang,
                        fx_evidence_ref: body.fx_evidence_ref,
                    }),
                )
            }
            [reconcile] if reconcile == "reconcile" => {
                let body: ReconcileBody = json_body(&body)?;
                (
                    required_holding(&body.holding_id)?,
                    AiGatewayHttpCommand::Reconcile(statement_id(body.statement_id)?),
                )
            }
            _ => return Err(missing()),
        };
        target
            .backend
            .command(AiGatewayHttpCommandRequest {
                host: &target.host,
                headers: &headers,
                holding_id,
                command,
            })
            .await
    }
    .await;
    respond(result, false)
}

pub fn ai_gateway_http_router(targets: Vec<AiGatewayHttpTarget>) -> Result<Router, BusinessError> {
    let mut map = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.host.len() > 255
            || !valid_origin(&target.origin)
            || map.insert(target.host.clone(), target).is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/platform/billing/ai-gateway",
            get(read).post(command),
        )
        .route(
            "/api/v1/platform/billing/ai-gateway/{*path}",
            post(command_path),
        )
        .with_state(Arc::new(AiGatewayHttpState { targets: map })))
}
