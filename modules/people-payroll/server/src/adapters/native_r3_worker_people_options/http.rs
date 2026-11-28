//! Owner-local axum ingress for the native R3 project-worker-startup
//! people-options read. Host/Holding targets, Company scope, the one bounded
//! query value, payload order and error mapping are owned here; PostgreSQL
//! execution lives in `postgres`. There is no TypeScript SQL or business
//! fallback for this operation.

use super::application::WorkerPeopleOptionsRead;
use super::domain::{WorkerPersonOption, invalid, valid_id, valid_query};
use super::postgres::PgWorkerPeopleOptionsRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// One exact registered browser origin/Holding pair. Duplicates or loose
/// patterns are rejected so one Holding cannot read another Holding's people.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WorkerPeopleOptionsTarget {
    pub host: String,
    pub holding_slug: String,
}

struct OptionsState {
    targets: BTreeMap<(String, String), Arc<dyn WorkerPeopleOptionsRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_WORKER_PEOPLE_CONFIG_INVALID",
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

fn hex(value: u8) -> Option<u8> {
    match value {
        b'0'..=b'9' => Some(value - b'0'),
        b'a'..=b'f' => Some(value - b'a' + 10),
        b'A'..=b'F' => Some(value - b'A' + 10),
        _ => None,
    }
}

/// `URLSearchParams.get` decodes `+` and `%XX` before the strict zod schema
/// trims the value. Reproduce that here so the native reader sees the same
/// display-name substring the retired route did.
fn decode(value: &str) -> Result<String, BusinessError> {
    let bytes = value.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut index = 0;
    while index < bytes.len() {
        match bytes[index] {
            b'+' => {
                out.push(b' ');
                index += 1;
            }
            b'%' => {
                let high = bytes.get(index + 1).copied().and_then(hex).ok_or_else(invalid)?;
                let low = bytes.get(index + 2).copied().and_then(hex).ok_or_else(invalid)?;
                out.push(high * 16 + low);
                index += 3;
            }
            byte => {
                out.push(byte);
                index += 1;
            }
        }
    }
    String::from_utf8(out).map_err(|_| invalid())
}

/// `projectWorkerPeopleSchema` is strict: only one optional `query` key,
/// trimmed to at most 120 characters. A duplicate or unknown key fails closed.
fn query_value(query: Option<&str>) -> Result<String, BusinessError> {
    let mut found: Option<String> = None;
    for pair in query.unwrap_or("").split('&').filter(|part| !part.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name != "query" {
            return Err(invalid());
        }
        if found.replace(decode(value)?).is_some() {
            return Err(invalid());
        }
    }
    let trimmed = found.unwrap_or_default().trim().to_owned();
    if !valid_query(&trimmed) {
        return Err(invalid());
    }
    Ok(trimmed)
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

#[derive(serde::Serialize)]
struct PeopleEnvelope {
    #[serde(rename = "companyId")]
    company_id: String,
    people: Vec<WorkerPersonOption>,
}

#[derive(serde::Serialize)]
struct OptionsResponse {
    data: PeopleEnvelope,
}

fn bounded(body: OptionsResponse) -> Response {
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
    entries: impl IntoIterator<Item = (WorkerPeopleOptionsTarget, Arc<dyn WorkerPeopleOptionsRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn WorkerPeopleOptionsRead>>, BusinessError> {
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
    configured: BTreeMap<(String, String), Arc<dyn WorkerPeopleOptionsRead>>,
) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people/project-worker-startup/people",
            get(options),
        )
        .with_state(Arc::new(OptionsState { targets: configured }))
}

/// Native router with one Pg owner reader per registered Holding pool. The lead
/// mounts this on the private native People Payroll axum service.
pub fn router(
    targets: Vec<(WorkerPeopleOptionsTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    Ok(finish(configure_readers(targets.into_iter().map(|(target, pool)| {
        (
            target,
            Arc::new(PgWorkerPeopleOptionsRead::new(pool)) as Arc<dyn WorkerPeopleOptionsRead>,
        )
    }))?))
}

/// Reader-injection path used by owner-local contract tests.
pub fn router_with_reader(
    targets: Vec<(WorkerPeopleOptionsTarget, Arc<dyn WorkerPeopleOptionsRead>)>,
) -> Result<Router, BusinessError> {
    Ok(finish(configure_readers(targets)?))
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
        let company = unique_header(&headers, CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?;
        let query = query_value(request.uri().query())?;
        let rows = reader.people(company, &query).await?;
        Ok::<_, BusinessError>(OptionsResponse {
            data: PeopleEnvelope {
                company_id: company.to_owned(),
                people: rows,
            },
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
    use super::super::application::PeopleFuture;
    use super::super::domain::unavailable;
    use erpx_platform_runtime_server::public::http_support::serde_json::json;

    struct Probe {
        rows: Vec<WorkerPersonOption>,
        fail: bool,
        seen: Arc<std::sync::Mutex<Vec<(String, String)>>>,
    }

    impl WorkerPeopleOptionsRead for Probe {
        fn people<'a>(
            &'a self,
            company: &'a str,
            query: &'a str,
        ) -> PeopleFuture<'a, Vec<WorkerPersonOption>> {
            let rows = self.rows.clone();
            let fail = self.fail;
            let seen = self.seen.clone();
            Box::pin(async move {
                seen.lock()
                    .unwrap()
                    .push((company.to_owned(), query.to_owned()));
                if fail {
                    return Err(unavailable());
                }
                Ok(rows)
            })
        }
    }

    fn target(host: &str, slug: &str) -> WorkerPeopleOptionsTarget {
        WorkerPeopleOptionsTarget {
            host: host.into(),
            holding_slug: slug.into(),
        }
    }

    fn rows() -> Vec<WorkerPersonOption> {
        vec![WorkerPersonOption {
            id: "p-1".into(),
            legal_name: "Ada".into(),
            version: 3,
            engagement_id: "e-1".into(),
            engagement_version: 2,
        }]
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        let reader: Arc<dyn WorkerPeopleOptionsRead> = Arc::new(Probe {
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

    #[test]
    fn payload_matches_the_retired_typescript_envelope() {
        let body = serde_json::to_string(&OptionsResponse {
            data: PeopleEnvelope {
                company_id: "acme-co".into(),
                people: rows(),
            },
        })
        .unwrap();
        assert_eq!(
            body,
            r#"{"data":{"companyId":"acme-co","people":[{"id":"p-1","legalName":"Ada","version":3,"engagementId":"e-1","engagementVersion":2}]}}"#
        );
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

    #[tokio::test]
    async fn router_scopes_company_and_validates_query() {
        let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
        let reader: Arc<dyn WorkerPeopleOptionsRead> = Arc::new(Probe {
            rows: rows(),
            fail: false,
            seen: seen.clone(),
        });
        let router =
            router_with_reader(vec![(target("acme.test", "acme"), reader)]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let get = |host: &str, slug: &str, company: &str, query: &str| {
            format!(
                "GET /api/v1/holdings/{slug}/people/project-worker-startup/people{query} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nx-erpx-context-company: {company}\r\n\r\n"
            )
        };
        let ok = http(addr, get("acme.test", "acme", "acme-co", "?query=%20Ada%20")).await;
        assert_eq!(ok.0, 200);
        assert_eq!(ok.1["data"]["companyId"], json!("acme-co"));
        assert_eq!(ok.1["data"]["people"][0]["legalName"], json!("Ada"));
        assert_eq!(
            seen.lock().unwrap().last().cloned(),
            Some(("acme-co".to_owned(), "Ada".to_owned()))
        );
        assert_eq!(http(addr, get("other.test", "acme", "acme-co", "")).await.0, 404);
        assert_eq!(http(addr, get("acme.test", "other", "acme-co", "")).await.0, 404);
        assert_eq!(http(addr, get("acme.test", "acme", "bad id", "")).await.0, 422);
        assert_eq!(
            http(addr, get("acme.test", "acme", "acme-co", "?query=a&query=b")).await.0,
            422
        );
        assert_eq!(
            http(addr, get("acme.test", "acme", "acme-co", "?unknown=1")).await.0,
            422
        );
        server.abort();

        let failing: Arc<dyn WorkerPeopleOptionsRead> = Arc::new(Probe {
            rows: vec![],
            fail: true,
            seen: Arc::new(std::sync::Mutex::new(Vec::new())),
        });
        let router =
            router_with_reader(vec![(target("acme.test", "acme"), failing)]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let failed = http(addr, get("acme.test", "acme", "acme-co", "")).await;
        assert_eq!(failed.0, 503);
        assert_eq!(
            failed.1["error"]["code"],
            "PEOPLE_PAYROLL_WORKER_PEOPLE_READ_UNAVAILABLE"
        );
        server.abort();
    }
}
