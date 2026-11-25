//! Owner-local axum router for the lane 33 holding-binding reads.
//!
//! Mirrors the target/header/company isolation and error mapping of
//! `server-commerce::adapters::quality_product_read`. The caller submits only a
//! validated resource-slot scope; the owner executes the SQL and the projection.

use super::application::SharedHoldingBindingRead;
use super::domain::{
    BindingQuery, HoldingBinding, parse_scope, valid_slug,
};
use super::postgres::PgHoldingBindingRead;
use crate::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode, header, routing::get, serde_json,
};
use crate::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const SCOPE: HeaderName = HeaderName::from_static("x-erpx-allowed-holdings");
const SLUG: HeaderName = HeaderName::from_static("x-erpx-holding-slug");

/// Exact registered Holding target; the lead mounts one router per owner.
#[derive(Clone, Debug)]
pub struct LaneTarget {
    pub host: String,
    pub holding_slug: String,
}

struct ReadState {
    targets: BTreeMap<(String, String), SharedHoldingBindingRead>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "RUNTIME_HOLDING_BINDING_READ_REQUEST_INVALID",
        "Use one bounded Runtime holding-binding read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "RUNTIME_HOLDING_BINDING_READ_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
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

fn binding_json(binding: HoldingBinding) -> serde_json::Value {
    serde_json::json!({
        "holdingId": binding.holding_id,
        "slug": binding.slug,
        "displayName": binding.display_name,
    })
}

/// Build with a caller-supplied reader. Used by composition and by contract
/// tests; no SQL is introduced here.
pub fn router_with_reader(
    reader: SharedHoldingBindingRead,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    reader.clone(),
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/runtime/holding-bindings/{kind}",
            get(read_binding),
        )
        .with_state(Arc::new(ReadState {
            targets: configured,
        })))
}

/// Pool-backed variant matching the lane contract. The lead mounts this router.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(
        Arc::new(PgHoldingBindingRead::new(pool)),
        targets,
    )
}

async fn read_binding(
    State(state): State<Arc<ReadState>>,
    Path((slug, kind)): Path<(String, String)>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        let kind = BindingQuery::parse(&kind).ok_or_else(invalid)?;
        let scope = unique_header(request.headers(), SCOPE)?
            .and_then(parse_scope)
            .ok_or_else(invalid)?;
        let body = match kind {
            BindingQuery::BySlug => {
                let slug = unique_header(request.headers(), SLUG)?
                    .filter(|value| valid_slug(value))
                    .ok_or_else(invalid)?;
                let binding = reader.by_slug(&scope, slug).await?;
                serde_json::json!({"binding": binding.map(binding_json)})
            }
            BindingQuery::Default => {
                let binding = reader.default_binding(&scope).await?;
                serde_json::json!({"binding": binding.map(binding_json)})
            }
            BindingQuery::Active => {
                let bindings = reader.active(&scope).await?;
                serde_json::json!({"bindings": bindings.into_iter().map(binding_json).collect::<Vec<_>>()})
            }
        };
        Ok::<_, BusinessError>(body)
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
    use super::super::application::{HoldingBindingRead, unavailable};
    use crate::adapters::native_lane_33::domain::HoldingBinding as Binding;
    use tower::ServiceExt;

    struct Fake;

    impl HoldingBindingRead for Fake {
        fn by_slug<'a>(
            &'a self,
            _scope: &'a [String],
            slug: &'a str,
        ) -> crate::adapters::native_lane_33::application::HoldingBindingReadFuture<'a, Option<Binding>>
        {
            let slug = slug.to_owned();
            Box::pin(async move {
                Ok(Some(Binding {
                    holding_id: "h001".into(),
                    slug,
                    display_name: "Acme".into(),
                }))
            })
        }
        fn default_binding<'a>(
            &'a self,
            _scope: &'a [String],
        ) -> crate::adapters::native_lane_33::application::HoldingBindingReadFuture<'a, Option<Binding>>
        {
            Box::pin(async { Ok(None) })
        }
        fn active<'a>(
            &'a self,
            _scope: &'a [String],
        ) -> crate::adapters::native_lane_33::application::HoldingBindingReadFuture<'a, Vec<Binding>>
        {
            Box::pin(async {
                Ok(vec![Binding {
                    holding_id: "h001".into(),
                    slug: "acme".into(),
                    display_name: "Acme".into(),
                }])
            })
        }
    }

    fn targets() -> Vec<LaneTarget> {
        vec![LaneTarget {
            host: "acme.jesadakorn.com".into(),
            holding_slug: "acme".into(),
        }]
    }

    fn request(kind: &str, extra: &[(&str, &str)]) -> Request {
        let mut builder = axum::http::Request::builder()
            .method("GET")
            .uri(format!(
                "/api/v1/holdings/acme/runtime/holding-bindings/{kind}"
            ))
            .header("host", "acme.jesadakorn.com")
            .header("x-erpx-allowed-holdings", "h001,h002");
        for (name, value) in extra {
            builder = builder.header(*name, *value);
        }
        builder.body(axum::body::Body::empty()).unwrap()
    }

    async fn body(response: Response) -> serde_json::Value {
        let bytes = axum::body::to_bytes(response.into_body(), 64 * 1024)
            .await
            .unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn target_configuration_is_closed() {
        assert!(router_with_reader(Arc::new(Fake), targets()).is_ok());
        assert!(router_with_reader(Arc::new(Fake), vec![]).is_err());
        let duplicate = vec![
            LaneTarget { host: "a".into(), holding_slug: "acme".into() },
            LaneTarget { host: "b".into(), holding_slug: "acme".into() },
        ];
        // Distinct targets are allowed; exact duplicates are rejected.
        assert!(router_with_reader(Arc::new(Fake), duplicate).is_ok());
        assert!(router_with_reader(
            Arc::new(Fake),
            vec![LaneTarget { host: "a".into(), holding_slug: "acme".into() }; 2]
        )
        .is_err());
    }

    #[tokio::test]
    async fn by_slug_returns_bounded_binding() {
        let app = router_with_reader(Arc::new(Fake), targets()).unwrap();
        let response = app
            .clone()
            .oneshot(request("by-slug", &[("x-erpx-holding-slug", "acme")]))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()["cache-control"], "private, no-store");
        let value = body(response).await;
        assert_eq!(value["binding"]["holdingId"], "h001");
        assert_eq!(value["binding"]["slug"], "acme");
        assert_eq!(value["binding"]["displayName"], "Acme");
    }

    #[tokio::test]
    async fn active_returns_ordered_bindings_array() {
        let app = router_with_reader(Arc::new(Fake), targets()).unwrap();
        let response = app.oneshot(request("active", &[])).await.unwrap();
        let value = body(response).await;
        assert_eq!(value["bindings"][0]["holdingId"], "h001");
    }

    #[tokio::test]
    async fn malformed_scope_and_unknown_target_fail_closed() {
        let app = router_with_reader(Arc::new(Fake), targets()).unwrap();
        let response = app
            .clone()
            .oneshot(request("default", &[("x-erpx-allowed-holdings", "H001;DROP")]))
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let value = body(response).await;
        assert_eq!(
            value["error"]["code"],
            "RUNTIME_HOLDING_BINDING_READ_REQUEST_INVALID"
        );

        let mut bad = request("default", &[]);
        bad.headers_mut().insert(
            header::HOST,
            HeaderValue::from_static("unknown.jesadakorn.com"),
        );
        let response = app.oneshot(bad).await.unwrap();
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn unavailable_reader_maps_to_503() {
        struct Down;
        impl HoldingBindingRead for Down {
            fn by_slug<'a>(
                &'a self,
                _s: &'a [String],
                _slug: &'a str,
            ) -> crate::adapters::native_lane_33::application::HoldingBindingReadFuture<'a, Option<Binding>>
            {
                Box::pin(async { Err(unavailable()) })
            }
            fn default_binding<'a>(
                &'a self,
                _s: &'a [String],
            ) -> crate::adapters::native_lane_33::application::HoldingBindingReadFuture<'a, Option<Binding>>
            {
                Box::pin(async { Err(unavailable()) })
            }
            fn active<'a>(
                &'a self,
                _s: &'a [String],
            ) -> crate::adapters::native_lane_33::application::HoldingBindingReadFuture<'a, Vec<Binding>>
            {
                Box::pin(async { Err(unavailable()) })
            }
        }
        let app = router_with_reader(Arc::new(Down), targets()).unwrap();
        let response = app.oneshot(request("default", &[])).await.unwrap();
        assert_eq!(response.status(), StatusCode::SERVICE_UNAVAILABLE);
        let value = body(response).await;
        assert_eq!(
            value["error"]["code"],
            "RUNTIME_HOLDING_BINDING_READ_UNAVAILABLE"
        );
    }
}
