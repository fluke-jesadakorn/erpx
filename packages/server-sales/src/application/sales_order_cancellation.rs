//! Native Sales confirmed sales-order cancellation use case.
//!
//! The caller owns the retained serializable Holding transaction. The
//! application resolves the canonical receipt before a fresh write, re-reads
//! current authority, locks the scoped order, applies one exact expected-version
//! cancellation, releases Inventory reservations, writes receipt/audit/outbox
//! in the same unit and validates persisted readback before returning.
use crate::domain::sales_order_cancellation::{
    CancellationReceiptRecord, OPERATION, SalesCancellationActor, SalesCancellationCommand,
    SalesCancellationPlan, SalesCancellationReceipt, SalesCancellationRow,
    authorize_confirmed_cancellation, idempotency_conflict, permission_required, readback_invalid,
    request_invalid, source_invalid, transition_conflict, validate_readback,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::future::Future;

pub type SalesCancellationFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub struct CancellationAuditEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub authority_json: &'a str,
}

pub struct CancellationOutboxEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub state: &'a str,
    pub authority_json: &'a str,
}

pub struct CancellationReceiptEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub actor: &'a SalesCancellationActor,
    pub request_hash: &'a str,
    pub record_id: &'a str,
    pub result_json: &'a str,
}

pub trait SalesCancellationRepository: Send {
    type Error: From<BusinessError> + Send;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a SalesCancellationActor,
        write: bool,
        approve: bool,
        now: TimestampMillis,
    ) -> SalesCancellationFuture<'a, bool, Self::Error>;

    fn lock_command<'a>(&'a mut self, key: &'a str)
    -> SalesCancellationFuture<'a, (), Self::Error>;

    fn digest<'a>(&'a mut self, value: &'a str)
    -> SalesCancellationFuture<'a, String, Self::Error>;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> SalesCancellationFuture<'a, Option<CancellationReceiptRecord>, Self::Error>;

    fn lock_order<'a>(
        &'a mut self,
        actor: &'a SalesCancellationActor,
        id: &'a str,
    ) -> SalesCancellationFuture<'a, Option<SalesCancellationRow>, Self::Error>;

    fn count_fulfillments<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> SalesCancellationFuture<'a, i64, Self::Error>;

    /// Inventory-owned reservation release for the exact scoped order. A
    /// repository without this port must fail closed.
    fn release_reservations<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
    ) -> SalesCancellationFuture<'a, (), Self::Error>;

    fn transition<'a>(
        &'a mut self,
        plan: &'a SalesCancellationPlan,
    ) -> SalesCancellationFuture<'a, bool, Self::Error>;

    fn insert_receipt<'a>(
        &'a mut self,
        entry: &'a CancellationReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> SalesCancellationFuture<'a, bool, Self::Error>;

    fn audit<'a>(
        &'a mut self,
        actor: &'a SalesCancellationActor,
        entry: &'a CancellationAuditEntry<'a>,
        now: TimestampMillis,
    ) -> SalesCancellationFuture<'a, (), Self::Error>;

    fn outbox<'a>(
        &'a mut self,
        entry: &'a CancellationOutboxEntry<'a>,
        now: TimestampMillis,
    ) -> SalesCancellationFuture<'a, (), Self::Error>;

    fn readback_order<'a>(
        &'a mut self,
        actor: &'a SalesCancellationActor,
        id: &'a str,
    ) -> SalesCancellationFuture<'a, Option<SalesCancellationRow>, Self::Error>;
}

fn validate_hash(value: &str) -> Result<(), BusinessError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(source_invalid());
    }
    Ok(())
}

fn replay(
    receipt: Option<CancellationReceiptRecord>,
    request_hash: &str,
    order_id: &str,
    expected_version: i64,
) -> Result<Option<SalesCancellationReceipt>, BusinessError> {
    let Some(receipt) = receipt else {
        return Ok(None);
    };
    if receipt.request_hash != request_hash {
        return Err(idempotency_conflict());
    }
    if receipt.record_id.as_deref() != Some(order_id) {
        return Err(source_invalid());
    }
    let parsed: SalesCancellationReceipt =
        serde_json::from_str(&receipt.result_json).map_err(|_| source_invalid())?;
    parsed.validate(order_id, expected_version)?;
    Ok(Some(parsed))
}

/// Cancel one confirmed sales order that has no delivery.
pub async fn cancel_confirmed_sales_order<R: SalesCancellationRepository>(
    actor: &SalesCancellationActor,
    command: &SalesCancellationCommand,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<SalesCancellationReceipt, R::Error> {
    actor.require_write(now)?;
    command.validate()?;
    repository.lock_command(command.key.as_str()).await?;
    let request_hash = repository.digest(&command.identity(actor)).await?;
    validate_hash(&request_hash)?;
    if let Some(prior) = replay(
        repository.receipt(command.key.as_str()).await?,
        &request_hash,
        command.order_id.as_str(),
        command.expected_version,
    )? {
        return Ok(prior);
    }
    if !repository
        .current_authority(actor, true, false, now)
        .await?
    {
        return Err(permission_required().into());
    }
    let order = repository
        .lock_order(actor, command.order_id.as_str())
        .await?
        .ok_or_else(crate::domain::sales_order_cancellation::not_found)?;
    // Confirmed cancellation always needs the independent approval grant.
    if !repository.current_authority(actor, true, true, now).await? {
        return Err(crate::domain::sales_order_cancellation::approval_required().into());
    }
    let fulfillment_count = repository
        .count_fulfillments(command.order_id.as_str())
        .await?;
    let plan = authorize_confirmed_cancellation(actor, command, &order, fulfillment_count, now)?;
    repository
        .release_reservations(&actor.company_id, &order.id)
        .await?;
    if !repository.transition(&plan).await? {
        return Err(transition_conflict().into());
    }
    let result_json = serde_json::to_string(&plan.receipt).map_err(|_| request_invalid())?;
    if !repository
        .insert_receipt(
            &CancellationReceiptEntry {
                key: command.key.as_str(),
                operation: OPERATION,
                actor,
                request_hash: &request_hash,
                record_id: &plan.order_id,
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
            &CancellationAuditEntry {
                key: command.key.as_str(),
                operation: OPERATION,
                record_type: "commerce_order",
                record_id: &plan.order_id,
                version: plan.receipt.version,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    repository
        .outbox(
            &CancellationOutboxEntry {
                key: command.key.as_str(),
                operation: OPERATION,
                record_type: "commerce_order",
                record_id: &plan.order_id,
                version: plan.receipt.version,
                state: &plan.next_state,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    let readback = repository
        .readback_order(actor, &plan.order_id)
        .await?
        .ok_or_else(readback_invalid)?;
    validate_readback(&plan, &readback)?;
    Ok(plan.receipt)
}
