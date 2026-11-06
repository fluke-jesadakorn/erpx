//! Native PMS room-board and front-desk HTTP ingress.
//!
//! Transport facts only: registered Host/Holding target, one bounded read
//! selection and business-error mapping. Normal Account session resolution,
//! selected Company/branch scope, the serializable unit, the People staff port
//! and the Commercial package entitlement remain composition responsibilities
//! behind [`PmsHttpBackend`].
use crate::domain::front_desk::PmsFrontDeskSelection;
use crate::domain::room_board::{PmsBoardSection, PmsBoardSelection, cp_len};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const QUERY_LIMIT: usize = 4 * 1024;

pub type PmsHttpFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct PmsHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub selection: PmsBoardSelection,
}

pub struct PmsHttpFrontDeskRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub selection: PmsFrontDeskSelection,
}

#[derive(Clone, Debug)]
pub struct PmsHttpResponse {
    /// Owner read data. The backend adds the validated current context.
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}
impl PmsHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company, permission or staff label.
pub trait PmsHttpBackend: Send + Sync {
    fn read<'a>(&'a self, request: PmsHttpReadRequest<'a>) -> PmsHttpFuture<'a, PmsHttpResponse>;
    /// Front-desk reads are added without forcing an existing room-board
    /// backend to serve a queue it does not implement. Composition must opt in
    /// explicitly; the default fails closed.
    fn front_desk<'a>(
        &'a self,
        _request: PmsHttpFrontDeskRequest<'a>,
    ) -> PmsHttpFuture<'a, PmsHttpResponse> {
        Box::pin(async {
            Err(BusinessError::new(
                FailureKind::Unavailable,
                "PMS_FRONT_DESK_UNAVAILABLE",
                "The native hotel front desk is unavailable.",
            ))
        })
    }
}

pub struct PmsHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding, for example
    /// `https://acme.jesadakorn.com`.
    pub origin: String,
    pub backend: Arc<dyn PmsHttpBackend>,
}

struct PmsHttpState {
    targets: BTreeMap<(String, String), PmsHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}
fn malformed() -> BusinessError {
    BusinessError::invalid(
        "PMS_BOARD_REQUEST_INVALID",
        "Use one bounded room board selection.",
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
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
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
    state: &'a PmsHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a PmsHttpTarget, BusinessError> {
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

fn id(value: &str, code: &'static str) -> Result<String, BusinessError> {
    // Retained `idField` measures Unicode scalars (Zod 4.6.7), matching
    // the domain `cp_len` rule. `RecordId` is byte-capped and would reject
    // non-ASCII ids the domain accepts.
    if value.is_empty() || cp_len(value) > 200 || value.chars().any(char::is_control) {
        return Err(BusinessError::invalid(
            code,
            "Use a valid bounded identifier.",
        ));
    }
    Ok(value.to_owned())
}

fn selection(request: &Request) -> Result<PmsBoardSelection, BusinessError> {
    let Some(query) = request.uri().query() else {
        return Ok(PmsBoardSelection::default());
    };
    if query.len() > QUERY_LIMIT {
        return Err(malformed());
    }
    let mut parsed = PmsBoardSelection::default();
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
            "property" => parsed.property_id = Some(id(&value, "PMS_BOARD_REQUEST_INVALID")?),
            "section" => {
                parsed.section = PmsBoardSection::parse(&value).ok_or_else(malformed)?;
            }
            "status" => parsed.status = value,
            "q" => parsed.q = value,
            "building" => parsed.building = value,
            "floor" => parsed.floor = value,
            "typeId" => {
                parsed.type_id = if value.is_empty() { None } else { Some(value) };
            }
            "page" => {
                parsed.page = value.parse::<u32>().map_err(|_| malformed())?;
            }
            "selectedId" => {
                parsed.selected_id = Some(id(&value, "PMS_BOARD_REQUEST_INVALID")?);
            }
            "from" => {
                parsed.from = if value.is_empty() { None } else { Some(value) };
            }
            "to" => {
                parsed.to = if value.is_empty() { None } else { Some(value) };
            }
            _ => return Err(malformed()),
        }
    }
    parsed.normalize();
    parsed.validate()?;
    Ok(parsed)
}

fn front_desk_malformed() -> BusinessError {
    BusinessError::invalid(
        "PMS_FRONT_DESK_REQUEST_INVALID",
        "Use one bounded front-desk selection.",
    )
}

fn front_desk_selection(request: &Request) -> Result<PmsFrontDeskSelection, BusinessError> {
    let Some(query) = request.uri().query() else {
        return Ok(PmsFrontDeskSelection::default());
    };
    if query.len() > QUERY_LIMIT {
        return Err(front_desk_malformed());
    }
    let mut parsed = PmsFrontDeskSelection::default();
    let mut seen = std::collections::BTreeSet::new();
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        let name = percent_decode(name)?;
        let value = percent_decode(value)?;
        if !seen.insert(name.clone()) {
            return Err(front_desk_malformed());
        }
        match name.as_str() {
            "property" => parsed.property_id = Some(id(&value, "PMS_FRONT_DESK_REQUEST_INVALID")?),
            "status" => parsed.status = value,
            "q" => parsed.q = value,
            "page" => {
                parsed.page = value.parse::<u32>().map_err(|_| front_desk_malformed())?;
            }
            "selectedId" => {
                parsed.selected_id = Some(id(&value, "PMS_FRONT_DESK_REQUEST_INVALID")?)
            }
            "from" => {
                parsed.from = if value.is_empty() { None } else { Some(value) };
            }
            "to" => {
                parsed.to = if value.is_empty() { None } else { Some(value) };
            }
            _ => return Err(front_desk_malformed()),
        }
    }
    parsed.normalize();
    parsed.validate()?;
    Ok(parsed)
}

pub fn pms_http_router(targets: Vec<PmsHttpTarget>) -> Result<Router, BusinessError> {
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
                "PMS_BOARD_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    Ok(Router::new()
        .route("/api/v1/holdings/{holding_slug}/pms/rooms", get(read_rooms))
        .route(
            "/api/v1/holdings/{holding_slug}/pms/front-desk",
            get(read_front_desk),
        )
        .with_state(Arc::new(PmsHttpState {
            targets: configured,
        })))
}

async fn read_rooms(
    State(state): State<Arc<PmsHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let selection = selection(&request)?;
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| malformed())?;
        target
            .backend
            .read(PmsHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                selection,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

async fn read_front_desk(
    State(state): State<Arc<PmsHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let target = target(&state, request.headers(), &slug)?;
        let selection = front_desk_selection(&request)?;
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| front_desk_malformed())?;
        target
            .backend
            .front_desk(PmsHttpFrontDeskRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                selection,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}
