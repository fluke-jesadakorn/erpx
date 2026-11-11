//! Owner-local HTTP adapter tests for native Messaging ingress, teams/channels
//! and the fail-closed Files port.
use erpx_server_collaboration::public::messaging_http::{
    FileQuery, HistoryQuery, MessagingAttachmentKind, MessagingAttachmentReply,
    MessagingChannelsData, MessagingFileBinding, MessagingFileContent, MessagingFilesBindRequest,
    MessagingFilesContentRequest, MessagingFilesPort, MessagingHistoryData,
    MessagingHttpAttachmentRequest, MessagingHttpBackend, MessagingHttpCommand,
    MessagingHttpCommandRequest, MessagingHttpFuture, MessagingHttpReadReply,
    MessagingHttpReadRequest, MessagingHttpReceipt, MessagingHttpState, MessagingHttpTarget,
    MessagingMembersData, MessagingOverviewData, MessagingReadKind, bind_attachment,
    create_channel, create_room, create_team, read_file, read_history, read_overview,
    read_team_members, send_message,
};
use erpx_server_collaboration::public::messaging_native::{
    MessageRow, Receipt as RoomReceipt, RoomView,
};
use erpx_server_collaboration::public::messaging_teams::{Receipt as TeamReceipt, TeamSnapshot};
use erpx_server_runtime::public::http_support::{
    Path, Query, Request, Response, State, StatusCode,
    body::{Body, to_bytes},
    header,
};
use serde_json::Value;
use std::sync::{Arc, Mutex};

#[derive(Default)]
struct Probe {
    reads: Mutex<Vec<String>>,
    commands: Mutex<Vec<String>>,
    attachments: Mutex<Vec<String>>,
}

fn room() -> RoomView {
    RoomView {
        id: "room-1".into(),
        kind: "group".into(),
        name: "Team".into(),
        owner_id: "user".into(),
        version: 1,
        last_seq: 3,
        unread: 1,
        members: vec!["user".into()],
        last_message_at: Some(1),
    }
}

fn team() -> TeamSnapshot {
    TeamSnapshot {
        id: "team-1".into(),
        company_id: "company".into(),
        name: "Team".into(),
        owner_id: "user".into(),
        version: 1,
        members: vec!["user".into()],
        created_at: 1,
    }
}

impl MessagingHttpBackend for Probe {
    fn read<'a>(
        &'a self,
        request: MessagingHttpReadRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingHttpReadReply> {
        let kind = match &request.kind {
            MessagingReadKind::Overview => "overview",
            MessagingReadKind::History { .. } => "history",
            MessagingReadKind::Members { .. } => "members",
            MessagingReadKind::TeamChannels { .. } => "channels",
        };
        self.reads
            .lock()
            .unwrap()
            .push(format!("{kind}|{}|{}", request.host, request.holding_slug));
        Box::pin(async move {
            Ok(match request.kind {
                MessagingReadKind::Overview => {
                    MessagingHttpReadReply::Overview(Box::new(MessagingOverviewData {
                        teams: vec![team()],
                        rooms: vec![room()],
                    }))
                }
                MessagingReadKind::History { .. } => {
                    MessagingHttpReadReply::History(Box::new(MessagingHistoryData {
                        room: room(),
                        messages: vec![MessageRow {
                            id: "message-1".into(),
                            room_id: "room-1".into(),
                            seq: 1,
                            sender_id: "user".into(),
                            sender_name: "User".into(),
                            body: "Hello".into(),
                            created_at: 1,
                        }],
                        has_more: false,
                    }))
                }
                MessagingReadKind::Members { id, is_team } => {
                    MessagingHttpReadReply::Members(Box::new(MessagingMembersData {
                        id,
                        is_team,
                        members: vec![serde_json::json!({"id":"user","name":"User","email":"user@example.test"})],
                        detail: serde_json::json!({"id":"members"}),
                    }))
                }
                MessagingReadKind::TeamChannels { team_id } => {
                    MessagingHttpReadReply::Channels(Box::new(MessagingChannelsData {
                        team: TeamSnapshot {
                            id: team_id,
                            ..team()
                        },
                        channels: vec![],
                    }))
                }
            })
        })
    }

    fn command<'a>(
        &'a self,
        request: MessagingHttpCommandRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingHttpReceipt> {
        let kind = match &request.command {
            MessagingHttpCommand::Room(_) => "room",
            MessagingHttpCommand::Team(_) => "team",
        };
        self.commands
            .lock()
            .unwrap()
            .push(format!("{kind}|{}|{}", request.host, request.holding_slug));
        Box::pin(async move {
            Ok(match request.command {
                MessagingHttpCommand::Room(_) => MessagingHttpReceipt::Room {
                    receipt: RoomReceipt {
                        id: "message-key".into(),
                        version: 1,
                        status: "sent",
                        seq: 4,
                    },
                },
                MessagingHttpCommand::Team(_) => MessagingHttpReceipt::Team {
                    receipt: TeamReceipt {
                        id: "team-key".into(),
                        version: 1,
                        status: "created",
                    },
                },
            })
        })
    }

    fn attachment<'a>(
        &'a self,
        request: MessagingHttpAttachmentRequest<'a>,
        _files: &'a dyn MessagingFilesPort,
    ) -> MessagingHttpFuture<'a, MessagingAttachmentReply> {
        match &request.kind {
            MessagingAttachmentKind::Bind {
                room_id,
                attachment_set_ids,
                ..
            } => {
                let marker = format!("bind|{room_id}|{}", attachment_set_ids.len());
                self.attachments.lock().unwrap().push(marker);
                Box::pin(async move {
                    Ok(MessagingAttachmentReply::Bound(MessagingFileBinding {
                        id: "binding-1".into(),
                        version: 1,
                        status: "pending_scan",
                    }))
                })
            }
            MessagingAttachmentKind::Content { file_id, variant } => {
                let marker = format!("content|{file_id}|{variant}");
                self.attachments.lock().unwrap().push(marker);
                Box::pin(async move {
                    Ok(MessagingAttachmentReply::Content(MessagingFileContent {
                        media_type: "image/png".into(),
                        filename: "shot.png".into(),
                        body: b"png".to_vec(),
                    }))
                })
            }
        }
    }
}

struct Files;
impl MessagingFilesPort for Files {
    fn bind<'a>(
        &'a self,
        _request: MessagingFilesBindRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingFileBinding> {
        Box::pin(async move {
            Ok(MessagingFileBinding {
                id: "binding-1".into(),
                version: 1,
                status: "pending_scan",
            })
        })
    }
    fn content<'a>(
        &'a self,
        _request: MessagingFilesContentRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingFileContent> {
        Box::pin(async move {
            Ok(MessagingFileContent {
                media_type: "image/png".into(),
                filename: "shot.png".into(),
                body: b"png".to_vec(),
            })
        })
    }
}

fn target(probe: Arc<Probe>, files: bool) -> MessagingHttpTarget {
    MessagingHttpTarget {
        host: "acme.example.test".into(),
        holding_slug: "acme".into(),
        origin: "https://acme.example.test".into(),
        backend: probe,
        files: files.then(|| Arc::new(Files) as Arc<dyn MessagingFilesPort>),
    }
}

fn state(probe: Arc<Probe>, files: bool) -> Arc<MessagingHttpState> {
    Arc::new(MessagingHttpState::new(vec![target(probe, files)]).unwrap())
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
        builder = builder.header("idempotency-key", "messaging-http-key-0001");
    }
    builder.body(Body::from(body.to_owned())).unwrap()
}

async fn reply(response: Response) -> (StatusCode, Value) {
    let status = response.status();
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    (status, serde_json::from_slice(&bytes).unwrap())
}

#[tokio::test]
async fn overview_history_and_members_bind_the_exact_holding() {
    let probe = Arc::new(Probe::default());
    let state = state(probe.clone(), false);

    let response = read_overview(
        State(state.clone()),
        Path("acme".into()),
        request("GET", "", false),
    )
    .await;
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["rooms"][0]["id"], "room-1");
    assert_eq!(value["data"]["teams"][0]["id"], "team-1");

    let response = read_history(
        State(state.clone()),
        Path(("acme".into(), "room-1".into())),
        Query(HistoryQuery {
            before: Some("2".into()),
            after: None,
            limit: None,
        }),
        request("GET", "", false),
    )
    .await;
    let (_, value) = reply(response).await;
    assert_eq!(value["data"]["messages"][0]["id"], "message-1");
    assert_eq!(value["data"]["hasMore"], false);

    let response = read_team_members(
        State(state),
        Path(("acme".into(), "team-1".into())),
        request("GET", "", false),
    )
    .await;
    let (_, value) = reply(response).await;
    assert_eq!(value["data"]["isTeam"], true);
    assert_eq!(
        probe.reads.lock().unwrap().as_slice(),
        [
            "overview|acme.example.test|acme",
            "history|acme.example.test|acme",
            "members|acme.example.test|acme"
        ]
    );
}

#[tokio::test]
async fn room_team_and_channel_commands_keep_origin_and_key_rules() {
    let probe = Arc::new(Probe::default());
    let state = state(probe.clone(), false);

    let mut no_origin = request("POST", r#"{"body":"Hello"}"#, true);
    no_origin.headers_mut().remove("origin");
    let response = send_message(
        State(state.clone()),
        Path(("acme".into(), "room-1".into())),
        no_origin,
    )
    .await;
    assert_eq!(reply(response).await.0, StatusCode::FORBIDDEN);

    let response = create_room(
        State(state.clone()),
        Path("acme".into()),
        request(
            "POST",
            r#"{"kind":"group","name":"Team","members":["user-2"]}"#,
            true,
        ),
    )
    .await;
    assert_eq!(reply(response).await.0, StatusCode::OK);

    let response = create_team(
        State(state.clone()),
        Path("acme".into()),
        request("POST", r#"{"name":"Team","members":["user-2"]}"#, true),
    )
    .await;
    assert_eq!(reply(response).await.0, StatusCode::OK);

    let response = create_channel(
        State(state),
        Path(("acme".into(), "team-1".into())),
        request("POST", r#"{"name":"Ops","private":true}"#, true),
    )
    .await;
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["id"], "team-key");
    assert_eq!(
        probe.commands.lock().unwrap().as_slice(),
        [
            "room|acme.example.test|acme",
            "team|acme.example.test|acme",
            "team|acme.example.test|acme"
        ]
    );
}

#[tokio::test]
async fn attachments_fail_closed_without_files_and_serve_content_with_files() {
    let probe = Arc::new(Probe::default());
    let response = bind_attachment(
        State(state(probe.clone(), false)),
        Path(("acme".into(), "room-1".into())),
        request("POST", r#"{"attachmentSetIds":["set-1"]}"#, true),
    )
    .await;
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
    assert_eq!(value["error"]["code"], "FILES_PORT_UNAVAILABLE");

    let state = state(probe.clone(), true);
    let response = bind_attachment(
        State(state.clone()),
        Path(("acme".into(), "room-1".into())),
        request("POST", r#"{"attachmentSetIds":["set-1"]}"#, true),
    )
    .await;
    let (status, value) = reply(response).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(value["data"]["status"], "pending_scan");

    let response = read_file(
        State(state),
        Path(("acme".into(), "file-1".into())),
        Query(FileQuery {
            variant: Some("preview".into()),
        }),
        request("GET", "", false),
    )
    .await;
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
    let bytes = to_bytes(response.into_body(), 1 << 20).await.unwrap();
    assert_eq!(&bytes[..], b"png");
    assert_eq!(
        probe.attachments.lock().unwrap().as_slice(),
        ["bind|room-1|1", "content|file-1|preview"]
    );
}
