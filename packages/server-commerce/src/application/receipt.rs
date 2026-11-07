//! Native Commerce purchase goods-receipt use case.
//!
//! The caller owns one serializable Holding transaction. Commerce locks its
//! command key, resolves the exact idempotency receipt, re-reads current IAM,
//! fences every Commerce/Inventory receipt source, inserts the fulfillment and
//! its lines, applies the Inventory public seam, advances the order and writes
//! receipt/audit/outbox. No quantity, value, warehouse or scope comes from a
//! caller-supplied label.
use crate::application::order::{AuditEntry, OutboxEntry, ReceiptEntry};
use crate::domain::order::{
    CommerceOrderActor, idempotency_conflict, permission_required, validate_hash,
};
use crate::domain::receipt::{
    CommerceReceipt, CommerceReceiptLine, PreparedStockReceipt, RECEIPT_OPERATION, ReceiptCommand,
    ReceiptFulfillmentLinePlan, ReceiptFulfillmentPlan, ReceiptLineValue, ReceiptOrder,
    ReceiptOrderAdvance, ReceiptReadback, ReceiptStockApplied, ReceiptStockRequest,
    expected_order_state, fx_guard_required, line_value, not_found, prior_receipt,
    readback_invalid, replay_incomplete, request_invalid, service_line_forbidden, source_changed,
    validate_readback,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use sqlx::PgConnection;

pub trait CommerceReceiptRepository {
    fn ready<'a>(&'a mut self, c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError>;

    fn lock_command<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        key: &'a str,
    ) -> PgWorkFuture<'a, (), BusinessError>;

    fn digest<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        value: &'a str,
    ) -> PgWorkFuture<'a, String, BusinessError>;

    fn receipt<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        key: &'a str,
    ) -> PgWorkFuture<'a, Option<crate::domain::order::OrderCommandReceipt>, BusinessError>;

    fn current_authority<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a CommerceOrderActor,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    fn order_source<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a CommerceOrderActor,
        order_id: &'a str,
    ) -> PgWorkFuture<'a, Option<ReceiptOrder>, BusinessError>;

    fn insert_fulfillment<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a ReceiptFulfillmentPlan,
    ) -> PgWorkFuture<'a, (), BusinessError>;

    fn increment_fulfilled<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        line: &'a ReceiptLineValue,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    fn advance_order<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a ReceiptOrderAdvance,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    fn readback<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a CommerceOrderActor,
        order_id: &'a str,
        fulfillment_id: &'a str,
    ) -> PgWorkFuture<'a, Option<ReceiptReadback>, BusinessError>;

    fn insert_receipt<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        entry: &'a ReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    fn audit<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a CommerceOrderActor,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, (), BusinessError>;

    fn outbox<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        entry: &'a OutboxEntry<'a>,
        now: TimestampMillis,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}

/// Inventory-owned seam. The implementation must borrow the caller's
/// transaction, fence the same source relations and never commit or retry.
pub trait CommerceReceiptStock {
    type Plan: Send + Sync;
    type Applied: Send + Sync;

    fn lock<'a>(&'a mut self, c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError>;

    fn prepare<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        request: &'a ReceiptStockRequest,
    ) -> PgWorkFuture<'a, PreparedStockReceipt<Self::Plan>, BusinessError>;

    fn apply<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a Self::Plan,
        fulfillment_id: &'a str,
        command_key: &'a str,
        actor: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, Self::Applied, BusinessError>;

    fn applied_facts(&self, applied: &Self::Applied) -> ReceiptStockApplied;

    fn require_applied<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a Self::Plan,
        applied: &'a Self::Applied,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}

/// Apply one purchase goods receipt. `stock` must be the Inventory public seam
/// backed by the same `c`; composition wires that implementation and the
/// Quality hold port.
pub async fn receive_purchase_order<R, S>(
    actor: &CommerceOrderActor,
    command: &ReceiptCommand,
    repository: &mut R,
    stock: &mut S,
    c: &mut PgConnection,
    now: TimestampMillis,
) -> Result<CommerceReceipt, PgTransactionError<BusinessError>>
where
    R: CommerceReceiptRepository + ?Sized,
    S: CommerceReceiptStock + ?Sized,
{
    actor.require_receipt(now)?;
    command.validate()?;
    repository.ready(c).await?;
    repository.lock_command(c, command.key.as_str()).await?;
    let identity = command.identity(actor)?;
    let request_hash = repository.digest(c, &identity).await?;
    validate_hash(&request_hash)?;
    if let Some(prior) = prior_receipt(
        repository.receipt(c, command.key.as_str()).await?,
        &request_hash,
        command.order_id.as_str(),
    )? {
        return Ok(prior);
    }
    if !repository.current_authority(c, actor, now).await? {
        return Err(permission_required().into());
    }
    stock.lock(c).await?;
    let order = repository
        .order_source(c, actor, command.order_id.as_str())
        .await?
        .ok_or_else(not_found)?;
    order.validate(actor)?;
    if order.version != command.expected_version {
        return Err(source_changed().into());
    }
    if order.currency != order.base_currency {
        return Err(fx_guard_required().into());
    }
    let mut values = Vec::with_capacity(command.lines.len());
    for line in &command.lines {
        let source = order
            .lines
            .iter()
            .find(|value| value.id == line.line_id)
            .ok_or_else(source_changed)?;
        if source.item_type != "stock" {
            return Err(service_line_forbidden().into());
        }
        values.push(line_value(&order, source, line.quantity_milli)?);
    }
    let request = ReceiptStockRequest {
        scope: crate::domain::receipt::ReceiptStockScope {
            company_id: actor.company_id.as_str().to_owned(),
            branch_id: actor
                .branch_id
                .as_ref()
                .map(|value| value.as_str().to_owned()),
            department_id: actor
                .department_id
                .as_ref()
                .map(|value| value.as_str().to_owned()),
            order_id: command.order_id.as_str().to_owned(),
        },
        expected_order_version: command.expected_version,
        date: command.date.clone(),
        lines: command
            .lines
            .iter()
            .zip(&values)
            .map(|(line, value)| crate::domain::receipt::ReceiptStockLine {
                line_id: line.line_id.clone(),
                quantity_milli: value.quantity_milli,
                stock_value_minor: value.stock_value_minor,
                allocations: line
                    .allocations
                    .iter()
                    .map(
                        |allocation| crate::domain::receipt::ReceiptStockAllocation {
                            lot_code: allocation.lot_code.clone(),
                            serial_number: allocation.serial_number.clone(),
                            quantity_milli: allocation.quantity_milli,
                            disposition: allocation.disposition,
                            quarantine_reason: allocation.quarantine_reason.clone(),
                            quarantine_due_date: allocation.quarantine_due_date.clone(),
                            manufactured_date: allocation.manufactured_date.clone(),
                            expiry_date: allocation.expiry_date.clone(),
                        },
                    )
                    .collect(),
            })
            .collect(),
    };
    let prepared = stock.prepare(c, &request).await?;
    if prepared.lines.len() != values.len() {
        return Err(source_changed().into());
    }
    for (plan, value) in prepared.lines.iter().zip(&values) {
        if plan.line_id != value.line_id
            || plan.product_id != value.product_id
            || plan.quantity_milli != value.quantity_milli
            || plan.stock_value_minor != value.stock_value_minor
            || plan.expected_fulfilled_milli != value.expected_fulfilled_milli
        {
            return Err(source_changed().into());
        }
    }
    let fulfillment_id = format!("delivery-{}", command.key.as_str());
    let fulfillment = ReceiptFulfillmentPlan {
        id: fulfillment_id.clone(),
        number: format!("GR-{}", request_hash[..24].to_uppercase()),
        order_id: command.order_id.as_str().to_owned(),
        date: command.date.clone(),
        actor_id: actor.user_id.as_str().to_owned(),
        now: now.get(),
        lines: values
            .iter()
            .enumerate()
            .map(|(index, value)| ReceiptFulfillmentLinePlan {
                index,
                order_line_id: value.line_id.clone(),
                quantity_milli: value.quantity_milli,
                value_minor: value.stock_value_minor,
                subtotal_minor: value.subtotal_minor,
                tax_minor: value.tax_minor,
                stock_currency: order.base_currency.clone(),
            })
            .collect(),
    };
    repository.insert_fulfillment(c, &fulfillment).await?;
    let applied = stock
        .apply(
            c,
            &prepared.plan,
            &fulfillment.id,
            command.key.as_str(),
            actor.user_id.as_str(),
            now.get(),
        )
        .await?;
    let applied_facts = stock.applied_facts(&applied);
    if applied_facts.replayed {
        return Err(replay_incomplete().into());
    }
    if applied_facts.fulfillment_id != fulfillment.id {
        return Err(source_changed().into());
    }
    if applied_facts.lines.len() != values.len() {
        return Err(source_changed().into());
    }
    for value in &values {
        if !repository.increment_fulfilled(c, value).await? {
            return Err(source_changed().into());
        }
    }
    let mut receipt = CommerceReceipt {
        order_id: command.order_id.as_str().to_owned(),
        fulfillment_id: fulfillment.id.clone(),
        state: String::new(),
        version: command
            .expected_version
            .checked_add(1)
            .ok_or_else(request_invalid)?,
        total_value_minor: values
            .iter()
            .try_fold(0_i64, |total, value| {
                total.checked_add(value.stock_value_minor)
            })
            .ok_or_else(crate::domain::receipt::money_overflow)?,
        lines: values
            .iter()
            .map(|value| CommerceReceiptLine {
                line_id: value.line_id.clone(),
                quantity_milli: value.quantity_milli,
                value_minor: value.stock_value_minor,
            })
            .collect(),
    };
    receipt.state = expected_order_state(&order, &receipt);
    let advance = ReceiptOrderAdvance {
        order_id: command.order_id.as_str().to_owned(),
        company_id: actor.company_id.as_str().to_owned(),
        expected_version: command.expected_version,
        now: now.get(),
    };
    if !repository.advance_order(c, &advance).await? {
        return Err(source_changed().into());
    }
    stock.require_applied(c, &prepared.plan, &applied).await?;
    let current = repository
        .readback(c, actor, command.order_id.as_str(), &fulfillment.id)
        .await?
        .ok_or_else(readback_invalid)?;
    validate_readback(
        &receipt,
        &receipt.state,
        receipt.version,
        &order,
        &fulfillment,
        &applied_facts,
        &current,
    )?;
    let result_json = receipt.to_result_json()?;
    if !repository
        .insert_receipt(
            c,
            &ReceiptEntry {
                key: command.key.as_str(),
                operation: RECEIPT_OPERATION,
                actor,
                request_hash: &request_hash,
                record_id: command.order_id.as_str(),
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
            c,
            actor,
            &AuditEntry {
                key: command.key.as_str(),
                operation: RECEIPT_OPERATION,
                record_type: "commerce_order",
                record_id: command.order_id.as_str(),
                version: receipt.version,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    repository
        .outbox(
            c,
            &OutboxEntry {
                key: command.key.as_str(),
                operation: RECEIPT_OPERATION,
                record_type: "commerce_order",
                record_id: command.order_id.as_str(),
                version: receipt.version,
                state: &receipt.state,
                sales_id: None,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    Ok(receipt)
}
