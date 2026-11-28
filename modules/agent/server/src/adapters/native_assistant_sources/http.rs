//! Owner-local axum ingress for the native assistant source read. Host/Holding
//! targets, bounded context headers, the one query key and the response envelope
//! are owned here; SQL execution lives in `postgres`.

use super::application::{AssistantSourceQuery, AssistantSourcesRead, assistant_sources};
use super::domain::invalid;
use super::postgres::PgAssistantSourcesRead;
use erpx_platform_runtime_server::public::http_support::{
    HeaderMap, HeaderName, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State,
    StatusCode, header, routing::get, serde_json,
};
use erpx_platform_runtime_server::public::postgres::DatabasePool;
use erpx_platform_shared_server::public::{BusinessError, FailureKind};
use std::{collections::BTreeMap, sync::Arc};

const COMPANY: HeaderName = HeaderName::from_static("x-erpx-context-company");
const USER: HeaderName = HeaderName::from_static("x-erpx-context-user");
const ASSIGNMENT: HeaderName = HeaderName::from_static("x-erpx-context-assignment");
const SCOPE: HeaderName = HeaderName::from_static("x-erpx-context-scope");

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct AssistantSourcesTarget {
    pub host: String,
    pub holding_slug: String,
}

struct SourceState {
    targets: BTreeMap<(String, String), Arc<dyn AssistantSourcesRead>>,
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

fn bounded(value: serde_json::Value) -> Response {
    let mut response = (StatusCode::OK, Json(value)).into_response();
    for (name, item) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::REFERRER_POLICY, "no-referrer"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
    ] {
        response
            .headers_mut()
            .insert(name, HeaderValue::from_static(item));
    }
    response
}

fn valid_slug(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 128
        && value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
}

fn unique(headers: &HeaderMap, name: HeaderName) -> Result<Option<&str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(invalid());
    }
    first
        .map(|value| value.to_str().map_err(|_| invalid()))
        .transpose()
}

fn header_value(headers: &HeaderMap, name: HeaderName) -> Result<String, BusinessError> {
    unique(headers, name)?.filter(|value| !value.is_empty()).map(str::to_owned).ok_or_else(invalid)
}

/// `URLSearchParams.get("path")` semantics: exactly one accepted key; unknown
/// keys are ignored, duplicates fail closed.
fn query_path(query: Option<&str>) -> Result<String, BusinessError> {
    let mut found = None;
    for pair in query.unwrap_or("").split('&').filter(|pair| !pair.is_empty()) {
        let (key, value) = pair.split_once('=').unwrap_or((pair, ""));
        if key == "path" {
            if found.replace(value.to_owned()).is_some() {
                return Err(invalid());
            }
        }
    }
    found.ok_or_else(invalid)
}

fn configure(
    entries: impl IntoIterator<Item = (AssistantSourcesTarget, Arc<dyn AssistantSourcesRead>)>,
) -> Result<BTreeMap<(String, String), Arc<dyn AssistantSourcesRead>>, BusinessError> {
    let mut targets = BTreeMap::new();
    for (target, reader) in entries {
        if target.host.is_empty()
            || target.host.len() > 255
            || !valid_slug(&target.holding_slug)
            || targets
                .insert((target.host.clone(), target.holding_slug.clone()), reader)
                .is_some()
        {
            return Err(BusinessError::invalid(
                "AGENT_ASSISTANT_SOURCES_CONFIG_INVALID",
                "Use exact distinct registered Holding targets.",
            ));
        }
    }
    if targets.is_empty() {
        return Err(BusinessError::invalid(
            "AGENT_ASSISTANT_SOURCES_CONFIG_INVALID",
            "Use exact distinct registered Holding targets.",
        ));
    }
    Ok(targets)
}

fn finish(targets: BTreeMap<(String, String), Arc<dyn AssistantSourcesRead>>) -> Router {
    Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/assistant/sources",
            get(sources),
        )
        .with_state(Arc::new(SourceState { targets }))
}

/// Native router with one Pg owner reader per registered Holding pool.
pub fn router_for_holdings(
    targets: Vec<(AssistantSourcesTarget, DatabasePool)>,
) -> Result<Router, BusinessError> {
    Ok(finish(configure(targets.into_iter().map(|(target, pool)| {
        (
            target,
            Arc::new(PgAssistantSourcesRead::new(pool)) as Arc<dyn AssistantSourcesRead>,
        )
    }))?))
}

/// Reader-injection path used by owner-local contract tests.
pub fn router_with_reader(
    reader: Arc<dyn AssistantSourcesRead>,
    targets: Vec<AssistantSourcesTarget>,
) -> Result<Router, BusinessError> {
    Ok(finish(configure(
        targets.into_iter().map(|target| (target, reader.clone())),
    )?))
}

async fn sources(
    State(state): State<Arc<SourceState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let query = request.uri().query().map(str::to_owned);
    let result: Result<serde_json::Value, BusinessError> = async {
        let host = unique(&headers, header::HOST)?
            .filter(|value| !value.is_empty())
            .ok_or_else(invalid)?;
        let reader = state
            .targets
            .get(&(host.to_owned(), slug))
            .ok_or_else(|| {
                BusinessError::new(
                    FailureKind::NotFound,
                    "NOT_FOUND",
                    "Not found.",
                )
            })?
            .clone();
        let input = AssistantSourceQuery::new(
            &header_value(&headers, COMPANY)?,
            &header_value(&headers, USER)?,
            &header_value(&headers, ASSIGNMENT)?,
            &header_value(&headers, SCOPE)?,
            &query_path(query.as_deref())?,
        )?;
        assistant_sources(&input, reader.as_ref()).await
    }
    .await;
    match result {
        Ok(value) => bounded(value),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_one_path_key_is_accepted() {
        assert_eq!(query_path(Some("cursor=3&path=%2Fhome")).unwrap(), "%2Fhome");
        assert!(query_path(Some("path=%2Fhome&path=%2Fwork")).is_err());
        assert!(query_path(None).is_err());
    }

    #[test]
    fn targets_must_be_exact_and_distinct() {
        struct Noop;
        impl AssistantSourcesRead for Noop {
            fn saved<'a>(
                &'a self,
                _: &'a AssistantSourceQuery,
            ) -> super::super::application::AssistantSourcesFuture<'a, Vec<super::super::domain::AssistantSourceRef>>
            {
                Box::pin(async { Ok(vec![]) })
            }
        }
        let reader: Arc<dyn AssistantSourcesRead> = Arc::new(Noop);
        let target = || AssistantSourcesTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
        };
        assert!(router_with_reader(reader.clone(), vec![target()]).is_ok());
        assert!(router_with_reader(reader, vec![target(), target()]).is_err());
    }
}
