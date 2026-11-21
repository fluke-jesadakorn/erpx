//! Owner-local HTTP ingress for the Manage ERPX onboarding command.
//!
//! Transport only: exact Host selection, a bounded JSON body, private responses
//! and business-error mapping. The composition backend verifies the current
//! Account session, the request origin and owns both transactions. This router
//! never mints a session, verified email, Holding, Company or authority.
use crate::adapters::onboarding_http::{
    ManageErpxHttpFuture, ManageErpxHttpResponse, bounded, error_response, invalid_request,
    missing, unique_header, valid_host,
};
use erpx_server_runtime::public::http_support::{
    HeaderMap, IntoResponse, Json, Request, Response, Router, State, StatusCode, body::to_bytes,
    header, post, serde_json,
};
use erpx_server_shared::public::BusinessError;
use std::{collections::BTreeMap, sync::Arc};

/// The retained browser body is bounded at 24,000 bytes.
pub const MANAGE_ERPX_COMMAND_BODY_LIMIT: usize = 24_000;

pub struct ManageErpxOnboardingCommandRequest<'a> {
    pub host: &'a str,
    pub headers: &'a HeaderMap,
    pub body: &'a serde_json::Value,
}

/// Composition-owned Account verification and owner command unit. The
/// backend must reject an origin/session that the current request did not
/// verify, and must pass the current Account's verified identity.
pub trait ManageErpxOnboardingCommandBackend: Send + Sync {
    fn onboarding_command<'a>(
        &'a self,
        request: ManageErpxOnboardingCommandRequest<'a>,
    ) -> ManageErpxHttpFuture<'a, ManageErpxHttpResponse>;
}

pub struct ManageErpxOnboardingCommandTarget {
    pub host: String,
    pub backend: Arc<dyn ManageErpxOnboardingCommandBackend>,
}

struct ManageErpxOnboardingCommandState {
    targets: BTreeMap<String, ManageErpxOnboardingCommandTarget>,
}

fn payload_too_large() -> Response {
    (
        StatusCode::PAYLOAD_TOO_LARGE,
        Json(serde_json::json!({
            "error": {
                "code": "INPUT_TOO_LARGE",
                "message": "Please shorten your request.",
            }
        })),
    )
        .into_response()
}

fn invalid_json() -> Response {
    (
        StatusCode::UNPROCESSABLE_ENTITY,
        Json(serde_json::json!({
            "error": {
                "code": "INVALID_JSON",
                "message": "Provide a JSON request.",
            }
        })),
    )
        .into_response()
}

fn target<'a>(
    state: &'a ManageErpxOnboardingCommandState,
    headers: &HeaderMap,
) -> Result<&'a ManageErpxOnboardingCommandTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state.targets.get(host).ok_or_else(missing)
}

/// Build the owner-local command router. It is additive to the mounted
/// `manage_erpx_router` GET and can be merged beside it by the lead.
pub fn manage_erpx_onboarding_command_router(
    targets: Vec<ManageErpxOnboardingCommandTarget>,
) -> Result<Router, BusinessError> {
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
        .route("/api/v1/onboarding", post(onboarding_command))
        .with_state(Arc::new(ManageErpxOnboardingCommandState {
            targets: configured,
        })))
}

async fn onboarding_command(
    State(state): State<Arc<ManageErpxOnboardingCommandState>>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let target = match target(&state, &headers) {
        Ok(target) => target,
        Err(error) => return error_response(error),
    };
    let content_length = match unique_header(&headers, header::CONTENT_LENGTH) {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    if let Some(length) = content_length.filter(|value| !value.is_empty()) {
        let length = match length.parse::<usize>() {
            Ok(length) => length,
            Err(_) => return error_response(invalid_request()),
        };
        if length > MANAGE_ERPX_COMMAND_BODY_LIMIT {
            return payload_too_large();
        }
    }
    let bytes = match to_bytes(request.into_body(), MANAGE_ERPX_COMMAND_BODY_LIMIT).await {
        Ok(bytes) => bytes,
        Err(_) => return payload_too_large(),
    };
    let body: serde_json::Value = match serde_json::from_slice(&bytes) {
        Ok(body) => body,
        Err(_) => return invalid_json(),
    };
    match target
        .backend
        .onboarding_command(ManageErpxOnboardingCommandRequest {
            host: &target.host,
            headers: &headers,
            body: &body,
        })
        .await
    {
        Ok(reply) => bounded(reply.value()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::onboarding_http::ManageErpxHttpResponse;
    use erpx_server_runtime::public::http_support::{HeaderValue, body::Body};
    use erpx_server_shared::public::FailureKind;

    fn response() -> ManageErpxHttpResponse {
        ManageErpxHttpResponse {
            data: serde_json::json!({"state": "active", "href": "/holdings/acme/home"}),
            context: serde_json::json!({"userId": "owner"}),
        }
    }

    struct Success;
    impl ManageErpxOnboardingCommandBackend for Success {
        fn onboarding_command<'a>(
            &'a self,
            request: ManageErpxOnboardingCommandRequest<'a>,
        ) -> ManageErpxHttpFuture<'a, ManageErpxHttpResponse> {
            Box::pin(async move {
                assert_eq!(request.host, "acme.example.test");
                assert_eq!(request.body["companyName"], "Acme Company");
                Ok(response())
            })
        }
    }

    struct Denied;
    impl ManageErpxOnboardingCommandBackend for Denied {
        fn onboarding_command<'a>(
            &'a self,
            _: ManageErpxOnboardingCommandRequest<'a>,
        ) -> ManageErpxHttpFuture<'a, ManageErpxHttpResponse> {
            Box::pin(async {
                Err(BusinessError::new(
                    FailureKind::Conflict,
                    "ONBOARDING_ALREADY_STARTED",
                    "Synthetic conflicting onboarding.",
                ))
            })
        }
    }

    fn state(
        backend: Arc<dyn ManageErpxOnboardingCommandBackend>,
    ) -> Arc<ManageErpxOnboardingCommandState> {
        Arc::new(ManageErpxOnboardingCommandState {
            targets: BTreeMap::from([(
                "acme.example.test".to_owned(),
                ManageErpxOnboardingCommandTarget {
                    host: "acme.example.test".to_owned(),
                    backend,
                },
            )]),
        })
    }

    fn request(body: Body, additional_host: Option<&str>) -> Request {
        let mut request = Request::builder()
            .method("POST")
            .uri("/api/v1/onboarding")
            .header(header::HOST, "acme.example.test")
            .body(body)
            .unwrap();
        if let Some(host) = additional_host {
            request
                .headers_mut()
                .append(header::HOST, HeaderValue::from_str(host).unwrap());
        }
        request
    }

    async fn body(response: Response) -> serde_json::Value {
        let bytes = to_bytes(response.into_body(), 128 * 1024).await.unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }

    fn valid_body() -> Body {
        Body::from(r#"{"companyName":"Acme Company","branchName":"Main","slug":"acme"}"#)
    }

    #[test]
    fn command_router_configuration_requires_distinct_nonempty_registered_hosts() {
        let target = || ManageErpxOnboardingCommandTarget {
            host: "acme.example.test".into(),
            backend: Arc::new(Success),
        };
        assert!(manage_erpx_onboarding_command_router(vec![target()]).is_ok());
        assert!(manage_erpx_onboarding_command_router(vec![target(), target()]).is_err());
        assert!(
            manage_erpx_onboarding_command_router(vec![ManageErpxOnboardingCommandTarget {
                host: String::new(),
                backend: Arc::new(Success),
            }])
            .is_err()
        );
        assert!(manage_erpx_onboarding_command_router(vec![]).is_err());
    }

    #[test]
    fn handler_returns_private_json_and_maps_transport_failures() {
        sqlx::test_block_on(async {
            let response =
                onboarding_command(State(state(Arc::new(Success))), request(valid_body(), None))
                    .await;
            assert_eq!(response.status(), StatusCode::OK);
            assert_eq!(
                response.headers().get(header::CACHE_CONTROL).unwrap(),
                "private, no-store"
            );
            let value = body(response).await;
            assert_eq!(value["data"]["state"], "active");
            assert_eq!(value["data"]["href"], "/holdings/acme/home");
            assert_eq!(value["context"]["userId"], "owner");

            let response = onboarding_command(
                State(state(Arc::new(Success))),
                request(Body::from("{}"), Some("other.example.test")),
            )
            .await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(
                body(response).await["error"]["code"],
                "MANAGE_ERPX_REQUEST_INVALID"
            );

            let response = onboarding_command(
                State(state(Arc::new(Success))),
                request(Body::from("{"), None),
            )
            .await;
            assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(body(response).await["error"]["code"], "INVALID_JSON");

            let response =
                onboarding_command(State(state(Arc::new(Denied))), request(valid_body(), None))
                    .await;
            assert_eq!(response.status(), StatusCode::CONFLICT);
            assert_eq!(
                body(response).await["error"]["code"],
                "ONBOARDING_ALREADY_STARTED"
            );
        });
    }

    #[test]
    fn oversized_bodies_fail_before_the_backend() {
        sqlx::test_block_on(async {
            let response = onboarding_command(
                State(state(Arc::new(Success))),
                request(
                    Body::from("x".repeat(MANAGE_ERPX_COMMAND_BODY_LIMIT + 1)),
                    None,
                ),
            )
            .await;
            assert_eq!(response.status(), StatusCode::PAYLOAD_TOO_LARGE);
            assert_eq!(body(response).await["error"]["code"], "INPUT_TOO_LARGE");
        });
    }

    #[test]
    fn a_missing_host_is_not_found() {
        sqlx::test_block_on(async {
            let mut request = request(valid_body(), None);
            request.headers_mut().remove(header::HOST);
            let response = onboarding_command(State(state(Arc::new(Success))), request).await;
            assert_eq!(response.status(), StatusCode::NOT_FOUND);
        });
    }
}
