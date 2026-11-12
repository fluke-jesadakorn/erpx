//! Native PMS guest-account/charge command HTTP ingress.
//!
//! Transport facts only: exact registered Host/Holding target, Origin,
//! Idempotency-Key, bounded JSON body and a bounded work-context selection.
//! Account session resolution, current IAM/assignment authority, the
//! serializable owner unit and the PMS actor remain composition
//! responsibilities behind [`PmsGuestAccountHttpBackend`]. None of the context
//! values below are trusted as authority; composition must resolve and compare
//! them with the retained session before it builds a
//! [`crate::domain::guest_account::PmsGuestAccountActor`].
use crate::domain::guest_account::{
    PmsAdjustmentInput, PmsChargeAddInput, PmsChargeImportInput, PmsGuestAccountCommand,
    PmsGuestAccountInput, PmsGuestAccountOpenInput, PmsGuestAccountOperation,
    PmsGuestAccountUpdateInput, PmsTransferInput, invalid as command_invalid,
};
use crate::domain::room_board::cp_len;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::post, serde_json,
};
use erpx_server_shared::public::{AssignmentId, BusinessError, CommandKey, CompanyId, FailureKind};
use serde::Deserialize;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const BODY_LIMIT: usize = 16 * 1024;
const QUERY_LIMIT: usize = 2 * 1024;
const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

const CONTEXT: [&str; 4] = [
    "x-erpx-context-company",
    "x-erpx-context-revision",
    "x-erpx-context-assignment",
    "x-erpx-context-assignment-version",
];

pub type PmsGuestAccountHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Selected work context supplied with the command. These are selectors for
/// composition to resolve against the retained Account session; they are not a
/// proof of authority, a staff label or a Company grant.
#[derive(Clone, Debug)]
pub struct PmsGuestAccountHttpContext {
    pub company_id: String,
    pub branch_slug: Option<String>,
    pub department_slug: Option<String>,
    pub assignment_id: String,
    pub revision: i64,
    pub assignment_version: i64,
}

pub struct PmsGuestAccountHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub context: PmsGuestAccountHttpContext,
    pub key: CommandKey,
    pub command: PmsGuestAccountCommand,
}

#[derive(Clone, Debug)]
pub struct PmsGuestAccountHttpResponse {
    /// Owner command result. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}
impl PmsGuestAccountHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

pub trait PmsGuestAccountHttpBackend: Send + Sync {
    fn command<'a>(
        &'a self,
        request: PmsGuestAccountHttpCommandRequest<'a>,
    ) -> PmsGuestAccountHttpFuture<'a, PmsGuestAccountHttpResponse>;
}

pub struct PmsGuestAccountHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PmsGuestAccountHttpBackend>,
}

struct PmsGuestAccountHttpState {
    targets: BTreeMap<(String, String), PmsGuestAccountHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    command_invalid()
}

fn context_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "WORK_CONTEXT_CHANGED",
        "Your selected work context changed. Review the current workspace and role.",
    )
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
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

fn bounded(body: serde_json::Value) -> Response {
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

fn target<'a>(
    state: &'a PmsGuestAccountHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PmsGuestAccountHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

fn percent_decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == b'%' {
            let high = bytes.get(index + 1).copied().and_then(hex);
            let low = bytes.get(index + 2).copied().and_then(hex);
            let (Some(high), Some(low)) = (high, low) else {
                return Err(malformed());
            };
            out.push(high * 16 + low);
            index += 3;
        } else if bytes[index] == b'+' {
            out.push(b' ');
            index += 1;
        } else {
            out.push(bytes[index]);
            index += 1;
        }
    }
    String::from_utf8(out).map_err(|_| malformed())
}

fn whole(value: &str, minimum: i64) -> Result<i64, BusinessError> {
    let parsed = value.parse::<i64>().map_err(|_| context_changed())?;
    if !(minimum..=MAX_SAFE_INTEGER).contains(&parsed) || parsed.to_string() != value {
        return Err(context_changed());
    }
    Ok(parsed)
}

fn selector(value: String) -> Result<String, BusinessError> {
    if value.is_empty() || cp_len(&value) > 200 || value.chars().any(char::is_control) {
        return Err(malformed());
    }
    Ok(value)
}

fn context(
    headers: &HeaderMap,
    query: Option<&str>,
) -> Result<PmsGuestAccountHttpContext, BusinessError> {
    let header = |name: &'static str| {
        unique_header(headers, HeaderName::from_static(name))?
            .ok_or_else(context_changed)
            .map(str::to_owned)
    };
    let company_id = CompanyId::new(header(CONTEXT[0])?).map_err(|_| context_changed())?;
    let revision = whole(&header(CONTEXT[1])?, 0)?;
    let assignment_id = AssignmentId::new(header(CONTEXT[2])?).map_err(|_| context_changed())?;
    let assignment_version = whole(&header(CONTEXT[3])?, 1)?;
    let (branch_slug, department_slug) = match query {
        Some(query) if !query.is_empty() => {
            if query.len() > QUERY_LIMIT {
                return Err(malformed());
            }
            let mut branch = None;
            let mut department = None;
            let mut seen = std::collections::BTreeSet::new();
            for pair in query.split('&') {
                if pair.is_empty() {
                    continue;
                }
                let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
                let name = percent_decode(name)?;
                let value = percent_decode(value)?;
                if !seen.insert(name.clone()) {
                    return Err(malformed());
                }
                match name.as_str() {
                    "branch" => branch = Some(selector(value)?),
                    "department" => department = Some(selector(value)?),
                    _ => return Err(malformed()),
                }
            }
            (branch, department)
        }
        _ => (None, None),
    };
    Ok(PmsGuestAccountHttpContext {
        company_id: company_id.as_str().to_owned(),
        branch_slug,
        department_slug,
        assignment_id: assignment_id.as_str().to_owned(),
        revision,
        assignment_version,
    })
}

fn command_origin(
    target: &PmsGuestAccountHttpTarget,
    headers: &HeaderMap,
) -> Result<(), BusinessError> {
    if unique_header(headers, header::ORIGIN)? != Some(target.origin.as_str()) {
        return Err(origin_forbidden());
    }
    Ok(())
}

fn command_key(headers: &HeaderMap) -> Result<CommandKey, BusinessError> {
    let key =
        unique_header(headers, HeaderName::from_static("idempotency-key"))?.ok_or_else(|| {
            BusinessError::invalid(
                "COMMAND_KEY_REQUIRED",
                "Use a valid Idempotency-Key for this action.",
            )
        })?;
    CommandKey::new(key)
}

fn command_content_type(headers: &HeaderMap) -> Result<(), BusinessError> {
    if !unique_header(headers, header::CONTENT_TYPE)?.is_some_and(|value| {
        value
            .split(';')
            .next()
            .is_some_and(|value| value.trim().eq_ignore_ascii_case("application/json"))
    }) {
        return Err(command_invalid());
    }
    Ok(())
}

async fn command_body(request: Request) -> Result<serde_json::Value, BusinessError> {
    let bytes = to_bytes(request.into_body(), BODY_LIMIT)
        .await
        .map_err(|_| {
            BusinessError::invalid(
                "BODY_TOO_LARGE",
                "Use a smaller guest account command request.",
            )
        })?;
    if bytes.is_empty() {
        return Err(command_invalid());
    }
    let value: serde_json::Value = serde_json::from_slice(&bytes).map_err(|_| command_invalid())?;
    if !value.is_object() {
        return Err(command_invalid());
    }
    Ok(value)
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct CreateBody {
    /// Peeked before decoding this shape; kept so unknown-field rejection still
    /// sees the retained `operation` member.
    #[allow(dead_code)]
    operation: String,
    property_id: String,
    expected_version: i64,
    guest_id: String,
    #[serde(default)]
    reservation_id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct UpdateBody {
    /// Peeked before decoding this shape; kept so unknown-field rejection still
    /// sees the retained `operation` member.
    #[allow(dead_code)]
    operation: String,
    property_id: String,
    expected_version: i64,
    account_id: String,
    name: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChargeAddBody {
    /// Peeked before decoding this shape; kept so unknown-field rejection still
    /// sees the retained `operation` member.
    #[allow(dead_code)]
    operation: String,
    property_id: String,
    expected_version: i64,
    guest_account_id: String,
    price: i64,
    description: String,
    #[serde(default)]
    stay_id: String,
    #[serde(default)]
    source_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct ChargeImportBody {
    /// Peeked before decoding this shape; kept so unknown-field rejection still
    /// sees the retained `operation` member.
    #[allow(dead_code)]
    operation: String,
    property_id: String,
    expected_version: i64,
    guest_account_id: String,
    #[serde(default)]
    stay_id: String,
    source_key: String,
    description: String,
    net: i64,
    service: i64,
    tax: i64,
    total: i64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct AdjustmentBody {
    /// Peeked before decoding this shape; kept so unknown-field rejection still
    /// sees the retained `operation` member.
    #[allow(dead_code)]
    operation: String,
    property_id: String,
    expected_version: i64,
    guest_account_id: String,
    #[serde(default)]
    original_entry_id: String,
    description: String,
    net: i64,
    service: i64,
    tax: i64,
    total: i64,
    #[serde(default)]
    source_key: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct TransferBody {
    /// Peeked before decoding this shape; kept so unknown-field rejection still
    /// sees the retained `operation` member.
    #[allow(dead_code)]
    operation: String,
    property_id: String,
    expected_version: i64,
    entry_id: String,
    to_guest_account_id: String,
    reason: String,
}

fn decode_command(value: serde_json::Value) -> Result<PmsGuestAccountCommand, BusinessError> {
    let operation = value
        .get("operation")
        .and_then(serde_json::Value::as_str)
        .and_then(PmsGuestAccountOperation::parse)
        .ok_or_else(command_invalid)?;
    let mut command = match operation {
        PmsGuestAccountOperation::Create => {
            let body: CreateBody = serde_json::from_value(value).map_err(|_| command_invalid())?;
            PmsGuestAccountCommand {
                operation,
                property_id: body.property_id,
                expected_version: body.expected_version,
                input: PmsGuestAccountInput::Create(PmsGuestAccountOpenInput {
                    guest_id: body.guest_id,
                    reservation_id: body.reservation_id,
                    name: body.name,
                }),
            }
        }
        PmsGuestAccountOperation::Update => {
            let body: UpdateBody = serde_json::from_value(value).map_err(|_| command_invalid())?;
            PmsGuestAccountCommand {
                operation,
                property_id: body.property_id,
                expected_version: body.expected_version,
                input: PmsGuestAccountInput::Update(PmsGuestAccountUpdateInput {
                    account_id: body.account_id,
                    name: body.name,
                }),
            }
        }
        PmsGuestAccountOperation::ChargeAdd => {
            let body: ChargeAddBody =
                serde_json::from_value(value).map_err(|_| command_invalid())?;
            PmsGuestAccountCommand {
                operation,
                property_id: body.property_id,
                expected_version: body.expected_version,
                input: PmsGuestAccountInput::ChargeAdd(PmsChargeAddInput {
                    account_id: body.guest_account_id,
                    price: body.price,
                    description: body.description,
                    stay_id: body.stay_id,
                    source_key: body.source_key,
                }),
            }
        }
        PmsGuestAccountOperation::ChargeImport => {
            let body: ChargeImportBody =
                serde_json::from_value(value).map_err(|_| command_invalid())?;
            PmsGuestAccountCommand {
                operation,
                property_id: body.property_id,
                expected_version: body.expected_version,
                input: PmsGuestAccountInput::ChargeImport(PmsChargeImportInput {
                    account_id: body.guest_account_id,
                    stay_id: body.stay_id,
                    source_key: body.source_key,
                    description: body.description,
                    net: body.net,
                    service: body.service,
                    tax: body.tax,
                    total: body.total,
                }),
            }
        }
        PmsGuestAccountOperation::Adjustment => {
            let body: AdjustmentBody =
                serde_json::from_value(value).map_err(|_| command_invalid())?;
            PmsGuestAccountCommand {
                operation,
                property_id: body.property_id,
                expected_version: body.expected_version,
                input: PmsGuestAccountInput::Adjustment(PmsAdjustmentInput {
                    account_id: body.guest_account_id,
                    original_entry_id: body.original_entry_id,
                    description: body.description,
                    net: body.net,
                    service: body.service,
                    tax: body.tax,
                    total: body.total,
                    source_key: body.source_key,
                }),
            }
        }
        PmsGuestAccountOperation::Transfer => {
            let body: TransferBody =
                serde_json::from_value(value).map_err(|_| command_invalid())?;
            PmsGuestAccountCommand {
                operation,
                property_id: body.property_id,
                expected_version: body.expected_version,
                input: PmsGuestAccountInput::Transfer(PmsTransferInput {
                    entry_id: body.entry_id,
                    to_account_id: body.to_guest_account_id,
                    reason: body.reason,
                }),
            }
        }
    };
    command.normalize();
    command.validate()?;
    Ok(command)
}

pub fn pms_guest_account_router(
    targets: Vec<PmsGuestAccountHttpTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || !valid_origin(&target.origin)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), target)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PMS_GUEST_ACCOUNT_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pms/guest-account",
            post(command),
        )
        .with_state(Arc::new(PmsGuestAccountHttpState {
            targets: configured,
        })))
}

async fn command(
    State(state): State<Arc<PmsGuestAccountHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        command_origin(target, request.headers())?;
        let key = command_key(request.headers())?;
        command_content_type(request.headers())?;
        let selected = context(request.headers(), request.uri().query())?;
        let headers = request.headers().clone();
        let body = command_body(request).await?;
        let command = decode_command(body)?;
        target
            .backend
            .command(PmsGuestAccountHttpCommandRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                context: selected,
                key,
                command,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
