//! Axum ingress for the native Account profile read.
//!
//! The owner owns the exact Host/Holding target allow-list, caller identity
//! validation, the `{ data: profile }` envelope and response headers. The
//! application handler owns the read and projection; the Pg adapter owns the
//! only SQL. There is no TypeScript fallback, plan reply or payload overlay.

use crate::application::account_profile::{AccountProfileReader, ProfileQuery, read_profile};
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use crate::domain::account_profile::valid_user_id;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");

pub struct AccountProfileReadTarget {
    pub host: String,
    pub holding_slug: String,
    pub reader: Arc<dyn AccountProfileReader>,
}

struct AccountProfileReadState {
    targets: BTreeMap<(String, String), Arc<dyn AccountProfileReader>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "ACCOUNT_PROFILE_UNAVAILABLE",
        "The native Account profile service is unavailable.",
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
        return Err(BusinessError::invalid(
            "ACCOUNT_PROFILE_REQUEST_INVALID",
            "Use one bounded account profile read.",
        ));
    }
    first
        .map(|value| value.to_str().map_err(|_| BusinessError::invalid(
            "ACCOUNT_PROFILE_REQUEST_INVALID",
            "Use one bounded account profile read.",
        )))
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

/// Exact response headers used by the retired TypeScript route.
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

/// Lead mounts the returned Router in the private service. Targets must be
/// exact, distinct registered Holding Hosts; an empty allow-list is rejected.
pub fn account_profile_read_router(
    targets: Vec<AccountProfileReadTarget>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for target in targets {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    target.reader,
                )
                .is_some()
        {
            return Err(BusinessError::invalid(
                "ACCOUNT_PROFILE_READ_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "ACCOUNT_PROFILE_READ_CONFIGURATION_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/account/profile",
            get(current),
        )
        .with_state(Arc::new(AccountProfileReadState {
            targets: configured,
        })))
}

async fn current(
    State(state): State<Arc<AccountProfileReadState>>,
    Path(slug): Path<String>,
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
        let user_id = unique_header(request.headers(), CONTEXT_USER)?
            .filter(|value| valid_user_id(value))
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::Unauthenticated,
                    "SIGN_IN_REQUIRED",
                    "Sign in to read your account.",
                )
            })?;
        let profile = read_profile(
            reader.as_ref(),
            ProfileQuery {
                user_id: user_id.to_owned(),
            },
        )
        .await
        .map_err(|_| unavailable())?
        .ok_or_else(|| {
            BusinessError::new(
                FailureKind::Unauthenticated,
                "SIGN_IN_REQUIRED",
                "Sign in again to read your account.",
            )
        })?;
        Ok::<_, BusinessError>(serde_json::json!({"data": profile}))
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
    use crate::application::account_profile::AccountProfileFuture;
    use crate::domain::account_profile::ProfileRecord;
    use std::sync::Mutex;

    struct Probe {
        record: Option<ProfileRecord>,
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl AccountProfileReader for Probe {
        fn read<'a>(&'a self, user_id: &'a str) -> AccountProfileFuture<'a, Option<ProfileRecord>> {
            let seen = self.seen.clone();
            let record = self.record.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(user_id.to_owned());
                Ok(record)
            })
        }
    }

    fn target(reader: Arc<dyn AccountProfileReader>) -> AccountProfileReadTarget {
        AccountProfileReadTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            reader,
        }
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, String, serde_json::Value) {
        use tokio::io::{AsyncReadExt, AsyncWriteExt};
        let mut stream = tokio::net::TcpStream::connect(addr).await.unwrap();
        stream.write_all(raw.as_bytes()).await.unwrap();
        let mut bytes = Vec::new();
        stream.read_to_end(&mut bytes).await.unwrap();
        let text = String::from_utf8_lossy(&bytes);
        let status = text
            .lines()
            .next()
            .and_then(|line| line.split_whitespace().nth(1))
            .and_then(|value| value.parse().ok())
            .unwrap();
        let (head, body) = text.split_once("\r\n\r\n").unwrap_or((text.as_ref(), ""));
        (
            status,
            head.to_owned(),
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        )
    }

    async fn serve(
        reader: Arc<dyn AccountProfileReader>,
    ) -> (u16, tokio::task::JoinHandle<()>) {
        let router = account_profile_read_router(vec![target(reader)]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        (addr.port(), server)
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let probe: Arc<dyn AccountProfileReader> = Arc::new(Probe {
            record: None,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        assert!(account_profile_read_router(vec![target(probe.clone())]).is_ok());
        assert!(account_profile_read_router(vec![target(probe.clone()), target(probe.clone())]).is_err());
        assert!(account_profile_read_router(vec![]).is_err());
        let bad = AccountProfileReadTarget {
            holding_slug: "BAD".into(),
            ..target(probe)
        };
        assert!(account_profile_read_router(vec![bad]).is_err());
    }

    #[tokio::test]
    async fn returns_exact_owner_payload_and_headers() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Arc::new(Probe {
            record: Some(ProfileRecord {
                name: "Ada".into(),
                email: "ada@example.test".into(),
                language: Some("th".into()),
                appearance: Some("dark".into()),
                version: Some(7),
            }),
            seen: seen.clone(),
        });
        let (port, server) = serve(probe).await;
        let response = http(
            format!("127.0.0.1:{port}").parse().unwrap(),
            format!(
                "GET /api/v1/holdings/acme/account/profile HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-user: user-1\r\nConnection: close\r\n\r\n"
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(
            response.2,
            serde_json::json!({"data":{"name":"Ada","email":"ada@example.test","preferences":{"language":"th","appearance":"dark","version":7}}})
        );
        assert!(response.1.to_ascii_lowercase().contains("cache-control: private, no-store"));
        assert!(response.1.to_ascii_lowercase().contains("referrer-policy: no-referrer"));
        assert!(response.1.to_ascii_lowercase().contains("x-content-type-options: nosniff"));
        assert_eq!(seen.lock().unwrap().as_slice(), &["user-1".to_owned()]);
        server.abort();
    }

    #[tokio::test]
    async fn missing_identity_and_unknown_target_fail_closed() {
        let probe: Arc<dyn AccountProfileReader> = Arc::new(Probe {
            record: None,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let (port, server) = serve(probe).await;
        let missing_user = http(
            format!("127.0.0.1:{port}").parse().unwrap(),
            format!(
                "GET /api/v1/holdings/acme/account/profile HTTP/1.1\r\nHost: acme.example.test\r\nConnection: close\r\n\r\n"
            ),
        )
        .await;
        assert_eq!(missing_user.0, 401);
        let unknown = http(
            format!("127.0.0.1:{port}").parse().unwrap(),
            format!(
                "GET /api/v1/holdings/acme/account/profile HTTP/1.1\r\nHost: other.example.test\r\nx-erpx-context-user: user-1\r\nConnection: close\r\n\r\n"
            ),
        )
        .await;
        assert_eq!(unknown.0, 404);
        server.abort();
    }
}
