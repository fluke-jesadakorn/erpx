//! Owner-local native read port for the Holding invitation activation panel.
//!
//! The write side is owned by `account_invitations_http.rs`. This module owns
//! the read-only GET companion: exact registered Host/Holding target, bounded
//! caller scope headers, one owner repository read inside a caller-owned
//! transaction, and the byte-compatible snake_case projection the panel already
//! consumes. The router has no fallback and never trusts a caller-supplied row.

use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::Serialize;
use sqlx::{Connection, Row};
use std::collections::BTreeMap;
use std::{future::Future, pin::Pin, sync::Arc};

const CONTEXT_HOLDING: HeaderName = HeaderName::from_static("x-erpx-context-holding");
const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");

/// The projection consumed by the activation panel. Field names intentionally
/// match the TypeScript owner contract exactly (snake_case).
#[derive(Clone, Debug, Serialize, PartialEq, Eq)]
pub struct AccountInvitationListItem {
    pub id: String,
    pub person_id: String,
    pub email: String,
    pub status: String,
    pub expires_at: i64,
    pub delivery_state: Option<String>,
    pub attempts: Option<i64>,
    pub error: Option<String>,
    pub sent_at: Option<i64>,
}

/// Company-isolated read scope derived from the selected Holding workspace.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InvitationListScope {
    pub holding_id: String,
    pub company_id: String,
    pub branch_id: Option<String>,
}

pub type AccountInvitationListFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Composition injects the owner reader behind this port. The owner owns the
/// SQL, the transaction and the projection; the router only transports facts.
pub trait AccountInvitationListRead: Send + Sync {
    fn list<'a>(
        &'a self,
        scope: InvitationListScope,
    ) -> AccountInvitationListFuture<'a, Vec<AccountInvitationListItem>>;
}

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "INVITATION_LIST_UNAVAILABLE",
        "The native invitation list service is unavailable.",
    )
}

/// Pg-backed owner reader. The exact owner SQL and its fail-closed transaction
/// checks live here; persistence faults map to an explicit 503.
pub struct PgAccountInvitationListRead {
    control: DatabasePool,
}

impl PgAccountInvitationListRead {
    pub fn new(control: DatabasePool) -> Self {
        Self { control }
    }
}

impl AccountInvitationListRead for PgAccountInvitationListRead {
    fn list<'a>(
        &'a self,
        scope: InvitationListScope,
    ) -> AccountInvitationListFuture<'a, Vec<AccountInvitationListItem>> {
        let unit = PgUnitOfWork::new(self.control.clone());
        Box::pin(async move {
            unit.run::<_, BusinessError, _>(TransactionOptions::default(), move |connection| {
                let scope = scope.clone();
                Box::pin(async move {
                    if !connection.is_in_transaction() {
                        return Err(PgTransactionError::Business(unavailable()));
                    }
                    let rows = sqlx::query(
                        "SELECT i.id,i.person_id,i.email, \
                         CASE WHEN i.status='pending' AND i.expires_at<=(floor(extract(epoch FROM clock_timestamp()))::bigint*1000) \
                           THEN 'expired' ELSE i.status END status, \
                         i.expires_at,d.state delivery_state,d.attempts,d.error,d.sent_at \
                         FROM account_invitations i \
                         LEFT JOIN account_invitation_delivery_jobs d ON d.invitation_id=i.id \
                         WHERE i.holding_id=$1 AND i.company_id=$2 AND ($3::text IS NULL OR i.branch_id=$3) \
                         ORDER BY i.created_at DESC LIMIT 100",
                    )
                    .bind(&scope.holding_id)
                    .bind(&scope.company_id)
                    .bind(&scope.branch_id)
                    .fetch_all(&mut *connection)
                    .await
                    .map_err(|_| unavailable())?;
                    Ok(rows
                        .iter()
                        .map(|row| AccountInvitationListItem {
                            id: row.get("id"),
                            person_id: row.get("person_id"),
                            email: row.get("email"),
                            status: row.get("status"),
                            expires_at: row.get("expires_at"),
                            delivery_state: row.get("delivery_state"),
                            attempts: row.get("attempts"),
                            error: row.get("error"),
                            sent_at: row.get("sent_at"),
                        })
                        .collect())
                })
            })
            .await
            .map_err(|error| match error {
                erpx_server_runtime::public::postgres::PgTransactionError::Business(error) => error,
                erpx_server_runtime::public::postgres::PgTransactionError::Persistence(_) => {
                    unavailable()
                }
            })
        })
    }
}

pub struct AccountInvitationListTarget {
    pub host: String,
    pub holding_slug: String,
}

struct AccountInvitationListState {
    targets: BTreeMap<(String, String), Arc<dyn AccountInvitationListRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "ACCOUNT_INVITATION_LIST_REQUEST_INVALID",
        "Use one bounded invitation list read.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn valid_id(value: &str) -> bool {
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
/// command router; no new SQL or business rule is introduced at transport.
pub fn account_invitation_list_read_router(
    reader: Arc<dyn AccountInvitationListRead>,
    targets: Vec<AccountInvitationListTarget>,
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
                "ACCOUNT_INVITATION_LIST_READ_CONFIGURATION_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if configured.is_empty() {
        return Err(BusinessError::invalid(
            "ACCOUNT_INVITATION_LIST_READ_CONFIGURATION_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/account/invitations",
            get(list),
        )
        .with_state(Arc::new(AccountInvitationListState {
            targets: configured,
        })))
}

async fn list(
    State(state): State<Arc<AccountInvitationListState>>,
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
        let holding_id = unique_header(request.headers(), CONTEXT_HOLDING)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let company_id = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let branch_id = unique_header(request.headers(), CONTEXT_BRANCH)?
            .filter(|value| !value.is_empty() && valid_id(value))
            .map(str::to_owned);
        let invitations = reader
            .list(InvitationListScope {
                holding_id: holding_id.to_owned(),
                company_id: company_id.to_owned(),
                branch_id,
            })
            .await?;
        Ok::<_, BusinessError>(serde_json::json!({"invitations": invitations}))
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
        items: Vec<AccountInvitationListItem>,
        seen: Arc<Mutex<Vec<InvitationListScope>>>,
    }

    impl AccountInvitationListRead for Probe {
        fn list<'a>(
            &'a self,
            scope: InvitationListScope,
        ) -> AccountInvitationListFuture<'a, Vec<AccountInvitationListItem>> {
            let seen = self.seen.clone();
            let items = self.items.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(scope);
                Ok(items)
            })
        }
    }

    fn target() -> AccountInvitationListTarget {
        AccountInvitationListTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        }
    }

    fn item() -> AccountInvitationListItem {
        AccountInvitationListItem {
            id: "inv-1".into(),
            person_id: "person-1".into(),
            email: "ada@example.test".into(),
            status: "pending".into(),
            expires_at: 1_800_000_000_000,
            delivery_state: Some("sent".into()),
            attempts: Some(2),
            error: None,
            sent_at: Some(1_790_000_000_000),
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

    async fn serve(reader: Arc<dyn AccountInvitationListRead>) -> (u16, tokio::task::JoinHandle<()>) {
        let router = account_invitation_list_read_router(reader, vec![target()]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        (addr.port(), server)
    }

    #[test]
    fn configuration_requires_exact_distinct_holdings() {
        let probe = Arc::new(Probe {
            items: Vec::new(),
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        assert!(account_invitation_list_read_router(probe.clone(), vec![target()]).is_ok());
        assert!(account_invitation_list_read_router(probe.clone(), vec![target(), target()]).is_err());
        assert!(account_invitation_list_read_router(probe.clone(), vec![]).is_err());
        assert!(account_invitation_list_read_router(
            probe,
            vec![AccountInvitationListTarget {
                holding_slug: "BAD".into(),
                ..target()
            }]
        )
        .is_err());
    }

    #[tokio::test]
    async fn read_returns_exact_panel_projection_and_scope() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let probe = Arc::new(Probe {
            items: vec![item()],
            seen: seen.clone(),
        });
        let (port, server) = serve(probe).await;
        let response = http(
            format!("127.0.0.1:{port}").parse().unwrap(),
            format!(
                "GET /api/v1/holdings/acme/account/invitations HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-holding: holding-1\r\nx-erpx-context-company: company-1\r\nx-erpx-context-branch: branch-1\r\nConnection: close\r\n\r\n"
            ),
        )
        .await;
        assert_eq!(response.0, 200);
        assert_eq!(
            response.1,
            serde_json::json!({"invitations":[{
                "id":"inv-1","person_id":"person-1","email":"ada@example.test",
                "status":"pending","expires_at":1_800_000_000_000i64,
                "delivery_state":"sent","attempts":2i64,"error":null,"sent_at":1_790_000_000_000i64
            }]})
        );
        assert_eq!(
            seen.lock().unwrap().as_slice(),
            &[InvitationListScope {
                holding_id: "holding-1".into(),
                company_id: "company-1".into(),
                branch_id: Some("branch-1".into()),
            }]
        );
        server.abort();
    }

    #[tokio::test]
    async fn missing_scope_or_unknown_target_is_rejected() {
        let probe = Arc::new(Probe {
            items: Vec::new(),
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let (port, server) = serve(probe).await;
        let invalid = http(
            format!("127.0.0.1:{port}").parse().unwrap(),
            format!(
                "GET /api/v1/holdings/acme/account/invitations HTTP/1.1\r\nHost: acme.example.test\r\nx-erpx-context-holding: holding-1\r\nConnection: close\r\n\r\n"
            ),
        )
        .await;
        assert_eq!(invalid.0, 422);
        let unknown = http(
            format!("127.0.0.1:{port}").parse().unwrap(),
            format!(
                "GET /api/v1/holdings/acme/account/invitations HTTP/1.1\r\nHost: other.example.test\r\nx-erpx-context-holding: holding-1\r\nx-erpx-context-company: company-1\r\nConnection: close\r\n\r\n"
            ),
        )
        .await;
        assert_eq!(unknown.0, 404);
        server.abort();
    }
}
