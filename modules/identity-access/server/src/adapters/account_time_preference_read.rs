//! Owner-local native read port for the Account time preference.
//!
//! The write side is served by `account_time_preference_http.rs`. This module
//! owns the read-only GET companion: exact registered Host/Holding target, a
//! bounded caller identity header, one owner repository read inside a
//! caller-owned transaction, and the byte-compatible camelCase projection. The
//! router has no fallback and never trusts a caller-supplied preference.

use crate::adapters::account_time_preference_postgres::PgTimePreferenceReader;
use crate::domain::account_time_preference::TimePreference;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::{
    DatabasePool, PgUnitOfWork, TransactionOptions,
};
use erpx_platform_shared_server::public::{BusinessError, FailureKind, RecordId};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const CONTEXT_USER: HeaderName = HeaderName::from_static("x-erpx-context-user");

pub type AccountTimePreferenceReadFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the owner reader behind this port. The owner owns the
/// SQL, the transaction and the projection; the router only transports facts.
pub trait AccountTimePreferenceRead: Send + Sync {
    fn current<'a>(
        &'a self,
        user_id: &'a str,
    ) -> AccountTimePreferenceReadFuture<'a, Option<TimePreference>>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "TIME_PREFERENCE_UNAVAILABLE",
        "The native time preference service is unavailable.",
    )
}

/// Pg-backed owner reader. `PgTimePreferenceReader` already owns the exact SQL
/// and its fail-closed transaction checks; this adapter only supplies the
/// Control pool and maps persistence faults to an explicit 503.
pub struct PgAccountTimePreferenceRead {
    control: DatabasePool,
}

impl PgAccountTimePreferenceRead {
    pub fn new(control: DatabasePool) -> Self {
        Self { control }
    }
}

impl AccountTimePreferenceRead for PgAccountTimePreferenceRead {
    fn current<'a>(
        &'a self,
        user_id: &'a str,
    ) -> AccountTimePreferenceReadFuture<'a, Option<TimePreference>> {
        let unit = PgUnitOfWork::new(self.control.clone());
        let user_id = user_id.to_owned();
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let user_id = user_id.clone();
                Box::pin(async move {
                    let preference = PgTimePreferenceReader::new(&mut *connection)
                        .current(&user_id)
                        .await
                        .map_err(|_| unavailable())?;
                    Ok(preference)
                })
            })
            .await
            .map_err(|error| match error {
                erpx_platform_runtime_server::public::postgres::PgTransactionError::Business(error) => error,
                erpx_platform_runtime_server::public::postgres::PgTransactionError::Persistence(_) => {
                    unavailable()
                }
            })
        })
    }
}

pub struct AccountTimePreferenceReadTarget {
    pub host: String,
    pub holding_slug: String,
}

struct AccountTimePreferenceReadState {
    targets: BTreeMap<(String, String), Arc<dyn AccountTimePreferenceRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ACCOUNT_TIME_PREFERENCE_REQUEST_INVALID",
        "Use one bounded account time-preference read.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn valid_user_id(value: &str) -> bool {
    RecordId::new(value).is_ok()
        && !value.is_empty()
        && value.len() <= 200
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_' || b == b'.' || b == b':')
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

/// Build the native read router. The lead mounts it alongside the mounted
/// command router; no new SQL or business rule is introduced here.
pub fn account_time_preference_read_router(
    reader: Arc<dyn AccountTimePreferenceRead>,
    targets: Vec<AccountTimePreferenceReadTarget>,
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
            return Err(BusinessError::invalid(
                "ACCOUNT_TIME_PREFERENCE_READ_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "ACCOUNT_TIME_PREFERENCE_READ_CONFIGURATION_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/account/time-preference",
            get(current),
        )
        .with_state(Arc::new(AccountTimePreferenceReadState {
            targets: configured,
        })))
}

async fn current(
    State(state): State<Arc<AccountTimePreferenceReadState>>,
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
            .ok_or_else(|| BusinessError::new(FailureKind::Unauthenticated, "SIGN_IN_REQUIRED", "Sign in to read your time settings."))?;
        let preference = reader.current(user_id).await?.unwrap_or_default();
        Ok::<_, BusinessError>(serde_json::json!({"preference": preference}))
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
    use std::sync::Mutex;

    struct Probe {
        preference: Option<TimePreference>,
        seen: Arc<Mutex<Vec<String>>>,
    }

    impl AccountTimePreferenceRead for Probe {
        fn current<'a>(
            &'a self,
            user_id: &'a str,
        ) -> AccountTimePreferenceReadFuture<'a, Option<TimePreference>> {
            let seen = self.seen.clone();
            let preference = self.preference.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(user_id.to_owned());
                Ok(preference)
            })
        }
    }

    fn target() -> AccountTimePreferenceReadTarget {
        AccountTimePreferenceReadTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        }
    }

    async fn http(addr: std::net::SocketAddr, raw: String) -> (u16, serde_json::Value) {
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
        let body = text.split("\r\n\r\n").nth(1).unwrap_or("");
        (
            status,
            serde_json::from_str(body).unwrap_or(serde_json::Value::Null),
        )
    }

    async fn serve(
        reader: Arc<dyn AccountTimePreferenceRead>,
    ) -> (u16, tokio::task::JoinHandle<()>) {
        let router = account_time_preference_read_router_with(reader);
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        (addr.port(), server)
    }

    fn account_time_preference_read_router_with(
        reader: Arc<dyn AccountTimePreferenceRead>,
    ) -> Router {
        account_time_preference_read_router(reader, vec![target()]).unwrap()
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let probe = Arc::new(Probe {
            preference: None,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        assert!(account_time_preference_read_router(probe.clone(), vec![target()]).is_ok());
        assert!(account_time_preference_read_router(probe.clone(), vec![target(), target()]).is_err());
        assert!(
            account_time_preference_read_router(probe.clone(), vec![]).is_err()
        );
        assert!(account_time_preference_read_router(
            probe,
            vec![AccountTimePreferenceReadTarget {
                holding_slug: "BAD".into(),
                ..target()
            }]
        )
        .is_err());
    }

    #[tokio::test]
    async fn read_returns_exact_camel_case_projection() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Arc::new(Probe {
            preference: Some(TimePreference {
                mode: "manual".into(),
                time_zone: Some("Asia/Bangkok".into()),
                hour_cycle: "h12".into(),
                version: 4,
            }),
            seen: seen.clone(),
        });
        let (port, server) = serve(probe).await;
        let response = http(
            format!("127.0.0.1:{port}").parse().unwrap(),
            format!(
                "GET /api/v1/holdings/acme/account/time-preference HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-user: user-1\r\nConnection: close\r\n\r\n"
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(
            response.1,
            serde_json::json!({"preference":{"mode":"manual","timeZone":"Asia/Bangkok","hourCycle":"h12","version":4}})
        );
        assert_eq!(seen.lock().unwrap().as_slice(), &["user-1".to_owned()]);
        server.abort();
    }

    #[tokio::test]
    async fn missing_row_defaults_and_unknown_target_is_not_found() {
        let probe = Arc::new(Probe {
            preference: None,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let (port, server) = serve(probe).await;
        let response = http(
            format!("127.0.0.1:{port}").parse().unwrap(),
            format!(
                "GET /api/v1/holdings/acme/account/time-preference HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-user: user-1\r\nConnection: close\r\n\r\n"
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(
            response.1,
            serde_json::json!({"preference":{"mode":"system","timeZone":null,"hourCycle":"h23","version":0}})
        );
        let unknown = http(
            format!("127.0.0.1:{port}").parse().unwrap(),
            format!(
                "GET /api/v1/holdings/acme/account/time-preference HTTP/1.1\r\nHost: other.example.test\r\nx-erpx-context-user: user-1\r\nConnection: close\r\n\r\n"
            ),
        )
        .await;
        assert_eq!(unknown.0, 404);
        server.abort();
    }
}
