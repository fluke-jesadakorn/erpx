//! Owner-local axum ingress for the native R3 country-packs read. Host/Holding
//! targets, Company scope, payload order and error mapping are owned here;
//! PostgreSQL execution lives in `postgres`. There is no TypeScript SQL
//! fallback for this operation.

use super::application::CountryPacksRead;
use super::domain::{CountryPackRow, invalid, valid_id, valid_slug};
use super::postgres::PgCountryPacksRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, IntoResponse, Json, Path, Request, Response, Router,
    State, StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

/// One exact registered browser origin/Holding pair. Duplicates or loose
/// patterns are rejected so one Holding cannot read another Holding's data.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CountryPacksTarget {
    pub host: String,
    pub holding_slug: String,
}

struct PacksState {
    targets: BTreeMap<(String, String), Arc<dyn CountryPacksRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_COUNTRY_PACKS_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
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

/// The retained route is plain `Response.json({ data })` with no extra
/// headers; byte parity requires no Cache-Control/Referrer-Policy injection.
#[derive(serde::Serialize)]
struct PacksResponse {
    data: Vec<CountryPackRow>,
}

fn configure_readers(
    entries: impl IntoIterator<Item = (CountryPacksTarget, Arc<dyn CountryPacksRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn CountryPacksRead>>, BusinessError> {
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

fn finish(configured: BTreeMap<(String, String), Arc<dyn CountryPacksRead>>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people/country-packs",
            get(country_packs),
        )
        .with_state(Arc::new(PacksState { targets: configured }))
}

/// Native router with one Pg owner reader per registered Holding pool.
pub fn router(
    targets: Vec<(CountryPacksTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    Ok(finish(configure_readers(targets.into_iter().map(|(target, pool)| {
        (
            target,
            Arc::new(PgCountryPacksRead::new(pool)) as Arc<dyn CountryPacksRead>,
        )
    }))?))
}

/// Reader-injection path used by owner-local contract tests.
pub fn router_with_reader(
    targets: Vec<(CountryPacksTarget, Arc<dyn CountryPacksRead>)>,
) -> Result<Router, BusinessError> {
    Ok(finish(configure_readers(targets)?))
}

async fn country_packs(
    State(state): State<Arc<PacksState>>,
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
        let rows = reader.packs(company).await?;
        Ok::<_, BusinessError>(PacksResponse { data: rows })
    }
    .await;
    match result {
        Ok(body) => (StatusCode::OK, Json(body)).into_response(),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::application::CountryPackFuture;
    use super::super::domain::unavailable;
    use erpx_platform_runtime_server::public::http_support::serde_json::json;

    struct Probe {
        rows: Vec<CountryPackRow>,
        fail: bool,
    }

    impl CountryPacksRead for Probe {
        fn packs<'a>(&'a self, _company: &'a str) -> CountryPackFuture<'a, Vec<CountryPackRow>> {
            let rows = self.rows.clone();
            let fail = self.fail;
            Box::pin(async move {
                if fail {
                    return Err(unavailable());
                }
                Ok(rows)
            })
        }
    }

    fn target(host: &str, slug: &str) -> CountryPacksTarget {
        CountryPacksTarget {
            host: host.into(),
            holding_slug: slug.into(),
        }
    }

    fn row() -> CountryPackRow {
        CountryPackRow {
            id: "pack-1".into(),
            country_code: "TH".into(),
            version: "1".into(),
            status: "active".into(),
            effective_from: Some("2026-01-01".into()),
            effective_to: None,
            adapter_id: None,
            source_digest: None,
            record_version: 1,
            export_formats_json: json!(["csv"]),
            validated_by: Some("auditor".into()),
            updated_at: "2026-01-02".into(),
        }
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        let reader: Arc<dyn CountryPacksRead> = Arc::new(Probe {
            rows: vec![],
            fail: false,
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
    fn payload_matches_the_retained_typescript_envelope() {
        let body = serde_json::to_string(&PacksResponse { data: vec![row()] }).unwrap();
        assert_eq!(
            body,
            r#"{"data":[{"id":"pack-1","country_code":"TH","version":"1","status":"active","effective_from":"2026-01-01","effective_to":null,"adapter_id":null,"source_digest":null,"record_version":1,"export_formats_json":["csv"],"validated_by":"auditor","updated_at":"2026-01-02"}]}"#
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
    async fn router_scopes_holding_and_validates_company() {
        let reader: Arc<dyn CountryPacksRead> = Arc::new(Probe {
            rows: vec![row()],
            fail: false,
        });
        let router = router_with_reader(vec![(target("acme.test", "acme"), reader)]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let get = |host: &str, slug: &str, company: &str| {
            format!(
                "GET /api/v1/holdings/{slug}/people/country-packs HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nx-erpx-context-company: {company}\r\n\r\n"
            )
        };
        let ok = http(addr, get("acme.test", "acme", "acme-co")).await;
        assert_eq!(ok.0, 200);
        assert_eq!(ok.1["data"][0]["country_code"], json!("TH"));
        assert_eq!(http(addr, get("other.test", "acme", "acme-co")).await.0, 404);
        assert_eq!(http(addr, get("acme.test", "other", "acme-co")).await.0, 404);
        assert_eq!(http(addr, get("acme.test", "acme", "bad id")).await.0, 422);
        server.abort();

        let failing: Arc<dyn CountryPacksRead> = Arc::new(Probe {
            rows: vec![],
            fail: true,
        });
        let router =
            router_with_reader(vec![(target("acme.test", "acme"), failing)]).unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_platform_runtime_server::public::http_support::serve(listener, router).await;
        });
        let failed = http(addr, get("acme.test", "acme", "acme-co")).await;
        assert_eq!(failed.0, 503);
        assert_eq!(
            failed.1["error"]["code"],
            "PEOPLE_PAYROLL_COUNTRY_PACKS_READ_UNAVAILABLE"
        );
        server.abort();
    }
}
