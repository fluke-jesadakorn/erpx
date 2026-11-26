//! Owner-local axum ingress for the active pay-group options read. Host/Holding
//! targets, Company scope, bounded request shape, payload order and error
//! mapping are owned here; PostgreSQL execution lives in `postgres`. There is
//! no TypeScript SQL or business fallback for this operation.

use super::application::PayGroupOptionsRead;
use super::domain::{PayGroupOption, invalid, valid_id};
use super::postgres::PgPayGroupOptionsRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// One exact registered browser origin/Holding pair. Duplicates or loose
/// patterns are rejected so one Holding cannot read another Holding's groups.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PayGroupOptionsTarget {
    pub host: String,
    pub holding_slug: String,
}

struct OptionsState {
    targets: BTreeMap<(String, String), Arc<dyn PayGroupOptionsRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_PGO_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
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

/// Row DTO preserves the retired TypeScript field declaration order instead of
/// a sorted JSON map, so `id,name,currency,frequency` stay in that order.
#[derive(serde::Serialize)]
struct GroupRow {
    id: String,
    name: String,
    currency: String,
    frequency: String,
}

#[derive(serde::Serialize)]
struct OptionsResponse {
    groups: Vec<GroupRow>,
}

fn bounded(body: impl serde::Serialize) -> Response {
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

fn configure_readers(
    entries: impl IntoIterator<Item = (PayGroupOptionsTarget, Arc<dyn PayGroupOptionsRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn PayGroupOptionsRead>>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(configured)
}

fn finish(
    configured: BTreeMap<(String, String), Arc<dyn PayGroupOptionsRead>>,
) -> Result<Router, BusinessError> {
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people-payroll/pay-group-options",
            get(options),
        )
        .with_state(Arc::new(OptionsState { targets: configured })))
}

/// Native router with one Pg owner reader per registered Holding pool. The lead
/// mounts this on the private native People Payroll axum service.
pub fn router(
    targets: Vec<(PayGroupOptionsTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    finish(configure_readers(targets.into_iter().map(|(target, pool)| {
        (
            target,
            Arc::new(PgPayGroupOptionsRead::new(pool)) as Arc<dyn PayGroupOptionsRead>,
        )
    }))?)
}

/// Reader-injection path used by owner-local contract tests.
pub fn router_with_reader(
    targets: Vec<(PayGroupOptionsTarget, Arc<dyn PayGroupOptionsRead>)>,
) -> Result<Router, BusinessError> {
    finish(configure_readers(targets)?)
}

async fn options(
    State(state): State<Arc<OptionsState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let headers = request.headers().clone();
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let rows = reader.options(company).await?;
        Ok::<_, BusinessError>(OptionsResponse {
            groups: rows
                .into_iter()
                .map(|row: PayGroupOption| GroupRow {
                    id: row.id,
                    name: row.name,
                    currency: row.currency,
                    frequency: row.frequency,
                })
                .collect(),
        })
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
    use super::super::application::OptionsFuture;
    use super::super::domain::unavailable;
    use erpx_server_runtime::public::http_support::serde_json::json;

    struct Probe {
        rows: Vec<PayGroupOption>,
        fail: bool,
        seen: Arc<std::sync::Mutex<Vec<String>>>,
    }

    impl PayGroupOptionsRead for Probe {
        fn options<'a>(&'a self, company: &'a str) -> OptionsFuture<'a, Vec<PayGroupOption>> {
            let rows = self.rows.clone();
            let fail = self.fail;
            let seen = self.seen.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(company.to_owned());
                if fail {
                    return Err(unavailable());
                }
                Ok(rows)
            })
        }
    }

    fn target(host: &str, slug: &str) -> PayGroupOptionsTarget {
        PayGroupOptionsTarget {
            host: host.into(),
            holding_slug: slug.into(),
        }
    }

    fn rows() -> Vec<PayGroupOption> {
        vec![PayGroupOption {
            id: "pg-1".into(),
            name: "Monthly".into(),
            currency: "THB".into(),
            frequency: "monthly".into(),
        }]
    }

    fn group_rows() -> Vec<GroupRow> {
        vec![GroupRow {
            id: "pg-1".into(),
            name: "Monthly".into(),
            currency: "THB".into(),
            frequency: "monthly".into(),
        }]
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        let reader: Arc<dyn PayGroupOptionsRead> = Arc::new(Probe {
            rows: vec![],
            fail: false,
            seen: Arc::new(std::sync::Mutex::new(Vec::new())),
        });
        assert!(router_with_reader(vec![(target("acme.test", "acme"), reader.clone())]).is_ok());
        assert!(router_with_reader(vec![
            (target("acme.test", "acme"), reader.clone()),
            (target("acme.test", "acme"), reader.clone()),
        ])
        .is_err());
        assert!(router_with_reader(vec![]).is_err());
        assert!(router_with_reader(vec![(target("acme.test", "Acme"), reader)]).is_err());
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

    /// Pinned retired TypeScript output for the exact fixture below. Keep in
    /// sync with `tests/unit/people_payroll/native-pay-group-options.test.ts`.
    #[test]
    fn payload_matches_the_retired_typescript_output() {
        use sha2::Digest;
        let body = serde_json::to_string(&OptionsResponse { groups: group_rows() }).unwrap();
        assert_eq!(
            body,
            r#"{"groups":[{"id":"pg-1","name":"Monthly","currency":"THB","frequency":"monthly"}]}"#
        );
        let hex = sha2::Sha256::digest(body.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "dce62e6f1ebac21b311cd31edd02c69b8e6f2c8e62d0d7104718a378e692a29d",
            "update this oracle in Rust and in tests/unit/people_payroll/native-pay-group-options.test.ts"
        );
    }

    #[tokio::test]
    async fn router_scopes_company_and_maps_errors() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let reader: Arc<dyn PayGroupOptionsRead> = Arc::new(Probe {
            rows: rows(),
            fail: false,
            seen: seen.clone(),
        });
        let router = router_with_reader(vec![(target("acme.test", "acme"), reader)]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let get = |host: &str, slug: &str, company: &str, query: &str| {
            format!(
                "GET /api/v1/holdings/{slug}/people-payroll/pay-group-options{query} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nx-erpx-context-company: {company}\r\n\r\n"
            )
        };
        let ok = http(addr, get("acme.test", "acme", "acme-co", "")).await;
        assert_eq!(ok.0, 200);
        assert_eq!(ok.1["groups"][0]["id"], json!("pg-1"));
        assert_eq!(ok.1["groups"][0]["currency"], json!("THB"));
        assert_eq!(
            seen.lock().unwrap().last().cloned(),
            Some("acme-co".to_owned())
        );
        assert_eq!(http(addr, get("other.test", "acme", "acme-co", "")).await.0, 404);
        assert_eq!(http(addr, get("acme.test", "other", "acme-co", "")).await.0, 404);
        assert_eq!(
            http(addr, get("acme.test", "acme", "bad id", "")).await.0,
            422
        );
        assert_eq!(
            http(addr, get("acme.test", "acme", "acme-co", "?x=1")).await.0,
            422
        );
        server.abort();

        let failing: Arc<dyn PayGroupOptionsRead> = Arc::new(Probe {
            rows: vec![],
            fail: true,
            seen: Arc::new(std::sync::Mutex::new(Vec::new())),
        });
        let router = router_with_reader(vec![(target("acme.test", "acme"), failing)]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let failed = http(addr, get("acme.test", "acme", "acme-co", "")).await;
        assert_eq!(failed.0, 503);
        assert_eq!(failed.1["error"]["code"], "PEOPLE_PAYROLL_PGO_READ_UNAVAILABLE");
        server.abort();
    }
}
