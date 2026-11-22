//! Owner-local HTTP ingress for the native Procurement supplier-bill command.
//!
//! Transport facts only: registered Host/Holding target, exact browser origin,
//! `Idempotency-Key`, bounded private JSON and business-error mapping. Current
//! Account session resolution, selected Company/branch/department scope,
//! purchase admission, the proof-derived [`SupplierBillCommandActor`] and the
//! caller-owned serializable transaction remain composition responsibilities
//! behind [`SupplierBillCommandHttpBackend`].
//!
//! Composition must build the actor from the current authorized work context
//! without accepting any of these values from the request:
//!
//! - `user_id` from the Account principal
//! - `assignment_id` and `assignment_version` from the selected assignment
//! - `company_id`, `branch_id` and `department_id` from the selected workspace
//! - `revision` from the selected work-context revision
//! - `permissions` from the current command IAM metadata
//! - `expires_at` from the current work-context expiry
//!
//! The retained purchase admission (`withSupplierBillPurchasedCommand`) must run
//! in that same unit before the `*_in_transaction` helpers, which wrap
//! [`crate::adapters::supplier_bill_command::PgSupplierBillCommand`] and keep
//! the Procurement writes plus the explicit cross-owner staging hooks inside
//! the caller transaction. Composition must authorize capture with
//! `finance.write` + `commerce.read`, decide with `finance.read` +
//! `finance.write` + `finance.approve`, and policy with `finance.approve` on a
//! mutating work selection; the router itself never grants authority.
use crate::adapters::supplier_bill_command::PgSupplierBillCommand;
use crate::application::supplier_bill_command::{
    SupplierBillCommandStaging, capture_supplier_bill, decide_supplier_bill,
    save_supplier_bill_policy,
};
use crate::domain::supplier_bill_command::{SupplierBillCommandActor, SupplierBillRecordReceipt};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, serde_json,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind, TimestampMillis};
use sqlx::PgConnection;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type SupplierBillCommandHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct SupplierBillCaptureHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub key: &'a CommandKey,
    pub input: &'a serde_json::Value,
    pub headers: &'a HeaderMap,
}

pub struct SupplierBillDecideHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub match_id: &'a str,
    pub key: &'a CommandKey,
    pub input: &'a serde_json::Value,
    pub headers: &'a HeaderMap,
}

pub struct SupplierBillPolicyHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub key: &'a CommandKey,
    pub input: &'a serde_json::Value,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct SupplierBillCommandHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl SupplierBillCommandHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account/Holding proof, purchase admission and owner
/// transaction adapter. The router never mints a session, actor, Company,
/// permission, or command receipt.
pub trait SupplierBillCommandHttpBackend: Send + Sync {
    fn capture<'a>(
        &'a self,
        request: SupplierBillCaptureHttpRequest<'a>,
    ) -> SupplierBillCommandHttpFuture<'a, SupplierBillCommandHttpResponse>;

    fn decide<'a>(
        &'a self,
        request: SupplierBillDecideHttpRequest<'a>,
    ) -> SupplierBillCommandHttpFuture<'a, SupplierBillCommandHttpResponse>;

    fn save_policy<'a>(
        &'a self,
        request: SupplierBillPolicyHttpRequest<'a>,
    ) -> SupplierBillCommandHttpFuture<'a, SupplierBillCommandHttpResponse>;
}

pub struct SupplierBillCommandHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn SupplierBillCommandHttpBackend>,
}

struct SupplierBillCommandHttpState {
    targets: BTreeMap<(String, String), SupplierBillCommandHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "SUPPLIER_BILL_INPUT_INVALID",
        "Use one bounded Procurement supplier-bill request.",
    )
}

fn key_required() -> BusinessError {
    BusinessError::invalid(
        "COMMAND_KEY_REQUIRED",
        "Use a valid Idempotency-Key for this action.",
    )
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn receipt_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "COMMAND_RECEIPT_INVALID",
        "The Procurement command acknowledgement could not be read back.",
    )
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(malformed());
    }
    first
        .map(|value| value.to_str().map_err(|_| malformed()))
        .transpose()
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

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'_')
        })
}

fn valid_match_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 128 && id.bytes().all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_'))
}

fn target<'a>(
    state: &'a SupplierBillCommandHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a SupplierBillCommandHttpTarget, BusinessError> {
    let host = unique_header(headers, "host")?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn require_origin(
    headers: &HeaderMap,
    target: &SupplierBillCommandHttpTarget,
) -> Result<(), BusinessError> {
    if unique_header(headers, "origin")?.is_some_and(|value| value != target.origin) {
        return Err(origin_forbidden());
    }
    Ok(())
}

fn require_json_content_type(headers: &HeaderMap) -> Result<(), BusinessError> {
    if !unique_header(headers, "content-type")?.is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    }) {
        return Err(malformed());
    }
    Ok(())
}

/// Native receipt has an exact deterministic shape per operation:
/// capture carries `bill-match-{key}` with version 1, decide carries the
/// match id with `expectedVersion + 1`, policy carries the Company id with
/// `expectedVersion + 1`.
fn require_capture_receipt(
    data: &serde_json::Value,
    key: &CommandKey,
) -> Result<(), BusinessError> {
    let Some(object) = data.as_object() else {
        return Err(receipt_invalid());
    };
    let expected_id = format!("bill-match-{}", key.as_str());
    if object.get("id").and_then(serde_json::Value::as_str) != Some(expected_id.as_str())
        || object.get("version").and_then(serde_json::Value::as_i64) != Some(1)
    {
        return Err(receipt_invalid());
    }
    Ok(())
}

fn require_decide_receipt(
    data: &serde_json::Value,
    match_id: &str,
    expected_version: i64,
) -> Result<(), BusinessError> {
    let Some(object) = data.as_object() else {
        return Err(receipt_invalid());
    };
    let Some(version) = expected_version.checked_add(1) else {
        return Err(receipt_invalid());
    };
    let status = object.get("status").and_then(serde_json::Value::as_str);
    if object.get("id").and_then(serde_json::Value::as_str) != Some(match_id)
        || object.get("version").and_then(serde_json::Value::as_i64) != Some(version)
        || !matches!(status, Some("approved") | Some("returned"))
    {
        return Err(receipt_invalid());
    }
    Ok(())
}

fn require_policy_receipt(data: &serde_json::Value) -> Result<(), BusinessError> {
    let Some(object) = data.as_object() else {
        return Err(receipt_invalid());
    };
    let mode = object.get("matchingMode").and_then(serde_json::Value::as_str);
    if object.get("version").and_then(serde_json::Value::as_i64).is_none_or(|v| v < 1)
        || !matches!(mode, Some("optional") | Some("required"))
    {
        return Err(receipt_invalid());
    }
    Ok(())
}

/// Native ingress for the retained supplier-bill routes. Other Procurement
/// write paths remain on the retained TypeScript owner until their own packet
/// is certified.
pub fn supplier_bill_command_http_router(
    targets: Vec<SupplierBillCommandHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "SUPPLIER_BILL_COMMAND_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/supplier-bills/capture",
            erpx_server_runtime::public::http_support::routing::post(capture_bill),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/supplier-bills/{match_id}/decision",
            erpx_server_runtime::public::http_support::routing::post(decide_bill),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/supplier-bill-policy",
            erpx_server_runtime::public::http_support::routing::post(save_policy),
        )
        .with_state(Arc::new(SupplierBillCommandHttpState {
            targets: configured,
        })))
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

fn bounded(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
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

async fn capture_bill(
    State(state): State<Arc<SupplierBillCommandHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(malformed());
        }
        let target = target(&state, request.headers(), &slug)?;
        require_origin(request.headers(), target)?;
        require_json_content_type(request.headers())?;
        let key = unique_header(request.headers(), "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(key_required)?;
        let key = CommandKey::new(key)?;
        let headers = request.headers().clone();
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        let input: serde_json::Value = serde_json::from_slice(&body).map_err(|_| malformed())?;
        if !input.is_object() {
            return Err(malformed());
        }
        let response = target
            .backend
            .capture(SupplierBillCaptureHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                key: &key,
                input: &input,
                headers: &headers,
            })
            .await?;
        require_capture_receipt(&response.data, &key)?;
        if !response.context.is_object() {
            return Err(receipt_invalid());
        }
        Ok(response.value())
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

async fn decide_bill(
    State(state): State<Arc<SupplierBillCommandHttpState>>,
    Path((slug, match_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(malformed());
        }
        if !valid_match_id(&match_id) {
            return Err(malformed());
        }
        let target = target(&state, request.headers(), &slug)?;
        require_origin(request.headers(), target)?;
        require_json_content_type(request.headers())?;
        let key = unique_header(request.headers(), "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(key_required)?;
        let key = CommandKey::new(key)?;
        let headers = request.headers().clone();
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        let input: serde_json::Value = serde_json::from_slice(&body).map_err(|_| malformed())?;
        let expected_version = input
            .get("expectedVersion")
            .and_then(serde_json::Value::as_i64)
            .ok_or_else(malformed)?;
        let response = target
            .backend
            .decide(SupplierBillDecideHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                match_id: &match_id,
                key: &key,
                input: &input,
                headers: &headers,
            })
            .await?;
        require_decide_receipt(&response.data, &match_id, expected_version)?;
        if !response.context.is_object() {
            return Err(receipt_invalid());
        }
        Ok(response.value())
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

async fn save_policy(
    State(state): State<Arc<SupplierBillCommandHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(malformed());
        }
        let target = target(&state, request.headers(), &slug)?;
        require_origin(request.headers(), target)?;
        require_json_content_type(request.headers())?;
        let key = unique_header(request.headers(), "idempotency-key")?
            .filter(|value| !value.is_empty())
            .ok_or_else(key_required)?;
        let key = CommandKey::new(key)?;
        let headers = request.headers().clone();
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        let input: serde_json::Value = serde_json::from_slice(&body).map_err(|_| malformed())?;
        if !input.is_object() {
            return Err(malformed());
        }
        let response = target
            .backend
            .save_policy(SupplierBillPolicyHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                key: &key,
                input: &input,
                headers: &headers,
            })
            .await?;
        require_policy_receipt(&response.data)?;
        if !response.context.is_object() {
            return Err(receipt_invalid());
        }
        Ok(response.value())
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

/// Runs the native supplier-bill capture over the caller-owned serializable
/// transaction. Composition supplies the proof-derived actor, the JSON input
/// and the cross-owner staging hooks; this helper never opens a transaction
/// of its own.
pub async fn capture_supplier_bill_in_transaction<S>(
    connection: &mut PgConnection,
    actor: &SupplierBillCommandActor,
    staging: &mut S,
    input: &serde_json::Value,
    key: &CommandKey,
    now: TimestampMillis,
) -> Result<SupplierBillRecordReceipt, PgTransactionError<BusinessError>>
where
    S: SupplierBillCommandStaging<Error = PgTransactionError<BusinessError>>,
{
    let mut repository = PgSupplierBillCommand::new(connection);
    capture_supplier_bill(actor, &mut repository, staging, input, key.as_str(), now).await
}

/// Runs the native supplier-bill decision over the caller-owned serializable
/// transaction with the same staging contract.
pub async fn decide_supplier_bill_in_transaction<S>(
    connection: &mut PgConnection,
    actor: &SupplierBillCommandActor,
    staging: &mut S,
    match_id: &str,
    input: &serde_json::Value,
    key: &CommandKey,
    now: TimestampMillis,
) -> Result<SupplierBillRecordReceipt, PgTransactionError<BusinessError>>
where
    S: SupplierBillCommandStaging<Error = PgTransactionError<BusinessError>>,
{
    let mut repository = PgSupplierBillCommand::new(connection);
    decide_supplier_bill(
        actor,
        &mut repository,
        staging,
        match_id,
        input,
        key.as_str(),
        now,
    )
    .await
}

/// Runs the native supplier-bill policy update over the caller-owned
/// serializable transaction. The policy path has no cross-owner staging.
pub async fn save_supplier_bill_policy_in_transaction(
    connection: &mut PgConnection,
    actor: &SupplierBillCommandActor,
    input: &serde_json::Value,
    key: &CommandKey,
    now: TimestampMillis,
) -> Result<SupplierBillRecordReceipt, PgTransactionError<BusinessError>> {
    let mut repository = PgSupplierBillCommand::new(connection);
    save_supplier_bill_policy(actor, &mut repository, input, key.as_str(), now).await
}
