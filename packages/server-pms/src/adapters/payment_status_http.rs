//! Native PMS PromptPay payment-status HTTP ingress for the
//! `.../pms/stripe-payments` family (GET status read).
//!
//! Rust owns the exact registered Host/Holding target, the bounded `id` query
//! fact, the application use case and the complete response payload. The
//! handler executes no SQL: it calls a composition backend which resolves the
//! Account session, property scope and owner provider reconcile, then invokes
//! the owner use case over the sqlx adapter. No value below is trusted as
//! authority and no Finance/Inventory/People read happens here.
use crate::domain::payment_status::{PmsPaymentStatusFacts, PmsPaymentStatusView};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const QUERY_LIMIT: usize = 4 * 1024;
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

pub type PmsPaymentStatusHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct PmsPaymentStatusRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub facts: PmsPaymentStatusFacts,
}

/// Composition-owned Account session, scope and owner-unit adapter. The router
/// never mints a session, actor, Company, permission or staff label.
pub trait PmsPaymentStatusHttpBackend: Send + Sync {
    fn status<'a>(
        &'a self,
        request: PmsPaymentStatusRequest<'a>,
    ) -> PmsPaymentStatusHttpFuture<'a, PmsPaymentStatusView>;
}

/// Exact registered Holding target for the mounted owner router.
pub struct PmsPaymentStatusHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub backend: Arc<dyn PmsPaymentStatusHttpBackend>,
}

struct PmsPaymentStatusState {
    targets: BTreeMap<(String, String), PmsPaymentStatusHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn malformed() -> BusinessError {
    crate::domain::payment_status::request_invalid()
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

/// Bounded `id` query fact with duplicate-name refusal. The retained route used
/// `searchParams.get('id') ?? ''`; a missing/empty id flows to the use case,
/// which rejects it as a bounded request before any SQL is issued.
fn query_id(request: &Request) -> Result<String, BusinessError> {
    let Some(raw) = request.uri().query() else {
        return Ok(String::new());
    };
    if raw.len() > QUERY_LIMIT {
        return Err(malformed());
    }
    let mut id = None;
    for pair in raw.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if percent_decode(name)? != "id" {
            continue;
        }
        if id.is_some() {
            return Err(malformed());
        }
        id = Some(percent_decode(value)?);
    }
    Ok(id.unwrap_or_default())
}

fn configured_targets(
    targets: Vec<PmsPaymentStatusHttpTarget>,
) -> Result<BTreeMap<(String, String), PmsPaymentStatusHttpTarget>, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.holding_slug.is_empty()
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    target,
                )
                .is_some()
        {
            return Err(BusinessError::invalid(
                "PMS_PAYMENT_STATUS_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "PMS_PAYMENT_STATUS_CONFIG_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(configured)
}

/// Mount entry point with exact registered Holding targets and their backends.
pub fn pms_payment_status_router(
    targets: Vec<PmsPaymentStatusHttpTarget>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/pms/stripe-payments",
            get(payment_status),
        )
        .with_state(Arc::new(PmsPaymentStatusState {
            targets: configured_targets(targets)?,
        })))
}

async fn payment_status(
    State(state): State<Arc<PmsPaymentStatusState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let target = state
            .targets
            .get(&(host, slug.clone()))
            .ok_or_else(missing)?;
        let id = query_id(&request)?;
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| !value.is_empty())
            .ok_or_else(malformed)?
            .to_owned();
        let headers = request.headers().clone();
        let _ = to_bytes(request.into_body(), 0)
            .await
            .map_err(|_| malformed())?;
        let view = target
            .backend
            .status(PmsPaymentStatusRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                facts: PmsPaymentStatusFacts {
                    company_id: company,
                    id,
                },
            })
            .await?;
        Ok::<_, BusinessError>(serde_json::json!({"data": view}))
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
    use erpx_server_runtime::public::http_support::body::Body;

    struct Fake;

    impl PmsPaymentStatusHttpBackend for Fake {
        fn status<'a>(
            &'a self,
            request: PmsPaymentStatusRequest<'a>,
        ) -> PmsPaymentStatusHttpFuture<'a, PmsPaymentStatusView> {
            let id = request.facts.id.clone();
            Box::pin(async move {
                if id == "missing" {
                    return Err(crate::domain::payment_status::not_found());
                }
                Ok(PmsPaymentStatusView {
                    id,
                    state: "pending".into(),
                    kind: "receipt".into(),
                    amount_minor: 100,
                    qr_data: Some("qr".into()),
                    expires_at: 7,
                    entry_id: None,
                    refund_state: None,
                    payer_email_provided: true,
                })
            })
        }
    }

    fn state() -> Arc<PmsPaymentStatusState> {
        Arc::new(PmsPaymentStatusState {
            targets: configured_targets(vec![PmsPaymentStatusHttpTarget {
                host: "acme.jesadakorn.com".into(),
                holding_slug: "acme".into(),
                backend: Arc::new(Fake),
            }])
            .unwrap(),
        })
    }

    async fn call(uri: &str, host: &str, company: Option<&str>) -> Response {
        let mut builder = Request::builder().header(header::HOST, host).uri(uri);
        if let Some(company) = company {
            builder = builder.header(CONTEXT_COMPANY, company);
        }
        payment_status(
            State(state()),
            Path("acme".to_owned()),
            builder.body(Body::empty()).unwrap(),
        )
        .await
    }

    #[test]
    fn rejects_invalid_target_configuration() {
        let backend: Arc<dyn PmsPaymentStatusHttpBackend> = Arc::new(Fake);
        assert_eq!(
            pms_payment_status_router(vec![])
                .unwrap_err()
                .code,
            "PMS_PAYMENT_STATUS_CONFIG_INVALID"
        );
        assert!(pms_payment_status_router(vec![PmsPaymentStatusHttpTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
            backend,
        }])
        .is_ok());
    }

    #[tokio::test]
    async fn foreign_host_and_missing_scope_fail_closed() {
        assert_eq!(
            call(
                "/api/v1/holdings/acme/pms/stripe-payments?id=p-1",
                "attacker.example.test",
                Some("company")
            )
            .await
            .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            call(
                "/api/v1/holdings/acme/pms/stripe-payments?id=p-1",
                "acme.jesadakorn.com",
                None
            )
            .await
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn duplicate_id_query_is_rejected() {
        assert_eq!(
            call(
                "/api/v1/holdings/acme/pms/stripe-payments?id=a&id=b",
                "acme.jesadakorn.com",
                Some("company")
            )
            .await
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[tokio::test]
    async fn complete_payload_envelope() {
        let response = call(
            "/api/v1/holdings/acme/pms/stripe-payments?id=p-1",
            "acme.jesadakorn.com",
            Some("company"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["data"]["id"], "p-1");
        assert_eq!(value["data"]["amountMinor"], 100);
        assert_eq!(value["data"]["qrData"], "qr");
        assert_eq!(value["data"]["payerEmailProvided"], true);
        assert!(value["data"].get("amount_minor").is_none());
    }

    #[tokio::test]
    async fn backend_not_found_maps_to_retained_code() {
        let response = call(
            "/api/v1/holdings/acme/pms/stripe-payments?id=missing",
            "acme.jesadakorn.com",
            Some("company"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
        let body = to_bytes(response.into_body(), 1024).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(value["error"]["code"], "PMS_PAYMENT_NOT_FOUND");
    }
}
