//! Native Ecommerce channel-draft use case.
//!
//! The caller's serializable Holding transaction supplies the owner unit. The
//! application resolves the canonical command receipt before a fresh write,
//! inserts one draft channel, writes the receipt/audit/outbox in the same unit
//! and validates persisted readback before returning.
use crate::application::{AuditEntry, EcommerceCommandRepository, OutboxEntry, ReceiptEntry};
use crate::domain::actor::EcommerceActor;
use crate::domain::channel::{CreateChannel, OPERATION, new_receipt, replay, validate_hash};
use crate::domain::command::{
    EcommerceReceipt, idempotency_conflict, readback_invalid, request_invalid,
};
use erpx_server_shared::public::TimestampMillis;

pub async fn create_channel<R: EcommerceCommandRepository>(
    actor: &EcommerceActor,
    command: &CreateChannel,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<EcommerceReceipt, R::Error> {
    actor.require_write(now)?;
    let validated = command.validate()?;
    let draft = command.draft(validated);
    let identity = draft.identity(actor);
    let request_hash = repository.digest(&identity).await?;
    validate_hash(&request_hash)?;
    if let Some(prior) = replay(
        repository.receipt(command.key().as_str()).await?,
        &request_hash,
        &draft.id,
    )? {
        return Ok(prior);
    }
    if !repository.insert_channel(actor, &draft, now).await? {
        return Err(idempotency_conflict().into());
    }
    let receipt = new_receipt(&draft.id);
    let result_json = serde_json::to_string(&receipt.to_wire()).map_err(|_| request_invalid())?;
    if !repository
        .insert_receipt(
            &ReceiptEntry {
                key: command.key().as_str(),
                operation: OPERATION,
                actor,
                request_hash: &request_hash,
                record_id: &draft.id,
                result_json: &result_json,
            },
            now,
        )
        .await?
    {
        return Err(idempotency_conflict().into());
    }
    let authority = actor.authority_json();
    repository
        .audit(
            actor,
            &AuditEntry {
                key: command.key().as_str(),
                operation: OPERATION,
                record_type: "ecommerce_channel",
                record_id: &draft.id,
                version: 1,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    repository
        .outbox(
            &OutboxEntry {
                key: command.key().as_str(),
                operation: OPERATION,
                record_type: "ecommerce_channel",
                record_id: &draft.id,
                version: 1,
                state: "draft",
                authority_json: &authority,
            },
            now,
        )
        .await?;
    let readback = repository
        .channel_readback(&draft.id, &actor.company_id)
        .await?
        .ok_or_else(readback_invalid)?;
    draft.validate_readback(actor, &readback)?;
    Ok(receipt)
}
