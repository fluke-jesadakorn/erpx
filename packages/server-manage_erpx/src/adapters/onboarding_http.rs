//! Owner-local HTTP ingress for the Manage ERPX onboarding view.
//!
//! Transport responsibilities only: registered Host selection, bounded JSON,
//! private responses and business-error mapping. The composition backend
//! supplies the verified Account session and the owner read transaction.
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type ManageErpxHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct ManageErpxOnboardingRequest<'a> {
    pub host: &'a str,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct ManageErpxHttpResponse {
    pub data: serde_json::Value,
    pub context: serde_json::Value,
}

impl ManageErpxHttpResponse {
    pub fn value(self) -> serde_json::Value {
        serde_json::json!({"data": self.data, "context": self.context})
    }
}

/// Composition-owned Account verification and owner unit. The router never
/// mints a session, verified email, Holding, Company or authority.
pub trait ManageErpxHttpBackend: Send + Sync {
    fn onboarding_view<'a>(
        &'a self,
        request: ManageErpxOnboardingRequest<'a>,
    ) -> ManageErpxHttpFuture<'a, ManageErpxHttpResponse>;
}

pub struct ManageErpxHttpTarget {
    pub host: String,
    pub backend: Arc<dyn ManageErpxHttpBackend>,
}

struct ManageErpxHttpState {
    targets: BTreeMap<String, ManageErpxHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid_request() -> BusinessError {
    BusinessError::invalid(
        "MANAGE_ERPX_REQUEST_INVALID",
        "Use one exact registered application host.",
    )
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid_request());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid_request()))
        .transpose()
}

fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 255
        && host.bytes().all(|byte| {
            byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'-' | b':' | b'[' | b']')
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
    state: &'a ManageErpxHttpState,
    headers: &HeaderMap,
) -> Result<&'a ManageErpxHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state.targets.get(host).ok_or_else(missing)
}

/// Build the owner-local router. Every configured host is an exact production
/// or development front origin; no wildcard or path-derived target exists.
pub fn manage_erpx_router(targets: Vec<ManageErpxHttpTarget>) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if !valid_host(&target.host) || configured.insert(target.host.clone(), target).is_some() {
            return Err(BusinessError::invalid(
                "MANAGE_ERPX_CONFIGURATION_INVALID",
                "Use exact distinct registered Manage ERPX hosts.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "MANAGE_ERPX_CONFIGURATION_INVALID",
            "At least one registered Manage ERPX host is required.",
        ));
    }
    Ok(Router::new()
        .route("/api/v1/onboarding", get(read_onboarding))
        .with_state(Arc::new(ManageErpxHttpState {
            targets: configured,
        })))
}

async fn read_onboarding(
    State(state): State<Arc<ManageErpxHttpState>>,
    request: Request,
) -> Response {
    let result = async move {
        let headers = request.headers().clone();
        let target = target(&state, &headers)?;
        target
            .backend
            .onboarding_view(ManageErpxOnboardingRequest {
                host: &target.host,
                headers: &headers,
            })
            .await
    }
    .await;
    match result {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::{Body, body::to_bytes};

    fn noop_response() -> ManageErpxHttpResponse {
        ManageErpxHttpResponse {
            data: serde_json::json!({
                "enabled": true,
                "slot": {
                    "slug": "acme",
                    "company_name": "Acme Company",
                    "branch_name": "Main",
                    "state": "provisioning"
                }
            }),
            context: serde_json::json!({"userId": "owner"}),
        }
    }

    struct Success;
    impl ManageErpxHttpBackend for Success {
        fn onboarding_view<'a>(
            &'a self,
            request: ManageErpxOnboardingRequest<'a>,
        ) -> ManageErpxHttpFuture<'a, ManageErpxHttpResponse> {
            Box::pin(async move {
                assert_eq!(request.host, "acme.example.test");
                Ok(noop_response())
            })
        }
    }

    struct Conflict;
    impl ManageErpxHttpBackend for Conflict {
        fn onboarding_view<'a>(
            &'a self,
            _: ManageErpxOnboardingRequest<'a>,
        ) -> ManageErpxHttpFuture<'a, ManageErpxHttpResponse> {
            Box::pin(async {
                Err(BusinessError::new(
                    FailureKind::Conflict,
                    "ONBOARDING_CHANGED",
                    "Synthetic changed onboarding state.",
                ))
            })
        }
    }

    fn request(hosts: &[&str]) -> Request {
        let mut request = Request::builder()
            .uri("/api/v1/onboarding")
            .body(Body::empty())
            .unwrap();
        for host in hosts {
            request
                .headers_mut()
                .append(header::HOST, HeaderValue::from_str(host).unwrap());
        }
        request
    }

    fn state(backend: Arc<dyn ManageErpxHttpBackend>) -> Arc<ManageErpxHttpState> {
        Arc::new(ManageErpxHttpState {
            targets: BTreeMap::from([(
                "acme.example.test".to_owned(),
                ManageErpxHttpTarget {
                    host: "acme.example.test".to_owned(),
                    backend,
                },
            )]),
        })
    }

    async fn body(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 64 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    #[test]
    fn router_configuration_requires_distinct_nonempty_registered_hosts() {
        let target = || ManageErpxHttpTarget {
            host: "acme.example.test".into(),
            backend: Arc::new(Success),
        };
        assert!(manage_erpx_router(vec![target()]).is_ok());
        assert!(manage_erpx_router(vec![target(), target()]).is_err());
        assert!(
            manage_erpx_router(vec![ManageErpxHttpTarget {
                host: String::new(),
                backend: Arc::new(Success),
            }])
            .is_err()
        );
        assert!(
            manage_erpx_router(vec![ManageErpxHttpTarget {
                host: "bad host/path".into(),
                backend: Arc::new(Success),
            }])
            .is_err()
        );
        assert!(manage_erpx_router(vec![]).is_err());
    }

    #[test]
    fn handler_returns_private_bounded_json_and_maps_transport_failures() {
        sqlx::test_block_on(async {
            let response = read_onboarding(
                State(state(Arc::new(Success))),
                request(&["acme.example.test"]),
            )
            .await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers().get(header::CACHE_CONTROL).unwrap(),
                "private, no-store"
            );
            assert_eq!(
                response.headers().get(header::REFERRER_POLICY).unwrap(),
                "no-referrer"
            );
            assert_eq!(
                response
                    .headers()
                    .get(header::X_CONTENT_TYPE_OPTIONS)
                    .unwrap(),
                "nosniff"
            );
            let value = body(response).await;
            assert_eq!(value["data"]["slot"]["slug"], "acme");
            assert_eq!(value["context"]["userId"], "owner");

            let response = read_onboarding(
                State(state(Arc::new(Success))),
                request(&["missing.example.test"]),
            )
            .await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
            assert_eq!(body(response).await["error"]["code"], "NOT_FOUND");

            let response = read_onboarding(
                State(state(Arc::new(Success))),
                request(&["acme.example.test", "other.example.test"]),
            )
            .await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(
                body(response).await["error"]["code"],
                "MANAGE_ERPX_REQUEST_INVALID"
            );

            let response = read_onboarding(
                State(state(Arc::new(Conflict))),
                request(&["acme.example.test"]),
            )
            .await;
            assert_eq!(response.status(), StatusCode::CONFLICT);
            assert_eq!(body(response).await["error"]["code"], "ONBOARDING_CHANGED");
        });
    }
}
