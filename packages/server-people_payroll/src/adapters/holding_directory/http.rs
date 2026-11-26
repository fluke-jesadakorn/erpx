//! Owner-local axum ingress for the Holding people directory. The router owns
//! the exact registered Holding target, bounded transport and payload shape;
//! Account authority arrives through a composition port and Pg execution stays
//! in this crate's `postgres` adapter. No SQL, no session and no authority is
//! minted here.

use super::application::{HoldingDirectoryAuthority, HoldingDirectoryRead, read_holding_directory};
use super::domain::{
    HoldingDirectoryScope, HoldingPeopleDirectory, invalid, valid_id, valid_slug,
};
use super::postgres::PgHoldingDirectoryRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    header, routing::get, serde_json,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

/// One exact registered browser origin/Holding pair plus its Account authority.
/// Duplicates or loose patterns are rejected so one Holding cannot read another.
pub struct HoldingDirectoryTarget {
    pub host: String,
    pub holding_slug: String,
    pub authority: Arc<dyn HoldingDirectoryAuthority>,
}

struct DirectoryState {
    targets: BTreeMap<(String, String), (Arc<dyn HoldingDirectoryAuthority>, Arc<dyn HoldingDirectoryRead>)>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "PEOPLE_PAYROLL_HOLDING_DIRECTORY_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
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

fn error_response(error: BusinessError) -> Response {
    (
        status(error.kind),
        Json(serde_json::json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

/// Response wrapper keeps the retired TypeScript field order (`data`, then the
/// directory arrays in declaration order) instead of a sorted `Value` map.
#[derive(serde::Serialize)]
struct HoldingDirectoryResponse {
    data: HoldingPeopleDirectory,
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

pub fn unique_header(headers: &HeaderMap, name: header::HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn finish(
    entries: Vec<(
        HoldingDirectoryTarget,
        Arc<dyn HoldingDirectoryRead>,
    )>,
) -> Result<Router, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
        let key = (target.host.clone(), target.holding_slug.clone());
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || configured
                .insert(key, (target.authority, reader))
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/people-public-directory",
            get(directory),
        )
        .with_state(Arc::new(DirectoryState {
            targets: configured,
        })))
}

/// Native router with one Pg owner reader per registered Holding pool.
pub fn router(
    targets: Vec<(HoldingDirectoryTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    finish(
        targets
            .into_iter()
            .map(|(target, pool)| {
                (
                    target,
                    Arc::new(PgHoldingDirectoryRead::new(pool)) as Arc<dyn HoldingDirectoryRead>,
                )
            })
            .collect(),
    )
}

/// Reader-injection path used by owner-local contract tests.
pub fn router_with_reader(
    targets: Vec<(HoldingDirectoryTarget, Arc<dyn HoldingDirectoryRead>)>,
) -> Result<Router, BusinessError> {
    finish(targets)
}

async fn directory(
    State(state): State<Arc<DirectoryState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async move {
        let headers = request.headers().clone();
        let host = unique_header(&headers, header::HOST)?
            .filter(|host| !host.is_empty())
            .ok_or_else(missing)?;
        let (authority, reader) = state
            .targets
            .get(&(host.to_owned(), slug.clone()))
            .ok_or_else(missing)?;
        if request.uri().query().is_some() {
            return Err(invalid());
        }
        let scope: HoldingDirectoryScope =
            authority.work_scope(host, &slug, &headers).await?;
        if scope.holding_id.is_empty()
            || !valid_id(&scope.holding_id)
            || !valid_id(&scope.binding_id)
            || !valid_id(&scope.company_id)
            || scope.scope_holding_id.as_deref().is_some_and(|v| !valid_id(v))
            || scope.scope_company_id.as_deref().is_some_and(|v| !valid_id(v))
            || !valid_id(&scope.assignment_id)
            || !valid_id(&scope.user_id)
            || !valid_id(&scope.role)
        {
            return Err(invalid());
        }
        let data: HoldingPeopleDirectory = read_holding_directory(reader.as_ref(), &scope).await?;
        Ok::<_, BusinessError>(HoldingDirectoryResponse { data })
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
    use super::super::application::HoldingDirectoryFuture;
    use super::super::domain::{
        DirectoryCompany, DirectoryLevel, DirectoryNode, DirectoryPerson, DirectoryPosition,
        HoldingDirectoryFacts,
    };
    use erpx_server_runtime::public::http_support::serde_json::json;
    use sha2::Digest;

    struct Authority {
        scope: HoldingDirectoryScope,
    }

    impl HoldingDirectoryAuthority for Authority {
        fn work_scope<'a>(
            &'a self,
            _: &'a str,
            _: &'a str,
            _: &'a HeaderMap,
        ) -> HoldingDirectoryFuture<'a, HoldingDirectoryScope> {
            let scope = self.scope.clone();
            Box::pin(async move { Ok(scope) })
        }
    }

    struct Reader {
        active: bool,
    }

    impl HoldingDirectoryRead for Reader {
        fn read<'a>(
            &'a self,
            _: &'a HoldingDirectoryScope,
        ) -> HoldingDirectoryFuture<'a, HoldingDirectoryFacts> {
            let active = self.active;
            Box::pin(async move {
                Ok(HoldingDirectoryFacts {
                    assignment_active: active,
                    companies: vec![DirectoryCompany {
                        id: "c1".into(),
                        slug: "acme".into(),
                        name: "Acme".into(),
                    }],
                    ..Default::default()
                })
            })
        }
    }

    fn scope() -> HoldingDirectoryScope {
        HoldingDirectoryScope {
            holding_id: "h1".into(),
            binding_id: "h1".into(),
            company_id: "c1".into(),
            scope_holding_id: Some("h1".into()),
            scope_company_id: Some("c1".into()),
            assignment_id: "a1".into(),
            user_id: "user-1".into(),
            role: "holding_owner".into(),
            assignment_scope: "holding".into(),
        }
    }

    fn target(host: &str, slug: &str) -> HoldingDirectoryTarget {
        HoldingDirectoryTarget {
            host: host.into(),
            holding_slug: slug.into(),
            authority: Arc::new(Authority { scope: scope() }),
        }
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        let reader: Arc<dyn HoldingDirectoryRead> = Arc::new(Reader { active: true });
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

    #[tokio::test]
    async fn router_scopes_holding_and_maps_errors() {
        let router = router_with_reader(vec![(
            target("acme.test", "acme"),
            Arc::new(Reader { active: true }) as Arc<dyn HoldingDirectoryRead>,
        )])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let get = |host: &str, slug: &str| {
            format!(
                "GET /api/v1/holdings/{slug}/people-public-directory HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
            )
        };
        let ok = http(addr, get("acme.test", "acme")).await;
        assert_eq!(ok.0, 200);
        assert_eq!(ok.1["data"]["companies"][0]["name"], json!("Acme"));
        let wrong_host = http(addr, get("other.test", "acme")).await;
        assert_eq!(wrong_host.0, 404);
        let wrong_slug = http(addr, get("acme.test", "other")).await;
        assert_eq!(wrong_slug.0, 404);
        server.abort();

        let mut forbidden = scope();
        forbidden.scope_company_id = Some("c2".into());
        let router = router_with_reader(vec![(
            HoldingDirectoryTarget {
                host: "acme.test".into(),
                holding_slug: "acme".into(),
                authority: Arc::new(Authority { scope: forbidden }),
            },
            Arc::new(Reader { active: true }) as Arc<dyn HoldingDirectoryRead>,
        )])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        assert_eq!(http(addr, get("acme.test", "acme")).await.0, 403);
        server.abort();

        let router = router_with_reader(vec![(
            target("acme.test", "acme"),
            Arc::new(Reader { active: false }) as Arc<dyn HoldingDirectoryRead>,
        )])
        .unwrap();
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let server = tokio::spawn(async move {
            let _ = erpx_server_runtime::public::http_support::serve(listener, router).await;
        });
        let inactive = http(addr, get("acme.test", "acme")).await;
        assert_eq!(inactive.0, 403);
        assert_eq!(inactive.1["error"]["code"], "HOLDING_DIRECTORY_FORBIDDEN");
        server.abort();
    }

    /// Pinned retired TypeScript output for the exact fixture below. Keep in
    /// sync with `tests/unit/organization/holding-people-directory-parity.test.ts`.
    #[test]
    fn payload_matches_the_retired_typescript_output() {
        let facts = HoldingDirectoryFacts {
            assignment_active: true,
            companies: vec![DirectoryCompany {
                id: "c1".into(),
                slug: "acme".into(),
                name: "Acme".into(),
            }],
            nodes: vec![DirectoryNode {
                id: "n1".into(),
                company_id: "c1".into(),
                kind: "company".into(),
                slug: "acme".into(),
                name: "Acme HQ".into(),
                parent_id: None,
                branch_id: None,
            }],
            levels: vec![DirectoryLevel {
                id: "l1".into(),
                company_id: "c1".into(),
                code: "L1".into(),
                name: "One".into(),
                number: Some(1),
            }],
            positions: vec![DirectoryPosition {
                id: "p1".into(),
                company_id: "c1".into(),
                branch_id: None,
                department_id: None,
                title: "CEO".into(),
                level_id: "l1".into(),
                reports_to_position_id: None,
                is_ceo: true,
            }],
            people: vec![DirectoryPerson {
                id: "e1".into(),
                person_id: "person-1".into(),
                user_id: Some("user-1".into()),
                engagement_id: "e1".into(),
                company_id: "c1".into(),
                branch_id: None,
                department_id: None,
                position_id: Some("p1".into()),
                legal_name: "Ada Lovelace".into(),
                job_title: "CEO".into(),
                work_email: Some("ada@example.test".into()),
                work_phone: None,
            }],
        };
        let data: HoldingPeopleDirectory = HoldingPeopleDirectory {
            companies: facts.companies,
            nodes: facts.nodes,
            levels: facts.levels,
            positions: facts.positions,
            people: facts.people,
        };
        let body = serde_json::to_string(&HoldingDirectoryResponse { data }).unwrap();
        assert_eq!(body, "{\"data\":{\"companies\":[{\"id\":\"c1\",\"slug\":\"acme\",\"name\":\"Acme\"}],\"nodes\":[{\"id\":\"n1\",\"companyId\":\"c1\",\"kind\":\"company\",\"slug\":\"acme\",\"name\":\"Acme HQ\",\"parentId\":null,\"branchId\":null}],\"levels\":[{\"id\":\"l1\",\"companyId\":\"c1\",\"code\":\"L1\",\"name\":\"One\",\"number\":1}],\"positions\":[{\"id\":\"p1\",\"companyId\":\"c1\",\"branchId\":null,\"departmentId\":null,\"title\":\"CEO\",\"levelId\":\"l1\",\"reportsToPositionId\":null,\"isCeo\":true}],\"people\":[{\"id\":\"e1\",\"personId\":\"person-1\",\"userId\":\"user-1\",\"engagementId\":\"e1\",\"companyId\":\"c1\",\"branchId\":null,\"departmentId\":null,\"positionId\":\"p1\",\"legalName\":\"Ada Lovelace\",\"jobTitle\":\"CEO\",\"workEmail\":\"ada@example.test\",\"workPhone\":null}]}}");
        let hex = sha2::Sha256::digest(body.as_bytes())
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        assert_eq!(
            hex,
            "7fdd3261202ff75d9822102d8d7e7ecba59990a6793a542342ee564804a101dc",
            "update this oracle and tests/unit/organization/holding-people-directory-parity.test.ts"
        );
    }
}
