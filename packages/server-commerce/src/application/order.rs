//! Native Commerce quotation/sales-order read and lifecycle use cases.
//!
//! The caller owns the retained serializable Holding transaction. Reads resolve
//! the current IAM assignment before owner rows. Commands resolve the canonical
//! receipt before a fresh write, lock the scoped order, apply one exact
//! expected-version transition, write receipt/audit/outbox in the same unit and
//! validate persisted readback before returning.
use crate::domain::order::{
    CommerceOrderActor, CommerceOrderReceipt, ConvertedReadback, OrderCommandReceipt, OrderState,
    SalesOrderDocument, SalesOrderLineRow, SalesOrderRow, TransitionCommand, TransitionPlan,
    authorize_transition, idempotency_conflict, not_found, permission_required, readback_invalid,
    replay, request_invalid, transition_identity, validate_conversion_readback, validate_document,
    validate_hash, validate_readback, version_conflict,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis};
use std::future::Future;

pub type CommerceOrderFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub struct AuditEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub authority_json: &'a str,
}

pub struct OutboxEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub state: &'a str,
    pub sales_id: Option<&'a str>,
    pub authority_json: &'a str,
}

pub struct ReceiptEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub actor: &'a CommerceOrderActor,
    pub request_hash: &'a str,
    pub record_id: &'a str,
    pub result_json: &'a str,
}

pub trait CommerceOrderReadRepository: Send {
    type Error: From<BusinessError> + Send;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        write: bool,
        approve: bool,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error>;

    fn order<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        id: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error>;

    fn lines<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
    ) -> CommerceOrderFuture<'a, Vec<SalesOrderLineRow>, Self::Error>;
}

pub trait CommerceOrderCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        write: bool,
        approve: bool,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error>;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> CommerceOrderFuture<'a, (), Self::Error>;

    fn digest<'a>(&'a mut self, value: &'a str) -> CommerceOrderFuture<'a, String, Self::Error>;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> CommerceOrderFuture<'a, Option<OrderCommandReceipt>, Self::Error>;

    fn lock_order<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        id: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error>;

    fn count_fulfillments<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> CommerceOrderFuture<'a, i64, Self::Error>;

    fn transition<'a>(
        &'a mut self,
        plan: &'a TransitionPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error>;

    fn insert_converted_order<'a>(
        &'a mut self,
        plan: &'a TransitionPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error>;

    fn copy_converted_lines<'a>(
        &'a mut self,
        quote_id: &'a str,
        sales_id: &'a str,
    ) -> CommerceOrderFuture<'a, u64, Self::Error>;

    fn converted_readback<'a>(
        &'a mut self,
        sales_id: &'a str,
        company: &'a CompanyId,
    ) -> CommerceOrderFuture<'a, Option<ConvertedReadback>, Self::Error>;

    fn readback_order<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        id: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SalesOrderRow>, Self::Error>;

    fn insert_receipt<'a>(
        &'a mut self,
        entry: &'a ReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, bool, Self::Error>;

    fn audit<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, (), Self::Error>;

    fn outbox<'a>(
        &'a mut self,
        entry: &'a OutboxEntry<'a>,
        now: TimestampMillis,
    ) -> CommerceOrderFuture<'a, (), Self::Error>;
}

/// Read one Company/branch/department-scoped quotation or sales-order document.
/// The current IAM assignment is re-read before the owner rows.
pub async fn order_detail<R: CommerceOrderReadRepository>(
    actor: &CommerceOrderActor,
    id: &str,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<SalesOrderDocument, R::Error> {
    actor.require_read(now)?;
    if !repository
        .current_authority(actor, false, false, now)
        .await?
    {
        return Err(permission_required().into());
    }
    let order = repository.order(actor, id).await?.ok_or_else(not_found)?;
    let lines = repository.lines(&actor.company_id, id).await?;
    validate_document(actor, order, lines, now).map_err(Into::into)
}

/// Apply one safe owner-local order transition.
///
/// `approve` is quotation-only and refuses a sales-order approval because the
/// retained Finance credit-limit guard is not available to this slice. A
/// confirmed sales-order cancellation is refused for the same reason: its
/// retained Inventory reservation/reversal guard cannot be bypassed.
pub async fn transition_order<R: CommerceOrderCommandRepository>(
    actor: &CommerceOrderActor,
    command: &TransitionCommand,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<CommerceOrderReceipt, R::Error> {
    actor.require_write(now)?;
    command.validate()?;
    repository.lock_command(command.key.as_str()).await?;
    let identity = transition_identity(actor, command);
    let request_hash = repository.digest(&identity).await?;
    validate_hash(&request_hash)?;
    if let Some(prior) = replay(
        repository.receipt(command.key.as_str()).await?,
        &request_hash,
        command.order_id.as_str(),
        command.expected_version,
        command.action,
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
        .ok_or_else(not_found)?;
    let needs_approver = command.action.needs_approver()
        || (command.action == crate::domain::order::OrderAction::Cancel
            && order.state() == Some(OrderState::Confirmed));
    if needs_approver && !repository.current_authority(actor, true, true, now).await? {
        return Err(crate::domain::order::approval_required().into());
    }
    let fulfillment_count = if command.action == crate::domain::order::OrderAction::Cancel {
        repository
            .count_fulfillments(command.order_id.as_str())
            .await?
    } else {
        0
    };
    let plan = authorize_transition(
        actor,
        command,
        &order,
        fulfillment_count,
        &request_hash,
        now,
    )?;
    if !repository.transition(&plan).await? {
        return Err(version_conflict().into());
    }
    if let Some(sales_id) = &plan.sales_id {
        if !repository.insert_converted_order(&plan).await? {
            return Err(readback_invalid().into());
        }
        let copied = repository
            .copy_converted_lines(plan.order_id.as_str(), sales_id)
            .await?;
        let converted = repository
            .converted_readback(sales_id, &actor.company_id)
            .await?
            .ok_or_else(readback_invalid)?;
        validate_conversion_readback(&plan, &order, &converted, copied)?;
    }
    let receipt = plan.receipt.clone();
    let result_json = serde_json::to_string(&receipt.to_wire()).map_err(|_| request_invalid())?;
    if !repository
        .insert_receipt(
            &ReceiptEntry {
                key: command.key.as_str(),
                operation: plan.action.operation(),
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
            &AuditEntry {
                key: command.key.as_str(),
                operation: plan.action.operation(),
                record_type: "commerce_order",
                record_id: &plan.order_id,
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
                operation: plan.action.operation(),
                record_type: "commerce_order",
                record_id: &plan.order_id,
                version: receipt.version,
                state: &receipt.state,
                sales_id: receipt.sales_id.as_deref(),
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
    Ok(receipt)
}
