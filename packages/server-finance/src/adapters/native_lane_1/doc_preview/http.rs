//! Owner-local axum ingress for the lane 1 document preview reads.

use super::application::{DocPreviewReader, DocPreviewService};
use super::domain::DocPreviewKind;
use super::postgres::PgDocPreviewReader;
use crate::adapters::native_lane_1::domain::{CompanyScope, valid_identifier};
use crate::adapters::native_lane_1::http::LaneTarget;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, body::to_bytes, header, json, post,
};
use erpx_server_runtime::public::postgres::DatabasePool;
use erpx_server_shared::public::{BusinessError, FailureKind, RecordId};
use serde::Deserialize;
use std::collections::BTreeMap;
use std::sync::Arc;

const CONTEXT_COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const MAX_BODY: usize = 16 * 1024;
type PreviewReader = Arc<dyn DocPreviewReader>;

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PreviewRequest {
    kind: String,
    #[serde(default)]
    journal_id: Option<String>,
}

struct PreviewBinding {
    company_id: String,
    reader: PreviewReader,
}

struct PreviewState {
    targets: BTreeMap<(String, String), PreviewBinding>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE_01_DOC_PREVIEW_REQUEST_INVALID",
        "Use one bounded document preview read.",
    )
}

fn configuration_invalid() -> BusinessError {
    BusinessError::invalid(
        "LANE_01_DOC_PREVIEW_TARGETS_INVALID",
        "Use exact distinct registered Holding Company targets.",
    )
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-' || b == b'_')
}

fn valid_id(value: &str) -> bool {
    RecordId::new(value).is_ok() && valid_identifier(value)
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

fn state_with_readers(
    entries: Vec<(LaneTarget, PreviewReader)>,
) -> Result<Arc<PreviewState>, BusinessError> {
    let mut configured = BTreeMap::new();
    for (target, reader) in entries {
        if target.host.is_empty()
            || !valid_slug(&target.holding_slug)
            || !valid_id(&target.company_id)
            || configured
                .insert(
                    (target.host.clone(), target.holding_slug.clone()),
                    PreviewBinding {
                        company_id: target.company_id,
                        reader,
                    },
                )
                .is_some()
        {
            return Err(configuration_invalid());
        }
    }
    if configured.is_empty() {
        return Err(configuration_invalid());
    }
    Ok(Arc::new(PreviewState {
        targets: configured,
    }))
}

fn state(
    targets: Vec<LaneTarget>,
    reader: PreviewReader,
) -> Result<Arc<PreviewState>, BusinessError> {
    state_with_readers(
        targets
            .into_iter()
            .map(|target| (target, reader.clone()))
            .collect(),
    )
}

fn mount(state: Arc<PreviewState>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/finance/document-posting-preview",
            post(preview),
        )
        .with_state(state)
}

pub fn router(pool: DatabasePool, targets: Vec<LaneTarget>) -> Result<Router, BusinessError> {
    router_with_reader(Arc::new(PgDocPreviewReader::new(pool)), targets)
}

/// One Pg-backed preview reader per registered Holding pool.
pub fn router_for_holdings(
    targets: Vec<(LaneTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    let entries = targets
        .into_iter()
        .map(|(target, pool)| {
            let reader: PreviewReader = Arc::new(PgDocPreviewReader::new(pool));
            (target, reader)
        })
        .collect();
    Ok(mount(state_with_readers(entries)?))
}

/// Injectable variant retained for owner tests.
pub fn router_with_reader(
    reader: PreviewReader,
    targets: Vec<LaneTarget>,
) -> Result<Router, BusinessError> {
    Ok(mount(state(targets, reader)?))
}

async fn preview(
    State(state): State<Arc<PreviewState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let result = async {
        let host = unique_header(request.headers(), header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(missing)?
            .to_owned();
        let company = unique_header(request.headers(), CONTEXT_COMPANY)?
            .filter(|value| valid_id(value))
            .ok_or_else(invalid)?
            .to_owned();
        let binding = state
            .targets
            .get(&(host, slug.clone()))
            .ok_or_else(missing)?;
        if company != binding.company_id {
            return Err(invalid());
        }
        let bytes = to_bytes(request.into_body(), MAX_BODY)
            .await
            .map_err(|_| invalid())?;
        let body: PreviewRequest = serde_json::from_slice(&bytes).map_err(|_| invalid())?;
        let kind = DocPreviewKind::parse(&body.kind)?;
        if kind.needs_journal() != body.journal_id.is_some() {
            return Err(invalid());
        }
        let service = DocPreviewService::new(Arc::clone(&binding.reader));
        let projection = service
            .read(
                &CompanyScope {
                    company_id: company,
                },
                kind,
                body.journal_id.as_deref(),
            )
            .await?;
        Ok::<_, BusinessError>(json!({
            "data": {"kind": kind.as_str(), "rows": projection.into_rows()}
        }))
    }
    .await;
    match result {
        Ok(body) => bounded(body),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::super::application::DocPreviewFuture;
    use super::*;

    #[derive(Default)]
    struct Probe;

    impl DocPreviewReader for Probe {
        fn accounts<'a>(
            &'a self,
            _scope: &'a CompanyScope,
        ) -> DocPreviewFuture<'a, Vec<super::super::domain::PreviewAccountRow>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn journal_lines<'a>(
            &'a self,
            _scope: &'a CompanyScope,
            _journal_id: &'a str,
        ) -> DocPreviewFuture<'a, Vec<super::super::domain::PreviewJournalLineRow>> {
            Box::pin(async { Ok(vec![]) })
        }
        fn stock_configuration<'a>(
            &'a self,
            _scope: &'a CompanyScope,
        ) -> DocPreviewFuture<'a, Option<super::super::domain::PreviewStockConfigurationRow>>
        {
            Box::pin(async { Ok(None) })
        }
    }

    #[test]
    fn two_holdings_bind_to_distinct_preview_readers() {
        let acme: PreviewReader = Arc::new(Probe);
        let northstar: PreviewReader = Arc::new(Probe);
        let state = state_with_readers(vec![
            (
                LaneTarget {
                    host: "acme.example.test".into(),
                    holding_slug: "acme".into(),
                    company_id: "acme-co".into(),
                },
                acme.clone(),
            ),
            (
                LaneTarget {
                    host: "northstar.example.test".into(),
                    holding_slug: "northstar".into(),
                    company_id: "northstar-co".into(),
                },
                northstar.clone(),
            ),
        ])
        .expect("state");
        assert_eq!(state.targets.len(), 2);
        let a = &state.targets[&("acme.example.test".to_owned(), "acme".to_owned())].reader;
        let n =
            &state.targets[&("northstar.example.test".to_owned(), "northstar".to_owned())].reader;
        assert!(Arc::ptr_eq(a, &acme));
        assert!(Arc::ptr_eq(n, &northstar));
        assert!(!Arc::ptr_eq(a, n));
        assert!(router_for_holdings(Vec::new()).is_err());
    }

    #[test]
    fn request_shape_is_strict() {
        let parsed: PreviewRequest =
            serde_json::from_slice(br#"{"kind":"doc-accounts"}"#).expect("bounded");
        assert!(parsed.journal_id.is_none());
        assert_eq!(
            DocPreviewKind::parse(&parsed.kind),
            Ok(DocPreviewKind::Accounts)
        );
        let injected = br#"{"kind":"doc-accounts","sql":"SELECT 1"}"#;
        assert!(serde_json::from_slice::<PreviewRequest>(injected).is_err());
    }
}
