//! Owner-local axum read router for native lane 3.
//!
//! Composition mounts this router on the private native API. The route owns
//! Holding/company isolation, validation and error mapping; it carries no SQL
//! and accepts no caller-supplied predicate.

use super::application::{current_invoice_lines, InvoiceLineRead};
use super::domain::{
    configuration_invalid, invalid, valid_id, InvoiceLineRequest, InvoiceLinesResponse,
};
use super::postgres::PgInvoiceLineRead;
use erpx_platform_runtime_server::public::http_support::{
    body, header, serde_json, HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request,
    Response, Router, State, StatusCode, get,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
/// Node's fetch overwrites `Host` with the loopback authority, so the private
/// transport also carries the registered Holding host in a dedicated header.
const NATIVE_HOST: HeaderName = HeaderName::from_static("x-erpx-native-host");
const MAX_BODY: usize = 16 * 1024;

/// Exact registered Holding targets. Both fields are required and distinct.
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct InvoiceLineBody {
    invoice_id: String,
    revision: i64,
}

struct Lane3State {
    targets: BTreeMap<(String, String), Arc<dyn InvoiceLineRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
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

fn bounded(body: impl serde::Serialize) -> Response {
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

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn build_state(
    targets: Vec<(LaneTarget, Arc<dyn InvoiceLineRead>)>,
) -> Result<Arc<Lane3State>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Arc::new(Lane3State { targets: configured }))
}

fn into_router(state: Arc<Lane3State>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/lane3/invoice-lines",
            get(current),
        )
        .with_state(state)
}

/// Backward-compatible single-pool constructor: one reader shared by every
/// registered target. Kept for the existing mounted-route shape and tests.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    let reader: Arc<dyn InvoiceLineRead> = Arc::new(PgInvoiceLineRead::new(pool));
    let injected = targets
        .into_iter()
        .map(|target| (target, reader.clone()))
        .collect();
    Ok(into_router(build_state(injected)?))
}

/// Reader-injection constructor used by the owner's tests. Each target carries
/// its own reader, so two targets can be proved to use two distinct readers.
pub fn router_with_reader(
    targets: Vec<(LaneTarget, Arc<dyn InvoiceLineRead>)>,
) -> Result<Router, BusinessError> {
    Ok(into_router(build_state(targets)?))
}

/// Per-Holding constructor. Composition supplies one `(LaneTarget, DatabasePool)`
/// pair per Holding; this builds one `PgInvoiceLineRead` per pair and stores it
/// keyed by `(host, holding_slug)`, so every Holding reads its own database.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let injected = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: Arc<dyn InvoiceLineRead> = Arc::new(PgInvoiceLineRead::new(pool));
            (target, reader)
        })
        .collect();
    Ok(into_router(build_state(injected)?))
}

async fn current(
    State(state): State<Arc<Lane3State>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), NATIVE_HOST)?
            .or(unique_header(request.headers(), header::HOST)?)
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let reader = state
            .targets
            .get(&(host, slug.clone()))
            .ok_or_else(missing)?
            .clone();
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?
            .to_owned();
        let bytes = body::to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid())?;
        let payload: InvoiceLineBody =
            serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let lines = current_invoice_lines(
            reader.as_ref(),
            &InvoiceLineRequest {
                company_id: company,
                invoice_id: payload.invoice_id,
                revision: payload.revision,
            },
        )
        .await?;
        Ok::<_, BusinessError>(InvoiceLinesResponse::new(lines))
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
    use crate::adapters::native_lane_3::application::{InvoiceLineRead, InvoiceLineReadFuture};
    use crate::adapters::native_lane_3::domain::{fixture, InvoiceLine, InvoiceLineRequest};

    struct Fake;
    impl InvoiceLineRead for Fake {
        fn lines<'a>(&'a self, _request: &'a InvoiceLineRequest) -> InvoiceLineReadFuture<'a> {
            Box::pin(async { Ok(fixture()) })
        }
    }

    fn reader() -> Arc<dyn InvoiceLineRead> {
        Arc::new(Fake)
    }

    fn target(host: &str, slug: &str) -> LaneTarget {
        LaneTarget { host: host.into(), holding_slug: slug.into() }
    }

    #[test]
    fn router_rejects_empty_and_duplicate_targets() {
        let error = router_with_reader(vec![]).err().expect("empty rejected");
        assert_eq!(error.code, "LANE3_INVOICE_LINES_CONFIG_INVALID");

        let error = router_with_reader(vec![
            (target("acme.example.com", "acme"), reader()),
            (target("acme.example.com", "acme"), reader()),
        ])
        .err()
        .expect("duplicate rejected");
        assert_eq!(error.code, "LANE3_INVOICE_LINES_CONFIG_INVALID");
    }

    #[test]
    fn router_rejects_invalid_host_or_slug() {
        for target in [
            target("", "acme"),
            target("acme.example.com", ""),
            target("acme.example.com", "Acme"),
            target("acme.example.com", "a/b"),
        ] {
            let error = router_with_reader(vec![(target, reader())])
                .err()
                .expect("invalid target rejected");
            assert_eq!(error.code, "LANE3_INVOICE_LINES_CONFIG_INVALID");
        }
    }

    #[test]
    fn router_accepts_one_registered_target() {
        assert!(router_with_reader(vec![(target("acme.example.com", "acme"), reader())]).is_ok());
    }

    /// The per-Holding constructor builds one reader per `(target, pool)` pair.
    /// Distinct readers are observable because each target resolves to its own
    /// key in the state map and each request executes that target's reader.
    #[tokio::test]
    async fn two_targets_route_to_two_distinct_readers() {
        struct Fixed(String);
        impl InvoiceLineRead for Fixed {
            fn lines<'a>(&'a self, _request: &'a InvoiceLineRequest) -> InvoiceLineReadFuture<'a> {
                let mut line = fixture().remove(0);
                line.id = self.0.clone();
                Box::pin(async { Ok(vec![line]) })
            }
        }
        let acme: Arc<dyn InvoiceLineRead> = Arc::new(Fixed("acme-line".into()));
        let northstar: Arc<dyn InvoiceLineRead> = Arc::new(Fixed("northstar-line".into()));
        assert!(!Arc::ptr_eq(&acme, &northstar), "readers must be distinct");

        let state = Arc::new(Lane3State {
            targets: BTreeMap::from([
                (("acme.example.com".into(), "acme".into()), acme.clone()),
                (("northstar.example.com".into(), "northstar".into()), northstar.clone()),
            ]),
        });
        let body = serde_json::to_vec(&serde_json::json!({"invoiceId":"inv-1","revision":3}))
            .expect("body serializes");
        let request = |host: &str, slug: &str| {
            Request::builder()
                .method("POST")
                .uri(format!("/api/v1/holdings/{slug}/finance/lane3/invoice-lines"))
                .header("host", host)
                .header("x-erpx-native-host", host)
                .header("x-erpx-context-company", "company-1")
                .body(body::Body::from(body.clone()))
                .expect("request")
        };
        let response = current(State(state.clone()), Path("acme".into()), request("acme.example.com", "acme")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = body::to_bytes(response.into_body(), 16 * 1024).await.expect("body");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(value["data"]["lines"][0]["id"], "acme-line");

        let response = current(State(state), Path("northstar".into()), request("northstar.example.com", "northstar")).await;
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = body::to_bytes(response.into_body(), 16 * 1024).await.expect("body");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(value["data"]["lines"][0]["id"], "northstar-line");
    }

    #[tokio::test]
    async fn unavailable_maps_to_503_without_leaking_details() {
        let response = error_response(super::super::domain::unavailable());
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let bytes = body::to_bytes(response.into_body(), 1024).await.expect("body");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(value["error"]["code"], "LANE3_INVOICE_LINES_UNAVAILABLE");
        assert_eq!(value["error"]["message"], "The native Finance invoice-line service is unavailable.");
    }

    #[tokio::test]
    async fn success_adds_private_no_store_headers() {
        let response = bounded(InvoiceLinesResponse::new(vec![InvoiceLine {
            id: "x".into(), company_id: "company-1".into(), invoice_id: "i".into(), source_version: 1, ordinal: 0,
            description: "d".into(), quantity_millis: None, unit_price_minor: None, tax_bps: None,
            subtotal_minor: 0, tax_minor: 0, total_minor: 0, account_id: "a".into(),
            tax_account_id: None, source_snapshot_json: "{}".into(), currency: "THB".into(),
            origin_kind: "native_line".into(),
        }]));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-store");
        assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
        assert_eq!(response.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        let bytes = body::to_bytes(response.into_body(), 16 * 1024).await.expect("body");
        let value: serde_json::Value = serde_json::from_slice(&bytes).expect("json");
        assert_eq!(value["data"]["lines"][0]["id"], "x");
    }

    #[test]
    fn identity_rules_are_bounded() {
        assert!(valid_slug("acme-1"));
        assert!(!valid_slug("Acme"));
        assert!(!valid_slug("../acme"));
        assert!(valid_id("inv-1"));
        assert!(!valid_id("inv/1"));
    }
}
