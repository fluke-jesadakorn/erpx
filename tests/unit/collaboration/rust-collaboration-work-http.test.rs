//! Owner-local HTTP adapter tests for native Collaboration action-card and
//! record-relation ingress.
use erpx_server_collaboration::public::collaboration_work::Receipt;
use erpx_server_collaboration::public::collaboration_work_http::{
    CollaborationCardData, CollaborationRelationData, CollaborationWorkHttpBackend,
    CollaborationWorkHttpCommandRequest, CollaborationWorkHttpFuture,
    CollaborationWorkHttpReadReply, CollaborationWorkHttpReadRequest, CollaborationWorkHttpState,
    CollaborationWorkHttpTarget, CollaborationWorkReadKind, CollaborationWorkspaceData, ReadQuery,
    command, read_card, read_relation, read_workspace,
};
use erpx_server_runtime::public::http_support::{
    Path, Query, Request, Response, State, StatusCode,
    body::{Body, to_bytes},
    header,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Probe {
    reads: Mutex<Vec<String>>,
    commands: Mutex<Vec<String>>,
}

impl CollaborationWorkHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: CollaborationWorkHttpReadRequest<'a>,
    ) -> CollaborationWorkHttpFuture<'a, CollaborationWorkHttpReadReply> {
        let kind = match &request.kind {
            CollaborationWorkReadKind::Workspace => "workspace",
            CollaborationWorkReadKind::Card { .. } => "card",
            CollaborationWorkReadKind::Relation { .. } => "relation",
        };
        self.reads
            .lock()
            .unwrap()
            .push(format!("{kind}|{}|{}", request.host, request.holding_slug));
        Box::pin(async move {
            Ok(match request.kind {
                CollaborationWorkReadKind::Workspace => CollaborationWorkHttpReadReply::Workspace(
                    Box::new(CollaborationWorkspaceData {
                        access: json!({"canWrite":true,"canReview":false}),
                        relations: vec![],
                        cards: vec![],
                        people: vec![],
                        source_types: vec!["support_ticket".into()],
                    }),
                ),
                CollaborationWorkReadKind::Card { id } => {
                    CollaborationWorkHttpReadReply::Card(Box::new(CollaborationCardData {
                        access: json!({"canWrite":true}),
                        card: json!({"id":id,"status":"open"}),
                    }))
                }
                CollaborationWorkReadKind::Relation { id } => {
                    CollaborationWorkHttpReadReply::Relation(Box::new(CollaborationRelationData {
                        access: json!({"canWrite":true}),
                        relation: json!({"id":id,"status":"active"}),
                    }))
                }
            })
        })
    }

    fn command<'a>(
        &'a self,
        request: CollaborationWorkHttpCommandRequest<'a>,
    ) -> CollaborationWorkHttpFuture<'a, Receipt> {
        self.commands.lock().unwrap().push(format!(
            "{:?}|{}|{}",
            request.change, request.host, request.holding_slug
        ));
        Box::pin(async move {
            Ok(Receipt {
                id: "record-relation-key".into(),
                version: 1,
                status: "active",
            })
        })
    }
}

fn target(probe: Arc<Probe>) -> CollaborationWorkHttpTarget {
    CollaborationWorkHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: probe,
    }
}

fn state(probe: Arc<Probe>) -> Arc<CollaborationWorkHttpState> {
    Arc::new(CollaborationWorkHttpState::new(vec![target(probe)]).unwrap())
}

fn request(method: &str, body: &str, key: bool) -> Request {
    let mut builder = Request::builder()
        .method(method)
        .header("host", "acme.example.test")
        .header("content-type", "application/json");
    if method == "POST" {
        builder = builder.header("origin", "https://acme.example.test");
    }
    if key {
        builder = builder.header("idempotency-key", "collaboration-http-key-0001");
    }
    builder.body(Body::from(body.to_owned())).unwrap()
}

async fn reply(response: Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn workspace_card_and_relation_reads_bind_the_exact_holding() {
    let probe = Arc::new(Probe::default());
    let state = state(probe.clone());

    let response = read_workspace(
        State(state.clone()),
        Path("acme".into()),
        Query(ReadQuery { format: None }),
        request("GET", "", false),
    )
    .await;
    assert_eq!(
        response.headers().get(header::CACHE_CONTROL).unwrap(),
        "private, no-store"
    );
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["access"]["canWrite"], true);

    let response = read_card(
        State(state.clone()),
        Path(("acme".into(), "card-1".into())),
        request("GET", "", false),
    )
    .await;
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["card"]["id"], "card-1");

    let response = read_relation(
        State(state),
        Path(("acme".into(), "relation-1".into())),
        request("GET", "", false),
    )
    .await;
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["relation"]["id"], "relation-1");
    assert_eq!(
        probe.reads.lock().unwrap().as_slice(),
        [
            "workspace|acme.example.test|acme",
            "card|acme.example.test|acme",
            "relation|acme.example.test|acme"
        ]
    );
}

#[tokio::test]
async fn relation_and_card_commands_reuse_the_existing_wire_shape() {
    let probe = Arc::new(Probe::default());
    let state = state(probe.clone());

    let relation = json!({
        "entity":"relation",
        "sourceType":"support_ticket",
        "sourceId":"ticket-1",
        "targetType":"field_service_order",
        "targetId":"order-1",
        "relation":"duplicates",
        "reason":"Same customer issue"
    })
    .to_string();
    let response = command(
        State(state.clone()),
        Path("acme".into()),
        request("POST", &relation, true),
    )
    .await;
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["status"], "active");

    let remove = json!({
        "entity":"relation",
        "action":"remove",
        "id":"record-relation-key",
        "expectedVersion":2,
        "reason":"No longer related"
    })
    .to_string();
    let response = command(
        State(state.clone()),
        Path("acme".into()),
        request("POST", &remove, true),
    )
    .await;
    assert_eq!(reply(response).await.0, StatusCode::OK);

    let complete = json!({
        "entity":"card",
        "id":"card-1",
        "action":"complete",
        "expectedVersion":3,
        "evidence":"Delivered",
        "reason":"Work complete"
    })
    .to_string();
    let response = command(
        State(state),
        Path("acme".into()),
        request("POST", &complete, true),
    )
    .await;
    assert_eq!(reply(response).await.0, StatusCode::OK);

    let commands = probe.commands.lock().unwrap();
    assert!(commands[0].contains("CreateRelation"));
    assert!(commands[1].contains("RemoveRelation"));
    assert!(commands[2].contains("ChangeCard"));
}

#[tokio::test]
async fn malformed_commands_and_missing_keys_never_reach_the_owner() {
    let probe = Arc::new(Probe::default());
    let state = state(probe.clone());

    for (body, key) in [
        (r#"{"entity":"relation","action":"remove"}"#, true),
        (r#"{"entity":"card","id":"card-1"}"#, true),
        (r#"{"entity":"bogus"}"#, true),
        (
            r#"{"entity":"relation","sourceType":"support_ticket","sourceId":"a","targetType":"support_ticket","targetId":"b","relation":"related","reason":"Because"}"#,
            false,
        ),
    ] {
        let response = command(
            State(state.clone()),
            Path("acme".into()),
            request("POST", body, key),
        )
        .await;
        assert_eq!(reply(response).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    }
    assert!(probe.commands.lock().unwrap().is_empty());
}
