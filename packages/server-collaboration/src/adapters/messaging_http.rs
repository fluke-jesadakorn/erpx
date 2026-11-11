//! Native HTTP ingress for Messaging conversations, teams, channels and
//! attachment binding.
//!
//! Transport facts only: exact registered Holding host/slug, browser origin,
//! one idempotency key, bounded JSON and error mapping. Session resolution,
//! current Company membership, team ownership, Files access and the serializable
//! owner unit belong to the composition backend, which must implement
//! [`MessagingHttpBackend`]. The router never mints an actor, Company or
//! permission, and attachment routes fail closed when no Files port is supplied.
use crate::adapters::http_ingress::{
    bounded, command_key, error_response, json_body, unique_header, valid_origin,
};
use crate::adapters::messaging_http_wire::{
    DecodedMessagingCommand, decode_room_command, decode_team_command,
};
use crate::adapters::messaging_native::{MessageRow, RoomView};
use crate::adapters::messaging_teams::ChannelView;
use crate::domain::messaging_native::{Change, HistoryCursor, Receipt as RoomReceipt};
use crate::domain::messaging_teams::{Receipt as TeamReceipt, TeamSnapshot};
use erpx_server_runtime::public::http_support::{
    HeaderMap, HeaderValue, Path, Query, Request, Response, Router, State,
    body::Body,
    header,
    routing::{get, post},
};
use erpx_server_shared::public::{BusinessError, CommandKey, FailureKind};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::{collections::BTreeMap, future::Future, pin::Pin, sync::Arc};

const DEFAULT_PAGE: usize = 50;
const PREVIEW_VARIANT: &str = "preview";

pub type MessagingHttpFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagingOverviewData {
    pub teams: Vec<TeamSnapshot>,
    pub rooms: Vec<RoomView>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagingHistoryData {
    pub room: RoomView,
    pub messages: Vec<MessageRow>,
    #[serde(rename = "hasMore")]
    pub has_more: bool,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagingMembersData {
    pub id: String,
    pub is_team: bool,
    pub members: Vec<String>,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MessagingChannelsData {
    pub team: TeamSnapshot,
    pub channels: Vec<ChannelView>,
}

#[derive(Clone)]
pub enum MessagingReadKind {
    Overview,
    History {
        room_id: String,
        before: Option<HistoryCursor>,
        after: Option<HistoryCursor>,
        limit: usize,
    },
    Members {
        id: String,
        is_team: bool,
    },
    TeamChannels {
        team_id: String,
    },
}

pub struct MessagingHttpReadRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub kind: MessagingReadKind,
}

#[derive(Clone)]
pub enum MessagingHttpCommand {
    Room(Change),
    Team(crate::domain::messaging_teams::Change),
}

pub struct MessagingHttpCommandRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub command: MessagingHttpCommand,
    pub input: Value,
    pub key: CommandKey,
}

#[derive(Clone, Debug, Serialize)]
#[serde(tag = "resource", rename_all = "camelCase")]
pub enum MessagingHttpReceipt {
    Room {
        #[serde(flatten)]
        receipt: RoomReceipt,
    },
    Team {
        #[serde(flatten)]
        receipt: TeamReceipt,
    },
}

pub enum MessagingHttpReadReply {
    Overview(Box<MessagingOverviewData>),
    History(Box<MessagingHistoryData>),
    Members(Box<MessagingMembersData>),
    Channels(Box<MessagingChannelsData>),
}

#[derive(Clone, Debug, Serialize)]
pub struct MessagingFileBinding {
    pub id: String,
    pub version: i64,
    pub status: &'static str,
}

#[derive(Clone, Debug)]
pub struct MessagingFileContent {
    pub media_type: String,
    pub filename: String,
    pub body: Vec<u8>,
}

pub struct MessagingFilesBindRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub room_id: &'a str,
    pub attachment_set_ids: &'a [String],
    pub message_id: Option<&'a str>,
}

pub struct MessagingFilesContentRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    pub file_id: &'a str,
    pub variant: &'a str,
}

/// Files owner port. Composition implements quarantine/scan/audience checks;
/// the collaboration router only transports the requested binding.
pub trait MessagingFilesPort: Send + Sync {
    fn bind<'a>(
        &'a self,
        request: MessagingFilesBindRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingFileBinding>;
    fn content<'a>(
        &'a self,
        request: MessagingFilesContentRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingFileContent>;
}

pub enum MessagingAttachmentKind<'a> {
    Bind {
        room_id: &'a str,
        attachment_set_ids: &'a [String],
        message_id: Option<&'a str>,
    },
    Content {
        file_id: &'a str,
        variant: &'a str,
    },
}

pub struct MessagingHttpAttachmentRequest<'a> {
    pub host: &'a str,
    pub holding_slug: &'a str,
    pub headers: &'a HeaderMap,
    /// Present for binding commands, absent for content reads.
    pub key: Option<CommandKey>,
    pub kind: MessagingAttachmentKind<'a>,
}

pub enum MessagingAttachmentReply {
    Bound(MessagingFileBinding),
    Content(MessagingFileContent),
}

/// Composition-owned session, scope, Files and owner-unit adapter.
pub trait MessagingHttpBackend: Send + Sync {
    fn read<'a>(
        &'a self,
        request: MessagingHttpReadRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingHttpReadReply>;
    fn command<'a>(
        &'a self,
        request: MessagingHttpCommandRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingHttpReceipt>;
    fn attachment<'a>(
        &'a self,
        request: MessagingHttpAttachmentRequest<'a>,
        files: &'a dyn MessagingFilesPort,
    ) -> MessagingHttpFuture<'a, MessagingAttachmentReply>;
}

#[derive(Clone)]
pub struct MessagingHttpTarget {
    pub host: String,
    pub holding_slug: String,
    pub origin: String,
    pub backend: Arc<dyn MessagingHttpBackend>,
    /// Absence is the fail-closed state for every attachment route.
    pub files: Option<Arc<dyn MessagingFilesPort>>,
}

pub struct MessagingHttpState {
    targets: BTreeMap<(String, String), MessagingHttpTarget>,
}

impl MessagingHttpState {
    /// Validate exact distinct registered Holding targets.
    pub fn new(targets: Vec<MessagingHttpTarget>) -> Result<Self, BusinessError> {
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
                    "MESSAGING_HTTP_CONFIGURATION_INVALID",
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
pub struct HistoryQuery {
    #[serde(default)]
    pub before: Option<String>,
    #[serde(default)]
    pub after: Option<String>,
    #[serde(default)]
    pub limit: Option<String>,
}

#[derive(Deserialize)]
pub struct FileQuery {
    #[serde(default)]
    pub variant: Option<String>,
}

fn missing() -> BusinessError {
    BusinessError::new(FailureKind::NotFound, "NOT_FOUND", "Not found.")
}

fn files_unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_PORT_UNAVAILABLE",
        "Attachment access is unavailable.",
    )
}

pub fn messaging_router(targets: Vec<MessagingHttpTarget>) -> Result<Router, BusinessError> {
    let state = MessagingHttpState::new(targets)?;
    Ok(Router::new()
        .route(
            "/api/v1/holdings/{holding_slug}/messaging",
            get(read_overview),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/messaging/rooms",
            post(create_room),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/messaging/rooms/{room_id}/messages",
            get(read_history).post(send_message),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/messaging/rooms/{room_id}/read",
            post(mark_read),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/messaging/rooms/{room_id}/members",
            get(read_room_members),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/messaging/rooms/{room_id}/attachments",
            post(bind_attachment),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/messaging/files/{file_id}",
            get(read_file),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/messaging/teams",
            post(create_team),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/messaging/teams/{team_id}/channels",
            get(read_channels).post(create_channel),
        )
        .route(
            "/api/v1/holdings/{holding_slug}/messaging/teams/{team_id}/members",
            get(read_team_members),
        )
        .with_state(Arc::new(state)))
}

fn target<'a>(
    state: &'a MessagingHttpState,
    headers: &HeaderMap,
    slug: &str,
) -> Result<&'a MessagingHttpTarget, BusinessError> {
    let host = unique_header(headers, header::HOST)?
        .filter(|value| !value.is_empty())
        .ok_or_else(missing)?;
    state
        .targets
        .get(&(host.to_owned(), slug.to_owned()))
        .ok_or_else(missing)
}

fn check_origin(target: &MessagingHttpTarget, headers: &HeaderMap) -> Result<(), BusinessError> {
    match unique_header(headers, header::ORIGIN)? {
        Some(value) if value == target.origin => Ok(()),
        _ => Err(BusinessError::new(
            FailureKind::Forbidden,
            "ORIGIN_FORBIDDEN",
            "Use the application origin.",
        )),
    }
}

fn read_limit(value: Option<&str>) -> Result<usize, BusinessError> {
    match value {
        None => Ok(DEFAULT_PAGE),
        Some(raw) => raw
            .parse::<usize>()
            .ok()
            .filter(|value| (1..=100).contains(value))
            .ok_or_else(|| {
                BusinessError::invalid("MESSAGING_LIMIT_INVALID", "Choose a smaller page.")
            }),
    }
}

fn cursor(value: Option<&str>) -> Result<Option<HistoryCursor>, BusinessError> {
    value.map(HistoryCursor::decode).transpose()
}

pub async fn read_overview(
    State(state): State<Arc<MessagingHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        target
            .backend
            .read(MessagingHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: MessagingReadKind::Overview,
            })
            .await
    }
    .await;
    reply(
        result,
        |reply| matches!(reply, MessagingHttpReadReply::Overview(_)),
        |reply| match reply {
            MessagingHttpReadReply::Overview(value) => bounded(*value),
            _ => unreachable!(),
        },
    )
    .await
}

fn reply<F, G>(
    result: Result<MessagingHttpReadReply, BusinessError>,
    matches: F,
    render: G,
) -> Pin<Box<dyn Future<Output = Response> + Send>>
where
    F: Fn(&MessagingHttpReadReply) -> bool + Send + 'static,
    G: FnOnce(MessagingHttpReadReply) -> Response + Send + 'static,
{
    Box::pin(async move {
        match result {
            Ok(reply) if matches(&reply) => render(reply),
            Ok(_) => error_response(BusinessError::invalid(
                "MESSAGING_HTTP_RESPONSE_INVALID",
                "The native Messaging reader returned the wrong shape.",
            )),
            Err(error) => error_response(error),
        }
    })
}

pub async fn read_history(
    State(state): State<Arc<MessagingHttpState>>,
    Path((slug, room_id)): Path<(String, String)>,
    Query(query): Query<HistoryQuery>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        let before = cursor(query.before.as_deref())?;
        let after = cursor(query.after.as_deref())?;
        if before.is_some() && after.is_some() {
            return Err(BusinessError::invalid(
                "CURSOR_INVALID",
                "Choose one message cursor.",
            ));
        }
        target
            .backend
            .read(MessagingHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: MessagingReadKind::History {
                    room_id,
                    before,
                    after,
                    limit: read_limit(query.limit.as_deref())?,
                },
            })
            .await
    }
    .await;
    reply(
        result,
        |reply| matches!(reply, MessagingHttpReadReply::History(_)),
        |reply| match reply {
            MessagingHttpReadReply::History(value) => bounded(*value),
            _ => unreachable!(),
        },
    )
    .await
}

pub async fn read_room_members(
    State(state): State<Arc<MessagingHttpState>>,
    Path((slug, room_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    read_members(state, slug, room_id, false, request).await
}

pub async fn read_team_members(
    State(state): State<Arc<MessagingHttpState>>,
    Path((slug, team_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    read_members(state, slug, team_id, true, request).await
}

async fn read_members(
    state: Arc<MessagingHttpState>,
    slug: String,
    id: String,
    is_team: bool,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        target
            .backend
            .read(MessagingHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: MessagingReadKind::Members { id, is_team },
            })
            .await
    }
    .await;
    reply(
        result,
        |reply| matches!(reply, MessagingHttpReadReply::Members(_)),
        |reply| match reply {
            MessagingHttpReadReply::Members(value) => bounded(*value),
            _ => unreachable!(),
        },
    )
    .await
}

pub async fn read_channels(
    State(state): State<Arc<MessagingHttpState>>,
    Path((slug, team_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?;
        target
            .backend
            .read(MessagingHttpReadRequest {
                host: &target.host,
                holding_slug: &target.holding_slug,
                headers: &headers,
                kind: MessagingReadKind::TeamChannels { team_id },
            })
            .await
    }
    .await;
    reply(
        result,
        |reply| matches!(reply, MessagingHttpReadReply::Channels(_)),
        |reply| match reply {
            MessagingHttpReadReply::Channels(value) => bounded(*value),
            _ => unreachable!(),
        },
    )
    .await
}

struct PreparedCommand {
    target: MessagingHttpTarget,
    command: MessagingHttpCommand,
    input: Value,
    key: CommandKey,
}

async fn prepare_command(
    state: Arc<MessagingHttpState>,
    slug: String,
    request: Request,
    operation: &str,
    room_id: Option<String>,
    team_id: Option<String>,
) -> Result<PreparedCommand, BusinessError> {
    let target = target(&state, request.headers(), &slug)?.clone();
    check_origin(&target, request.headers())?;
    let headers = request.headers().clone();
    let key = command_key(&headers)?;
    let raw = json_body(request).await?;
    let decoded = if operation.starts_with("messaging.team.")
        || operation.starts_with("messaging.channel.")
    {
        decode_team_command(operation, team_id.as_deref(), &raw)?
    } else {
        decode_room_command(operation, room_id.as_deref(), &raw)?
    };
    let command = match decoded {
        DecodedMessagingCommand::Room(change) => MessagingHttpCommand::Room(change),
        DecodedMessagingCommand::Team(change) => MessagingHttpCommand::Team(change),
    };
    Ok(PreparedCommand {
        target,
        command,
        input: raw,
        key,
    })
}

async fn execute_command(prepared: PreparedCommand, headers: HeaderMap) -> Response {
    let PreparedCommand {
        target,
        command,
        input,
        key,
    } = prepared;
    match target
        .backend
        .command(MessagingHttpCommandRequest {
            host: &target.host,
            holding_slug: &target.holding_slug,
            headers: &headers,
            command,
            input,
            key,
        })
        .await
    {
        Ok(receipt) => bounded(receipt),
        Err(error) => error_response(error),
    }
}

async fn command_response(
    prepared: Result<PreparedCommand, BusinessError>,
    headers: HeaderMap,
) -> Response {
    match prepared {
        Ok(command) => execute_command(command, headers).await,
        Err(error) => error_response(error),
    }
}

pub async fn create_room(
    State(state): State<Arc<MessagingHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared = prepare_command(state, slug, request, "messaging.room.create", None, None).await;
    command_response(prepared, headers).await
}

pub async fn send_message(
    State(state): State<Arc<MessagingHttpState>>,
    Path((slug, room_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared =
        prepare_command(state, slug, request, "messaging.send", Some(room_id), None).await;
    command_response(prepared, headers).await
}

pub async fn mark_read(
    State(state): State<Arc<MessagingHttpState>>,
    Path((slug, room_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared =
        prepare_command(state, slug, request, "messaging.read", Some(room_id), None).await;
    command_response(prepared, headers).await
}

pub async fn create_team(
    State(state): State<Arc<MessagingHttpState>>,
    Path(slug): Path<String>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared = prepare_command(state, slug, request, "messaging.team.create", None, None).await;
    command_response(prepared, headers).await
}

pub async fn create_channel(
    State(state): State<Arc<MessagingHttpState>>,
    Path((slug, team_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let prepared = prepare_command(
        state,
        slug,
        request,
        "messaging.channel.create",
        None,
        Some(team_id),
    )
    .await;
    command_response(prepared, headers).await
}

pub async fn bind_attachment(
    State(state): State<Arc<MessagingHttpState>>,
    Path((slug, room_id)): Path<(String, String)>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let result = async {
        let target = target(&state, &headers, &slug)?.clone();
        check_origin(&target, &headers)?;
        let key = command_key(&headers)?;
        let raw = json_body(request).await?;
        let ids = attachment_ids(&raw)?;
        let message_id = raw
            .get("messageId")
            .and_then(Value::as_str)
            .filter(|value| !value.is_empty());
        let files = target.files.as_deref().ok_or_else(files_unavailable)?;
        target
            .backend
            .attachment(
                MessagingHttpAttachmentRequest {
                    host: &target.host,
                    holding_slug: &target.holding_slug,
                    headers: &headers,
                    key: Some(key),
                    kind: MessagingAttachmentKind::Bind {
                        room_id: &room_id,
                        attachment_set_ids: &ids,
                        message_id,
                    },
                },
                files,
            )
            .await
    }
    .await;
    match result {
        Ok(MessagingAttachmentReply::Bound(receipt)) => bounded(receipt),
        Ok(_) => error_response(BusinessError::invalid(
            "MESSAGING_HTTP_RESPONSE_INVALID",
            "The native attachment reader returned the wrong shape.",
        )),
        Err(error) => error_response(error),
    }
}

fn attachment_ids(raw: &Value) -> Result<Vec<String>, BusinessError> {
    let ids = raw
        .get("attachmentSetIds")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            BusinessError::invalid(
                "MESSAGING_INPUT_INVALID",
                "Choose current attachment sets to bind.",
            )
        })?;
    if ids.is_empty() || ids.len() > 10 {
        return Err(BusinessError::invalid(
            "MESSAGING_INPUT_INVALID",
            "Choose current attachment sets to bind.",
        ));
    }
    ids.iter()
        .map(|value| {
            value
                .as_str()
                .filter(|value| !value.is_empty() && value.len() <= 180)
                .map(str::to_owned)
                .ok_or_else(|| {
                    BusinessError::invalid(
                        "MESSAGING_INPUT_INVALID",
                        "Choose current attachment sets to bind.",
                    )
                })
        })
        .collect()
}

pub async fn read_file(
    State(state): State<Arc<MessagingHttpState>>,
    Path((slug, file_id)): Path<(String, String)>,
    Query(query): Query<FileQuery>,
    request: Request,
) -> Response {
    let headers = request.headers().clone();
    let variant = match query.variant.as_deref() {
        None | Some("original") => "original",
        Some(PREVIEW_VARIANT) => PREVIEW_VARIANT,
        Some(_) => {
            return error_response(BusinessError::invalid(
                "MESSAGING_INPUT_INVALID",
                "Choose an original or preview file.",
            ));
        }
    };
    let result = async {
        let target = target(&state, &headers, &slug)?.clone();
        let files = target.files.as_deref().ok_or_else(files_unavailable)?;
        target
            .backend
            .attachment(
                MessagingHttpAttachmentRequest {
                    host: &target.host,
                    holding_slug: &target.holding_slug,
                    headers: &headers,
                    key: None,
                    kind: MessagingAttachmentKind::Content {
                        file_id: &file_id,
                        variant,
                    },
                },
                files,
            )
            .await
    }
    .await;
    match result {
        Ok(MessagingAttachmentReply::Content(content)) => file_response(content),
        Ok(_) => error_response(BusinessError::invalid(
            "MESSAGING_HTTP_RESPONSE_INVALID",
            "The native attachment reader returned the wrong shape.",
        )),
        Err(error) => error_response(error),
    }
}

fn safe_filename(value: &str) -> String {
    let cleaned: String = value
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, ' ' | '.' | '-' | '_'))
        .take(160)
        .collect();
    if cleaned.is_empty() {
        "file".into()
    } else {
        cleaned
    }
}

fn file_response(content: MessagingFileContent) -> Response {
    let media_type = HeaderValue::from_str(&content.media_type)
        .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream"));
    let disposition = HeaderValue::from_str(&format!(
        "attachment; filename=\"{}\"",
        safe_filename(&content.filename)
    ))
    .unwrap_or_else(|_| HeaderValue::from_static("attachment"));
    let mut response = Response::new(Body::from(content.body));
    let headers = response.headers_mut();
    headers.insert(header::CONTENT_TYPE, media_type);
    headers.insert(header::CONTENT_DISPOSITION, disposition);
    for (name, value) in [
        (header::CACHE_CONTROL, "private, no-store"),
        (header::X_CONTENT_TYPE_OPTIONS, "nosniff"),
        (
            header::CONTENT_SECURITY_POLICY,
            "default-src 'none'; sandbox",
        ),
    ] {
        headers.insert(name, HeaderValue::from_static(value));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::{StatusCode, body::to_bytes};
    use std::sync::Mutex;

    #[derive(Default)]
    struct FakeBackend {
        reads: Mutex<Vec<(String, String, &'static str)>>,
        commands: Mutex<Vec<(String, String, String)>>,
        attachments: Mutex<Vec<String>>,
    }

    impl MessagingHttpBackend for FakeBackend {
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
            self.reads.lock().unwrap().push((
                request.host.into(),
                request.holding_slug.into(),
                kind,
            ));
            Box::pin(async move {
                Ok(match request.kind {
                    MessagingReadKind::Overview => {
                        MessagingHttpReadReply::Overview(Box::new(MessagingOverviewData {
                            teams: vec![],
                            rooms: vec![room_view()],
                        }))
                    }
                    MessagingReadKind::History { .. } => {
                        MessagingHttpReadReply::History(Box::new(MessagingHistoryData {
                            room: room_view(),
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
                            members: vec!["user".into()],
                        }))
                    }
                    MessagingReadKind::TeamChannels { team_id } => {
                        MessagingHttpReadReply::Channels(Box::new(MessagingChannelsData {
                            team: TeamSnapshot {
                                id: team_id,
                                company_id: "company".into(),
                                name: "Team".into(),
                                owner_id: "user".into(),
                                version: 1,
                                members: vec!["user".into()],
                                created_at: 1,
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
            self.commands.lock().unwrap().push((
                request.host.into(),
                request.holding_slug.into(),
                kind.into(),
            ));
            Box::pin(async move {
                Ok(match request.command {
                    MessagingHttpCommand::Room(_) => MessagingHttpReceipt::Room {
                        receipt: RoomReceipt {
                            id: "message-key".into(),
                            version: 1,
                            status: "sent",
                            seq: 3,
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
                    let room_id = (*room_id).to_owned();
                    let count = attachment_set_ids.len();
                    self.attachments
                        .lock()
                        .unwrap()
                        .push(format!("{room_id}:{count}"));
                    Box::pin(async move {
                        Ok(MessagingAttachmentReply::Bound(MessagingFileBinding {
                            id: "binding-1".into(),
                            version: 1,
                            status: "pending_scan",
                        }))
                    })
                }
                MessagingAttachmentKind::Content { file_id, variant } => {
                    let file_id = (*file_id).to_owned();
                    let variant = (*variant).to_owned();
                    self.attachments
                        .lock()
                        .unwrap()
                        .push(format!("{file_id}:{variant}"));
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

    struct FakeFiles;
    impl MessagingFilesPort for FakeFiles {
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

    fn room_view() -> RoomView {
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

    fn make_target(backend: Arc<FakeBackend>, files: bool) -> MessagingHttpTarget {
        MessagingHttpTarget {
            host: "acme.example.test".into(),
            holding_slug: "acme".into(),
            origin: "https://acme.example.test".into(),
            backend,
            files: files.then(|| Arc::new(FakeFiles) as Arc<dyn MessagingFilesPort>),
        }
    }

    fn http_state(target: MessagingHttpTarget) -> Arc<MessagingHttpState> {
        Arc::new(MessagingHttpState {
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
        let mut target = make_target(backend.clone(), false);
        target.origin = "http://acme.example.test".into();
        assert!(messaging_router(vec![target]).is_err());
        assert!(
            messaging_router(vec![
                make_target(backend.clone(), false),
                make_target(backend, false)
            ])
            .is_err()
        );
        assert!(
            messaging_router(vec![make_target(Arc::new(FakeBackend::default()), false)]).is_ok()
        );
    }

    #[tokio::test]
    async fn reads_forward_exact_host_holding_and_cursor_validation() {
        let backend = Arc::new(FakeBackend::default());
        let state = http_state(make_target(backend.clone(), false));
        let response =
            read_overview(State(state.clone()), Path("acme".into()), get_request()).await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["rooms"][0]["id"], "room-1");
        let response = read_history(
            State(state.clone()),
            Path(("acme".into(), "room-1".into())),
            Query(HistoryQuery {
                before: Some("3".into()),
                after: Some("1".into()),
                limit: None,
            }),
            get_request(),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(backend.reads.lock().unwrap().len(), 1);
        let response = read_team_members(
            State(state),
            Path(("acme".into(), "team-1".into())),
            get_request(),
        )
        .await;
        let (_, value) = body(response).await;
        assert_eq!(value["data"]["isTeam"], true);
    }

    #[tokio::test]
    async fn commands_keep_the_owner_denials_and_require_the_key() {
        let backend = Arc::new(FakeBackend::default());
        let state = http_state(make_target(backend.clone(), false));
        let response = send_message(
            State(state.clone()),
            Path(("acme".into(), "room-1".into())),
            post_request(
                r#"{"body":"Hello"}"#,
                Some("https://evil.test"),
                Some("messaging-key-0001"),
            ),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::FORBIDDEN);
        let response = send_message(
            State(state.clone()),
            Path(("acme".into(), "room-1".into())),
            post_request(
                r#"{"body":"Hello"}"#,
                Some("https://acme.example.test"),
                None,
            ),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::UNPROCESSABLE_ENTITY);
        assert!(backend.commands.lock().unwrap().is_empty());
        let response = send_message(
            State(state.clone()),
            Path(("acme".into(), "room-1".into())),
            post_request(
                r#"{"body":"Hello"}"#,
                Some("https://acme.example.test"),
                Some("messaging-key-0001"),
            ),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["seq"], 3);
        let response = create_channel(
            State(state),
            Path(("acme".into(), "team-1".into())),
            post_request(
                r#"{"name":"Ops","private":true}"#,
                Some("https://acme.example.test"),
                Some("messaging-key-0002"),
            ),
        )
        .await;
        assert_eq!(body(response).await.0, StatusCode::OK);
        assert_eq!(backend.commands.lock().unwrap()[1].2, "team".to_owned());
    }

    #[tokio::test]
    async fn attachments_fail_closed_without_files_and_bind_when_present() {
        let backend = Arc::new(FakeBackend::default());
        let response = bind_attachment(
            State(http_state(make_target(backend.clone(), false))),
            Path(("acme".into(), "room-1".into())),
            post_request(
                r#"{"attachmentSetIds":["set-1"]}"#,
                Some("https://acme.example.test"),
                Some("messaging-key-0003"),
            ),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::SERVICE_UNAVAILABLE);
        assert_eq!(value["error"]["code"], "FILES_PORT_UNAVAILABLE");
        assert!(backend.attachments.lock().unwrap().is_empty());

        let response = bind_attachment(
            State(http_state(make_target(backend.clone(), true))),
            Path(("acme".into(), "room-1".into())),
            post_request(
                r#"{"attachmentSetIds":["set-1"]}"#,
                Some("https://acme.example.test"),
                Some("messaging-key-0003"),
            ),
        )
        .await;
        let (status, value) = body(response).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(value["data"]["status"], "pending_scan");
        assert_eq!(backend.attachments.lock().unwrap().as_slice(), ["room-1:1"]);
    }

    #[tokio::test]
    async fn file_content_is_bounded_and_served_with_safe_headers() {
        let backend = Arc::new(FakeBackend::default());
        let response = read_file(
            State(http_state(make_target(backend.clone(), true))),
            Path(("acme".into(), "file-1".into())),
            Query(FileQuery {
                variant: Some("preview".into()),
            }),
            get_request(),
        )
        .await;
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(response.headers()[header::CONTENT_TYPE], "image/png");
        assert_eq!(
            response.headers()[header::CONTENT_DISPOSITION],
            "attachment; filename=\"shot.png\""
        );
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        assert_eq!(&bytes[..], b"png");
        assert_eq!(
            backend.attachments.lock().unwrap().as_slice(),
            ["file-1:preview"]
        );
    }
}
