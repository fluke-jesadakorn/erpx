//! Native Ecommerce channel/listing review use case.
//!
//! Review requires the current `commerce.sales` and `commerce.approve`
//! permissions, the `ecommerce` module, a live work context and a Company
//! assignment. The owner row is read `FOR UPDATE`, the transition is validated
//! against the current status/version and creator, the exact expected-version
//! CAS is applied, then receipt/audit/outbox and persisted readback are written
//! in the same unit.
use crate::application::{AuditEntry, EcommerceCommandRepository, OutboxEntry, ReceiptEntry};
use crate::domain::actor::EcommerceActor;
use crate::domain::command::{
    EcommerceReceipt, idempotency_conflict, not_found, readback_invalid, request_invalid,
    version_conflict,
};
use crate::domain::review::{ReviewCommand, replay, validate_hash};
use erpx_server_shared::public::TimestampMillis;

pub async fn review_ecommerce_record<R: EcommerceCommandRepository>(
    actor: &EcommerceActor,
    command: &ReviewCommand,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<EcommerceReceipt, R::Error> {
    actor.require_write(now)?;
    actor.require_company_review()?;
    let review = command.validate()?;
    let identity = review.identity(actor);
    let request_hash = repository.digest(&identity).await?;
    validate_hash(&request_hash)?;
    if let Some(prior) = replay(
        repository.receipt(command.key.as_str()).await?,
        &request_hash,
        &review,
    )? {
        return Ok(prior);
    }
    let source = repository
        .review_source(review.kind, &review.id, &actor.company_id)
        .await?
        .ok_or_else(not_found)?;
    review.validate_source(actor, &source)?;
    if !repository
        .commit_review(&review, &actor.company_id, actor, now)
        .await?
    {
        return Err(version_conflict().into());
    }
    let receipt = review.receipt();
    let operation = review.operation();
    let result_json = serde_json::to_string(&receipt.to_wire()).map_err(|_| request_invalid())?;
    if !repository
        .insert_receipt(
            &ReceiptEntry {
                key: command.key.as_str(),
                operation: &operation,
                actor,
                request_hash: &request_hash,
                record_id: &review.id,
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
                key: command.key.as_str(),
                operation: &operation,
                record_type: review.kind.record_type(),
                record_id: &review.id,
                version: receipt.version,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    repository
        .outbox(
            &OutboxEntry {
                key: command.key.as_str(),
                operation: &operation,
                record_type: review.kind.record_type(),
                record_id: &review.id,
                version: receipt.version,
                state: receipt.status.as_str(),
                authority_json: &authority,
            },
            now,
        )
        .await?;
    let readback = repository
        .review_readback(review.kind, &review.id, &actor.company_id)
        .await?
        .ok_or_else(readback_invalid)?;
    review.validate_readback(actor, &readback)?;
    Ok(receipt)
}
