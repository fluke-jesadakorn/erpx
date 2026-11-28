//! Owner-local axum ingress for the bounded Legal portal slice.
//!
//! Only the list read and the quick-access read/write are mounted. Detail,
//! download, upload and command need session/guest authority, file staging and
//! multi-owner transactional witnesses that are not yet available natively.
//! The verified actor and Holding binding arrive from composition; this router
//! never resolves a cookie, mints a session or falls back to TypeScript.

use std::{collections::BTreeMap, sync::Arc};

use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, get, header, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};

use super::application::{
    PortalAuthority, PortalPreferences, PortalReadRequest, PortalSaveRequest, PortalSource,
    portal_list, portal_quick_access_get, portal_quick_access_post,
};
use super::domain::{
    BODY_LIMIT, QuickAccessPreferenceWire, PortalShareWire, valid_predicate, valid_slug,
};

const KEY_HEADER: HeaderName = HeaderName::from_static("idempotency-key");

/// One registered portal target. `origin` is the exact public origin checked on
/// POST; `context_key` is the Holding binding id used by the preference owner.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PortalTarget {
    pub host: String,
    pub origin: String,
    pub holding_slug: String,
    pub context_key: String,
}

struct Entry {
    origin: String,
}

struct StateData {
    targets: BTreeMap<(String, String), Entry>,
    predicate: Arc<str>,
    authority: Arc<dyn PortalAuthority>,
    source: Arc<dyn PortalSource>,
    preferences: Arc<dyn PortalPreferences>,
}

fn invalid(code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::invalid(code, message)
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

fn key_required() -> BusinessError {
    invalid("COMMAND_KEY_REQUIRED", "A command key is required.")
}

fn too_large() -> BusinessError {
    invalid("INPUT_TOO_LARGE", "Shorten the content before saving.")
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

fn private_json(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
    response
        .headers_mut()
        .insert(header::CACHE_CONTROL, HeaderValue::from_static("private, no-store"));
    response
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(missing());
    }
    first.map(|value| value.to_str().map_err(|_| missing())).transpose()
}

fn target<'a>(
    state: &'a StateData,
    headers: &HeaderMap,
    slug: &str,
) -> Result<(String, &'a Entry), BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    let entry = state.targets.get(&(host.to_owned(), slug.to_owned())).ok_or_else(missing)?;
    Ok((host.to_owned(), entry))
}

async fn list(
    State(state): State<Arc<StateData>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result: Result<Vec<PortalShareWire>, BusinessError> = async {
        let (host, _) = target(&state, &headers, &slug)?;
        portal_list(
            state.authority.as_ref(),
            state.source.as_ref(),
            state.predicate.as_ref(),
            PortalReadRequest {
                host: &host,
                holding_slug: &slug,
                headers: &headers,
            },
        )
        .await
    }
    .await;
    match result {
        Ok(rows) => private_json(serde_json::json!({"data": rows})),
        Err(error) => error_response(error),
    }
}

async fn quick_get(
    State(state): State<Arc<StateData>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result: Result<QuickAccessPreferenceWire, BusinessError> = async {
        let (host, _) = target(&state, &headers, &slug)?;
        portal_quick_access_get(
            state.authority.as_ref(),
            state.preferences.as_ref(),
            PortalReadRequest {
                host: &host,
                holding_slug: &slug,
                headers: &headers,
            },
        )
        .await
    }
    .await;
    match result {
        Ok(preference) => private_json(serde_json::json!({"data": preference})),
        Err(error) => error_response(error),
    }
}

async fn quick_post(
    State(state): State<Arc<StateData>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result: Result<QuickAccessPreferenceWire, BusinessError> = async {
        let (host, entry) = target(&state, &headers, &slug)?;
        if unique_header(&headers, header::ORIGIN)? != Some(entry.origin.as_str()) {
            return Err(origin_forbidden());
        }
        let key = unique_header(&headers, KEY_HEADER.clone())?
            .filter(|value| !value.is_empty())
            .ok_or_else(key_required)?;
        let bytes = to_bytes(request.into_body(), BODY_LIMIT + 1)
            .await
            .map_err(|_| too_large())?;
        portal_quick_access_post(
            state.authority.as_ref(),
            state.source.as_ref(),
            state.preferences.as_ref(),
            state.predicate.as_ref(),
            PortalSaveRequest {
                host: &host,
                holding_slug: &slug,
                headers: &headers,
                body: &bytes,
                key,
            },
        )
        .await
    }
    .await;
    match result {
        Ok(preference) => private_json(serde_json::json!({"data": preference})),
        Err(error) => error_response(error),
    }
}

/// Build the bounded native router. Composition supplies verified authority,
/// the share reader and the existing preference owner; duplicate or malformed
/// targets are rejected before any route is installed.
pub fn router(
    targets: Vec<PortalTarget>,
    predicate: impl AsRef<str>,
    authority: Arc<dyn PortalAuthority>,
    source: Arc<dyn PortalSource>,
    preferences: Arc<dyn PortalPreferences>,
) -> Result<Router, BusinessError> {
    if !valid_predicate(predicate.as_ref()) {
        return Err(invalid(
            "LEGAL_PORTAL_CONFIG_INVALID",
            "Use the verified unenforced purchase predicate.",
        ));
    }
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || target.origin.is_empty()
            || target.context_key.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host, target.holding_slug), Entry { origin: target.origin })
                .is_some()
        {
            return Err(invalid(
                "LEGAL_PORTAL_CONFIG_INVALID",
                "Use exact distinct registered portal targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(invalid(
            "LEGAL_PORTAL_CONFIG_INVALID",
            "Use exact distinct registered portal targets.",
        ));
    }
    Ok(Router::new()
        .route("/api/legal-portal/{holding_slug}", get(list))
        .route(
            "/api/legal-portal/{holding_slug}/quick-access",
            get(quick_get).post(quick_post),
        )
        .with_state(Arc::new(StateData {
            targets: configured,
            predicate: Arc::from(predicate.as_ref()),
            authority,
            source,
            preferences,
        })))
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::application::{
        PortalActor, PortalFuture, PortalSaveInput, PortalVerified, PortalVerifyRequest,
    };
    use erpx_platform_runtime_server::public::http_support::body::Body;
    use std::sync::Mutex;

    struct Authority;
    impl PortalAuthority for Authority {
        fn verify<'a>(
            &'a self,
            _: PortalVerifyRequest<'a>,
        ) -> PortalFuture<'a, PortalVerified> {
            Box::pin(async move {
                Ok(PortalVerified {
                    actor: PortalActor {
                        user_id: "u1".into(),
                        email: "reader@example.test".into(),
                        session_id: "s1".into(),
                    },
                    context_key: "holding-1".into(),
                })
            })
        }
    }

    struct Source;
    impl PortalSource for Source {
        fn shares<'a>(
            &'a self,
            _: &'a PortalActor,
            _: &'a str,
        ) -> PortalFuture<'a, Vec<PortalShareWire>> {
            Box::pin(async move {
                Ok(vec![PortalShareWire {
                    id: "s1".into(),
                    document_id: "d1".into(),
                    expires_at: 9,
                    user_id: None,
                    name: "Name".into(),
                    version: 1,
                    checksum: "sum".into(),
                }])
            })
        }
    }

    #[derive(Default)]
    struct Preferences {
        saved: Mutex<Vec<PortalSaveInput>>,
    }
    impl PortalPreferences for Preferences {
        fn read<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
        ) -> PortalFuture<'a, QuickAccessPreferenceWire> {
            Box::pin(async move {
                Ok(QuickAccessPreferenceWire {
                    pinned_ids: None,
                    version: 2,
                })
            })
        }
        fn save<'a>(
            &'a self,
            input: &'a PortalSaveInput,
        ) -> PortalFuture<'a, QuickAccessPreferenceWire> {
            self.saved.lock().unwrap().push(input.clone());
            let version = input.expected_version + 1;
            Box::pin(async move {
                Ok(QuickAccessPreferenceWire {
                    pinned_ids: input.pinned_ids.clone(),
                    version,
                })
            })
        }
    }

    fn state(preferences: Arc<dyn PortalPreferences>) -> Arc<StateData> {
        Arc::new(StateData {
            targets: BTreeMap::from([(
                ("legal.example.test".to_owned(), "acme".to_owned()),
                Entry {
                    origin: "https://legal.example.test".to_owned(),
                },
            )]),
            predicate: Arc::from("TRUE"),
            authority: Arc::new(Authority),
            source: Arc::new(Source),
            preferences,
        })
    }

    fn request(method: &str, origin: Option<&str>, key: Option<&str>, body: &'static [u8]) -> Request {
        let mut builder = Request::builder().method(method).header(header::HOST, "legal.example.test");
        if let Some(origin) = origin {
            builder = builder.header(header::ORIGIN, origin);
        }
        if let Some(key) = key {
            builder = builder.header(KEY_HEADER.clone(), key);
        }
        builder.body(Body::from(body.to_vec())).expect("request")
    }

    #[test]
    fn router_configuration_is_fail_closed() {
        let pref: Arc<dyn PortalPreferences> = Arc::new(Preferences::default());
        let target = PortalTarget {
            host: "legal.example.test".into(),
            origin: "https://legal.example.test".into(),
            holding_slug: "acme".into(),
            context_key: "holding-1".into(),
        };
        assert!(router(
            vec![target.clone()],
            "TRUE",
            Arc::new(Authority),
            Arc::new(Source),
            pref.clone()
        )
        .is_ok());
        for (targets, predicate) in [
            (vec![], "TRUE"),
            (vec![target.clone()], "TRUE OR 1=1"),
            (vec![target.clone(), target.clone()], "TRUE"),
            (
                vec![PortalTarget {
                    holding_slug: "Acme".into(),
                    ..target
                }],
                "TRUE",
            ),
        ] {
            assert_eq!(
                router(targets, predicate, Arc::new(Authority), Arc::new(Source), pref.clone())
                    .unwrap_err()
                    .code,
                "LEGAL_PORTAL_CONFIG_INVALID"
            );
        }
    }

    #[tokio::test]
    async fn list_emits_the_exact_envelope_and_private_headers() {
        let response = list(
            State(state(Arc::new(Preferences::default()))),
            Path("acme".to_owned()),
            request("GET", None, None, b""),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            response.headers().get(header::CACHE_CONTROL).unwrap(),
            "private, no-store"
        );
        let body = to_bytes(response.into_body(), 4096).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"data":[{"id":"s1","document_id":"d1","expires_at":9,"user_id":null,"name":"Name","version":1,"checksum":"sum"}]})
        );
    }

    #[tokio::test]
    async fn quick_access_read_and_write_use_the_retired_shape_and_guards() {
        let preferences = Arc::new(Preferences::default());
        let dynamic: Arc<dyn PortalPreferences> = preferences.clone();
        let shared = state(dynamic);
        let read = quick_get(
            State(shared.clone()),
            Path("acme".to_owned()),
            request("GET", None, None, b""),
        )
        .await;
        assert_eq!(read.status(), StatusCode::OK);
        let body = to_bytes(read.into_body(), 4096).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"data":{"pinnedIds":null,"version":2}})
        );

        let saved = quick_post(
            State(shared.clone()),
            Path("acme".to_owned()),
            request(
                "POST",
                Some("https://legal.example.test"),
                Some("key-1"),
                br#"{"pinnedIds":["share:s1"],"expectedVersion":3}"#,
            ),
        )
        .await;
        assert_eq!(saved.status(), StatusCode::OK);
        let body = to_bytes(saved.into_body(), 4096).await.unwrap();
        assert_eq!(
            serde_json::from_slice::<serde_json::Value>(&body).unwrap(),
            serde_json::json!({"data":{"pinnedIds":["share:s1"],"version":4}})
        );
        assert_eq!(preferences.saved.lock().unwrap().len(), 1);

        for (origin, key, expected) in [
            (Some("https://evil.example.test"), Some("key-1"), StatusCode::FORBIDDEN),
            (Some("https://legal.example.test"), None, StatusCode::UNPROCESSABLE_ENTITY),
        ] {
            let response = quick_post(
                State(shared.clone()),
                Path("acme".to_owned()),
                request("POST", origin, key, br#"{"pinnedIds":null,"expectedVersion":0}"#),
            )
            .await;
            assert_eq!(response.status(), expected);
        }
        let unregistered = quick_get(
            State(shared),
            Path("other".to_owned()),
            request("GET", None, None, b""),
        )
        .await;
        assert_eq!(unregistered.status(), StatusCode::NOT_FOUND);
    }
}
