//! Owner-local HTTP ingress for the Billing seat-forecast baseline read.
//!
//! This is the native authority for `GET
//! /api/v1/holdings/{holding_slug}/billing/forecast`. Transport facts only:
//! exact registered Host/Holding target, exact browser Origin, bounded query
//! and business-error mapping. The Account/Platform session and current
//! Holding authority are composition responsibilities supplied through the
//! backend trait. The SQL lives in [`crate::adapters::forecast_read`], which
//! the composition backend calls on a Control connection; this handler never
//! prepares or executes SQL and never reads a Holding database.
use crate::domain::forecast_read::{SeatForecastBaseline, target_holding};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    get, header, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind, HoldingId};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type BillingForecastHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct BillingForecastHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    /// Platform selection of a customer Holding on the ERPX (`erpx`) surface.
    /// Bounded here and applied by composition when it builds the actor and
    /// acquires the Control connection; never taken from a browser header.
    pub platform_customer: Option<HoldingId>,
}

#[derive(Clone, Debug)]
pub struct BillingForecastHttpResponse {
    /// Exact `SeatForecastBaseline` wire payload.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

/// Composition owns the session, authority and Control connection.
pub trait BillingForecastHttpBackend: Send + Sync {
    fn forecast<'a>(
        &'a self,
        request: BillingForecastHttpRequest<'a>,
    ) -> BillingForecastHttpFuture<'a, BillingForecastHttpResponse>;
}

pub struct BillingForecastHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn BillingForecastHttpBackend>,
}

pub struct BillingForecastHttpState {
    targets: BTreeMap<(String, String), BillingForecastHttpTarget>,
}

struct ParsedTarget<'a> {
    target: &'a BillingForecastHttpTarget,
    host: &'a str,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "BILLING_FORECAST_REQUEST_INVALID",
        "Use one bounded Billing forecast request.",
    )
}

fn source_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "BILLING_FORECAST_SOURCE_INVALID",
        "The native Billing forecast response is unavailable.",
    )
}

fn platform_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "FORBIDDEN",
        "Platform billing permission required.",
    )
}

/// The platform read is addressed to the ERPX (`erpx`) Holding surface with an
/// explicit customer `holdingId`, mirroring the established native Billing
/// workspace selection. The owner bounds the selector before composition sees
/// it: `h003` is the ERPX management workspace, never a customer forecast, and
/// `target_holding` maps it to the same `HOLDING_REQUIRED` failure the retained
/// platform transport returned. Every other surface rejects any query.
fn platform_customer(
    query: Option<&str>,
    holding_slug: &str,
) -> Result<Option<HoldingId>, BusinessError> {
    let Some(query) = query else {
        return Ok(None);
    };
    if holding_slug != "erpx" {
        return Err(request_invalid());
    }
    let id = query
        .strip_prefix("holdingId=")
        .ok_or_else(request_invalid)?;
    if id.is_empty()
        || id.len() > 64
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-')
        })
    {
        return Err(request_invalid());
    }
    target_holding(id).map(Some)
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(request_invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| request_invalid()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    origin.strip_prefix("https://").is_some_and(|rest| {
        !rest.is_empty()
            && !rest
                .bytes()
                .any(|byte| matches!(byte, b'/' | b'?' | b'#' | b'@' | b':'))
    }) || origin
        .strip_prefix("http://127.0.0.1:")
        .is_some_and(|port| {
            !port.is_empty() && port.len() <= 5 && port.bytes().all(|byte| byte.is_ascii_digit())
        })
}

fn secured(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn json_response(status: StatusCode, value: Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    secured(response)
}

fn error_response(error: BusinessError) -> Response {
    let status = match error.kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    json_response(
        status,
        json!({"error": {"code": error.code, "message": error.message}}),
    )
}

impl BillingForecastHttpState {
    pub fn new(targets: Vec<BillingForecastHttpTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || target.holding_slug.is_empty()
                || target.holding_slug.len() > 128
                || !target.holding_slug.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                || !valid_origin(&target.origin)
                || configured
                    .insert((target.host.clone(), target.holding_slug.clone()), target)
                    .is_some()
            {
                return Err(BusinessError::invalid(
                    "BILLING_FORECAST_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        Ok(Self {
            targets: configured,
        })
    }

    fn target<'a>(
        &'a self,
        headers: &'a HeaderMap,
        holding_slug: &str,
    ) -> Result<ParsedTarget<'a>, BusinessError> {
        let host = unique_header(headers, "host")?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        if unique_header(headers, "origin")?.is_some_and(|value| value != target.origin) {
            return Err(origin_forbidden());
        }
        Ok(ParsedTarget { target, host })
    }

    /// Exact GET handler used by both the Router and owner tests.
    pub async fn forecast(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Response {
        match self.forecast_inner(headers, holding_slug, query).await {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn forecast_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Result<Response, BusinessError> {
        let customer = platform_customer(query, holding_slug)?;
        let target = self.target(headers, holding_slug)?;
        let response = target
            .target
            .backend
            .forecast(BillingForecastHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
                platform_customer: customer.clone(),
            })
            .await?;
        if !response.context.is_object() {
            return Err(source_invalid());
        }
        // The platform role is authority, never a request header. Composition
        // resolves it from the live session/assignment and renders it into the
        // context; the owner checks that rendered role before returning the
        // customer forecast. The holding-owner boundary stays in the owner
        // actor policy used by the composition backend.
        if customer.is_some() {
            let role = response
                .context
                .get("role")
                .and_then(serde_json::Value::as_str);
            if !matches!(role, Some("platform_admin" | "platform_auditor"))
                || response.context.get("platform").and_then(serde_json::Value::as_bool) != Some(true)
            {
                return Err(platform_forbidden());
            }
        }
        // Round-trip the complete wire payload through the owner domain. An
        // extra, missing or renamed field fails closed instead of being
        // silently projected into a partial client shape.
        let baseline: SeatForecastBaseline =
            serde_json::from_value(response.data).map_err(|_| source_invalid())?;
        if baseline.model != "seat_what_if"
            || !matches!(
                baseline.storage.coverage.as_str(),
                "measured" | "partial" | "unavailable"
            )
        {
            return Err(source_invalid());
        }
        Ok(json_response(
            StatusCode::OK,
            json!({"data": baseline, "context": response.context}),
        ))
    }
}

async fn forecast_handler(
    State(state): State<Arc<BillingForecastHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    state
        .forecast(request.headers(), &holding_slug, query.as_deref())
        .await
}

pub fn billing_forecast_router(
    targets: Vec<BillingForecastHttpTarget>,
) -> Result<Router, BusinessError> {
    let state = Arc::new(BillingForecastHttpState::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/billing/forecast",
            get(forecast_handler),
        )
        .with_state(state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::forecast_read::baseline;
    use erpx_server_shared::public::HoldingId;
    use std::sync::Mutex;

    struct Fake {
        seen: Mutex<Vec<String>>,
        data: Value,
        context: Value,
    }

    impl BillingForecastHttpBackend for Fake {
        fn forecast<'a>(
            &'a self,
            request: BillingForecastHttpRequest<'a>,
        ) -> BillingForecastHttpFuture<'a, BillingForecastHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "{}:{}:{}",
                    request.host,
                    request.holding_slug,
                    request
                        .platform_customer
                        .as_ref()
                        .map(HoldingId::as_str)
                        .unwrap_or("-")
                ));
                Ok(BillingForecastHttpResponse {
                    data: self.data.clone(),
                    context: self.context.clone(),
                })
            })
        }
    }

    fn baseline_value() -> Value {
        serde_json::to_value(
            baseline(
                &HoldingId::new("h001").unwrap(),
                1_700_000_000_000,
                None,
                None,
                None,
            )
            .unwrap(),
        )
        .unwrap()
    }

    fn state(data: Value) -> (BillingForecastHttpState, Arc<Fake>) {
        let fake = Arc::new(Fake {
            seen: Mutex::new(Vec::new()),
            data,
            context: json!({"userId": "admin"}),
        });
        let state = BillingForecastHttpState::new(vec![BillingForecastHttpTarget {
            host: "holding.erpxbrowser.test:3102".into(),
            holding_slug: "acme".into(),
            origin: "http://127.0.0.1:3102".into(),
            backend: fake.clone(),
        }])
        .unwrap();
        (state, fake)
    }

    fn platform_state(
        context: Value,
    ) -> (BillingForecastHttpState, Arc<Fake>) {
        let fake = Arc::new(Fake {
            seen: Mutex::new(Vec::new()),
            data: baseline_value(),
            context,
        });
        let state = BillingForecastHttpState::new(vec![BillingForecastHttpTarget {
            host: "erpx-internal.jesadakorn.com".into(),
            holding_slug: "erpx".into(),
            origin: "https://erpx-internal.jesadakorn.com".into(),
            backend: fake.clone(),
        }])
        .unwrap();
        (state, fake)
    }

    fn headers(origin: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("host", "holding.erpxbrowser.test:3102".parse().unwrap());
        if let Some(origin) = origin {
            headers.insert("origin", origin.parse().unwrap());
        }
        headers
    }

    fn platform_headers() -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert(
            "host",
            "erpx-internal.jesadakorn.com".parse().unwrap(),
        );
        headers.insert(
            "origin",
            "https://erpx-internal.jesadakorn.com".parse().unwrap(),
        );
        headers
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(future)
    }

    #[test]
    fn forecast_returns_the_complete_owner_payload() {
        let (state, fake) = state(baseline_value());
        let response = block_on(state.forecast(
            &headers(Some("http://127.0.0.1:3102")),
            "acme",
            None,
        ));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            fake.seen.lock().unwrap().as_slice(),
            ["holding.erpxbrowser.test:3102:acme:-"]
        );
        let body = block_on(async {
            let bytes = erpx_server_runtime::public::http_support::body::to_bytes(
                response.into_body(),
                64 * 1024,
            )
            .await
            .unwrap();
            serde_json::from_slice::<Value>(&bytes).unwrap()
        });
        let data = body.get("data").unwrap();
        assert_eq!(data.as_object().unwrap().len(), 8);
        assert_eq!(data.get("model").unwrap(), &json!("seat_what_if"));
        assert_eq!(
            data.get("storage").unwrap().get("coverage").unwrap(),
            &json!("unavailable")
        );
    }

    #[test]
    fn rejects_foreign_target_query_and_incomplete_payload() {
        let (router_state, _) = state(baseline_value());
        for (origin, slug, query) in [
            (Some("https://evil.test"), "acme", None),
            (Some("http://127.0.0.1:3102"), "other", None),
            (Some("http://127.0.0.1:3102"), "acme", Some("month=2027-01")),
        ] {
            let response = block_on(router_state.forecast(&headers(origin), slug, query));
            assert!(response.status().is_client_error());
        }
        let mut partial = baseline_value();
        partial.as_object_mut().unwrap().remove("seats");
        let (partial_state, _) = state(partial);
        let response = block_on(partial_state.forecast(&headers(Some("http://127.0.0.1:3102")), "acme", None));
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn platform_selection_carries_the_customer_and_the_canonical_envelope() {
        let (state, fake) = platform_state(json!({
            "userId": "operator", "holdingId": "h003", "holdingSlug": "erpx",
            "platform": true, "role": "platform_admin", "assignmentId": "platform-assignment"
        }));
        let response = block_on(state.forecast(
            &platform_headers(),
            "erpx",
            Some("holdingId=h001"),
        ));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            fake.seen.lock().unwrap().as_slice(),
            ["erpx-internal.jesadakorn.com:erpx:h001"]
        );
        assert_eq!(response.headers().get(header::CACHE_CONTROL).unwrap(), "private, no-store");
        let body = block_on(async {
            let bytes = erpx_server_runtime::public::http_support::body::to_bytes(
                response.into_body(),
                64 * 1024,
            )
            .await
            .unwrap();
            serde_json::from_slice::<Value>(&bytes).unwrap()
        });
        // The owner owns the envelope: exactly the canonical `{data, context}`
        // object, with the complete 8-field baseline and no projection.
        let object = body.as_object().unwrap();
        assert_eq!(object.keys().map(String::as_str).collect::<Vec<_>>(), ["context", "data"]);
        assert_eq!(object.get("data").unwrap().as_object().unwrap().len(), 8);
        assert_eq!(object.get("context").unwrap().get("role").unwrap(), &json!("platform_admin"));
    }

    #[test]
    fn platform_selection_fails_closed_for_reserved_holding_and_role() {
        let (state, fake) = platform_state(json!({
            "platform": true, "role": "platform_admin"
        }));
        let reserved = block_on(state.forecast(&platform_headers(), "erpx", Some("holdingId=h003")));
        assert_eq!(reserved.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(fake.seen.lock().unwrap().len(), 0);
        let malformed = block_on(state.forecast(&platform_headers(), "erpx", Some("holdingId=H001")));
        assert!(malformed.status().is_client_error());

        for context in [
            json!({"platform": true, "role": "platform_support"}),
            json!({"platform": false, "role": "holding_owner"}),
            json!({"platform": true}),
        ] {
            let (state, fake) = platform_state(context);
            let response = block_on(state.forecast(&platform_headers(), "erpx", Some("holdingId=h001")));
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
            let body = block_on(async {
                let bytes = erpx_server_runtime::public::http_support::body::to_bytes(
                    response.into_body(),
                    64 * 1024,
                )
                .await
                .unwrap();
                serde_json::from_slice::<Value>(&bytes).unwrap()
            });
            assert_eq!(body.get("error").unwrap().get("code").unwrap(), &json!("FORBIDDEN"));
            assert_eq!(fake.seen.lock().unwrap().len(), 1);
        }
    }

    #[test]
    fn rejects_a_customer_selector_outside_the_platform_surface() {
        let (state, fake) = state(baseline_value());
        let response = block_on(state.forecast(
            &headers(Some("http://127.0.0.1:3102")),
            "acme",
            Some("holdingId=h001"),
        ));
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(fake.seen.lock().unwrap().len(), 0);
    }
}
