//! Owner-local axum router for lane 12. Targets, the company context header and
//! error mapping follow `server-commerce::quality_product_read`: a request is
//! served only for an exact registered (host, holding_slug) pair and one bounded
//! company context. Missing or malformed configuration fails closed.

use super::application::PeopleTemplateRead;
use super::domain::{AttributeRead, LaneTarget};
use super::postgres::PgPeopleTemplateRead;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Query, Request, Response, Router,
    State, StatusCode, header, routing::get, serde_json::{Value, json},
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");

#[derive(Clone, Debug, Deserialize)]
struct ReadQuery {
    role: Option<String>,
    value: Option<String>,
}

struct LaneState {
    targets: BTreeMap<(String, String), Arc<dyn PeopleTemplateRead>>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LANE_12_READ_REQUEST_INVALID",
        "Use one bounded People-template attribute read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "NATIVE_LANE_12_CONFIG_INVALID",
        "Use exact distinct registered Holding targets.",
    )
}

fn valid_slug(slug: &str) -> bool {
    !slug.is_empty()
        && slug.len() <= 128
        && slug
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-' || byte == b'_')
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
        Json(json!({"error":{"code":error.code,"message":error.message}})),
    )
        .into_response()
}

fn bounded(body: Value) -> Response {
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

/// Build the native lane router from the owner's Pg pool. The lead mounts it;
/// no TS SQL or business rule is introduced here.
pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgPeopleTemplateRead::new(pool)), targets)
}

/// Shared constructor so unit tests can inject an in-memory owner reader.
pub fn router_with_reader(
    reader: Arc<dyn PeopleTemplateRead>,
    targets: Vec<LaneTarget>,
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
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/identity-access/people-template-attributes/{kind}",
            get(read),
        )
        .with_state(Arc::new(LaneState { targets: configured })))
}

async fn read(
    State(state): State<Arc<LaneState>>,
    Path((slug, kind)): Path<(String, String)>,
    Query(query): Query<ReadQuery>,
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
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| !value.is_empty() && value.len() <= 200)
            .ok_or_else(invalid)?;
        let read = AttributeRead::parse(
            &kind,
            company,
            query.role.as_deref(),
            query.value.as_deref(),
        )?;
        let page = reader.read(&read).await?;
        Ok::<_, BusinessError>(json!({"rows": page.rows, "rowCount": page.row_count}))
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
    use crate::adapters::native_lane_12::domain::ReadPage;
    use erpx_server_runtime::public::http_support::{Body, body::to_bytes};
    use serde_json::json;
    use std::sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    };
    use tower::ServiceExt;

    #[derive(Default)]
    struct RecordingReader {
        calls: AtomicUsize,
    }
    impl PeopleTemplateRead for RecordingReader {
        fn read<'a>(&'a self, read: &'a AttributeRead) -> super::super::application::AttributeReadFuture<'a, super::super::domain::AttributePage> {
            self.calls.fetch_add(1, Ordering::SeqCst);
            let read = read.clone();
            Box::pin(async move {
                Ok(ReadPage::new(vec![json!({
                    "kind": read.kind(),
                    "companyId": match read {
                        AttributeRead::Attributes { company_id } => company_id,
                        AttributeRead::ActiveRole { company_id, .. } => company_id,
                        AttributeRead::GroupReferences { company_id, .. } => company_id,
                    },
                })]))
            })
        }
    }

    fn target() -> Vec<LaneTarget> {
        vec![LaneTarget {
            host: "acme.jesadakorn.com".to_owned(),
            holding_slug: "acme".to_owned(),
        }]
    }

    async fn call(
        app: &Router,
        host: &str,
        company: Option<&str>,
        path: &str,
    ) -> (StatusCode, Value) {
        let mut builder = Request::builder()
            .method("GET")
            .uri(path)
            .header(header::HOST, host);
        if let Some(company) = company {
            builder = builder.header(CONTEXT_COMPANY, company);
        }
        let response = app
            .clone()
            .oneshot(builder.body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let value: Value = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        (status, value)
    }

    #[test]
    fn native_lane_12_rejects_invalid_target_configuration() {
        assert!(router_with_reader(Arc::new(RecordingReader::default()), Vec::new()).is_err());
        let bad_slug = vec![LaneTarget {
            host: "acme.jesadakorn.com".to_owned(),
            holding_slug: "ACME".to_owned(),
        }];
        assert!(router_with_reader(Arc::new(RecordingReader::default()), bad_slug).is_err());
        let duplicate = vec![
            LaneTarget { host: "acme.jesadakorn.com".to_owned(), holding_slug: "acme".to_owned() },
            LaneTarget { host: "acme.jesadakorn.com".to_owned(), holding_slug: "acme".to_owned() },
        ];
        assert!(router_with_reader(Arc::new(RecordingReader::default()), duplicate).is_err());
    }

    #[tokio::test]
    async fn native_lane_12_serves_bounded_company_scoped_reads() {
        let app = router_with_reader(Arc::new(RecordingReader::default()), target()).unwrap();
        let (status, body) = call(
            &app,
            "acme.jesadakorn.com",
            Some("acme-1"),
            "/api/v1/holdings/acme/identity-access/people-template-attributes/attributes",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["rowCount"], json!(1));
        assert_eq!(body["rows"][0]["kind"], json!("attributes"));
        assert_eq!(body["rows"][0]["companyId"], json!("acme-1"));

        let (status, body) = call(
            &app,
            "acme.jesadakorn.com",
            Some("acme-1"),
            "/api/v1/holdings/acme/identity-access/people-template-attributes/activeRole?role=iam.roles.manage",
        )
        .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["rows"][0]["kind"], json!("activeRole"));
    }

    #[tokio::test]
    async fn native_lane_12_fails_closed_on_unknown_target_or_bad_context() {
        let app = router_with_reader(Arc::new(RecordingReader::default()), target()).unwrap();
        let unknown = call(
            &app,
            "evil.example",
            Some("acme-1"),
            "/api/v1/holdings/acme/identity-access/people-template-attributes/attributes",
        )
        .await;
        assert_eq!(unknown.0, StatusCode::NOT_FOUND);
        assert_eq!(unknown.1["error"]["code"], json!("NOT_FOUND"));

        let no_company = call(
            &app,
            "acme.jesadakorn.com",
            None,
            "/api/v1/holdings/acme/identity-access/people-template-attributes/attributes",
        )
        .await;
        assert_eq!(no_company.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            no_company.1["error"]["code"],
            json!("NATIVE_LANE_12_READ_REQUEST_INVALID")
        );

        let unknown_kind = call(
            &app,
            "acme.jesadakorn.com",
            Some("acme-1"),
            "/api/v1/holdings/acme/identity-access/people-template-attributes/drop-table",
        )
        .await;
        assert_eq!(unknown_kind.0, StatusCode::UNPROCESSABLE_ENTITY);
    }
}
