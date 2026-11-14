//! Native HTTP ingress for Collaboration action cards, record relations and
//! the workplace inbox reads that reference them.
//!
//! Transport facts only. Session resolution, current records permission, module
//! access, source witnesses and the serializable owner unit belong to the
//! composition backend, which must implement [`CollaborationWorkHttpBackend`].
use crate::adapters::collaboration_work_wire::decode_work_command;
use crate::adapters::http_ingress::{
    bounded, command_key, error_response, json_body, unique_header, valid_origin,
};
use crate::domain::collaboration_work::{Change, Receipt};
use erpx_server_runtime::public::http_support::{
    HeaderMap, Path, Request, Response, Router, State, header, routing::get,
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

pub type CollaborationWorkHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollaborationCardData {
    /// Composition-supplied current access facts; never a replacement for the
    /// owner authorization performed in the backend.
    pub access: Value,
    pub card: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollaborationRelationData {
    pub access: Value,
    pub relation: Value,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CollaborationWorkspaceData {
    pub access: Value,
    pub relations: Vec<Value>,
    pub cards: Vec<Value>,
    pub people: Vec<Value>,
    pub source_types: Vec<String>,
}

#[derive(Clone)]
pub enum CollaborationWorkReadKind {
    Workspace,
    Card { id: String },
    Relation { id: String },
}

pub struct CollaborationWorkHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: CollaborationWorkReadKind,
}

pub struct CollaborationWorkHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub change: Change,
    pub input: Value,
    pub key: CommandKey,
}

pub enum CollaborationWorkHttpReadReply {
    Workspace(Box<CollaborationWorkspaceData>),
    Card(Box<CollaborationCardData>),
    Relation(Box<CollaborationRelationData>),
}

pub trait CollaborationWorkHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: CollaborationWorkHttpReadRequest<'a>,
    ) -> CollaborationWorkHttpFuture<'a, CollaborationWorkHttpReadReply>;
    fn command<'a>(
        &'a self,
        request: CollaborationWorkHttpCommandRequest<'a>,
    ) -> CollaborationWorkHttpFuture<'a, Receipt>;
}

#[derive(Clone)]
pub struct CollaborationWorkHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn CollaborationWorkHttpBackend>,
}

pub struct CollaborationWorkHttpState {
    targets: BTreeMap<(String, String), CollaborationWorkHttpTarget>,
}

impl CollaborationWorkHttpState {
    /// Validate exact distinct registered Holding targets.
    pub fn new(targets: Vec<CollaborationWorkHttpTarget>) -> Result<Self, BusinessError> {
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
                    "COLLABORATION_HTTP_CONFIGURATION_INVALID",
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
pub struct ReadQuery {
    #[serde(default)]
    pub format: Option<String>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

pub fn collaboration_work_router(
    targets: Vec<CollaborationWorkHttpTarget>,
) -> Result<Router, BusinessError> {
    let state = CollaborationWorkHttpState::new(targets)?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/collaboration",
            get(read_workspace).post(command),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/collaboration/cards/{id}",
            get(read_card),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/collaboration/relations/{id}",
            get(read_relation),
        )
        .with_state(Arc::new(state)))
}

fn target<'a>(
    state: &'a CollaborationWorkHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a CollaborationWorkHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn check_origin(
    target: &CollaborationWorkHttpTarget,
    headers: &HeaderMap,
) -> Result<(), BusinessError> {
    match unique_header(headers, header::ORIGIN)? {
        Some(value) if value == target.origin => Ok(()),
        _ => Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        )),
    }
}

pub async fn read_workspace(
    State(state): State<Arc<CollaborationWorkHttpState>>,
    Path(slug): Path<String>,
    erpx_server_runtime::public::http_support::Query(query): erpx_server_runtime::public::http_support::Query<ReadQuery>,
    request: Request,
) -> Response {
    if query
        .format
        .as_deref()
        .is_some_and(|format| format != "json")
    {
        return error_response(BusinessError::new(
            FailureKind::Unavailable,
            "NATIVE_COLLABORATION_EXPORT_UNAVAILABLE",
            "A native collaboration export is not available.",
        ));
    }
    read(state, slug, CollaborationWorkReadKind::Workspace, request).await
}

pub async fn read_card(
    State(state): State<Arc<CollaborationWorkHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    read(state, slug, CollaborationWorkReadKind::Card { id }, request).await
}

pub async fn read_relation(
    State(state): State<Arc<CollaborationWorkHttpState>>,
    Path((slug, id)): Path<(String, String)>,
    request: Request,
) -> Response {
    read(
        state,
        slug,
        CollaborationWorkReadKind::Relation { id },
        request,
    )
    .await
}

async fn read(
    state: Arc<CollaborationWorkHttpState>,
    slug: String,
    kind: CollaborationWorkReadKind,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        target
            .backend
            .read(CollaborationWorkHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind,
            })
            .await
    }
    .await;
    match result {
        Ok(CollaborationWorkHttpReadReply::Workspace(value)) => bounded(*value),
        Ok(CollaborationWorkHttpReadReply::Card(value)) => bounded(*value),
        Ok(CollaborationWorkHttpReadReply::Relation(value)) => bounded(*value),
        Err(error) => error_response(error),
    }
}

fn command_operation(raw: &Value) -> Result<(String, Option<String>), BusinessError> {
    let entity = raw.get("entity").and_then(Value::as_str).ok_or_else(|| {
        BusinessError::invalid(
            "COLLABORATION_INPUT_INVALID",
            "Choose a collaboration command.",
        )
    })?;
    match entity {
        "relation" => {
            if raw.get("action").and_then(Value::as_str) == Some("remove") {
                Ok((
                    "collaboration.relation.remove".into(),
                    Some(
                        raw.get("id")
                            .and_then(Value::as_str)
                            .filter(|value| !value.is_empty())
                            .ok_or_else(|| {
                                BusinessError::invalid(
                                    "COLLABORATION_INPUT_INVALID",
                                    "Choose a relation to remove.",
                                )
                            })?
                            .to_owned(),
                    ),
                ))
            } else {
                Ok(("collaboration.relation.create".into(), None))
            }
        }
        "card" => match raw.get("id").and_then(Value::as_str) {
            None => Ok(("collaboration.action.create".into(), None)),
            Some(id) if !id.is_empty() => {
                let action = raw
                    .get("action")
                    .and_then(Value::as_str)
                    .filter(|action| matches!(*action, "complete" | "reassign" | "cancel"))
                    .ok_or_else(|| {
                        BusinessError::invalid(
                            "COLLABORATION_INPUT_INVALID",
                            "Choose a card action.",
                        )
                    })?;
                Ok((
                    format!("collaboration.action.{action}"),
                    Some(id.to_owned()),
                ))
            }
            Some(_) => Err(BusinessError::invalid(
                "COLLABORATION_INPUT_INVALID",
                "Choose a card action.",
            )),
        },
        _ => Err(BusinessError::invalid(
            "COLLABORATION_INPUT_INVALID",
            "Choose a collaboration command.",
        )),
    }
}

pub async fn command(
    State(state): State<Arc<CollaborationWorkHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared = async {
        let target = target(&state, &headers, &slug)?.clone();
        check_origin(&target, &headers)?;
        let key = command_key(&headers)?;
        let raw = json_body(request).await?;
        let (operation, id) = command_operation(&raw)?;
        let decoded = decode_work_command(&operation, id.as_deref(), &raw)?;
        Ok(PreparedCommand {
            target,
            change: decoded.change,
            input: decoded.input,
            key,
        })
    }
    .await;
    match prepared {
        Ok(prepared) => {
            let PreparedCommand {
                target,
                change,
                input,
                key,
            } = prepared;
            match target
                .backend
                .command(CollaborationWorkHttpCommandRequest {
                    host: &target.host,
                    holding_slug: &target.holding_slug,
                    headers: &headers,
                    change,
                    input,
                    key,
                })
                .await
            {
                Ok(receipt) => bounded(receipt),
                Err(error) => error_response(error),
            }
        }
        Err(error) => error_response(error),
    }
}

struct PreparedCommand {
    target: CollaborationWorkHttpTarget,
    change: Change,
    input: Value,
    key: CommandKey,
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::{
        HeaderValue, StatusCode,
        body::{Body, to_bytes},
    };
    use serde_json::json;
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeBackend {
        reads: Mutex<Vec<(String, String, &'static str)>>,
        commands: Mutex<Vec<(String, String, Change)>>,
        failure: Option<BusinessError>,
    }

    impl CollaborationWorkHttpBackend for FakeBackend {
        fn read<'a>(
            &'a self,
            request: CollaborationWorkHttpReadRequest<'a>,
        ) -> CollaborationWorkHttpFuture<'a, CollaborationWorkHttpReadReply> {
            let kind = match &request.kind {
                CollaborationWorkReadKind::Workspace => "workspace",
                CollaborationWorkReadKind::Card { .. } => "card",
                CollaborationWorkReadKind::Relation { .. } => "relation",
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
                    CollaborationWorkReadKind::Workspace => {
                        CollaborationWorkHttpReadReply::Workspace(Box::new(
                            CollaborationWorkspaceData {
                                access: json!({"canWrite": true}),
                                relations: vec![],
                                cards: vec![],
                                people: vec![],
                                source_types: vec!["support_ticket".into()],
                            },
                        ))
                    }
                    CollaborationWorkReadKind::Card { id } => {
                        CollaborationWorkHttpReadReply::Card(Box::new(CollaborationCardData {
                            access: json!({"canWrite": true}),
                            card: json!({"id": id, "status": "open"}),
                        }))
                    }
                    CollaborationWorkReadKind::Relation { id } => {
                        CollaborationWorkHttpReadReply::Relation(Box::new(
                            CollaborationRelationData {
                                access: json!({"canWrite": true}),
                                relation: json!({"id": id, "status": "active"}),
                            },
                        ))
                    }
                })
            })
        }

        fn command<'a>(
            &'a self,
            request: CollaborationWorkHttpCommandRequest<'a>,
        ) -> CollaborationWorkHttpFuture<'a, Receipt> {
            self.commands.lock().unwrap().push((
                request.host.into(),
                request.holding_slug.into(),
                request.change.clone(),
            ));
            Box::pin(async move {
                if let Some(error) = &self.failure {
                    return Err(error.clone());
                }
                Ok(Receipt {
                    id: "record-relation-key".into(),
                    version: 1,
                    status: "active",
                })
            })
        }
    }

    fn make_target(backend: Arc<FakeBackend>) -> CollaborationWorkHttpTarget {
        CollaborationWorkHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
        }
    }

    fn state(target: CollaborationWorkHttpTarget) -> Arc<CollaborationWorkHttpState> {
        Arc::new(CollaborationWorkHttpState {
            targets: BTreeMap::from([((target.host.clone(), target.holding_slug.clone()), target)]),
        })
    }

    fn post_request(body: &str, origin: Option<&str>, key: Option<&str>) -> Request {
        let mut builder = Request::builder()
            .method("POST")
            .header("host", "acme.example.test")
            .header("content-type", "application/json");
        if let Some(origin) = origin {
            builder = builder.header("origin", origin);
        }
        if let Some(key) = key {
            builder = builder.header("idempotency-key", key);
        }
        builder.body(Body::from(body.to_owned())).unwrap()
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
        assert!(collaboration_work_router(vec![target]).is_err());
        assert!(
            collaboration_work_router(vec![make_target(backend.clone()), make_target(backend)])
                .is_err()
        );
        assert!(
            collaboration_work_router(vec![make_target(Arc::new(FakeBackend::default()))]).is_ok()
        );
    }

    #[tokio::test]
    async fn reads_bind_exact_host_and_holding() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(make_target(backend.clone()));
        let response = read_card(
            State(state.clone()),
            Path(("acme".into(), "card-1".into())),
            get_request(),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["card"]["id"], "card-1");
        let response = read_relation(
            State(state.clone()),
            Path(("acme".into(), "relation-1".into())),
            get_request(),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::OK);
        let mut wrong = get_request();
        wrong
            .headers_mut()
            .insert("host", HeaderValue::from_static("other.test"));
        let response = read_card(State(state), Path(("acme".into(), "card-1".into())), wrong).await;
        assert_eq!(body(response).await.0, StatusCode::NOT_FOUND);
        assert_eq!(backend.reads.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn command_shape_selects_the_reused_operation_and_requires_origin_and_key() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(make_target(backend.clone()));
        let payload = json!({
            "entity":"relation",
            "sourceType":"support_ticket",
            "sourceId":"ticket-1",
            "targetType":"field_service_order",
            "targetId":"order-1",
            "relation":"related",
            "reason":"Same customer issue"
        })
        .to_string();
        for request in [
            post_request(&payload, None, Some("collaboration-key-0001")),
            post_request(
                &payload,
                Some("https://evil.test"),
                Some("collaboration-key-0001"),
            ),
            post_request(&payload, Some("https://acme.example.test"), None),
        ] {
            let response = command(State(state.clone()), Path("acme".into()), request).await;
            assert!(matches!(
                body(response).await.0,
                StatusCode::FORBIDDEN | StatusCode::UNPROCESSABLE_ENTITY
            ));
        }
        assert!(backend.commands.lock().unwrap().is_empty());
        let response = command(
            State(state),
            Path("acme".into()),
            post_request(
                &payload,
                Some("https://acme.example.test"),
                Some("collaboration-key-0001"),
            ),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["status"], "active");
        assert!(matches!(
            backend.commands.lock().unwrap()[0].2,
            Change::CreateRelation { .. }
        ));
    }

    #[tokio::test]
    async fn card_actions_and_removals_require_their_identifiers() {
        let backend = Arc::new(FakeBackend::default());
        let state = state(make_target(backend.clone()));
        for payload in [
            json!({"entity":"relation","action":"remove","reason":"No longer related"}),
            json!({"entity":"card","id":"card-1","reason":"Done"}),
            json!({"entity":"card","action":"complete"}),
        ] {
            let response = command(
                State(state.clone()),
                Path("acme".into()),
                post_request(
                    &payload.to_string(),
                    Some("https://acme.example.test"),
                    Some("collaboration-key-0009"),
                ),
            )
            .await;
            assert_eq!(body(response).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        }
        let response = command(
            State(state),
            Path("acme".into()),
            post_request(
                &json!({
                    "entity":"card",
                    "id":"card-1",
                    "action":"complete",
                    "expectedVersion":2,
                    "evidence":"Delivered",
                    "reason":"Work is complete"
                })
                .to_string(),
                Some("https://acme.example.test"),
                Some("collaboration-key-0010"),
            ),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::OK);
        assert!(matches!(
            backend.commands.lock().unwrap()[0].2,
            Change::ChangeCard {
                expected_version: 2,
                ..
            }
        ));
    }
}
