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
use crate::domain::forecast_read::SeatForecastBaseline;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    get, header, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type BillingForecastHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct BillingForecastHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
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
        if query.is_some() {
            return Err(request_invalid());
        }
        let target = self.target(headers, holding_slug)?;
        let response = target
            .target
            .backend
            .forecast(BillingForecastHttpRequest {
                host: target.host,
                holding_slug: &target.target.holding_slug,
                headers,
            })
            .await?;
        if !response.context.is_object() {
            return Err(source_invalid());
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
    }

    impl BillingForecastHttpBackend for Fake {
        fn forecast<'a>(
            &'a self,
            request: BillingForecastHttpRequest<'a>,
        ) -> BillingForecastHttpFuture<'a, BillingForecastHttpResponse> {
            Box::pin(async move {
                self.seen.lock().unwrap().push(format!(
                    "{}:{}",
                    request.host, request.holding_slug
                ));
                Ok(BillingForecastHttpResponse {
                    data: self.data.clone(),
                    context: json!({"userId": "admin"}),
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

    fn headers(origin: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("host", "holding.erpxbrowser.test:3102".parse().unwrap());
        if let Some(origin) = origin {
            headers.insert("origin", origin.parse().unwrap());
        }
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
            ["holding.erpxbrowser.test:3102:acme"]
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
}
