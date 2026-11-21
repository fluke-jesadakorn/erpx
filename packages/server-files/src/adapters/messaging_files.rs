//! Files-owned Messaging attachment port.
//!
//! Messaging authorizes the current Account session and conversation membership
//! through [`MessagingFilesAudience`]. This adapter then enforces the Files
//! quarantine/scan lifecycle, binds only current draft `message_attachment`
//! sets, and reads bounded original/preview bytes through the injected private
//! store. It never duplicates storage, scanning or quarantine logic.
use crate::application::canonical_original::OriginalByteStore;
use erpx_server_collaboration::public::messaging_http::{
    MessagingFileBinding, MessagingFileContent, MessagingFilesBindRequest,
    MessagingFilesContentRequest, MessagingFilesPort, MessagingHttpFuture,
};
use erpx_server_runtime::public::http_support::HeaderMap;
use erpx_server_runtime::public::postgres::{
    DatabasePool, PgTransactionError, PgUnitOfWork, TransactionOptions,
};
use erpx_server_shared::public::{BusinessError, Clock, CompanyId, FailureKind};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, Row};
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;

pub type MessagingFilesFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// The maximum message attachment size accepted by the Messaging/Files schema.
pub const MAX_MESSAGING_FILE_BYTES: usize = 10 * 1024 * 1024;

fn unavailable() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "FILES_MESSAGING_UNAVAILABLE",
        "The private conversation file is temporarily unavailable.",
    )
}

fn not_found() -> BusinessError {
    BusinessError::new(
        FailureKind::NotFound,
        "MESSAGING_FILE_NOT_FOUND",
        "The conversation file is unavailable.",
    )
}

fn changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MESSAGING_ATTACHMENT_CHANGED",
        "The original conversation file or current account changed.",
    )
}

fn scan_pending() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "MESSAGING_ATTACHMENT_SCAN_REQUIRED",
        "Every message file must pass malware scanning before it can be bound.",
    )
}

async fn lock_write_sources(c: &mut PgConnection) -> Result<(), PgTransactionError<BusinessError>> {
    sqlx::query(
        "LOCK TABLE attachment_sets,attachment_links,attachments,attachment_events,messaging_files \
         IN SHARE ROW EXCLUSIVE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

async fn lock_read_sources(c: &mut PgConnection) -> Result<(), PgTransactionError<BusinessError>> {
    sqlx::query(
        "LOCK TABLE attachment_sets,attachment_links,attachments,messaging_files IN SHARE MODE",
    )
    .execute(&mut *c)
    .await?;
    Ok(())
}

fn business(error: PgTransactionError<BusinessError>) -> BusinessError {
    match error {
        PgTransactionError::Business(error) => error,
        PgTransactionError::Persistence(_) => unavailable(),
    }
}

/// Composition-owned audience resolver. It re-resolves the exact current
/// Account session from the passed request headers and rechecks current
/// conversation membership. It must not commit the caller's unit.
pub trait MessagingFilesAudience: Send + Sync {
    fn current<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        host: &'a str,
        holding_slug: &'a str,
        headers: &'a HeaderMap,
    ) -> MessagingFilesFuture<'a, MessagingFilesActor>;

    fn require_room<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        actor: &'a MessagingFilesActor,
        room_id: &'a str,
    ) -> MessagingFilesFuture<'a, ()>;
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MessagingFilesActor {
    pub company_id: CompanyId,
    pub user_id: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MessagingFileVariant {
    Original,
    Preview,
}

impl MessagingFileVariant {
    pub fn parse(value: &str) -> Result<Self, BusinessError> {
        match value {
            "original" => Ok(Self::Original),
            "preview" => Ok(Self::Preview),
            _ => Err(BusinessError::invalid(
                "MESSAGING_INPUT_INVALID",
                "Choose an original or preview file.",
            )),
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Original => "original",
            Self::Preview => "preview",
        }
    }
}

/// Pure quarantine/scan policy shared by bind and content reads.
pub fn valid_messaging_attachment_source(
    source_state: Option<&str>,
    scanned_at: Option<i64>,
    scanner_definition_at: Option<i64>,
    revoked_at: Option<i64>,
    destroyed_at: Option<i64>,
    bytes: Option<i64>,
    checksum_sha256: Option<&str>,
    media_type: Option<&str>,
) -> bool {
    source_state == Some("available")
        && scanned_at.is_some()
        && scanner_definition_at.is_some()
        && revoked_at.is_none()
        && destroyed_at.is_none()
        && bytes.is_some_and(|value| (1..=MAX_MESSAGING_FILE_BYTES as i64).contains(&value))
        && checksum_sha256.is_some_and(|value| {
            value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
        })
        && media_type.is_some_and(|value| !value.is_empty())
}

fn valid_command(value: &str) -> bool {
    !value.is_empty() && value.len() <= 200 && !value.chars().any(char::is_control)
}

fn command_key(
    headers: &HeaderMap,
    host: &str,
    holding_slug: &str,
    room_id: &str,
    message_id: Option<&str>,
    set_ids: &[String],
) -> String {
    if let Some(key) = headers
        .get("idempotency-key")
        .and_then(|value| value.to_str().ok())
        .filter(|value| valid_command(value))
    {
        return format!("messaging-attachment-{key}");
    }
    let mut sorted = set_ids.to_vec();
    sorted.sort();
    let material = format!(
        "{host}\0{holding_slug}\0{room_id}\0{}\0{}",
        message_id.unwrap_or_default(),
        sorted.join("\0")
    );
    format!(
        "messaging-attachment-{}",
        &hex::encode(Sha256::digest(material.as_bytes()))[..32]
    )
}

fn event_id(command: &str, set_id: &str) -> String {
    format!(
        "attachment-bind-{}",
        &hex::encode(Sha256::digest(format!("{command}\0{set_id}").as_bytes()))[..32]
    )
}

/// Files-owned Messaging attachment adapter. Session/audience resolution is
/// supplied by composition through [`MessagingFilesAudience`]; the port owns
/// only Files ledger checks, binding and the bounded byte read.
pub struct PgMessagingFiles<A, S> {
    unit: PgUnitOfWork,
    audience: Arc<A>,
    store: Arc<S>,
    clock: Arc<dyn Clock + Send + Sync>,
}

impl<A, S> PgMessagingFiles<A, S>
where
    A: MessagingFilesAudience + 'static,
    S: OriginalByteStore + 'static,
{
    pub fn new(
        pool: DatabasePool,
        audience: Arc<A>,
        store: Arc<S>,
        clock: Arc<dyn Clock + Send + Sync>,
    ) -> Self {
        Self {
            unit: PgUnitOfWork::new(pool),
            audience,
            store,
            clock,
        }
    }
}

impl<A, S> MessagingFilesPort for PgMessagingFiles<A, S>
where
    A: MessagingFilesAudience + 'static,
    S: OriginalByteStore + 'static,
{
    fn bind<'a>(
        &'a self,
        request: MessagingFilesBindRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingFileBinding> {
        Box::pin(async move {
            let host = request.host.to_owned();
            let holding_slug = request.holding_slug.to_owned();
            let headers = request.headers.clone();
            let room_id = request.room_id.to_owned();
            let set_ids = request.attachment_set_ids.to_vec();
            let message_id = request.message_id.map(str::to_owned);
            let audience = Arc::clone(&self.audience);
            let now = self.clock.now().get();
            let unit = self.unit.clone();
            unit.run_once(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    bind_attachment(
                        connection,
                        audience.as_ref(),
                        &host,
                        &holding_slug,
                        &headers,
                        &room_id,
                        &set_ids,
                        message_id.as_deref(),
                        now,
                    )
                    .await
                })
            })
            .await
            .map_err(business)
        })
    }

    fn content<'a>(
        &'a self,
        request: MessagingFilesContentRequest<'a>,
    ) -> MessagingHttpFuture<'a, MessagingFileContent> {
        Box::pin(async move {
            let host = request.host.to_owned();
            let holding_slug = request.holding_slug.to_owned();
            let headers = request.headers.clone();
            let file_id = request.file_id.to_owned();
            let variant = MessagingFileVariant::parse(request.variant)?;
            let audience = Arc::clone(&self.audience);
            let store = Arc::clone(&self.store);
            let now = self.clock.now().get();
            let unit = self.unit.clone();
            unit.run_once(TransactionOptions::default(), move |connection| {
                Box::pin(async move {
                    read_content(
                        connection,
                        audience.as_ref(),
                        store.as_ref(),
                        &host,
                        &holding_slug,
                        &headers,
                        &file_id,
                        variant,
                        now,
                    )
                    .await
                })
            })
            .await
            .map_err(business)
        })
    }
}

#[allow(clippy::too_many_arguments)]
async fn bind_attachment<A: MessagingFilesAudience>(
    c: &mut PgConnection,
    audience: &A,
    host: &str,
    holding_slug: &str,
    headers: &HeaderMap,
    room_id: &str,
    set_ids: &[String],
    message_id: Option<&str>,
    now: i64,
) -> Result<MessagingFileBinding, PgTransactionError<BusinessError>> {
    if set_ids.is_empty()
        || set_ids.len() > 10
        || set_ids
            .iter()
            .collect::<std::collections::BTreeSet<_>>()
            .len()
            != set_ids.len()
        || !(0..=9_007_199_254_740_991).contains(&now)
    {
        return Err(BusinessError::invalid(
            "MESSAGING_INPUT_INVALID",
            "Choose current attachment sets to bind.",
        )
        .into());
    }
    lock_write_sources(c).await?;
    let actor = audience.current(c, host, holding_slug, headers).await?;
    let room: Option<(String, i64)> = sqlx::query_as(
        "SELECT company_id,version FROM messaging_rooms WHERE id=$1 AND company_id=$2",
    )
    .bind(room_id)
    .bind(actor.company_id.as_str())
    .fetch_optional(&mut *c)
    .await?;
    let Some((_, room_version)) = room else {
        return Err(not_found().into());
    };
    audience.require_room(c, &actor, room_id).await?;
    if let Some(message_id) = message_id {
        let exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM messaging_messages WHERE id=$1 AND room_id=$2)",
        )
        .bind(message_id)
        .bind(room_id)
        .fetch_one(&mut *c)
        .await?;
        if !exists {
            return Err(not_found().into());
        }
    }
    let sets = sqlx::query(
        "SELECT id,component_ref,expected_target_version,policy_version FROM attachment_sets \
         WHERE id=ANY($1) AND company_id=$2 AND target_type='messaging_room' AND target_id=$3 \
         AND component_kind='message' AND purpose_key='message_attachment' \
         AND state='draft' AND policy_version=1",
    )
    .bind(set_ids)
    .bind(actor.company_id.as_str())
    .bind(room_id)
    .fetch_all(&mut *c)
    .await?;
    if sets.len() != set_ids.len() {
        return Err(changed().into());
    }
    let mut files = Vec::with_capacity(sets.len());
    for set in &sets {
        let set_id: String = set.get("id");
        let component_ref: Option<String> = set.get("component_ref");
        let expected_version: i64 = set.get("expected_target_version");
        let component_ref = component_ref.ok_or_else(changed)?;
        if expected_version != room_version {
            return Err(changed().into());
        }
        let links = sqlx::query(
            "SELECT l.id,l.attachment_id,l.state link_state,a.source_state,a.checksum_sha256,\
             a.media_type,a.bytes,a.revoked_at,a.destroyed_at,a.scanned_at,a.scanner_definition_at \
             FROM attachment_links l \
             LEFT JOIN attachments a ON a.id=l.attachment_id AND a.company_id=$2 \
             WHERE l.set_id=$1 ORDER BY l.created_at,l.id",
        )
        .bind(&set_id)
        .bind(actor.company_id.as_str())
        .fetch_all(&mut *c)
        .await?;
        if links.len() != 1 {
            return Err(changed().into());
        }
        let link = &links[0];
        let link_id: String = link.get("id");
        let link_state: String = link.get("link_state");
        let source_id: Option<String> = link.get("attachment_id");
        let source_id = source_id.ok_or_else(changed)?;
        if link_state != "draft"
            || !valid_messaging_attachment_source(
                link.get::<Option<String>, _>("source_state").as_deref(),
                link.get("scanned_at"),
                link.get("scanner_definition_at"),
                link.get("revoked_at"),
                link.get("destroyed_at"),
                link.get("bytes"),
                link.get::<Option<String>, _>("checksum_sha256").as_deref(),
                link.get::<Option<String>, _>("media_type").as_deref(),
            )
        {
            return Err(scan_pending().into());
        }
        let file: Option<String> = sqlx::query_scalar(
            "SELECT id FROM messaging_files WHERE id=$1 AND room_id=$2 AND created_by=$3 \
             AND attachment_set_id=$4 AND attachment_link_id=$5 AND attachment_source_id=$6 \
             AND ($7::text IS NULL OR message_id IS NULL OR message_id=$7)",
        )
        .bind(&component_ref)
        .bind(room_id)
        .bind(&actor.user_id)
        .bind(&set_id)
        .bind(&link_id)
        .bind(&source_id)
        .bind(message_id)
        .fetch_optional(&mut *c)
        .await?;
        if file.is_none() {
            return Err(changed().into());
        }
        files.push((set_id, component_ref, link_id));
    }
    let command = command_key(headers, host, holding_slug, room_id, message_id, set_ids);
    let binding_id = message_id.unwrap_or(room_id);
    for (set_id, component_ref, link_id) in &files {
        let updated = sqlx::query(
            "UPDATE attachment_links SET state='bound',bound_at=$1,binding_type='message_attachment',\
             binding_id=$2 WHERE id=$3 AND set_id=$4 AND state='draft'",
        )
        .bind(now)
        .bind(binding_id)
        .bind(link_id)
        .bind(set_id)
        .execute(&mut *c)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(changed().into());
        }
        let updated = sqlx::query(
            "UPDATE attachment_sets SET state='bound',bound_at=$1,bound_command_id=$2,\
             updated_at=$1 WHERE id=$3 AND company_id=$4 AND state='draft'",
        )
        .bind(now)
        .bind(&command)
        .bind(set_id)
        .bind(actor.company_id.as_str())
        .execute(&mut *c)
        .await?
        .rows_affected();
        if updated != 1 {
            return Err(changed().into());
        }
        if let Some(message_id) = message_id {
            let updated = sqlx::query(
                "UPDATE messaging_files SET message_id=$1 WHERE id=$2 AND room_id=$3 \
                 AND attachment_set_id=$4 AND (message_id IS NULL OR message_id=$1)",
            )
            .bind(message_id)
            .bind(component_ref)
            .bind(room_id)
            .bind(set_id)
            .execute(&mut *c)
            .await?
            .rows_affected();
            if updated != 1 {
                return Err(changed().into());
            }
        }
        sqlx::query(
            "INSERT INTO attachment_events(id,company_id,set_id,actor_id,action,details_json,created_at) \
             VALUES($1,$2,$3,$4,'set.bound',$5,$6)",
        )
        .bind(event_id(&command, set_id))
        .bind(actor.company_id.as_str())
        .bind(set_id)
        .bind(&actor.user_id)
        .bind(
            serde_json::json!({
                "bindingType": "message_attachment",
                "bindingId": binding_id,
                "roomId": room_id,
                "messageId": message_id,
                "fileId": component_ref,
                "linkId": link_id,
            })
            .to_string(),
        )
        .bind(now)
        .execute(&mut *c)
        .await?;
    }
    audience.require_room(c, &actor, room_id).await?;
    Ok(MessagingFileBinding {
        id: binding_id.to_owned(),
        version: room_version,
        status: "bound",
    })
}

#[allow(clippy::too_many_arguments)]
async fn read_content<A: MessagingFilesAudience, S: OriginalByteStore>(
    c: &mut PgConnection,
    audience: &A,
    store: &S,
    host: &str,
    holding_slug: &str,
    headers: &HeaderMap,
    file_id: &str,
    variant: MessagingFileVariant,
    now: i64,
) -> Result<MessagingFileContent, PgTransactionError<BusinessError>> {
    if file_id.is_empty()
        || file_id.len() > 200
        || file_id.chars().any(char::is_control)
        || !(0..=9_007_199_254_740_991).contains(&now)
    {
        return Err(not_found().into());
    }
    lock_read_sources(c).await?;
    let actor = audience.current(c, host, holding_slug, headers).await?;
    let row = sqlx::query(
        "SELECT f.room_id,f.created_by,f.original_name file_name,f.media_type file_type,\
         f.bytes file_bytes,f.checksum file_checksum,f.message_id,l.id link_id,l.state link_state,\
         s.id set_id,s.state set_state,s.company_id,s.target_type,s.target_id,s.purpose_key,\
         s.component_kind,l.binding_type,l.binding_id,a.id source_id,a.source_state,\
         a.original_name,a.media_type,a.bytes,a.checksum_sha256,a.revoked_at,a.destroyed_at,\
         a.scanned_at,a.scanner_definition_at,a.object_key,a.preview_object_key,\
         a.preview_bytes,a.preview_checksum_sha256,a.preview_media_type \
         FROM messaging_files f \
         JOIN attachment_links l ON l.id=f.attachment_link_id \
         JOIN attachment_sets s ON s.id=l.set_id AND s.id=f.attachment_set_id \
         LEFT JOIN attachments a ON a.id=l.attachment_id AND a.id=f.attachment_source_id \
           AND a.company_id=s.company_id \
         WHERE f.id=$1 AND l.attachment_id=f.attachment_source_id",
    )
    .bind(file_id)
    .fetch_optional(&mut *c)
    .await?
    .ok_or_else(not_found)?;
    let company: String = row.get("company_id");
    let room_id: String = row.get("room_id");
    let target_id: String = row.get("target_id");
    let target_type: String = row.get("target_type");
    let purpose_key: String = row.get("purpose_key");
    let component_kind: String = row.get("component_kind");
    let set_state: String = row.get("set_state");
    let link_state: String = row.get("link_state");
    let binding_type: Option<String> = row.get("binding_type");
    let binding_id: Option<String> = row.get("binding_id");
    let created_by: String = row.get("created_by");
    let message_id: Option<String> = row.get("message_id");
    let source_state: Option<String> = row.get("source_state");
    let scanned_at: Option<i64> = row.get("scanned_at");
    let scanner_definition_at: Option<i64> = row.get("scanner_definition_at");
    let revoked_at: Option<i64> = row.get("revoked_at");
    let destroyed_at: Option<i64> = row.get("destroyed_at");
    let source_bytes: Option<i64> = row.get("bytes");
    let source_checksum: Option<String> = row.get("checksum_sha256");
    let source_media_type: Option<String> = row.get("media_type");
    if company != actor.company_id.as_str()
        || target_type != "messaging_room"
        || target_id != room_id
        || purpose_key != "message_attachment"
        || component_kind != "message"
        || set_state != "bound"
        || link_state != "bound"
        || binding_type.as_deref() != Some("message_attachment")
        || binding_id.is_none()
        || message_id.is_none() && created_by != actor.user_id
        || !valid_messaging_attachment_source(
            source_state.as_deref(),
            scanned_at,
            scanner_definition_at,
            revoked_at,
            destroyed_at,
            source_bytes,
            source_checksum.as_deref(),
            source_media_type.as_deref(),
        )
    {
        return Err(not_found().into());
    }
    audience.require_room(c, &actor, &room_id).await?;
    let (object_key, bytes, checksum, media_type, filename) = match variant {
        MessagingFileVariant::Original => {
            let object_key: Option<String> = row.get("object_key");
            let checksum: Option<String> = row.get("checksum_sha256");
            let media_type: Option<String> = row.get("media_type");
            let filename: Option<String> = row.get("original_name");
            (
                object_key.ok_or_else(not_found)?,
                source_bytes.ok_or_else(not_found)?,
                checksum.ok_or_else(not_found)?,
                media_type.ok_or_else(not_found)?,
                filename
                    .or_else(|| row.get::<Option<String>, _>("file_name"))
                    .unwrap_or_else(|| "file".into()),
            )
        }
        MessagingFileVariant::Preview => {
            let object_key: Option<String> = row.get("preview_object_key");
            let bytes: Option<i64> = row.get("preview_bytes");
            let checksum: Option<String> = row.get("preview_checksum_sha256");
            let media_type: Option<String> = row.get("preview_media_type");
            (
                object_key.ok_or_else(not_found)?,
                bytes.ok_or_else(not_found)?,
                checksum.ok_or_else(not_found)?,
                media_type.ok_or_else(not_found)?,
                row.get::<Option<String>, _>("file_name")
                    .unwrap_or_else(|| "preview".into()),
            )
        }
    };
    if !(1..=MAX_MESSAGING_FILE_BYTES as i64).contains(&bytes) {
        return Err(not_found().into());
    }
    let body = store
        .read_verified(
            &object_key,
            bytes as usize,
            &checksum,
            MAX_MESSAGING_FILE_BYTES,
        )
        .await
        .map_err(|error: BusinessError| PgTransactionError::from(error))?;
    if body.len() != bytes as usize {
        return Err(changed().into());
    }
    audience.require_room(c, &actor, &room_id).await?;
    sqlx::query(
        "INSERT INTO attachment_access_events(id,company_id,attachment_id,link_id,actor_id,\
         variant,outcome,created_at) VALUES($1,$2,$3,$4,$5,$6,'allowed',$7)",
    )
    .bind(format!("attachment-access-{}", uuid::Uuid::new_v4()))
    .bind(actor.company_id.as_str())
    .bind(
        row.get::<Option<String>, _>("source_id")
            .ok_or_else(not_found)?,
    )
    .bind(row.get::<String, _>("link_id"))
    .bind(&actor.user_id)
    .bind(variant.as_str())
    .bind(now)
    .execute(&mut *c)
    .await?;
    Ok(MessagingFileContent {
        media_type,
        filename,
        body,
    })
}
