//! Owner-local axum ingress for the native POS room-charge reversal queue.
//!
//! Transport facts only: registered Host/Holding target, exact bounded Company
//! and Branch headers, composition-supplied authority/package decisions, a
//! strict query and business-error mapping. The pool-backed owner reader owns
//! the SQL, the projection and the complete response payload.

use crate::adapters::native_room_charges::application::{read, PosRoomChargeReader};
use crate::adapters::native_room_charges::domain::{
    ActorDecision, PackageDecision, RoomChargeQuery, invalid,
};
use crate::adapters::native_room_charges::postgres::PgRoomChargeReader;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    get, header, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use serde::Deserialize;
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: &str = "x-erpx-context-company";
const WORKSPACE_BRANCH: &str = "x-erpx-workspace-branch";
const ACTOR_DECISION: &str = "x-erpx-pos-room-actor";
const PACKAGE_DECISION: &str = "x-erpx-pos-room-package";
const MAX_QUERY_BYTES: usize = 1024;
const MAX_DECISION_BYTES: usize = 4096;

pub struct RoomChargeTarget {
    pub host: String,
    pub holding_slug: String,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ActorHeader {
    #[serde(default, rename = "posApprove")]
    pos_approve: bool,
    #[serde(default, rename = "pmsApprove")]
    pms_approve: bool,
    #[serde(default)]
    department: bool,
    #[serde(default, rename = "selfIds")]
    self_ids: Vec<String>,
}

#[derive(Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct PackageHeader {
    #[serde(default)]
    enforced: bool,
    #[serde(default)]
    history: bool,
    #[serde(default, rename = "settleExisting")]
    settle_existing: bool,
}

struct RoomChargeState {
    targets: BTreeMap<(String, String), Arc<dyn PosRoomChargeReader>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "POS_ROOM_CHARGES_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "POS_ROOM_CHARGES_UNAVAILABLE",
        "The native POS room-charge service is unavailable.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
}

fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.' | b':'))
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut decoded = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                decoded.push(b' ');
                index += 1;
            }
            b'%' => {
                let high = bytes.get(index + 1).copied().ok_or_else(invalid)?;
                let low = bytes.get(index + 2).copied().ok_or_else(invalid)?;
                let hex = |byte: u8| -> Option<u8> {
                    match byte {
                        b'0'..=b'9' => Some(byte - b'0'),
                        b'a'..=b'f' => Some(byte - b'a' + 10),
                        b'A'..=b'F' => Some(byte - b'A' + 10),
                        _ => None,
                    }
                };
                let byte = (hex(high).ok_or_else(invalid)? << 4)
                    | hex(low).ok_or_else(invalid)?;
                decoded.push(byte);
                index += 3;
            }
            byte => {
                decoded.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(decoded).map_err(|_| invalid())
}

/// Parse the single optional `property` filter, rejecting duplicates, unknown
/// keys and malformed percent escapes exactly like the retained Next query.
fn parse_property(query: Option<&str>) -> Result<Option<String>, BusinessError> {
    let Some(query) = query.filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if query.len() > MAX_QUERY_BYTES {
        return Err(invalid());
    }
    let mut seen = false;
    for pair in query.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').ok_or_else(invalid)?;
        if decode(name)? != "property" || seen {
            return Err(invalid());
        }
        seen = true;
        let value = decode(value)?;
        // The retained route forwards `?property=` as an empty string, so keep
        // it distinct from an absent filter rather than rewriting its meaning.
        if !value.is_empty() && !valid_id(&value) {
            return Err(invalid());
        }
        return Ok(Some(value));
    }
    Ok(None)
}

fn decision<T: for<'de> Deserialize<'de>>(
    headers: &HeaderMap,
    name: &str,
) -> Result<T, BusinessError> {
    let raw = unique_header(headers, name)?
        .filter(|value| !value.is_empty())
        .ok_or_else(invalid)?;
    if raw.len() > MAX_DECISION_BYTES {
        return Err(invalid());
    }
    serde_json::from_str(raw).map_err(|_| invalid())
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

impl RoomChargeState {
    fn with_readers(
        entries: Vec<(RoomChargeTarget, Arc<dyn PosRoomChargeReader>)>,
    ) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for (target, reader) in entries {
            if target.host.is_empty()
                || !valid_slug(&target.holding_slug)
                || configured
                    .insert(
                        (target.host.clone(), target.holding_slug.clone()),
                        reader,
                    )
                    .is_some()
            {
                return Err(configuration_invalid());
            }
        }
        if configured.is_empty() {
            return Err(configuration_invalid());
        }
        Ok(Self {
            targets: configured,
        })
    }

    async fn queue(&self, headers: &HeaderMap, slug: &str, query: Option<&str>) -> Response {
        match self.queue_inner(headers, slug, query).await {
            Ok(rows) => bounded(serde_json::json!({"data": rows})),
            Err(error) => error_response(error),
        }
    }

    async fn queue_inner(
        &self,
        headers: &HeaderMap,
        slug: &str,
        query: Option<&str>,
    ) -> Result<Vec<serde_json::Value>, BusinessError> {
        let host = unique_header(headers, header::HOST.as_str())?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let reader = self
            .targets
            .get(&(host.to_owned(), slug.to_owned()))
            .ok_or_else(missing)?;
        let company = unique_header(headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let branch = unique_header(headers, WORKSPACE_BRANCH)?
            .filter(|value| !value.is_empty() && valid_id(value))
            .map(str::to_owned);
        let actor_header: ActorHeader = decision(headers, ACTOR_DECISION)?;
        let package_header: PackageHeader = decision(headers, PACKAGE_DECISION)?;
        let query = RoomChargeQuery {
            company: company.to_owned(),
            branch,
            property: parse_property(query)?,
            actor: ActorDecision {
                pos_approve: actor_header.pos_approve,
                pms_approve: actor_header.pms_approve,
                department: actor_header.department,
                self_ids: actor_header.self_ids,
            },
            package: PackageDecision {
                enforced: package_header.enforced,
                history: package_header.history,
                settle_existing: package_header.settle_existing,
            },
        };
        read(reader.as_ref(), &query)
            .await
            .map_err(|error| {
                if matches!(error.kind, FailureKind::Unavailable) {
                    unavailable()
                } else {
                    error
                }
            })
    }
}

async fn queue(
    State(state): State<Arc<RoomChargeState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().map(str::to_owned);
    state.queue(&headers, &slug, query.as_deref()).await
}

/// Build the native queue router with one pool shared by every target.
/// The lead mounts it; no composition edit is needed and no SQL or business
/// rule is added to TypeScript.
pub fn router(pool: DatabasePool, targets: Vec<RoomChargeTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgRoomChargeReader::new(pool)), targets)
}

/// Router over one injected owner reader, kept for single-reader tests.
pub fn router_with_reader(
    reader: Arc<dyn PosRoomChargeReader>,
    targets: Vec<RoomChargeTarget>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

/// Router over one owner reader per registered Holding target. Composition
/// supplies a distinct `DatabasePool` for each Holding; each entry becomes its
/// own `PgRoomChargeReader`, keyed by `(host, holding_slug)` in the state map.
pub fn router_for_holdings(
    targets: Vec<(RoomChargeTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    router_with_readers(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgRoomChargeReader::new(pool)) as Arc<dyn PosRoomChargeReader>,
                )
            })
            .collect(),
    )
}

/// Injection path for the per-target map, used by `router_for_holdings` and by
/// the two-target isolation test. No pool or SQL is created here.
pub fn router_with_readers(
    entries: Vec<(RoomChargeTarget, Arc<dyn PosRoomChargeReader>)>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pos/room-charge-reversals",
            get(queue),
        )
        .with_state(Arc::new(RoomChargeState::with_readers(entries)?)))
}
