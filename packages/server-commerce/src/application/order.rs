//! Native Commerce quotation/sales-order read and lifecycle use cases.
//!
//! The caller owns the retained serializable Holding transaction. Reads resolve
//! the current IAM assignment before owner rows. Commands resolve the canonical
//! receipt before a fresh write, lock the scoped order, apply one exact
//! expected-version transition, write receipt/audit/outbox in the same unit and
//! validate persisted readback before returning.
use crate::domain::order::{
    CommerceOrderActor, CommerceOrderReceipt, ConvertedReadback, OrderCommandReceipt, OrderState,
    SalesOrderDocument, SalesOrderLineRow, SalesOrderRow, SaveOrderCommand, SaveOrderLinePlan,
    SaveOrderPlan, SaveOrderPriceList, SaveOrderProduct, SaveOrderReceipt, SaveOrderWarehouse,
    TransitionCommand, TransitionPlan, authorize_transition, idempotency_conflict, not_found,
    permission_required, readback_invalid, replay, replay_save, request_invalid, save_identity,
    transition_identity, validate_conversion_readback, validate_document, validate_hash,
    validate_readback, validate_save_readback, version_conflict,
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

/// Repository facts required by the native draft create/update slice. All
/// source reads happen in the same caller-owned serializable transaction as
/// the owner write.
pub trait CommerceOrderSaveRepository: CommerceOrderCommandRepository {
    fn setup_currency<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> CommerceOrderFuture<'a, Option<String>, Self::Error>;

    fn next_quote_number<'a>(
        &'a mut self,
        company: &'a CompanyId,
        date: &'a str,
    ) -> CommerceOrderFuture<'a, String, Self::Error>;

    fn save_warehouse<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        id: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SaveOrderWarehouse>, Self::Error>;

    fn save_product<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> CommerceOrderFuture<'a, Option<SaveOrderProduct>, Self::Error>;

    fn save_party_valid<'a>(
        &'a mut self,
        company: &'a CompanyId,
        party_id: &'a str,
        currency: &'a str,
    ) -> CommerceOrderFuture<'a, bool, Self::Error>;

    fn save_price_list<'a>(
        &'a mut self,
        company: &'a CompanyId,
        party_id: &'a str,
        date: &'a str,
        selected: Option<&'a str>,
    ) -> CommerceOrderFuture<'a, Option<SaveOrderPriceList>, Self::Error>;

    fn insert_order<'a>(
        &'a mut self,
        plan: &'a SaveOrderPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error>;

    fn update_order<'a>(
        &'a mut self,
        plan: &'a SaveOrderPlan,
    ) -> CommerceOrderFuture<'a, bool, Self::Error>;

    fn delete_order_lines<'a>(
        &'a mut self,
        order_id: &'a str,
    ) -> CommerceOrderFuture<'a, (), Self::Error>;

    fn insert_order_line<'a>(
        &'a mut self,
        line: &'a SaveOrderLinePlan,
    ) -> CommerceOrderFuture<'a, (), Self::Error>;

    fn readback_lines<'a>(
        &'a mut self,
        actor: &'a CommerceOrderActor,
        order_id: &'a str,
    ) -> CommerceOrderFuture<'a, Vec<SalesOrderLineRow>, Self::Error>;
}

/// Create or update one exact draft quotation/sales order.
///
/// The retained TypeScript writer resolves pricing, party, warehouse and
/// product guards and writes order header/lines plus receipt/audit/outbox in
/// one unit. Purchase orders and request/RFQ handoffs are outside this owner
/// slice and remain retained.
pub async fn save_order<R: CommerceOrderSaveRepository>(
    actor: &CommerceOrderActor,
    command: &SaveOrderCommand,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<SaveOrderReceipt, R::Error> {
    actor.require_write(now)?;
    command.validate()?;
    repository.lock_command(command.key.as_str()).await?;
    let identity = save_identity(actor, command);
    let request_hash = repository.digest(&identity).await?;
    validate_hash(&request_hash)?;
    if let Some(prior) = replay_save(
        repository.receipt(command.key.as_str()).await?,
        &request_hash,
        command,
    )? {
        return Ok(prior);
    }
    if !repository
        .current_authority(actor, true, false, now)
        .await?
    {
        return Err(permission_required().into());
    }
    let currency = repository
        .setup_currency(&actor.company_id)
        .await?
        .ok_or_else(crate::domain::order::save_setup_required)?;
    let existing = match &command.order_id {
        Some(id) => {
            let row = repository
                .lock_order(actor, id.as_str())
                .await?
                .ok_or_else(not_found)?;
            if row.version != command.expected_version().unwrap_or(0) {
                return Err(version_conflict().into());
            }
            Some(row)
        }
        None => None,
    };
    let party_valid = repository
        .save_party_valid(&actor.company_id, &command.input.party_id, &currency)
        .await?;
    let warehouse = match command.input.warehouse_id.as_deref() {
        Some(id) => repository.save_warehouse(actor, id).await?,
        None => None,
    };
    let mut products = Vec::with_capacity(command.input.lines.len());
    for line in &command.input.lines {
        products.push(
            repository
                .save_product(&actor.company_id, &line.product_id)
                .await?,
        );
    }
    let price_list = repository
        .save_price_list(
            &actor.company_id,
            &command.input.party_id,
            &command.input.order_date,
            command.input.price_list_id.as_deref(),
        )
        .await?;
    if command.input.price_list_id.is_some() && price_list.is_none() {
        return Err(crate::domain::order::price_list_invalid().into());
    }
    let number = if command.order_id.is_none() {
        match command.input.kind {
            crate::domain::order::OrderKind::Quote => {
                repository
                    .next_quote_number(&actor.company_id, &command.input.order_date)
                    .await?
            }
            crate::domain::order::OrderKind::Sales => {
                crate::domain::order::sales_order_number(&command.key)
            }
        }
    } else {
        String::new()
    };
    let plan = crate::domain::order::build_save_plan(
        actor,
        command,
        currency,
        existing,
        warehouse,
        party_valid,
        &products,
        price_list,
        number,
        now,
    )?;
    if plan.create {
        if !repository.insert_order(&plan).await? {
            return Err(crate::domain::order::order_conflict().into());
        }
    } else {
        if !repository.update_order(&plan).await? {
            return Err(version_conflict().into());
        }
        repository.delete_order_lines(&plan.id).await?;
    }
    for line in &plan.lines {
        repository.insert_order_line(line).await?;
    }
    let receipt = SaveOrderReceipt {
        id: plan.id.clone(),
        version: plan.version,
    };
    let result_json = serde_json::to_string(&receipt.to_wire()).map_err(|_| request_invalid())?;
    if !repository
        .insert_receipt(
            &ReceiptEntry {
                key: command.key.as_str(),
                operation: command.operation(),
                actor,
                request_hash: &request_hash,
                record_id: &plan.id,
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
                operation: command.operation(),
                record_type: "commerce_order",
                record_id: &plan.id,
                version: plan.version,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    repository
        .outbox(
            &OutboxEntry {
                key: command.key.as_str(),
                operation: command.operation(),
                record_type: "commerce_order",
                record_id: &plan.id,
                version: plan.version,
                state: "draft",
                sales_id: None,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    let readback = repository
        .readback_order(actor, &plan.id)
        .await?
        .ok_or_else(readback_invalid)?;
    let lines = repository.readback_lines(actor, &plan.id).await?;
    validate_save_readback(&plan, &readback, &lines)?;
    Ok(receipt)
}
