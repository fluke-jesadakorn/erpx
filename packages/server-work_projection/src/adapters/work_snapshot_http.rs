//! Owner-local HTTP ingress for the native Work snapshot read.
//!
//! Transport facts only: registered Host/Holding target, exact origin, bounded
//! query and business-error mapping. The Account session, current work
//! authority and the retained serializable Holding transaction stay
//! composition responsibilities supplied through the backend trait.
use crate::adapters::work_snapshot_read::NativeWorkSnapshot;
use crate::domain::work_card_availability::WorkQueue;
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, IntoResponse, Json, Path, Request, Response, Router, State, StatusCode,
    get, header, serde_json,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde_json::{Value, json};
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type WorkSnapshotHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

pub struct WorkSnapshotHttpRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
}

#[derive(Clone, Debug)]
pub struct WorkSnapshotHttpResponse {
    /// Native availability snapshot `{items, unavailableSources}`.
    pub data: Value,
    /// Validated current context rendered by the composition backend.
    pub context: Value,
}

/// Composition owns the Account session, current work role and PostgreSQL unit.
pub trait WorkSnapshotHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: WorkSnapshotHttpRequest<'a>,
    ) -> WorkSnapshotHttpFuture<'a, WorkSnapshotHttpResponse>;
}

pub struct WorkSnapshotHttpTarget {
    pub host: String,
    pub holding_slug: String,
    /// Exact browser origin for this registered Holding.
    pub origin: String,
    pub backend: Arc<dyn WorkSnapshotHttpBackend>,
}

pub struct WorkSnapshotHttpState {
    targets: BTreeMap<(String, String), WorkSnapshotHttpTarget>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn input() -> BusinessError {
    BusinessError::invalid("VALIDATION_FAILED", "Use the current Work snapshot route.")
}

fn origin_forbidden() -> BusinessError {
    BusinessError::new(
        FailureKind::Forbidden,
        "ORIGIN_FORBIDDEN",
        "Use the application origin.",
    )
}

fn unique_header<'a>(headers: &'a HeaderMap, name: &str) -> Result<Option<&'a str>, BusinessError> {
    let mut values = headers.get_all(name).iter();
    let first = values.next();
    if values.next().is_some() {
        return Err(input());
    }
    first
        .map(|value| value.to_str().map_err(|_| input()))
        .transpose()
}

fn valid_origin(origin: &str) -> bool {
    let Some((scheme, authority)) = origin.split_once("://") else {
        return false;
    };
    if !matches!(scheme, "https" | "http") || authority.is_empty() || authority.len() > 255 {
        return false;
    }
    let (host, port) = match authority.rsplit_once(':') {
        Some((host, port)) => (host, Some(port)),
        None => (authority, None),
    };
    if host.is_empty()
        || host.len() > 253
        || host.starts_with('.')
        || host.ends_with('.')
        || !host.split('.').all(|label| {
            !label.is_empty()
                && label.len() <= 63
                && label
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                && !label.starts_with('-')
                && !label.ends_with('-')
        })
    {
        return false;
    }
    match port {
        None => true,
        Some(port) => {
            !port.is_empty()
                && port.len() <= 5
                && port.bytes().all(|byte| byte.is_ascii_digit())
                && port.parse::<u16>().is_ok_and(|value| value > 0)
        }
    }
}

fn secured(mut response: Response) -> Response {
    response.headers_mut().insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("private, no-store"),
    );
    response.headers_mut().insert(
        header::REFERRER_POLICY,
        HeaderValue::from_static("no-referrer"),
    );
    response.headers_mut().insert(
        header::X_CONTENT_TYPE_OPTIONS,
        HeaderValue::from_static("nosniff"),
    );
    response
}

fn json_response(status: StatusCode, value: Value) -> Response {
    let mut response = (status, Json(value)).into_response();
    response.headers_mut().insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("application/json"),
    );
    secured(response)
}

fn error_response(error: BusinessError) -> Response {
    let status = match error.kind {
        FailureKind::InvalidInput => StatusCode::UNPROCESSABLE_ENTITY,
        FailureKind::Unauthenticated => StatusCode::UNAUTHORIZED,
        FailureKind::Forbidden => StatusCode::FORBIDDEN,
        FailureKind::NotFound => StatusCode::NOT_FOUND,
        FailureKind::Conflict => StatusCode::CONFLICT,
        FailureKind::Unavailable => StatusCode::SERVICE_UNAVAILABLE,
    };
    json_response(
        status,
        json!({"error": {"code": error.code, "message": error.message}}),
    )
}

fn queue_name(queue: WorkQueue) -> &'static str {
    match queue {
        WorkQueue::Todo => "todo",
        WorkQueue::Waiting => "waiting",
        WorkQueue::Completed => "completed",
    }
}

/// Serializes the native availability facts with the retained `WorkSnapshot`
/// field names. Presentation fields (`title`, `detail`, `status`, `flow`) are
/// not part of the native facts and are never synthesized here.
pub fn native_work_snapshot_json(snapshot: &NativeWorkSnapshot) -> Value {
    json!({
        "items": snapshot
            .items
            .iter()
            .map(|item| json!({
                "id": item.id,
                "type": item.kind,
                "queue": queue_name(item.queue),
                "canAct": item.can_act,
                "actionKind": item.action_kind,
                "relationships": item.relationships,
                "updatedAt": item.updated_at,
                "sourceId": item.source_id,
                "href": item.href,
            }))
            .collect::<Vec<_>>(),
        "unavailableSources": snapshot.unavailable_sources,
    })
}

impl WorkSnapshotHttpState {
    pub fn new(targets: Vec<WorkSnapshotHttpTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || target.holding_slug.is_empty()
                || target.holding_slug.len() > 128
                || !target.holding_slug.bytes().all(|byte| {
                    byte.is_ascii_lowercase()
                        || byte.is_ascii_digit()
                        || matches!(byte, b'_' | b'-')
                })
                || !valid_origin(&target.origin)
                || configured
                    .insert((target.host.clone(), target.holding_slug.clone()), target)
                    .is_some()
            {
                return Err(BusinessError::invalid(
                    "WORK_SNAPSHOT_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        Ok(Self {
            targets: configured,
        })
    }

    fn target<'a>(
        &'a self,
        headers: &'a HeaderMap,
        holding_slug: &str,
    ) -> Result<&'a WorkSnapshotHttpTarget, BusinessError> {
        let host = unique_header(headers, "host")?.ok_or_else(missing)?;
        let target = self
            .targets
            .get(&(host.to_owned(), holding_slug.to_owned()))
            .ok_or_else(missing)?;
        let origin = unique_header(headers, "origin")?;
        if origin.is_some_and(|value| value != target.origin) {
            return Err(origin_forbidden());
        }
        Ok(target)
    }

    pub async fn read(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Response {
        match self.read_inner(headers, holding_slug, query).await {
            Ok(response) => response,
            Err(error) => error_response(error),
        }
    }

    async fn read_inner(
        &self,
        headers: &HeaderMap,
        holding_slug: &str,
        query: Option<&str>,
    ) -> Result<Response, BusinessError> {
        if query.is_some() {
            return Err(input());
        }
        let target = self.target(headers, holding_slug)?;
        let response = target
            .backend
            .read(WorkSnapshotHttpRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers,
            })
            .await?;
        Ok(json_response(
            StatusCode::OK,
            json!({"data": response.data, "context": response.context}),
        ))
    }
}

async fn read_handler(
    State(state): State<Arc<WorkSnapshotHttpState>>,
    Path(holding_slug): Path<String>,
    request: Request,
) -> Response {
    let query = request.uri().query().map(str::to_owned);
    state
        .read(request.headers(), &holding_slug, query.as_deref())
        .await
}

pub fn work_snapshot_router(targets: Vec<WorkSnapshotHttpTarget>) -> Result<Router, BusinessError> {
    let state = Arc::new(WorkSnapshotHttpState::new(targets)?);
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/work/native-snapshot",
            get(read_handler),
        )
        .with_state(state))
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::domain::work_card_availability::WorkItemFact;
    use std::sync::Mutex;

    struct Fake {
        seen: Mutex<Vec<String>>,
    }

    impl WorkSnapshotHttpBackend for Fake {
        fn read<'a>(
            &'a self,
            request: WorkSnapshotHttpRequest<'a>,
        ) -> WorkSnapshotHttpFuture<'a, WorkSnapshotHttpResponse> {
            Box::pin(async move {
                self.seen
                    .lock()
                    .unwrap()
                    .push(format!("read:{}:{}", request.host, request.holding_slug));
                Ok(WorkSnapshotHttpResponse {
                    data: json!({
                        "items": [{
                            "id": "maintenance:work",
                            "type": "maintenance_work",
                            "queue": "todo",
                            "canAct": true,
                            "actionKind": null,
                            "relationships": ["assigned"],
                            "updatedAt": 1,
                            "sourceId": "work",
                            "href": "/maintenance/work"
                        }],
                        "unavailableSources": ["supportWork"]
                    }),
                    context: json!({"companyId": "acme-co"}),
                })
            })
        }
    }

    fn state() -> (WorkSnapshotHttpState, Arc<Fake>) {
        let fake = Arc::new(Fake {
            seen: Mutex::new(Vec::new()),
        });
        let state = WorkSnapshotHttpState::new(vec![WorkSnapshotHttpTarget {
            host: "holding.erpxbrowser.test:3102".into(),
            holding_slug: "acme".into(),
            origin: "http://127.0.0.1:3102".into(),
            backend: fake.clone(),
        }])
        .unwrap();
        (state, fake)
    }

    fn block_on<F: std::future::Future>(future: F) -> F::Output {
        tokio::runtime::Runtime::new().unwrap().block_on(future)
    }

    fn headers(origin: Option<&str>) -> HeaderMap {
        let mut headers = HeaderMap::new();
        headers.insert("host", "holding.erpxbrowser.test:3102".parse().unwrap());
        if let Some(origin) = origin {
            headers.insert("origin", origin.parse().unwrap());
        }
        headers
    }

    #[test]
    fn read_forwards_the_exact_target_and_returns_only_facts() {
        let (state, fake) = state();
        let response = block_on(state.read(&headers(Some("http://127.0.0.1:3102")), "acme", None));
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            fake.seen.lock().unwrap().as_slice(),
            ["read:holding.erpxbrowser.test:3102:acme"]
        );
    }

    #[test]
    fn rejects_wrong_origin_unknown_holding_and_query() {
        let (state, _) = state();
        assert_eq!(
            block_on(state.read(&headers(Some("https://evil.test")), "acme", None)).status(),
            StatusCode::FORBIDDEN
        );
        assert_eq!(
            block_on(state.read(&headers(Some("http://127.0.0.1:3102")), "other", None)).status(),
            StatusCode::NOT_FOUND
        );
        assert_eq!(
            block_on(state.read(
                &headers(Some("http://127.0.0.1:3102")),
                "acme",
                Some("include=attention")
            ))
            .status(),
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }

    #[test]
    fn serializes_the_retained_snapshot_names_without_presentation_fields() {
        let snapshot = NativeWorkSnapshot {
            items: vec![WorkItemFact {
                id: "maintenance:work".into(),
                kind: "maintenance_work".into(),
                queue: WorkQueue::Todo,
                can_act: Some(true),
                action_kind: None,
                relationships: vec!["assigned".into()],
                updated_at: 1,
                source_id: Some("work".into()),
                href: "/maintenance/work".into(),
            }],
            unavailable_sources: vec!["supportWork"],
        };
        let value = native_work_snapshot_json(&snapshot);
        assert_eq!(value["items"][0]["type"], "maintenance_work");
        assert_eq!(value["items"][0]["queue"], "todo");
        assert_eq!(value["unavailableSources"][0], "supportWork");
        assert!(value["items"][0].get("title").is_none());
    }

    #[test]
    fn router_builds_and_rejects_duplicate_targets() {
        let (_, fake) = state();
        let target = || WorkSnapshotHttpTarget {
            host: "holding.erpxbrowser.test:3102".into(),
            holding_slug: "acme".into(),
            origin: "http://127.0.0.1:3102".into(),
            backend: fake.clone(),
        };
        assert!(work_snapshot_router(vec![target()]).is_ok());
        assert!(work_snapshot_router(vec![target(), target()]).is_err());
    }
}
