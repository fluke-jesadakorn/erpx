//! Owner-local axum router for the native Commerce entrypoint read family.
//!
//! Mirrors `quality_product_read.rs`: exact registered Holding targets, unique
//! Host/Company headers, bounded identities, no-store responses and explicit
//! error mapping. No SQL is planned or executed on the TypeScript side.

use super::application::{CommerceEntrypointRead, CommerceInvoiceRead, read_facts, read_invoice};
use super::domain::{validate, validate_invoice, valid_id};
use super::postgres::{PgCommerceEntrypointRead, PgCommerceInvoiceRead};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, post,
    serde_json::{self, Value},
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// Exact registered Holding target; the same pair must be declared in the
/// caller's native transport configuration.
#[derive(Clone, Debug)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

pub type LaneReader = Arc<dyn CommerceEntrypointRead>;
pub type InvoiceReader = Arc<dyn CommerceInvoiceRead>;

struct LaneState {
    targets: BTreeMap<(String, String), LaneReader>,
    invoice: BTreeMap<(String, String), InvoiceReader>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_ENTRYPOINT_READ_REQUEST_INVALID",
        "Use one bounded Commerce entrypoint read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "COMMERCE_ENTRYPOINT_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
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
    first.map(|value| value.to_str().map_err(|_| invalid())).transpose()
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

fn insert_readers<T>(
    entries: Vec<(LaneTarget, T)>,
    configured: &mut BTreeMap<(String, String), T>,
) -> Result<(), BusinessError> {
    for (target, reader) in entries {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    Ok(())
}

/// Build both per-Holding reader maps. Each pair is keyed by its exact
/// `(host, holding_slug)` identity; no reader is shared across Holdings.
fn state_with_both(
    targets: Vec<(LaneTarget, LaneReader)>,
    invoice: Vec<(LaneTarget, InvoiceReader)>,
) -> Result<Arc<LaneState>, BusinessError> {
    let mut configured = BTreeMap::new();
    insert_readers(targets, &mut configured)?;
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    let mut invoice_configured = BTreeMap::new();
    insert_readers(invoice, &mut invoice_configured)?;
    Ok(Arc::new(LaneState {
        targets: configured,
        invoice: invoice_configured,
    }))
}

/// Entrypoint-only variant retained for the earlier tests.
fn state_with_readers(entries: Vec<(LaneTarget, LaneReader)>) -> Result<Arc<LaneState>, BusinessError> {
    state_with_both(entries, Vec::new())
}

/// Shared-reader variant retained for single-pool callers and tests.
fn state(targets: Vec<LaneTarget>, reader: LaneReader) -> Result<Arc<LaneState>, BusinessError> {
    state_with_readers(targets.into_iter().map(|target| (target, reader.clone())).collect())
}

fn mount(lane: Arc<LaneState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/entrypoint-facts",
            post(read),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/commerce/invoice-read",
            post(read_invoice_request),
        )
        .with_state(lane)
}

/// Mountable router. Composition supplies one pool and exact targets; the
/// owner owns both readers, their SQL and their response projections.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_for_holdings(targets.into_iter().map(|target| (target, pool.clone())).collect())
}

/// Per-Holding mount: each target carries its own database pool and produces
/// its own pair of owner readers, so every Holding reads from its own database.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let (entrypoint, invoice): (Vec<_>, Vec<_>) = targets
        .into_iter()
        .map(|(target, pool)| {
            let entrypoint_pool = pool.clone();
            (
                (
                    target.clone(),
                    Arc::new(PgCommerceEntrypointRead::new(entrypoint_pool)) as LaneReader,
                ),
                (
                    target,
                    Arc::new(PgCommerceInvoiceRead::new(pool)) as InvoiceReader,
                ),
            )
        })
        .unzip();
    Ok(mount(state_with_both(entrypoint, invoice)?))
}

/// Injectable variant used by the owner's own tests.
pub fn router_with_reader(reader: LaneReader, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    Ok(mount(state(targets, reader)?))
}

/// Exact target key and Company scope shared by both read families.
fn context(headers: &HeaderMap, slug: &str) -> Result<((String, String), String), BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    let company = unique_header(headers, CONTEXT_COMPANY)?
        .filter(|value| valid_id(value))
        .ok_or_else(invalid)?;
    Ok(((host.to_owned(), slug.to_owned()), company.to_owned()))
}

/// Pure request processing shared by the router and the unit tests.
async fn process(
    headers: &HeaderMap,
    slug: &str,
    body: &[u8],
    lane: &LaneState,
) -> Result<Value, BusinessError> {
    let (key, company) = context(headers, slug)?;
    let reader = lane.targets.get(&key).ok_or_else(missing)?;
    let value: Value = serde_json::from_slice(body).map_err(|_| invalid())?;
    let (query, company) = validate(&value, &company)?;
    let facts = read_facts(reader.as_ref(), &company, &query).await?;
    Ok(serde_json::json!({"facts": facts.map(|facts| facts.into_value())}))
}

/// Pure invoice/delivery processing. No reader is shared with another Holding
/// and a missing target fails closed rather than falling back.
async fn process_invoice(
    headers: &HeaderMap,
    slug: &str,
    body: &[u8],
    lane: &LaneState,
) -> Result<Value, BusinessError> {
    let (key, company) = context(headers, slug)?;
    let reader = lane.invoice.get(&key).ok_or_else(missing)?;
    let value: Value = serde_json::from_slice(body).map_err(|_| super::domain::invoice_invalid())?;
    let (query, company) = validate_invoice(&value, &company)?;
    let facts = read_invoice(reader.as_ref(), &company, &query).await?;
    Ok(facts.into_value())
}

async fn read(
    State(lane): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let headers = request.headers().clone();
        let bytes = to_bytes(request.into_body(), 64 * 1024)
            .await
            .map_err(|_| invalid())?;
        process(&headers, &slug, &bytes, &lane).await
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

async fn read_invoice_request(
    State(lane): State<Arc<LaneState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let headers = request.headers().clone();
        let bytes = to_bytes(request.into_body(), 64 * 1024)
            .await
            .map_err(|_| super::domain::invoice_invalid())?;
        process_invoice(&headers, &slug, &bytes, &lane).await
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
    use super::super::application::{EntrypointReadFuture, InvoiceReadFuture};
    use super::super::domain::{
        CommandTarget, EntrypointFacts, EntrypointKind, InvoiceFacts, InvoiceReadKind, SalesLine,
    };
    use std::{collections::BTreeMap, sync::Mutex};

    #[derive(Clone)]
    struct Probe {
        rows: BTreeMap<(String, EntrypointKind, String), EntrypointFacts>,
        seen: Arc<Mutex<Vec<(String, EntrypointKind, String)>>>,
    }

    impl CommerceEntrypointRead for Probe {
        fn read<'a>(
            &'a self,
            company: &'a str,
            kind: EntrypointKind,
            id: &'a str,
        ) -> EntrypointReadFuture<'a, Option<EntrypointFacts>> {
            let seen = self.seen.clone();
            let rows = self.rows.clone();
            let company = company.to_owned();
            let id = id.to_owned();
            Box::pin(async move {
                seen.lock().expect("lock").push((company.clone(), kind, id.clone()));
                Ok(rows.get(&(company, kind, id)).cloned())
            })
        }
    }

    fn probe() -> Arc<Probe> {
        let mut rows = BTreeMap::new();
        rows.insert(
            ("co-1".to_owned(), EntrypointKind::OrderFacts, "o-1".to_owned()),
            EntrypointFacts {
                kind: "sales".to_owned(),
                state: Some("draft".to_owned()),
            },
        );
        rows.insert(
            ("co-1".to_owned(), EntrypointKind::ReturnKind, "r-1".to_owned()),
            EntrypointFacts { kind: "sales".to_owned(), state: None },
        );
        Arc::new(Probe { rows, seen: Arc::new(Mutex::new(Vec::new())) })
    }

    fn headers(host: &str, company: &str) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(header::HOST, HeaderValue::from_str(host).expect("host"));
        headers.insert(CONTEXT_COMPANY, HeaderValue::from_str(company).expect("company"));
        headers
    }

    fn target() -> Vec<LaneTarget> {
        vec![LaneTarget {
            host: "acme.example.test".to_owned(),
            holding_slug: "acme".to_owned(),
        }]
    }

    #[tokio::test]
    async fn owner_facts_shape_and_company_scope() {
        let lane = state(target(), probe()).expect("state");
        let body = br#"{"kind":"orderFacts","id":"o-1"}"#;
        let value = process(
            &headers("acme.example.test", "co-1"),
            "acme",
            body,
            &lane,
        )
        .await
        .expect("facts");
        assert_eq!(value, serde_json::json!({"facts":{"kind":"sales","state":"draft"}}));
    }

    #[tokio::test]
    async fn missing_facts_are_null_not_authority() {
        let lane = state(target(), probe()).expect("state");
        let body = br#"{"kind":"invoiceKind","id":"missing"}"#;
        let value = process(
            &headers("acme.example.test", "co-1"),
            "acme",
            body,
            &lane,
        )
        .await
        .expect("facts");
        assert_eq!(value, serde_json::json!({"facts":null}));
    }

    #[tokio::test]
    async fn wrong_host_duplicate_headers_and_bad_company_fail_closed() {
        let lane = state(target(), probe()).expect("state");
        let body = br#"{"kind":"orderKind","id":"o-1"}"#;
        assert_eq!(
            process(&headers("attacker.example.test", "co-1"), "acme", body, &lane)
                .await
                .unwrap_err()
                .kind,
            FailureKind::NotFound
        );
        assert_eq!(
            process(&headers("acme.example.test", "bad company!"), "acme", body, &lane)
                .await
                .unwrap_err()
                .kind,
            FailureKind::InvalidInput
        );
        let mut duplicate = headers("acme.example.test", "co-1");
        duplicate.append(CONTEXT_COMPANY, HeaderValue::from_static("co-2"));
        assert_eq!(
            process(&duplicate, "acme", body, &lane).await.unwrap_err().kind,
            FailureKind::InvalidInput
        );
    }

    #[test]
    fn router_requires_exact_distinct_targets() {
        let reader: LaneReader = probe();
        assert!(router_with_reader(reader.clone(), target()).is_ok());
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(router_with_reader(reader.clone(), vec![target()[0].clone(), target()[0].clone()]).is_err());
        assert!(router_with_reader(
            reader,
            vec![LaneTarget { host: String::new(), holding_slug: "acme".to_owned() }],
        )
        .is_err());
    }

    /// Mirrors `router_for_holdings` with injected readers instead of pools:
    /// two targets must resolve to two distinct readers keyed by their own
    /// `(host, holding_slug)`, never a shared reader.
    #[tokio::test]
    async fn per_holding_readers_are_isolated() {
        let acme = probe();
        let northstar = probe();
        let lane = state_with_readers(vec![
            (
                LaneTarget {
                    host: "acme.example.test".to_owned(),
                    holding_slug: "acme".to_owned(),
                },
                acme.clone() as LaneReader,
            ),
            (
                LaneTarget {
                    host: "northstar.example.test".to_owned(),
                    holding_slug: "northstar".to_owned(),
                },
                northstar.clone() as LaneReader,
            ),
        ])
        .expect("state");
        assert_eq!(lane.targets.len(), 2);
        let acme_reader = lane
            .targets
            .get(&("acme.example.test".to_owned(), "acme".to_owned()))
            .expect("acme key");
        let northstar_reader = lane
            .targets
            .get(&("northstar.example.test".to_owned(), "northstar".to_owned()))
            .expect("northstar key");
        assert!(!Arc::ptr_eq(acme_reader, northstar_reader));

        let body = br#"{"kind":"orderFacts","id":"o-1"}"#;
        let acme_value = process(
            &headers("acme.example.test", "co-1"),
            "acme",
            body,
            &lane,
        )
        .await
        .expect("acme read");
        let northstar_value = process(
            &headers("northstar.example.test", "co-2"),
            "northstar",
            body,
            &lane,
        )
        .await
        .expect("northstar read");
        assert_eq!(acme_value, serde_json::json!({"facts":{"kind":"sales","state":"draft"}}));
        assert_eq!(northstar_value, serde_json::json!({"facts":null}));
        assert_eq!(
            *acme.seen.lock().expect("acme lock"),
            vec![("co-1".to_owned(), EntrypointKind::OrderFacts, "o-1".to_owned())]
        );
        assert_eq!(
            *northstar.seen.lock().expect("northstar lock"),
            vec![("co-2".to_owned(), EntrypointKind::OrderFacts, "o-1".to_owned())]
        );
        // A host/slug mismatch never falls back to the other Holding's reader.
        assert_eq!(
            process(&headers("acme.example.test", "co-1"), "northstar", body, &lane)
                .await
                .unwrap_err()
                .kind,
            FailureKind::NotFound
        );
    }

    #[derive(Clone)]
    struct InvoiceProbe {
        seen: Arc<Mutex<Vec<(String, InvoiceReadKind, String)>>>,
        target: Option<CommandTarget>,
        lines: Vec<SalesLine>,
    }

    impl CommerceInvoiceRead for InvoiceProbe {
        fn read<'a>(
            &'a self,
            company: &'a str,
            kind: InvoiceReadKind,
            id: &'a str,
        ) -> InvoiceReadFuture<'a, InvoiceFacts> {
            let seen = self.seen.clone();
            let target = self.target.clone();
            let lines = self.lines.clone();
            let company = company.to_owned();
            let id = id.to_owned();
            Box::pin(async move {
                seen.lock().expect("lock").push((company, kind, id));
                Ok(match kind {
                    InvoiceReadKind::DeliveryTarget => InvoiceFacts::Target(target),
                    InvoiceReadKind::InvoiceSalesLines => InvoiceFacts::Lines(lines),
                })
            })
        }
    }

    fn invoice_probe(target: Option<&str>) -> Arc<InvoiceProbe> {
        Arc::new(InvoiceProbe {
            seen: Arc::new(Mutex::new(Vec::new())),
            target: target.map(|order_id| CommandTarget {
                kind: "order".to_owned(),
                id: order_id.to_owned(),
                action: "invoice".to_owned(),
            }),
            lines: vec![SalesLine {
                id: "fl-1".to_owned(),
                fulfillment_id: "f-1".to_owned(),
                description: "service".to_owned(),
                quantity_milli: 1000,
                price_minor: 250,
                tax_bps: 700,
                subtotal_minor: 250,
                tax_minor: 18,
                item_type: "service".to_owned(),
                product_id: "p-1".to_owned(),
                completion_date: "2026-11-25".to_owned(),
            }],
        })
    }

    #[tokio::test]
    async fn invoice_target_and_lines_envelopes() {
        let reader = invoice_probe(Some("o-1"));
        let seen = reader.seen.clone();
        let lane = state_with_both(
            vec![(target()[0].clone(), probe() as LaneReader)],
            vec![(target()[0].clone(), reader as InvoiceReader)],
        )
        .expect("state");
        let target = process_invoice(
            &headers("acme.example.test", "co-1"),
            "acme",
            br#"{"kind":"deliveryTarget","id":"d-1"}"#,
            &lane,
        )
        .await
        .expect("target");
        assert_eq!(
            target,
            serde_json::json!({"target":{"kind":"order","id":"o-1","action":"invoice"}})
        );
        let lines = process_invoice(
            &headers("acme.example.test", "co-1"),
            "acme",
            br#"{"kind":"invoiceSalesLines","id":"inv-1"}"#,
            &lane,
        )
        .await
        .expect("lines");
        assert_eq!(lines["lines"][0]["id"], "fl-1");
        assert_eq!(lines["lines"][0]["quantity_milli"], 1000);
        assert!(lines.get("target").is_none());
        assert_eq!(
            *seen.lock().expect("seen"),
            vec![
                ("co-1".to_owned(), InvoiceReadKind::DeliveryTarget, "d-1".to_owned()),
                ("co-1".to_owned(), InvoiceReadKind::InvoiceSalesLines, "inv-1".to_owned()),
            ]
        );
    }

    #[tokio::test]
    async fn invoice_readers_are_isolated_per_holding() {
        let acme = invoice_probe(Some("acme-order"));
        let northstar = invoice_probe(Some("northstar-order"));
        let acme_seen = acme.seen.clone();
        let northstar_seen = northstar.seen.clone();
        let lane = state_with_both(
            vec![
                (target()[0].clone(), probe() as LaneReader),
                (
                    LaneTarget {
                        host: "northstar.example.test".to_owned(),
                        holding_slug: "northstar".to_owned(),
                    },
                    probe() as LaneReader,
                ),
            ],
            vec![
                (target()[0].clone(), acme as InvoiceReader),
                (
                    LaneTarget {
                        host: "northstar.example.test".to_owned(),
                        holding_slug: "northstar".to_owned(),
                    },
                    northstar as InvoiceReader,
                ),
            ],
        )
        .expect("state");
        let body = br#"{"kind":"deliveryTarget","id":"d-1"}"#;
        assert_eq!(
            process_invoice(&headers("acme.example.test", "co-1"), "acme", body, &lane)
                .await
                .expect("acme")["target"]["id"],
            "acme-order"
        );
        assert_eq!(
            process_invoice(&headers("northstar.example.test", "co-2"), "northstar", body, &lane)
                .await
                .expect("northstar")["target"]["id"],
            "northstar-order"
        );
        assert_eq!(
            *acme_seen.lock().expect("acme seen"),
            vec![("co-1".to_owned(), InvoiceReadKind::DeliveryTarget, "d-1".to_owned())]
        );
        assert_eq!(
            *northstar_seen.lock().expect("northstar seen"),
            vec![("co-2".to_owned(), InvoiceReadKind::DeliveryTarget, "d-1".to_owned())]
        );
        assert_eq!(
            process_invoice(&headers("acme.example.test", "co-1"), "northstar", body, &lane)
                .await
                .unwrap_err()
                .kind,
            FailureKind::NotFound
        );
        assert_eq!(
            process_invoice(
                &headers("acme.example.test", "bad company"),
                "acme",
                body,
                &lane,
            )
            .await
            .unwrap_err()
            .kind,
            FailureKind::InvalidInput
        );
    }
}
