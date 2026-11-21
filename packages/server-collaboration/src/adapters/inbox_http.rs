//! Native HTTP read ingress for the workplace Inbox.
//!
//! The Inbox summary/page joins Work, Records, Mentions and Notices. Those
//! owner reads remain composition ports; this router only validates the exact
//! registered Holding host/slug and bounded list query, then returns the
//! composition backend's authorized rows. Write operations (notice broadcast
//! and delivery state) still require the Workflow ledger and stay with the
//! retained TypeScript consumer for now.
use crate::adapters::http_ingress::{bounded, error_response, unique_header, valid_origin};
use erpx_server_runtime::public::http_support::{
    HeaderMap, Path, Query, Request, Response, Router, State, header, routing::get,
};
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const DEFAULT_PAGE: usize = 30;

pub type InboxHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxItemData {
    pub id: String,
    pub event_id: String,
    pub category: String,
    pub message: String,
    pub href: Option<String>,
    pub created_at: i64,
    pub read_at: Option<i64>,
    pub archived_at: Option<i64>,
    pub source_type: String,
    pub source_id: String,
    pub source: Option<Value>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InboxPageData {
    pub items: Vec<InboxItemData>,
    #[serde(rename = "nextCursor")]
    pub next_cursor: Option<String>,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
}

#[derive(Clone, Debug, Default, Serialize)]
pub struct InboxCategoryCounts {
    pub work: i64,
    pub calendar: i64,
    pub mention: i64,
    pub notice: i64,
    pub system: i64,
}

#[derive(Clone, Debug, Serialize)]
pub struct InboxSummaryData {
    pub unread: i64,
    pub categories: InboxCategoryCounts,
}

#[derive(Clone)]
pub enum InboxReadKind {
    Summary,
    Notifications {
        view: Option<String>,
        category: Option<String>,
        search: String,
        cursor: Option<String>,
        limit: usize,
    },
    People {
        query: String,
    },
    Notices,
    Notice {
        id: String,
    },
}

pub struct InboxHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: InboxReadKind,
}

pub enum InboxHttpReadReply {
    Summary(Box<InboxSummaryData>),
    Page(Box<InboxPageData>),
    Notices(Value),
    Notice(Value),
    People(Value),
}

pub trait InboxHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: InboxHttpReadRequest<'a>,
    ) -> InboxHttpFuture<'a, InboxHttpReadReply>;
}

#[derive(Clone)]
pub struct InboxHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn InboxHttpBackend>,
}

pub struct InboxHttpState {
    targets: BTreeMap<(String, String), InboxHttpTarget>,
}

impl InboxHttpState {
    /// Validate exact distinct registered Holding targets.
    pub fn new(targets: Vec<InboxHttpTarget>) -> Result<Self, BusinessError> {
        let mut configured = BTreeMap::new();
        for target in targets {
            if target.host.is_empty()
                || target.holding_slug.is_empty()
                || !valid_origin(&target.host, &target.origin)
                || configured
                    .insert((target.host.clone(), target.holding_slug.clone()), target)
                    .is_some()
            {
                return Err(BusinessError::invalid(
                    "INBOX_HTTP_CONFIGURATION_INVALID",
                    "Use exact distinct registered Holding targets.",
                ));
            }
        }
        Ok(Self {
            targets: configured,
        })
    }
}

#[derive(Deserialize)]
pub struct InboxQuery {
    #[serde(default)]
    pub view: Option<String>,
    #[serde(default)]
    pub category: Option<String>,
    #[serde(default)]
    pub search: Option<String>,
    #[serde(default)]
    pub cursor: Option<String>,
    #[serde(default)]
    pub limit: Option<String>,
    #[serde(default)]
    pub q: Option<String>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn inbox_router(targets: Vec<InboxHttpTarget>) -> Result<Router, BusinessError> {
    let state = InboxHttpState::new(targets)?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/inbox/summary",
            get(read_summary),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/inbox/notifications",
            get(read_notifications),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/inbox/people",
            get(read_people),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/inbox/notices",
            get(read_notices),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/inbox/notices/{notice_id}",
            get(read_notice),
        )
        .with_state(Arc::new(state)))
}

fn target<'a>(
    state: &'a InboxHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a InboxHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn read_limit(value: Option<&str>) -> Result<usize, BusinessError> {
    match value {
        None => Ok(DEFAULT_PAGE),
        Some(raw) => raw
            .parse::<usize>()
            .ok()
            .filter(|value| (1..=100).contains(value))
            .ok_or_else(|| BusinessError::invalid("INBOX_LIMIT_INVALID", "Choose a smaller page.")),
    }
}

fn bounded_text(
    value: Option<&str>,
    max: usize,
    code: &'static str,
) -> Result<Option<String>, BusinessError> {
    match value {
        None => Ok(None),
        Some(value) if value.encode_utf16().count() <= max => Ok(Some(value.to_owned())),
        Some(_) => Err(BusinessError::invalid(code, "Shorten the Inbox selection.")),
    }
}

fn category(value: Option<&str>) -> Result<Option<String>, BusinessError> {
    match value {
        None | Some("") => Ok(None),
        Some(value @ ("work" | "calendar" | "mention" | "notice" | "system")) => {
            Ok(Some(value.to_owned()))
        }
        Some(_) => Err(BusinessError::invalid(
            "INBOX_CATEGORY_INVALID",
            "Choose a valid Inbox category.",
        )),
    }
}

pub async fn read_summary(
    State(state): State<Arc<InboxHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    read(state, slug, InboxReadKind::Summary, request).await
}

pub async fn read_notifications(
    State(state): State<Arc<InboxHttpState>>,
    Path(slug): Path<String>,
    Query(query): Query<InboxQuery>,
    request: Request,
) -> Response {
    let kind = match (|| {
        Ok::<_, BusinessError>(InboxReadKind::Notifications {
            view: bounded_text(query.view.as_deref(), 40, "INBOX_VIEW_INVALID")?,
            category: category(query.category.as_deref())?,
            search: query
                .search
                .as_deref()
                .unwrap_or("")
                .trim()
                .chars()
                .take(100)
                .collect(),
            cursor: bounded_text(query.cursor.as_deref(), 512, "INVALID_CURSOR")?,
            limit: read_limit(query.limit.as_deref())?,
        })
    })() {
        Ok(kind) => kind,
        Err(error) => return error_response(error),
    };
    read(state, slug, kind, request).await
}

pub async fn read_people(
    State(state): State<Arc<InboxHttpState>>,
    Path(slug): Path<String>,
    Query(query): Query<InboxQuery>,
    request: Request,
) -> Response {
    let search = query.q.as_deref().unwrap_or("").trim();
    let kind = InboxReadKind::People {
        query: search.chars().take(160).collect(),
    };
    read(state, slug, kind, request).await
}

pub async fn read_notices(
    State(state): State<Arc<InboxHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    read(state, slug, InboxReadKind::Notices, request).await
}

pub async fn read_notice(
    State(state): State<Arc<InboxHttpState>>,
    Path((slug, notice_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    read(
        state,
        slug,
        InboxReadKind::Notice { id: notice_id },
        request,
    )
    .await
}

async fn read(
    state: Arc<InboxHttpState>,
    slug: String,
    kind: InboxReadKind,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        target
            .backend
            .read(InboxHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind,
            })
            .await
    }
    .await;
    match result {
        Ok(InboxHttpReadReply::Summary(value)) => bounded(*value),
        Ok(InboxHttpReadReply::Page(value)) => bounded(*value),
        Ok(InboxHttpReadReply::Notices(value))
        | Ok(InboxHttpReadReply::Notice(value))
        | Ok(InboxHttpReadReply::People(value)) => bounded(value),
        Err(error) => error_response(error),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::{
        StatusCode,
        body::{Body, to_bytes},
    };
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeBackend {
        reads: Mutex<Vec<(String, String, &'static str)>>,
        failure: Option<BusinessError>,
    }

    impl InboxHttpBackend for FakeBackend {
        fn read<'a>(
            &'a self,
            request: InboxHttpReadRequest<'a>,
        ) -> InboxHttpFuture<'a, InboxHttpReadReply> {
            let kind = match &request.kind {
                InboxReadKind::Summary => "summary",
                InboxReadKind::Notifications { .. } => "notifications",
                InboxReadKind::People { .. } => "people",
                InboxReadKind::Notices => "notices",
                InboxReadKind::Notice { .. } => "notice",
            };
            self.reads.lock().unwrap().push((
                request.host.into(),
                request.holding_slug.into(),
                kind,
            ));
            Box::pin(async move {
                if let Some(error) = &self.failure {
                    return Err(error.clone());
                }
                Ok(match request.kind {
                    InboxReadKind::Summary => {
                        InboxHttpReadReply::Summary(Box::new(InboxSummaryData {
                            unread: 2,
                            categories: InboxCategoryCounts {
                                mention: 2,
                                ..InboxCategoryCounts::default()
                            },
                        }))
                    }
                    InboxReadKind::Notifications { .. } => {
                        InboxHttpReadReply::Page(Box::new(InboxPageData {
                            items: vec![InboxItemData {
                                id: "delivery-1".into(),
                                event_id: "event-1".into(),
                                category: "mention".into(),
                                message: "You were mentioned".into(),
                                href: Some("/feed".into()),
                                created_at: 5,
                                read_at: None,
                                archived_at: None,
                                source_type: "social_comment".into(),
                                source_id: "comment-1".into(),
                                source: None,
                            }],
                            next_cursor: Some("cursor-1".into()),
                            has_more: true,
                        }))
                    }
                    InboxReadKind::People { .. } => {
                        InboxHttpReadReply::People(json!([{"id":"user-1","name":"User"}]))
                    }
                    InboxReadKind::Notices => InboxHttpReadReply::Notices(json!([])),
                    InboxReadKind::Notice { id } => {
                        InboxHttpReadReply::Notice(json!({"id": id, "acceptedCount": 1}))
                    }
                })
            })
        }
    }

    fn make_target(backend: Arc<FakeBackend>) -> InboxHttpTarget {
        InboxHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    fn state(target: InboxHttpTarget) -> Arc<InboxHttpState> {
        Arc::new(InboxHttpState {
            targets: BTreeMap::from([((target.host.clone(), target.holding_slug.clone()), target)]),
        })
    }

    fn query() -> InboxQuery {
        InboxQuery {
            view: None,
            category: None,
            search: None,
            cursor: None,
            limit: None,
            q: None,
        }
    }

    fn get_request() -> Request {
        Request::builder()
            .method("GET")
            .header("host", "acme.example.test")
            .body(Body::empty())
            .unwrap()
    }

    async fn body(response: Response) -> (StatusCode, Value) {
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        (status, serde_json::from_slice(&bytes).unwrap())
    }

    #[test]
    fn router_rejects_ambiguous_or_unregistered_targets() {
        let backend = Arc::new(FakeBackend::default());
        let mut target = make_target(backend.clone());
        target.origin = "http://acme.example.test".into();
        assert!(inbox_router(vec![target]).is_err());
        assert!(inbox_router(vec![make_target(backend.clone()), make_target(backend)]).is_err());
        assert!(inbox_router(vec![make_target(Arc::new(FakeBackend::default()))]).is_ok());
    }

    #[tokio::test]
    async fn summary_and_page_reads_bind_the_exact_host_and_holding() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(make_target(backend.clone()));
        let response = read_summary(State(state.clone()), Path("acme".into()), get_request()).await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["unread"], 2);
        assert_eq!(value["data"]["categories"]["mention"], 2);
        let response = read_notifications(
            State(state.clone()),
            Path("acme".into()),
            Query(InboxQuery {
                category: Some("mention".into()),
                search: Some(" mentioned ".into()),
                ..query()
            }),
            get_request(),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["items"][0]["id"], "delivery-1");
        assert_eq!(value["data"]["nextCursor"], "cursor-1");
        assert_eq!(backend.reads.lock().unwrap().len(), 2);
        let response = read_summary(State(state), Path("other".into()), get_request()).await;
        assert_eq!(body(response).await.0, StatusCode::NOT_FOUND);
    }

    #[tokio::test]
    async fn invalid_filters_are_rejected_before_the_backend() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(make_target(backend.clone()));
        for query in [
            InboxQuery {
                category: Some("bogus".into()),
                ..query()
            },
            InboxQuery {
                limit: Some("0".into()),
                ..query()
            },
            InboxQuery {
                limit: Some("101".into()),
                ..query()
            },
            InboxQuery {
                cursor: Some("x".repeat(513)),
                ..query()
            },
        ] {
            let response = read_notifications(
                State(state.clone()),
                Path("acme".into()),
                Query(query),
                get_request(),
            )
            .await;
            assert_eq!(body(response).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        }
        assert!(backend.reads.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn notices_people_and_owner_denials_pass_through() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(make_target(backend.clone()));
        let response = read_notice(
            State(state.clone()),
            Path(("acme".into(), "notice-1".into())),
            get_request(),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["id"], "notice-1");
        let response = read_people(
            State(state),
            Path("acme".into()),
            Query(InboxQuery {
                q: Some("User".into()),
                ..query()
            }),
            get_request(),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::OK);

        let denied = Arc::new(FakeBackend {
            failure: Some(BusinessError::new(
                FailureKind::Forbidden,
                "MEMBERSHIP_REQUIRED",
                "denied",
            )),
            ..FakeBackend::default()
        });
        let response = read_notices(
            State(state_target(denied)),
            Path("acme".into()),
            get_request(),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::FORBIDDEN);
        assert_eq!(value["error"]["code"], "MEMBERSHIP_REQUIRED");
    }

    fn state_target(backend: Arc<FakeBackend>) -> Arc<InboxHttpState> {
        state(make_target(backend))
    }
}
