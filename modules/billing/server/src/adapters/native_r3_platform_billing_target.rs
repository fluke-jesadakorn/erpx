//! Native R3 read for `GET /api/v1/platform/billing/target` (L05).
//!
//! The retained TypeScript route binds Billing owner SQL and builds the target
//! period in TypeScript. This module replaces that hop with owner-local SQLx:
//! the holding binding check, [`crate::adapters::target::read_target_billing_period`]
//! and the allow-listed `app_finance_projection` owner statement all execute in
//! one Control unit. The exact `{data:{...view,financeProjection}}` envelope and
//! `Cache-Control: private, no-store` header are preserved. No TypeScript call,
//! no plan-only SQL and no synthesised facts remain on this path.

use crate::adapters::billing_owner_write::{BillingOwnerWriteInput, billing_owner_statement};
use crate::domain::billing_owner_write::APP_FINANCE_PROJECTION;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Query, Response, Router, State,
    StatusCode, get, header, serde_json,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind, HoldingId, TimestampMillis};
use serde_json::{Value, json};
use sqlx::{AssertSqlSafe, Row};
use std::{
    collections::{BTreeMap, BTreeSet},
    future::Future,
    pin::Pin,
    sync::Arc,
    time::{SystemTime, UNIX_EPOCH},
};

// ---------------------------------------------------------------------------
// domain: pure types and invariants
// ---------------------------------------------------------------------------

/// Byte-for-byte the `billingMonth` and route month validation.
pub fn valid_month(value: &str) -> bool {
    let bytes = value.as_bytes();
    bytes.len() == 7
        && bytes[0].is_ascii_digit()
        && bytes[1].is_ascii_digit()
        && bytes[2].is_ascii_digit()
        && bytes[3].is_ascii_digit()
        && bytes[4] == b'-'
        && bytes[5].is_ascii_digit()
        && bytes[6].is_ascii_digit()
        && matches!(bytes[5], b'0' | b'1')
        && (bytes[5] == b'0' && matches!(bytes[6], b'1'..=b'9')
            || bytes[5] == b'1' && matches!(bytes[6], b'0'..=b'2'))
}

/// The ERPX management workspace is never a customer billing target.
pub fn valid_customer_holding(value: &str) -> bool {
    !value.is_empty()
        && value != "h003"
        && value.len() <= 64
        && value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
}

/// Owner SQL is allow-listed; never accept statement text from the transport.
pub const HOLDING_BINDING_SQL: &str =
    "SELECT 1 FROM holding_bindings WHERE holding_id=$1 AND holding_id<>'h003' LIMIT 1";

pub fn forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FORBIDDEN",
        "Platform billing permission required.",
    )
}

pub fn holding_required() -> BusinessError {
    BusinessError::invalid("HOLDING_REQUIRED", "Select a Customer Holding.")
}

pub fn invalid_month() -> BusinessError {
    BusinessError::invalid("INVALID_MONTH", "Use a month in YYYY-MM format.")
}

pub fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PLATFORM_BILLING_TARGET_REQUEST_INVALID",
        "Use one bounded platform billing target read.",
    )
}

pub fn holding_not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "NOT_FOUND",
        "Customer Holding not found.",
    )
}

pub fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BILLING_TARGET_SOURCE_UNAVAILABLE",
        "The native Billing target read is unavailable.",
    )
}

/// `Number(string)` from the retired finance projection, rendered as a JSON
/// number with no loss of integer shape.
pub fn number(raw: &str) -> Result<Value, BusinessError> {
    if let Ok(value) = raw.parse::<i64>() {
        return Ok(json!(value));
    }
    raw.parse::<f64>()
        .map(|value| json!(value))
        .map_err(|_| unavailable())
}

/// One owner read result. `financeProjection` is always emitted, even when the
/// period has no invoice, matching the TypeScript spread.
#[derive(Clone, Debug)]
pub struct TargetPayload {
    pub view: Value,
    pub finance_projection: Option<Value>,
}

impl TargetPayload {
    pub fn wire(&self) -> Value {
        let mut data = self.view.clone();
        if let Some(map) = data.as_object_mut() {
            map.insert(
                "financeProjection".to_owned(),
                self.finance_projection.clone().unwrap_or(Value::Null),
            );
        }
        json!({ "data": data })
    }
}

// ---------------------------------------------------------------------------
// application: query port, no SQL
// ---------------------------------------------------------------------------

pub type TargetFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Owner reader port. Composition injects the SQLx implementation; unit tests
/// inject a deterministic probe. No caller SQL reaches this port.
pub trait PlatformBillingTargetRead: Send + Sync {
    fn load<'a>(
        &'a self,
        holding: &'a HoldingId,
        month: &'a str,
        now: TimestampMillis,
    ) -> TargetFuture<'a, TargetPayload>;
}

/// Deterministic reader used by unit tests.
pub struct ProbePlatformBillingTargetRead<F> {
    load: F,
}

impl<F> ProbePlatformBillingTargetRead<F>
where
    F: for<'a> Fn(
            &'a HoldingId,
            &'a str,
            TimestampMillis,
        ) -> TargetFuture<'a, TargetPayload>
        + Send
        + Sync,
{
    pub fn new(load: F) -> Self {
        Self { load }
    }
}

impl<F> PlatformBillingTargetRead for ProbePlatformBillingTargetRead<F>
where
    F: for<'a> Fn(
            &'a HoldingId,
            &'a str,
            TimestampMillis,
        ) -> TargetFuture<'a, TargetPayload>
        + Send
        + Sync,
{
    fn load<'a>(
        &'a self,
        holding: &'a HoldingId,
        month: &'a str,
        now: TimestampMillis,
    ) -> TargetFuture<'a, TargetPayload> {
        (self.load)(holding, month, now)
    }
}

// ---------------------------------------------------------------------------
// adapters: SQLx/Postgres execution, row mapping
// ---------------------------------------------------------------------------

fn persistence(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Pool-backed owner reader. Every statement below is authored in this crate;
/// the transport supplies only a bounded holding id and month.
pub struct PgPlatformBillingTargetRead {
    pool: DatabasePool,
}

impl PgPlatformBillingTargetRead {
    pub fn new(pool: DatabasePool) -> Self {
        Self { pool }
    }
}

impl PlatformBillingTargetRead for PgPlatformBillingTargetRead {
    fn load<'a>(
        &'a self,
        holding: &'a HoldingId,
        month: &'a str,
        now: TimestampMillis,
    ) -> TargetFuture<'a, TargetPayload> {
        let unit = PgUnitOfWork::new(self.pool.clone());
        let holding = holding.clone();
        let month = month.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let holding = holding.clone();
                let month = month.clone();
                Box::pin(async move {
                    let binding = sqlx::query_scalar::<_, i32>(HOLDING_BINDING_SQL)
                        .bind(holding.as_str())
                        .fetch_optional(&mut *connection)
                        .await
                        .map_err(|_| unavailable())?;
                    if binding.is_none() {
                        return Err(PgTransactionError::Business(holding_not_found()));
                    }
                    let view = crate::adapters::target::read_target_billing_period(
                        &mut *connection,
                        &holding,
                        &month,
                        now,
                    )
                    .await
                    .map_err(persistence)?;
                    let view = view.to_wire()?;
                    let invoice_id = view
                        .get("invoice")
                        .and_then(|invoice| invoice.get("id"))
                        .and_then(Value::as_str)
                        .map(str::to_owned);
                    let finance_projection = match invoice_id {
                        Some(invoice_id) => {
                            let input = BillingOwnerWriteInput {
                                kind: APP_FINANCE_PROJECTION.to_owned(),
                                values: vec![json!(invoice_id), json!(holding.as_str())],
                                now: now.get(),
                            };
                            let statement = billing_owner_statement(&holding, &input)?;
                            let row = sqlx::query(AssertSqlSafe(statement.sql.as_str()))
                                .bind(invoice_id.as_str())
                                .bind(holding.as_str())
                                .fetch_optional(&mut *connection)
                                .await
                                .map_err(|_| unavailable())?;
                            match row {
                                Some(row) => {
                                    let read = |name: &str| -> Result<Value, BusinessError> {
                                        let raw: String =
                                            row.try_get(name).map_err(|_| unavailable())?;
                                        number(&raw)
                                    };
                                    let stripe_status: String =
                                        row.try_get("stripe_status").map_err(|_| unavailable())?;
                                    Some(json!({
                                        "invoiceSatang": read("invoice_satang")?,
                                        "receivableSatang": read("receivable_satang")?,
                                        "receivedSatang": read("received_satang")?,
                                        "reversedSatang": read("reversed_satang")?,
                                        "creditedSatang": read("credited_satang")?,
                                        "stripeStatus": stripe_status,
                                        "syncedAt": read("synced_at")?,
                                    }))
                                }
                                None => None,
                            }
                        }
                        None => None,
                    };
                    Ok(TargetPayload {
                        view,
                        finance_projection,
                    })
                })
            })
            .await
            .map_err(persistence)
        })
    }
}

// ---------------------------------------------------------------------------
// public: DTO + router constructor
// ---------------------------------------------------------------------------

const ROLE: HeaderName = HeaderName::from_static("x-erpx-platform-role");

struct PlatformBillingTargetState {
    hosts: BTreeSet<String>,
    reader: Arc<dyn PlatformBillingTargetRead>,
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
    let mut response = (
        status(error.kind),
        Json(json!({"error":{"code":error.code,"message":error.message}})),
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

fn unique_header<'a>(headers: &'a HeaderMap, name: HeaderName) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn now_millis() -> TimestampMillis {
    let millis = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|value| value.as_millis().min(i64::MAX as u128) as i64)
        .unwrap_or(0);
    TimestampMillis::new(millis)
}

fn route(hosts: Vec<String>, reader: Arc<dyn PlatformBillingTargetRead>) -> Router {
    Router::new()
        .route("/api/v1/platform/billing/target", get(target))
        .with_state(Arc::new(PlatformBillingTargetState {
            hosts: hosts.into_iter().collect(),
            reader,
        }))
}

/// Composition-facing constructor: exact registered Platform hosts share one
/// Control pool owned by this crate.
pub fn router(hosts: Vec<String>, pool: DatabasePool) -> Router {
    route(
        hosts,
        Arc::new(PgPlatformBillingTargetRead::new(pool)) as Arc<dyn PlatformBillingTargetRead>,
    )
}

/// Test-facing constructor with a deterministic owner reader.
pub fn router_with_reader(
    hosts: Vec<String>,
    reader: Arc<dyn PlatformBillingTargetRead>,
) -> Router {
    route(hosts, reader)
}

async fn target(
    State(state): State<Arc<PlatformBillingTargetState>>,
    headers: HeaderMap,
    Query(values): Query<BTreeMap<String, String>>,
) -> Response {
    let result = async {
        let host = unique_header(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(|| {
                BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
            })?;
        if !state.hosts.contains(host) {
            return Err(BusinessError::new(
                FailureKind::NotFound,
                "NOT_FOUND",
                "Not found.",
            ));
        }
        let role = unique_header(&headers, ROLE)?.ok_or_else(request_invalid)?;
        if role != "platform_admin" && role != "platform_auditor" {
            return Err(forbidden());
        }
        if values.len() > 2
            || values
                .keys()
                .any(|key| key != "holdingId" && key != "month")
        {
            return Err(request_invalid());
        }
        let holding_id = values.get("holdingId").ok_or_else(holding_required)?;
        if !valid_customer_holding(holding_id) {
            if holding_id == "h003" {
                return Err(holding_required());
            }
            return Err(request_invalid());
        }
        let holding = HoldingId::new(holding_id.as_str()).map_err(|_| request_invalid())?;
        let now = now_millis();
        let month = match values.get("month") {
            Some(month) if valid_month(month) => month.clone(),
            Some(_) => return Err(invalid_month()),
            None => crate::domain::forecast_read::billing_month(now.get()).map_err(|_| unavailable())?,
        };
        let payload = state.reader.load(&holding, &month, now).await?;
        Ok::<_, BusinessError>(payload.wire())
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_platform_runtime_server::public::http_support::serve;

    const HOST: &str = "acme.platform.test";

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((&text, ""));
        let cache = head
            .lines()
            .find(|line| line.to_ascii_lowercase().starts_with("cache-control:"))
            .map(|line| line.split_once(':').unwrap().1.trim().to_owned())
            .unwrap_or_default();
        (
            status,
            cache,
            serde_json::from_str(body).unwrap_or(Value::Null),
        )
    }

    fn payload(invoice: bool) -> TargetPayload {
        TargetPayload {
            view: json!({
                "holdingId": "h001", "month": "2026-11", "state": "ready",
                "invoice": if invoice { json!({"id":"inv-1"}) } else { Value::Null }
            }),
            finance_projection: invoice.then(|| {
                json!({
                    "invoiceSatang": 1000, "receivableSatang": 1000,
                    "receivedSatang": 0, "reversedSatang": 0, "creditedSatang": 0,
                    "stripeStatus": "paid", "syncedAt": 1764326400000i64
                })
            }),
        }
    }

    async fn serve_probe(
        seen: Arc<std::sync::Mutex<Vec<(String, String)>>>,
    ) -> (u16, tokio::task::JoinHandle<()>) {
        let reader = Arc::new(ProbePlatformBillingTargetRead::new(move |holding, month, _| {
            let holding = holding.as_str().to_owned();
            let month = month.to_owned();
            let seen = seen.clone();
            let answer = payload(true);
            Box::pin(async move {
                seen.lock().unwrap().push((holding, month));
                Ok::<_, BusinessError>(answer)
            })
        }));
        let router = router_with_reader(vec![HOST.to_owned()], reader);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = serve(listener, router).await;
        });
        (addr.port(), server)
    }

    fn request(query: &str, role: &str, host: &str) -> String {
        format!(
            "GET /api/v1/platform/billing/target?{query} HTTP/1.1\r\n\
             Host: {host}\r\n\
             Accept: application/json\r\n\
             Connection: close\r\n\
             x-erpx-platform-role: {role}\r\n\r\n"
        )
    }

    #[test]
    fn month_and_holding_validation_match_the_retired_guards() {
        assert!(valid_month("2026-11"));
        assert!(valid_month("2026-12"));
        assert!(!valid_month("2026-13"));
        assert!(!valid_month("2026-00"));
        assert!(!valid_month("202611"));
        assert!(valid_customer_holding("h001"));
        assert!(!valid_customer_holding("h003"));
        assert!(!valid_customer_holding("H001"));
        assert_eq!(number("1000").unwrap(), json!(1000));
        assert_eq!(number("1.5").unwrap(), json!(1.5));
    }

    #[test]
    fn envelope_always_carries_finance_projection() {
        let with = payload(true).wire();
        assert_eq!(with["data"]["financeProjection"]["invoiceSatang"], json!(1000));
        let without = payload(false).wire();
        assert!(without["data"]["financeProjection"].is_null());
    }

    #[tokio::test]
    async fn target_envelope_headers_and_owner_arguments() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let (port, server) = serve_probe(seen.clone()).await;
        let (status, cache, body) = http(
            std::net::SocketAddr::from(([127, 0, 0, 1], port)),
            request("holdingId=h001&month=2026-11", "platform_admin", HOST),
        )
        .await;
        server.abort();
        assert_eq!(status, 200);
        assert_eq!(cache, "private, no-store");
        assert_eq!(body["data"]["holdingId"], "h001");
        assert_eq!(body["data"]["financeProjection"]["stripeStatus"], "paid");
        assert_eq!(
            *seen.lock().unwrap(),
            vec![("h001".to_owned(), "2026-11".to_owned())]
        );
    }

    #[tokio::test]
    async fn authority_and_guard_failures_match_the_retired_errors() {
        let (port, server) = serve_probe(Arc::new(std::sync::Mutex::new(Vec::new()))).await;
        let addr = std::net::SocketAddr::from(([127, 0, 0, 1], port));
        let (status, _, body) = http(
            addr,
            request("holdingId=h001&month=2026-11", "company_user", HOST),
        )
        .await;
        assert_eq!(status, 403);
        assert_eq!(body["error"]["code"], "FORBIDDEN");
        let (status, _, body) = http(
            addr,
            request("holdingId=h003&month=2026-11", "platform_auditor", HOST),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "HOLDING_REQUIRED");
        let (status, _, body) = http(
            addr,
            request("holdingId=h001&month=2026-13", "platform_auditor", HOST),
        )
        .await;
        assert_eq!(status, 422);
        assert_eq!(body["error"]["code"], "INVALID_MONTH");
        let (status, _, body) = http(
            addr,
            request("holdingId=h001&month=2026-11", "platform_admin", "other.test"),
        )
        .await;
        server.abort();
        assert_eq!(status, 404);
        assert_eq!(body["error"]["code"], "NOT_FOUND");
    }
}
