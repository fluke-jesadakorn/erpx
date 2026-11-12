//! Native Messaging unsent-file discard on the caller's serializable unit.
//!
//! Messaging owns the conversation guard, the `messaging.file-discard` receipt
//! and the deferred-effects fence. The Files owner port owns the attachment
//! link/event withdrawal and is called in-process on the same connection, so a
//! discard cannot commit a receipt without the Files audit (or the reverse).
//! Nothing here begins, commits or retries a transaction.
use crate::adapters::command_ledger::{self, fingerprint, source_changed};
use crate::adapters::messaging_native::read_room;
use crate::domain::messaging_file_discard::{
    DiscardFileFacts, file_not_found, normalize_discard_reason, receipt_invalid,
    require_discardable, room_forbidden, source_changed as file_source_changed, valid_file_id,
};
use crate::domain::messaging_native::{MessagingActor, require_member, room_not_found};
use erpx_server_runtime::public::postgres::PgTransactionError;
use erpx_server_shared::public::{BusinessError, CompanyId};
use serde::Serialize;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

type UnitError = PgTransactionError<BusinessError>;

pub const DISCARD_OPERATION: &str = "messaging.file-discard";

pub type MessagingFileDiscardFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Exact bounded facts the Files owner needs. The port must withdraw only this
/// Company's draft link in the caller's transaction and never mint authority.
pub struct MessagingFileWithdrawRequest<'a> {
    pub actor_id: &'a str,
    pub company_id: &'a str,
    pub room_id: &'a str,
    pub link_id: &'a str,
    pub reason: &'a str,
    pub command_key: &'a str,
    pub now: i64,
}

/// Composition-supplied Files owner port. It is invoked inside the Messaging
/// serializable unit and must not open a second transaction.
pub trait MessagingFileDiscardPort: Send + Sync {
    fn withdraw<'a>(
        &'a self,
        connection: &'a mut PgConnection,
        request: MessagingFileWithdrawRequest<'a>,
    ) -> MessagingFileDiscardFuture<'a, ()>;
}

/// Identical receipt for every replay of the same command key.
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct DiscardReceipt {
    pub id: String,
    pub state: &'static str,
}

#[derive(Debug)]
#[expect(
    dead_code,
    reason = "The Debug form is the stable owner request fingerprint"
)]
struct DiscardFingerprint<'a> {
    id: &'a str,
    reason: &'a str,
}

#[derive(Debug)]
struct FileRow {
    room_id: String,
    created_by: String,
    message_id: Option<String>,
    attachment_link_id: Option<String>,
    room_version: i64,
}

fn decode_receipt(value: &serde_json::Value, file_id: &str) -> Result<DiscardReceipt, UnitError> {
    let id = value
        .get("id")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| UnitError::Business(receipt_invalid()))?;
    let state = value
        .get("state")
        .and_then(serde_json::Value::as_str)
        .ok_or_else(|| UnitError::Business(receipt_invalid()))?;
    if id != file_id || state != "withdrawn" {
        return Err(receipt_invalid().into());
    }
    Ok(DiscardReceipt {
        id: id.to_owned(),
        state: "withdrawn",
    })
}

/// Read the message file in the exact Company and recheck current room
/// membership. A missing file and a foreign file are indistinguishable.
async fn read_file(
    connection: &mut PgConnection,
    actor: &MessagingActor,
    file_id: &str,
) -> Result<FileRow, UnitError> {
    let company = CompanyId::new(actor.company_id.clone())?;
    let row = sqlx::query_as::<_, (String, String, Option<String>, Option<String>, i64)>(
        "SELECT f.room_id,f.created_by,f.message_id,f.attachment_link_id,r.version \
         FROM messaging_files f JOIN messaging_rooms r ON r.id=f.room_id \
         WHERE f.id=$1 AND r.company_id=$2",
    )
    .bind(file_id)
    .bind(company.as_str())
    .fetch_optional(&mut *connection)
    .await?
    .ok_or_else(|| UnitError::Business(file_not_found()))?;
    let room = read_room(connection, &company, &row.0)
        .await?
        .ok_or_else(|| UnitError::Business(room_not_found()))?;
    require_member(actor, &room).map_err(|_| UnitError::Business(room_forbidden()))?;
    Ok(FileRow {
        room_id: row.0,
        created_by: row.1,
        message_id: row.2,
        attachment_link_id: row.3,
        room_version: row.4,
    })
}

/// Discard one original unsent attachment.
///
/// The caller owns the SERIALIZABLE transaction and the purchase admission.
/// This function resolves replay first, refuses a file that is already bound to
/// a message or belongs to another user, withdraws the Files link through the
/// owner port, then writes the single Messaging receipt/audit/outbox row and
/// rechecks the conversation version that was authorized.
pub async fn discard_file<P: MessagingFileDiscardPort + ?Sized>(
    connection: &mut PgConnection,
    actor: &MessagingActor,
    assignment_id: &str,
    command_key: &str,
    file_id: &str,
    reason: &str,
    now: i64,
    port: &P,
) -> Result<DiscardReceipt, UnitError> {
    command_ledger::require_serializable(connection, "MESSAGING_SERIALIZABLE_REQUIRED").await?;
    if !valid_file_id(file_id) {
        return Err(UnitError::Business(
            crate::domain::messaging_file_discard::file_not_found(),
        ));
    }
    let reason = normalize_discard_reason(reason).map_err(UnitError::Business)?;

    // The retained TypeScript command re-reads the file and room before replay.
    // A moved conversation cannot inherit the earlier pre-read.
    let before = read_file(connection, actor, file_id).await?;
    let request = fingerprint(
        DISCARD_OPERATION,
        &DiscardFingerprint {
            id: file_id,
            reason: &reason,
        },
    );
    if let Some(value) = command_ledger::replay(
        connection,
        command_key,
        DISCARD_OPERATION,
        actor.user_id.as_str(),
        &before.room_id,
        &request,
    )
    .await?
    {
        return decode_receipt(&value, file_id);
    }

    let link_id = require_discardable(DiscardFileFacts {
        created_by: &before.created_by,
        message_id: before.message_id.as_deref(),
        attachment_link_id: before.attachment_link_id.as_deref(),
        actor: actor.user_id.as_str(),
    })
    .map_err(UnitError::Business)?
    .to_owned();

    port.withdraw(
        connection,
        MessagingFileWithdrawRequest {
            actor_id: actor.user_id.as_str(),
            company_id: actor.company_id.as_str(),
            room_id: &before.room_id,
            link_id: &link_id,
            reason: &reason,
            command_key,
            now,
        },
    )
    .await?;

    // The conversation version and exact file identity that were authorized
    // may not move under the Files write.
    let after = read_file(connection, actor, file_id).await?;
    if after.room_id != before.room_id
        || after.room_version != before.room_version
        || after.created_by != before.created_by
        || after.message_id.is_some()
        || after.attachment_link_id.as_deref() != Some(link_id.as_str())
    {
        return Err(UnitError::Business(file_source_changed()));
    }

    let receipt = DiscardReceipt {
        id: file_id.to_owned(),
        state: "withdrawn",
    };
    let value = serde_json::to_value(&receipt).map_err(|_| source_changed(
        "COLLABORATION_RECEIPT_INVALID",
        "The Messaging result could not be encoded.",
    ))?;
    command_ledger::write_ledger(
        connection,
        command_ledger::LedgerRows {
            key: command_key,
            operation: DISCARD_OPERATION,
            actor: actor.user_id.as_str(),
            assignment: assignment_id,
            record_id: &before.room_id,
            record_version: before.room_version,
            fingerprint: &request,
            result: &value,
            record_type: "messaging_file",
            purpose: DISCARD_OPERATION,
            event_type: DISCARD_OPERATION,
            now,
        },
    )
    .await?;
    Ok(receipt)
}
