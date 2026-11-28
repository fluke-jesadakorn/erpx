//! Native R3 building block for
//! `POST /api/v1/holdings/{holdingSlug}/pms/booking-changes` (L10).
//!
//! The retained TypeScript route delegates to `staffBookingChange`, which
//! composes a signed proposal (`publicChangeQuote`) and confirm/replay flow
//! (`confirmBookingChange`) over the full booking engine, Stripe checkout and
//! package purchase authority. This module is deliberately bounded to the
//! **owner SQL facts** that flow depends on: booking-change idempotency by
//! proposal id and the one still-pending change for a reservation. Both SQL
//! literals are byte-identical to the retired TypeScript templates and are
//! already registered in `native_read_sql.rs` (`pms_rd_booking_change`,
//! `pms_rd_booking_change_pending`); here the owner executes them itself with
//! SQLx instead of exporting a plan for TypeScript to bind.
//!
//! Additive and unique. It does not replace the TS POST route: the proposal
//! HMAC, booking snapshot, Stripe effects and mail remain in TypeScript. See
//! the worker report for the blocker and the exact `pub mod`/mount lines.

use erpx_platform_runtime_server::public::http_support::{
    body::to_bytes, header, routing::post, HeaderMap, HeaderName, HeaderValue, IntoResponse, Json,
    Path, Request, Response, Router, State, StatusCode, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use sqlx::{PgPool, Row};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub const CHANGE_SQL: &str =
    "SELECT request_hash,state FROM pms_booking_changes WHERE id=$1 AND reservation_id=$2";
pub const PENDING_SQL: &str = "SELECT id FROM pms_booking_changes WHERE reservation_id=$1 AND state='payment_pending' AND expires_at>$2 LIMIT 1";

pub const STATE_PATH: &str = "/api/v1/holdings/{holding_slug}/pms/booking-changes/state";

pub fn valid_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b':'))
}

pub fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'-' | b'_'))
}

fn invalid(code: &'static str) -> BusinessError {
    BusinessError::invalid(code, "Use one bounded native booking-change state read.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "PMS_BOOKING_CHANGE_STATE_UNAVAILABLE",
        "The native PMS booking-change reader is unavailable.",
    )
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ChangeState {
    pub request_hash: String,
    pub state: String,
    pub pending_id: Option<String>,
}

impl ChangeState {
    /// Exact `{data:...}` envelope. There are no `href` fields, so
    /// `canonicalDestinationLinks` is an identity transform on this payload.
    pub fn value(&self) -> serde_json::Value {
        serde_json::json!({
            "data": {
                "requestHash": self.request_hash,
                "state": self.state,
                "pendingId": self.pending_id,
            }
        })
    }
}

pub type StateFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub trait BookingChangeStateRead: Send + Sync {
    fn load<'a>(
        &'a self,
        company: &'a str,
        reservation: &'a str,
        proposal: &'a str,
        now: i64,
    ) -> StateFuture<'a, ChangeState>;
}

pub struct PgBookingChangeStateRead {
    pool: PgPool,
}

impl PgBookingChangeStateRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self {
            pool: pool.raw().clone(),
        }
    }
}

impl BookingChangeStateRead for PgBookingChangeStateRead {
    fn load<'a>(
        &'a self,
        company: &'a str,
        reservation: &'a str,
        proposal: &'a str,
        now: i64,
    ) -> StateFuture<'a, ChangeState> {
        let company = company.to_owned();
        let reservation = reservation.to_owned();
        let proposal = proposal.to_owned();
        let pool = self.pool.clone();
        Box::pin(async move {
            let _ = company;
            let row = sqlx::query(CHANGE_SQL)
                .bind(&proposal)
                .bind(&reservation)
                .fetch_optional(&pool)
                .await
                .map_err(|_| unavailable())?
                .ok_or_else(|| {
                    BusinessError::new(FailureKind::NotFound, "PMS_PROPOSAL_INVALID", "Review a new proposal")
                })?;
            let request_hash: String = row.try_get("request_hash").map_err(|_| unavailable())?;
            let state: String = row.try_get("state").map_err(|_| unavailable())?;
            let pending = sqlx::query(PENDING_SQL)
                .bind(&reservation)
                .bind(now)
                .fetch_optional(&pool)
                .await
                .map_err(|_| unavailable())?
                .map(|row| row.try_get::<String, _>("id"))
                .transpose()
                .map_err(|_| unavailable())?;
            Ok(ChangeState {
                request_hash,
                state,
                pending_id: pending,
            })
        })
    }
}

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StateTarget {
    pub host: String,
    pub holding_slug: String,
}

struct StateHttpState {
    targets: BTreeMap<(String, String), Arc<dyn BookingChangeStateRead>>,
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

fn unique_header<'a>(headers: &'a HeaderMap, name: HeaderName) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid("PMS_BOOKING_CHANGE_STATE_REQUEST_INVALID"));
    }
    first
        .map(|value| {
            value
                .to_str()
                .map_err(|_| invalid("PMS_BOOKING_CHANGE_STATE_REQUEST_INVALID"))
        })
        .transpose()
}

fn state(targets: Vec<(StateTarget, Arc<dyn BookingChangeStateRead>)>) -> Result<StateHttpState, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(invalid("PMS_BOOKING_CHANGE_STATE_CONFIG_INVALID"));
        }
    }
    if configured.is_empty() {
        return Err(invalid("PMS_BOOKING_CHANGE_STATE_CONFIG_INVALID"));
    }
    Ok(StateHttpState { targets: configured })
}

pub fn router(pool: DatabasePool, targets: Vec<StateTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(
        targets
            .into_iter()
            .map(|target| (target, pool.clone()))
            .collect(),
    )
}

pub fn router_for_holdings(
    targets: Vec<(StateTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let readers = targets
        .into_iter()
        .map(|(target, pool)| {
            (
                target,
                Arc::new(PgBookingChangeStateRead::new(pool)) as Arc<dyn BookingChangeStateRead>,
            )
        })
        .collect();
    Ok(Router::new()
        .route(STATE_PATH, post(read))
        .with_state(Arc::new(state(readers)?)))
}

pub fn router_with_readers(
    targets: Vec<(StateTarget, Arc<dyn BookingChangeStateRead>)>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(STATE_PATH, post(read))
        .with_state(Arc::new(state(targets)?)))
}

async fn read(
    State(state): State<Arc<StateHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        if !valid_slug(&slug) {
            return Err(invalid("PMS_BOOKING_CHANGE_STATE_REQUEST_INVALID"));
        }
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(|| invalid("PMS_BOOKING_CHANGE_STATE_REQUEST_INVALID"))?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(|| BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found."))?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(|| invalid("PMS_BOOKING_CHANGE_STATE_COMPANY_INVALID"))?
            .to_owned();
        let bytes = to_bytes(request.into_body(), 64 * 1024)
            .await
            .map_err(|_| invalid("PMS_BOOKING_CHANGE_STATE_BODY_INVALID"))?;
        let body: serde_json::Value =
            serde_json::from_slice(&bytes).map_err(|_| invalid("PMS_BOOKING_CHANGE_STATE_BODY_INVALID"))?;
        let reservation = body
            .get("reservationId")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("PMS_BOOKING_CHANGE_STATE_BODY_INVALID"))?;
        let proposal = body
            .get("proposalId")
            .and_then(serde_json::Value::as_str)
            .ok_or_else(|| invalid("PMS_BOOKING_CHANGE_STATE_BODY_INVALID"))?;
        if !valid_id(reservation) || !valid_id(proposal) {
            return Err(invalid("PMS_BOOKING_CHANGE_STATE_BODY_INVALID"));
        }
        let now = body
            .get("now")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        Ok::<_, BusinessError>(reader.load(&company, reservation, proposal, now).await?.value())
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sql_matches_retired_templates() {
        assert_eq!(CHANGE_SQL, "SELECT request_hash,state FROM pms_booking_changes WHERE id=$1 AND reservation_id=$2");
        assert_eq!(PENDING_SQL, "SELECT id FROM pms_booking_changes WHERE reservation_id=$1 AND state='payment_pending' AND expires_at>$2 LIMIT 1");
    }

    #[test]
    fn envelope_matches_canonical_destination_links_identity_for_no_href_fields() {
        let state = ChangeState {
            request_hash: "digest".to_owned(),
            state: "payment_pending".to_owned(),
            pending_id: Some("change-1".to_owned()),
        };
        let value = state.value();
        assert_eq!(value["data"]["requestHash"], "digest");
        assert_eq!(value["data"]["state"], "payment_pending");
        assert_eq!(value["data"]["pendingId"], "change-1");
        let encoded = value.to_string();
        assert!(!encoded.contains("Href"));
        assert!(!encoded.contains("href"));
    }

    #[test]
    fn rejects_duplicate_context_headers() {
        let mut headers = HeaderMap::new();
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-1"));
        headers.append(CONTEXT_COMPANY, HeaderValue::from_static("company-2"));
        assert!(unique_header(&headers, CONTEXT_COMPANY).is_err());
    }

    #[test]
    fn configuration_requires_exact_distinct_targets() {
        let reader = Arc::new(ProbeRead) as Arc<dyn BookingChangeStateRead>;
        let target = || StateTarget {
            host: "acme.example.test".to_owned(),
            holding_slug: "acme".to_owned(),
        };
        assert!(state(vec![(target(), reader.clone())]).is_ok());
        assert!(state(vec![(target(), reader.clone()), (target(), reader.clone())]).is_err());
        assert!(state(vec![]).is_err());
    }

    struct ProbeRead;

    impl BookingChangeStateRead for ProbeRead {
        fn load<'a>(
            &'a self,
            _company: &'a str,
            _reservation: &'a str,
            _proposal: &'a str,
            _now: i64,
        ) -> StateFuture<'a, ChangeState> {
            Box::pin(async {
                Ok(ChangeState {
                    request_hash: "digest".to_owned(),
                    state: "requested".to_owned(),
                    pending_id: None,
                })
            })
        }
    }
}
