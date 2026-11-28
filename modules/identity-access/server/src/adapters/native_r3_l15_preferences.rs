//! Additive native read parity for `GET /api/v1/account/preferences`.
//!
//! Retained Next route: `platform/workplace/src/routes/api/v1/account/preferences/route.ts`.
//! This module only adds an owner-owned router; it never mounts itself, deletes
//! the Next route, or executes SQL in TypeScript. The existing owner SQL lives
//! in `account_profile_postgres.rs` (`PgAccountProfileReader`) and is reused
//! through the `AccountProfileReader` port with no second query.
//!
//! Only the GET read is ported. PATCH remains a TypeScript command and is an
//! explicit blocker for deleting the public route (see the lane report).

use crate::{
    application::account_profile::{AccountProfileReader, ProfileQuery, read_profile},
    domain::account_profile::valid_user_id,
};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Query, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use hex::encode;
use sha2::{Digest, Sha256};
use std::{collections::{BTreeMap, HashMap}, future::Future, pin::Pin, sync::Arc};

const KEY_HEADER: &str = "accountKey";

pub type PreferencesSessionFuture<'a> =
    Pin<Box<dyn Future<Output = Result<Option<String>, BusinessError>> + Send + 'a>>;

/// Composition resolves the installed Better Auth session from the request
/// cookie and returns only the bounded user id. The owner never reads cookies
/// or mints sessions; this port keeps the authority with composition.
pub trait PreferencesSession: Send + Sync {
    fn current_user<'a>(&'a self, headers: &'a HeaderMap) -> PreferencesSessionFuture<'a>;
}

/// Exact registered public host -> origin target. Composition owns the host
/// allow-list; an empty or duplicate allow-list is rejected at construction.
pub struct PreferencesTarget {
    pub host: String,
    pub origin: String,
    pub session: Arc<dyn PreferencesSession>,
    pub reader: Arc<dyn AccountProfileReader>,
}

struct PreferencesState {
    targets: BTreeMap<String, PreferencesTarget>,
}

fn not_found() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn sign_in() -> BusinessError {
    BusinessError::new(
        FailureKind::Unauthenticated,
        "SIGN_IN_REQUIRED",
        "Sign in to save your preferences.",
    )
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

/// Byte-identical to `accountPreferenceKey` in the retained owner:
/// `createHash('sha256').update('account-profile:').update(userId).digest('hex')`.
pub fn account_preference_key(user_id: &str) -> String {
    let mut hash = Sha256::new();
    hash.update(b"account-profile:");
    hash.update(user_id.as_bytes());
    encode(hash.finalize())
}

fn with_headers(mut response: Response) -> Response {
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

fn error_response(error: BusinessError) -> Response {
    with_headers(
        (
            status(error.kind),
            Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
        )
            .into_response(),
    )
}

/// Serde structs preserve the retained TypeScript insertion order; `json!`
/// would sort the inner preference keys and break byte parity.
#[derive(serde::Serialize)]
struct PreferenceProjection {
    language: String,
    appearance: String,
    version: i64,
}

#[derive(serde::Serialize)]
struct PreferenceData {
    key: String,
    preference: PreferenceProjection,
}

#[derive(serde::Serialize)]
struct PreferencesEnvelope {
    data: PreferenceData,
}

fn unique_header(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(not_found());
    }
    first
        .map(|value| value.to_str().map_err(|_| not_found()))
        .transpose()
}

/// Owner-owned router constructor for the exact public path. Composition
/// mounts it once the retained Next route is retired; this module adds nothing
/// to any live router by itself.
pub fn native_l15_preferences_router(
    targets: Vec<PreferencesTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !target.origin.starts_with("https://")
            || configured.insert(target.host.clone(), target).is_some()
        {
            return Err(BusinessError::invalid(
                "ACCOUNT_PREFERENCES_ROUTER_CONFIGURATION_INVALID",
                "Use exact distinct registered public hosts and origins.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "ACCOUNT_PREFERENCES_ROUTER_CONFIGURATION_INVALID",
            "Use exact distinct registered public hosts and origins.",
        ));
    }
    Ok(Router::new()
        .route("/api/v1/account/preferences", get(current))
        .with_state(Arc::new(PreferencesState {
            targets: configured,
        })))
}

async fn current(
    State(state): State<Arc<PreferencesState>>,
    Query(query): Query<HashMap<String, String>>,
    request: Request,
) -> Response {
    let result = async move {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(not_found)?;
        let target = state.targets.get(host).ok_or_else(not_found)?;
        let origin = unique_header(request.headers(), header::ORIGIN)?
            .map(str::to_owned)
            .unwrap_or_else(|| format!("https://{host}"));
        if origin != target.origin || {
            let parsed = url::Url::parse(&origin).map_err(|_| not_found())?;
            parsed.host_str() != Some(host)
        } {
            return Err(not_found());
        }

        let headers = request.headers().clone();
        let user_id = target
            .session
            .current_user(&headers)
            .await?
            .filter(|value| valid_user_id(value))
            .ok_or_else(sign_in)?;
        let key = account_preference_key(&user_id);
        if query.get(KEY_HEADER).is_some_and(|value| value != &key) {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "ACCOUNT_CONTEXT_CHANGED",
                "Your account changed. Reload before editing these settings.",
            ));
        }

        let profile = read_profile(
            target.reader.as_ref(),
            ProfileQuery {
                user_id: user_id.clone(),
            },
        )
        .await?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::Unauthenticated,
                "SIGN_IN_REQUIRED",
                "Sign in again to read your preferences.",
            )
        })?;
        Ok::<_, BusinessError>(PreferencesEnvelope {
            data: PreferenceData {
                key,
                preference: PreferenceProjection {
                    language: profile.preferences.language,
                    appearance: profile.preferences.appearance,
                    version: profile.preferences.version,
                },
            },
        })
    }
    .await;
    match result {
        Ok(body) => with_headers((StatusCode::OK, Json(body)).into_response()),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{application::account_profile::AccountProfileFuture, domain::account_profile::ProfileRecord};
    use axum::{body::{Body, to_bytes}, http::Request as HttpRequest};
    use std::sync::Mutex;
    use tower::ServiceExt;

    #[derive(Clone)]
    struct Session(Option<String>);
    impl PreferencesSession for Session {
        fn current_user<'a>(&'a self, _: &'a HeaderMap) -> PreferencesSessionFuture<'a> {
            Box::pin(std::future::ready(Ok(self.0.clone())))
        }
    }

    struct Reader {
        record: Option<ProfileRecord>,
        seen: Arc<Mutex<Vec<String>>>,
    }
    impl AccountProfileReader for Reader {
        fn read<'a>(&'a self, user_id: &'a str) -> AccountProfileFuture<'a, Option<ProfileRecord>> {
            self.seen.lock().unwrap().push(user_id.to_owned());
            Box::pin(std::future::ready(Ok(self.record.clone())))
        }
    }

    fn target(session: Session, reader: Arc<dyn AccountProfileReader>) -> PreferencesTarget {
        PreferencesTarget {
            host: "acme.example.test".into(),
            origin: "https://acme.example.test".into(),
            session: Arc::new(session),
            reader,
        }
    }

    fn record() -> ProfileRecord {
        ProfileRecord {
            name: "Ada".into(),
            email: "ada@example.test".into(),
            language: Some("th".into()),
            appearance: Some("dark".into()),
            version: Some(7),
        }
    }

    fn router(
        session: Session,
        reader: Arc<dyn AccountProfileReader>,
    ) -> Router {
        native_l15_preferences_router(vec![target(session, reader)]).unwrap()
    }

    async fn get(router: Router, uri: &str, host: &str, origin: Option<&str>) -> Response {
        let mut builder = HttpRequest::builder().uri(uri).header(header::HOST, host);
        if let Some(origin) = origin {
            builder = builder.header(header::ORIGIN, origin);
        }
        router.oneshot(builder.body(Body::empty()).unwrap()).await.unwrap()
    }

    #[test]
    fn key_is_byte_identical_to_the_owner_hash() {
        assert_eq!(
            account_preference_key("user-1"),
            "113fd15ac4a7a2ea23d256f170ad6445c75f6f62304f11db519faddb20f54cb9"
        );
    }

    #[test]
    fn router_rejects_empty_and_duplicate_targets() {
        let reader: Arc<dyn AccountProfileReader> = Arc::new(Reader {
            record: None,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        assert!(native_l15_preferences_router(vec![]).is_err());
        let t = target(Session(Some("user-1".into())), reader.clone());
        assert!(native_l15_preferences_router(vec![t]).is_ok());
        assert!(native_l15_preferences_router(vec![
            target(Session(Some("user-1".into())), reader.clone()),
            target(Session(Some("user-1".into())), reader),
        ])
        .is_err());
    }

    #[tokio::test]
    async fn read_returns_the_exact_public_envelope_and_headers() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader = Arc::new(Reader { record: Some(record()), seen: seen.clone() });
        let response = get(
            router(Session(Some("user-1".into())), reader),
            "/api/v1/account/preferences",
            "acme.example.test",
            Some("https://acme.example.test"),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-store");
        assert_eq!(response.headers()[header::REFERRER_POLICY], "no-referrer");
        assert_eq!(response.headers()[header::X_CONTENT_TYPE_OPTIONS], "nosniff");
        let body = to_bytes(response.into_body(), 8192).await.unwrap();
        assert_eq!(
            &body[..],
            br#"{"data":{"key":"113fd15ac4a7a2ea23d256f170ad6445c75f6f62304f11db519faddb20f54cb9","preference":{"language":"th","appearance":"dark","version":7}}}"#
        );
        assert_eq!(seen.lock().unwrap().as_slice(), &["user-1".to_owned()]);
    }

    #[tokio::test]
    async fn missing_preference_row_uses_owner_defaults() {
        let reader = Arc::new(Reader {
            record: Some(ProfileRecord {
                language: None,
                appearance: None,
                version: None,
                ..record()
            }),
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let response = get(
            router(Session(Some("user-1".into())), reader),
            "/api/v1/account/preferences",
            "acme.example.test",
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        let body = to_bytes(response.into_body(), 8192).await.unwrap();
        let value: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            value["data"]["preference"],
            serde_json::json!({"language":"en","appearance":"system","version":0})
        );
    }

    #[tokio::test]
    async fn absent_user_fails_closed() {
        let reader = Arc::new(Reader { record: None, seen: Arc::new(Mutex::new(Vec::new())) });
        let response = get(
            router(Session(Some("user-1".into())), reader),
            "/api/v1/account/preferences",
            "acme.example.test",
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn host_origin_session_and_key_guard_fail_closed() {
        let reader: Arc<dyn AccountProfileReader> = Arc::new(Reader {
            record: Some(record()),
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let app = router(Session(Some("user-1".into())), reader.clone());
        assert_eq!(
            get(app.clone(), "/api/v1/account/preferences", "other.example.test", None)
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            get(app.clone(), "/api/v1/account/preferences", "acme.example.test", Some("https://evil.example.test"))
                .await
                .status(),
            StatusCode::NOT_FOUND
        );
        let anonymous = router(Session(None), reader.clone());
        let response = get(
            anonymous,
            "/api/v1/account/preferences",
            "acme.example.test",
            None,
        )
        .await;
        assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
        assert_eq!(response.headers()[header::CACHE_CONTROL], "private, no-store");
        let stale = get(
            app,
            "/api/v1/account/preferences?accountKey=stale",
            "acme.example.test",
            None,
        )
        .await;
        assert_eq!(stale.status(), StatusCode::CONFLICT);
        let body = to_bytes(stale.into_body(), 8192).await.unwrap();
        assert_eq!(body[..], *br#"{"error":{"code":"ACCOUNT_CONTEXT_CHANGED","message":"Your account changed. Reload before editing these settings."}}"#);
    }
}
