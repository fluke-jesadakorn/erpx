//! Owner-local HTTP ingress for the native Procurement supplier promise
//! command and its bounded purchase-order detail read.
//!
//! Transport facts only: registered Host/Holding target, exact browser origin,
//! `Idempotency-Key`, bounded private JSON and business-error mapping. Current
//! Account session resolution, selected Company/branch/department scope,
//! purchase admission, the proof-derived [`SupplierPromiseCommandActor`] and
//! the caller-owned serializable transaction remain composition
//! responsibilities behind [`SupplierPromiseCommandHttpBackend`].
//!
//! Composition must build the actor from the current authorized work context
//! without accepting any of these values from the request:
//!
//! - `user_id` from the Account principal
//! - `assignment_id` and `assignment_version` from the selected assignment
//! - `company_id`, `branch_id` and `department_id` from the selected workspace
//! - `revision` from the selected work-context revision
//! - `permissions` from the current command IAM metadata
//! - `modules` from the current Holding modules
//! - `expires_at` from the current work-context expiry
//!
//! The retained purchase admission (`withProcurementPurchasedCommand`) must run
//! in that same unit before [`record_supplier_promise_in_transaction`], which
//! wraps [`crate::adapters::supplier_promise_command::PgSupplierPromiseCommand`]
//! and keeps the Commerce order-version CAS inside the caller transaction.
//! Composition must authorize the command with `commerce.read`,
//! `commerce.purchase` and the `procurement` module on a mutating work
//! selection; the router itself never grants authority.
use crate::adapters::supplier_promise_command::PgSupplierPromiseCommand;
use crate::application::supplier_promise_command::record_supplier_promise;
use crate::domain::supplier_promise_command::{
    RecordSupplierPromiseCommand, SupplierPromiseCommandActor, SupplierPromiseRecordReceipt,
    promise_id,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{
    BusinessError, CommandKey, FailureKind, RecordId, TimestampMillis,
};
use sqlx::PgConnection;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 64 * 1024;

pub type SupplierPromiseCommandHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct SupplierPromiseCommandHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub order_id: &'a str,
    pub key: &'a CommandKey,
    pub command: &'a RecordSupplierPromiseCommand,
    pub headers: &'a HeaderMap,
}

pub struct SupplierPromiseDetailHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub order_id: &'a str,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct SupplierPromiseCommandHttpResponse {
    /// `{id, promiseId, version}` receipt from the native command.
    pub data: serde_json::Value,
    /// Composition-owned current work context.
    pub context: serde_json::Value,
}

impl SupplierPromiseCommandHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

#[derive(Clone, Debug)]
pub struct SupplierPromiseDetailHttpResponse {
    /// Bounded owner read data. Composition may add its own purchase overlay.
    pub data: serde_json::Value,
    /// Composition-owned current work context.
    pub context: serde_json::Value,
}

impl SupplierPromiseDetailHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account/Holding proof, purchase admission and owner
/// transaction adapter. The router never mints a session, actor, Company,
/// permission, entitlement, promise or command receipt.
pub trait SupplierPromiseCommandHttpBackend: Send + Sync {
    fn record<'a>(
        &'a self,
        request: SupplierPromiseCommandHttpRequest<'a>,
    ) -> SupplierPromiseCommandHttpFuture<'a, SupplierPromiseCommandHttpResponse>;

    /// Bounded detail read for the same route. Composition resolves the
    /// Account session, current Company/workspace and the commercial package
    /// decision in the retained serializable Holding transaction, calls
    /// `supplier_promise_detail` with the `purchase` overlay it read (or `None`
    /// when package enforcement is disabled) and returns the serialized
    /// `SupplierPromiseDetail` as `data`. The owner already computes
    /// `canRecord`; composition must not replace it with a locally invented
    /// purchase decision.
    fn detail<'a>(
        &'a self,
        request: SupplierPromiseDetailHttpRequest<'a>,
    ) -> SupplierPromiseCommandHttpFuture<'a, SupplierPromiseDetailHttpResponse>;
}

pub struct SupplierPromiseCommandHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn SupplierPromiseCommandHttpBackend>,
}

struct SupplierPromiseCommandHttpState {
    targets: BTreeMap<(String, String), SupplierPromiseCommandHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    BusinessError::invalid(
        "SUPPLIER_PROMISE_COMMAND_INVALID",
        "Use one bounded Procurement supplier promise request.",
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

fn detail_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "SUPPLIER_PROMISE_DETAIL_INVALID",
        "The Procurement supplier promise detail could not be read back.",
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

fn target<'a>(
    state: &'a SupplierPromiseCommandHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a SupplierPromiseCommandHttpTarget, BusinessError> {
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
    target: &SupplierPromiseCommandHttpTarget,
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

/// The native receipt has an exact deterministic shape: the purchase order id,
/// its command-key promise id and the expected version plus one.
fn require_receipt(
    data: &serde_json::Value,
    order_id: &str,
    key: &CommandKey,
    expected_version: i64,
) -> Result<(), BusinessError> {
    let Some(object) = data.as_object() else {
        return Err(receipt_invalid());
    };
    let expected_promise = promise_id(key);
    let Some(version) = expected_version.checked_add(1) else {
        return Err(receipt_invalid());
    };
    if object.get("id").and_then(serde_json::Value::as_str) != Some(order_id)
        || object.get("promiseId").and_then(serde_json::Value::as_str)
            != Some(expected_promise.as_str())
        || object.get("version").and_then(serde_json::Value::as_i64) != Some(version)
    {
        return Err(receipt_invalid());
    }
    Ok(())
}

/// Native ingress for the retained `/supplier-promises/{order_id}` Supply
/// route. Other Procurement write paths remain on the retained TypeScript
/// owner until their own packet is certified.
pub fn supplier_promise_command_http_router(
    targets: Vec<SupplierPromiseCommandHttpTarget>,
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
                "SUPPLIER_PROMISE_COMMAND_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/supplier-promises/{order_id}",
            get(read_detail).post(record_promise),
        )
        .with_state(Arc::new(SupplierPromiseCommandHttpState {
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

async fn record_promise(
    State(state): State<Arc<SupplierPromiseCommandHttpState>>,
    Path((slug, order_id)): Path<(String, String)>,
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
        let order_id = RecordId::new(order_id).map_err(|_| malformed())?;
        let headers = request.headers().clone();
        let body = to_bytes(request.into_body(), BODY_LIMIT)
            .await
            .map_err(|_| malformed())?;
        let command: RecordSupplierPromiseCommand =
            serde_json::from_slice(&body).map_err(|_| malformed())?;
        command.validate()?;
        let response = target
            .backend
            .record(SupplierPromiseCommandHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                order_id: order_id.as_str(),
                key: &key,
                command: &command,
                headers: &headers,
            })
            .await?;
        require_receipt(
            &response.data,
            order_id.as_str(),
            &key,
            command.expected_version,
        )?;
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

async fn read_detail(
    State(state): State<Arc<SupplierPromiseCommandHttpState>>,
    Path((slug, order_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        if request.uri().query().is_some() {
            return Err(malformed());
        }
        let target = target(&state, request.headers(), &slug)?;
        let order_id = RecordId::new(order_id).map_err(|_| malformed())?;
        let response = target
            .backend
            .detail(SupplierPromiseDetailHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                order_id: order_id.as_str(),
                headers: request.headers(),
            })
            .await?;
        if !response.data.is_object() || !response.context.is_object() {
            return Err(detail_invalid());
        }
        Ok(response.value())
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

/// Runs the native supplier promise command over the caller-owned serializable
/// transaction. Composition supplies the proof-derived actor, the admitted
/// order and `now`; this helper never opens a transaction of its own.
pub async fn record_supplier_promise_in_transaction(
    connection: &mut PgConnection,
    actor: &SupplierPromiseCommandActor,
    order_id: &str,
    command: &RecordSupplierPromiseCommand,
    key: &CommandKey,
    now: TimestampMillis,
) -> Result<SupplierPromiseRecordReceipt, PgTransactionError<BusinessError>> {
    let mut repository = PgSupplierPromiseCommand::new(connection);
    record_supplier_promise(actor, &mut repository, order_id, command, key.as_str(), now).await
}
