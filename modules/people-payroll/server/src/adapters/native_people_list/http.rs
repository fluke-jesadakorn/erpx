//! Owner-local axum ingress for the native people-directory list read. Host and
//! Holding targets, Company/branch/department context headers, bounded query
//! values, payload order and error mapping are owned here; the PostgreSQL
//! execution lives in `postgres`. No TypeScript SQL or business fallback exists
//! for this operation.

use super::application::{ListPeopleQuery, PeopleListRead, list_people};
use super::domain::{PersonListRow, filter, invalid, missing, unavailable, valid_id};
use super::postgres::PgPeopleListRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const CONTEXT_BRANCH: HeaderName = HeaderName::from_static("x-erpx-context-branch");
const CONTEXT_DEPARTMENT: HeaderName = HeaderName::from_static("x-erpx-context-department");

/// One exact registered browser origin/Holding pair. Duplicates or loose
/// patterns are rejected so one Holding cannot read another Holding's people.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct PeopleListTarget {
    pub host: String,
    pub holding_slug: String,
}

struct ListState {
    targets: BTreeMap<(String, String), Arc<dyn PeopleListRead>>,
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
struct ListResponse {
    data: Vec<PersonListRow>,
}

fn bounded(body: ListResponse) -> Response {
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

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_PEOPLE_LIST_CONFIG_INVALID",
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

/// Only the two query keys the retired route read are accepted. A duplicate
/// key or an unexpected value fails closed; unknown keys are ignored exactly as
/// the TypeScript `URLSearchParams.get` call ignored them.
fn query_value(query: Option<&str>, key: &str) -> Result<Option<String>, BusinessError> {
    let mut found = None;
    for pair in query.unwrap_or("").split('&').filter(|part| !part.is_empty()) {
        let (name, value) = pair.split_once('=').unwrap_or((pair, ""));
        if name == key {
            if found.replace(value.to_owned()).is_some() {
                return Err(invalid());
            }
        }
    }
    filter(found.as_deref())
}

fn configure(
    entries: impl IntoIterator<Item = (PeopleListTarget, Arc<dyn PeopleListRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn PeopleListRead>>, BusinessError> {
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

fn finish(targets: BTreeMap<(String, String), Arc<dyn PeopleListRead>>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people-payroll/people-list",
            get(people),
        )
        .with_state(Arc::new(ListState { targets }))
}

/// Native router with one Pg owner reader per registered Holding pool. The lead
/// mounts this on the private native People Payroll axum service.
pub fn router_for_holdings(
    targets: Vec<(PeopleListTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    Ok(finish(configure(targets.into_iter().map(|(target, pool)| {
        (
            target,
            Arc::new(PgPeopleListRead::new(pool)) as Arc<dyn PeopleListRead>,
        )
    }))?))
}

/// Reader-injection path used by owner-local contract tests.
pub fn router_with_reader(
    reader: Arc<dyn PeopleListRead>,
    targets: Vec<PeopleListTarget>,
) -> Result<Router, BusinessError> {
    Ok(finish(configure(
        targets.into_iter().map(|target| (target, reader.clone())),
    )?))
}

async fn people(
    State(state): State<Arc<ListState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let host = match unique_header(&headers, header::HOST) {
        Ok(Some(host)) if !host.is_empty() => host.to_owned(),
        _ => return error_response(missing()),
    };
    let reader = match state.targets.get(&(host, slug)) {
        Some(reader) => reader.clone(),
        None => return error_response(missing()),
    };
    let company = match unique_header(&headers, CONTEXT_COMPANY) {
        Ok(Some(value)) if valid_id(value) => value.to_owned(),
        _ => return error_response(invalid()),
    };
    let branch = match unique_header(&headers, CONTEXT_BRANCH) {
        Ok(value) => value.filter(|value| !value.is_empty()).map(str::to_owned),
        Err(error) => return error_response(error),
    };
    let department = match unique_header(&headers, CONTEXT_DEPARTMENT) {
        Ok(value) => value.filter(|value| !value.is_empty()).map(str::to_owned),
        Err(error) => return error_response(error),
    };
    let raw = request.uri().query();
    let level = match query_value(raw, "levelId") {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    let filter_department = match query_value(raw, "departmentId") {
        Ok(value) => value,
        Err(error) => return error_response(error),
    };
    let query = ListPeopleQuery::new(
        &company,
        branch.as_deref(),
        department.as_deref(),
        level.as_deref(),
        filter_department.as_deref(),
    );
    match list_people(reader.as_ref(), &query).await {
        Ok(rows) => bounded(ListResponse { data: rows }),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::adapters::native_people_list::application::ListFuture;
    use crate::adapters::native_people_list::domain::{fixture_row, unavailable};
    use crate::adapters::native_people_list::postgres::SQL_ORACLE_SHA256;
    use erpx_platform_runtime_server::public::http_support::serde_json::json;
    use sha2::Digest;
    use std::sync::Mutex;

    struct Probe {
        rows: Vec<PersonListRow>,
        fail: bool,
        seen: Arc<Mutex<Vec<ListPeopleQuery>>>,
    }

    impl PeopleListRead for Probe {
        fn list<'a>(&'a self, query: &'a ListPeopleQuery) -> ListFuture<'a, Vec<PersonListRow>> {
            let query = query.clone();
            let rows = self.rows.clone();
            let fail = self.fail;
            let seen = self.seen.clone();
            Box::pin(async move {
                seen.lock().unwrap().push(query);
                if fail {
                    return Err(unavailable());
                }
                Ok(rows)
            })
        }
    }

    fn target(host: &str, slug: &str) -> PeopleListTarget {
        PeopleListTarget {
            host: host.into(),
            holding_slug: slug.into(),
        }
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        let reader: Arc<dyn PeopleListRead> = Arc::new(Probe {
            rows: vec![],
            fail: false,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        assert!(router_with_reader(reader.clone(), vec![target("acme.test", "acme")]).is_ok());
        assert!(router_with_reader(
            reader.clone(),
            vec![target("acme.test", "acme"), target("acme.test", "acme")],
        )
        .is_err());
        assert!(router_with_reader(reader.clone(), vec![]).is_err());
        assert!(router_with_reader(reader, vec![target("acme.test", "Acme")]).is_err());
    }

    #[test]
    fn payload_matches_the_retired_typescript_output() {
        let body = serde_json::to_string(&ListResponse {
            data: vec![fixture_row()],
        })
        .unwrap();
        let hex = sha2::Sha256::digest(body.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "d9b65f42da52f75dfb1bc00945a66b972ee0f5906ae59c798612c5a8534084e0",
            "update this oracle in Rust and tests/unit/people_payroll/native-people-list.test.ts"
        );
        assert_eq!(SQL_ORACLE_SHA256.len(), 64);
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
    async fn router_scopes_the_holding_and_binds_both_filters() {
        let seen = Arc::new(Mutex::new(Vec::new()));
        let reader: Arc<dyn PeopleListRead> = Arc::new(Probe {
            rows: vec![fixture_row()],
            fail: false,
            seen: seen.clone(),
        });
        let router = router_with_reader(reader, vec![target("acme.test", "acme")]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let get = |host: &str, slug: &str, company: &str, query: &str| {
            format!(
                "GET /api/v1/holdings/{slug}/people-payroll/people-list{query} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\
                 x-erpx-context-company: {company}\r\nx-erpx-context-branch: branch-1\r\n\
                 x-erpx-context-department: dept-1\r\n\r\n"
            )
        };
        let ok = http(
            addr,
            get(
                "acme.test",
                "acme",
                "acme-co",
                "?levelId=unset&departmentId=dept-2",
            ),
        )
        .await;
        assert_eq!(ok.0, 200);
        assert_eq!(ok.1["data"][0]["id"], json!("person-1"));
        let bound = seen.lock().unwrap().last().cloned().unwrap();
        assert_eq!(bound.company, "acme-co");
        assert_eq!(bound.branch.as_deref(), Some("branch-1"));
        assert_eq!(bound.department.as_deref(), Some("dept-1"));
        assert_eq!(bound.level.as_deref(), Some("unset"));
        assert_eq!(bound.filter_department.as_deref(), Some("dept-2"));

        assert_eq!(http(addr, get("other.test", "acme", "acme-co", "")).await.0, 404);
        assert_eq!(http(addr, get("acme.test", "other", "acme-co", "")).await.0, 404);
        assert_eq!(
            http(addr, get("acme.test", "acme", "bad company", "")).await.0,
            422
        );
        assert_eq!(
            http(
                addr,
                get("acme.test", "acme", "acme-co", "?levelId=drop%20table")
            )
            .await
            .0,
            422
        );
        assert_eq!(
            http(
                addr,
                get("acme.test", "acme", "acme-co", "?levelId=a&levelId=b")
            )
            .await
            .0,
            422
        );
        server.abort();

        let failing: Arc<dyn PeopleListRead> = Arc::new(Probe {
            rows: vec![],
            fail: true,
            seen: Arc::new(Mutex::new(Vec::new())),
        });
        let router =
            router_with_reader(failing, vec![target("fail.test", "fail")]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let failed = http(
            addr,
            "GET /api/v1/holdings/fail/people-payroll/people-list HTTP/1.1\r\nHost: fail.test\r\nConnection: close\r\nx-erpx-context-company: acme-co\r\n\r\n".into(),
        )
        .await;
        assert_eq!(failed.0, 503);
        assert_eq!(
            failed.1["error"]["code"],
            "PEOPLE_PAYROLL_PEOPLE_LIST_READ_UNAVAILABLE"
        );
        server.abort();
    }
}
