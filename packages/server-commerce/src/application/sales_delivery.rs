//! Native Commerce sales-order goods/mixed delivery use case.
//!
//! The caller owns one serializable Holding transaction. This use case locks
//! the command key, resolves the exact idempotency receipt, re-reads current
//! IAM, fences every Commerce/Inventory delivery source, prepares and applies
//! the Inventory stock issue, inserts the fulfillment, posts the Finance
//! delivery valuation/journal, advances the order and writes
//! receipt/audit/outbox. No quantity, value, warehouse, lot or scope comes from
//! a caller-supplied label.
//!
//! Frozen order inside the unit:
//! 1. Commerce readiness, command lock, request digest, receipt replay, IAM
//! 2. Inventory `lock_stock_issue_sources`
//! 3. Commerce current sales-order source
//! 4. Inventory `read_stock_issue_source` / `prepare_stock_issue`
//! 5. Commerce fulfillment insert
//! 6. Inventory `apply_stock_issue`
//! 7. Finance delivery valuation and goods journal, attach, valuation evidence
//! 8. Commerce fulfilled-line increment, order advance, readback
//! 9. Command receipt, audit and outbox
use crate::application::order::{AuditEntry, OutboxEntry, ReceiptEntry};
use crate::domain::order::{
    CommerceOrderActor, OrderCommandReceipt, idempotency_conflict, permission_required,
    validate_hash,
};
use crate::domain::receipt::{
    CommerceReceipt, CommerceReceiptLine, GoodsEffectReadback, GoodsValuation, GoodsValuationPlan,
    GoodsValuationRequest, ReceiptOrder, ReceiptOrderAdvance, ReceiptReadback,
    expected_order_state, goods_journal_id, goods_readback_invalid, goods_valuation_invalid,
    prior_receipt, validate_goods_readback,
};
use crate::domain::sales_delivery::{
    DELIVERY_OPERATION, PreparedSalesDelivery, SalesDeliveryCommand, SalesDeliveryFulfillmentPlan,
    SalesDeliveryJournalRequest, SalesDeliveryLineValue, SalesDeliveryPostingSettings,
    SalesDeliveryStockApplied, SalesDeliveryStockRequest, apply_prepared_delivery,
    build_sales_delivery_fulfillment, goods_setup_required, not_found, readback_invalid,
    replay_incomplete, sales_delivery_stock_request, sales_delivery_valuation_plan,
    sales_delivery_values, source_changed, validate_sales_delivery_readback,
};
use erpx_server_runtime::public::postgres::{PgTransactionError, PgWorkFuture};
use erpx_server_shared::public::{BusinessError, TimestampMillis};
use sqlx::PgConnection;

/// Commerce-owned sales delivery repository. The implementation borrows the
/// caller's serializable transaction and never commits, retries or opens a
/// second connection. Generic Commerce readback/receipt/audit/outbox evidence
/// is the same shape as the purchase receipt path.
pub trait CommerceSalesDeliveryRepository {
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
    ) -> PgWorkFuture<'a, Option<OrderCommandReceipt>, BusinessError>;

    /// Re-reads the current IAM assignment. `mixed` additionally requires the
    /// Sales preparation permission.
    fn current_authority<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a CommerceOrderActor,
        now: TimestampMillis,
        mixed: bool,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    /// Current scoped sales order, its lines and the Company chart base
    /// currency.
    fn order_source<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        actor: &'a CommerceOrderActor,
        order_id: &'a str,
    ) -> PgWorkFuture<'a, Option<ReceiptOrder>, BusinessError>;

    /// Company delivery posting settings. A missing or inactive chart/account
    /// set fails the delivery closed before any stock or journal write.
    fn delivery_settings<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
    ) -> PgWorkFuture<'a, Option<SalesDeliveryPostingSettings>, BusinessError>;

    fn insert_fulfillment<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a SalesDeliveryFulfillmentPlan,
    ) -> PgWorkFuture<'a, (), BusinessError>;

    /// Attach the goods journal to the exact new delivery fulfillment. Returns
    /// false when the row is missing, already attached or outside the Company.
    fn attach_journal<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        fulfillment_id: &'a str,
        order_id: &'a str,
        journal_id: &'a str,
    ) -> PgWorkFuture<'a, bool, BusinessError>;

    fn insert_goods_valuation<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a GoodsValuationPlan,
    ) -> PgWorkFuture<'a, (), BusinessError>;

    fn goods_readback<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
        fulfillment_id: &'a str,
    ) -> PgWorkFuture<'a, Option<GoodsEffectReadback>, BusinessError>;

    fn increment_fulfilled<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        line: &'a SalesDeliveryLineValue,
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

/// Inventory-owned stock issue seam. The implementation must borrow the
/// caller's transaction, fence the same source relations and never commit or
/// retry. `prepare` returns the authoritative quantities and functional values.
pub trait CommerceSalesDeliveryStock {
    type Source: Send + Sync;
    type Plan: Send + Sync;
    type Applied: Send + Sync;

    fn lock<'a>(&'a mut self, c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError>;

    fn read<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        company: &'a str,
        branch: Option<&'a str>,
        department: Option<&'a str>,
        order_id: &'a str,
    ) -> PgWorkFuture<'a, Self::Source, BusinessError>;

    fn prepare<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        source: &'a Self::Source,
        request: &'a SalesDeliveryStockRequest,
    ) -> PgWorkFuture<'a, PreparedSalesDelivery<Self::Plan>, BusinessError>;

    fn apply<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a Self::Plan,
        fulfillment_id: &'a str,
        command_key: &'a str,
        actor: &'a str,
        now: i64,
    ) -> PgWorkFuture<'a, Self::Applied, BusinessError>;

    fn applied_facts(&self, applied: &Self::Applied) -> SalesDeliveryStockApplied;

    fn require_applied<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        plan: &'a Self::Plan,
        applied: &'a Self::Applied,
    ) -> PgWorkFuture<'a, (), BusinessError>;
}

/// Finance-owned functional valuation and delivery goods-journal seam. The
/// implementation borrows the caller's transaction, re-resolves the Company
/// chart and quote, and never commits or retries. Commerce supplies the frozen
/// functional stock cost; Finance reconciles it.
pub trait CommerceSalesDeliveryGoods: Send {
    fn lock<'a>(&'a mut self, c: &'a mut PgConnection) -> PgWorkFuture<'a, (), BusinessError>;

    fn resolve<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        request: &'a GoodsValuationRequest,
    ) -> PgWorkFuture<'a, GoodsValuation, BusinessError>;

    fn post<'a>(
        &'a mut self,
        c: &'a mut PgConnection,
        request: &'a SalesDeliveryJournalRequest,
        valuation: &'a GoodsValuation,
    ) -> PgWorkFuture<'a, String, BusinessError>;
}

/// Apply one sales goods/mixed delivery. `stock` must be the Inventory public
/// seam backed by the same `c`; composition wires that implementation and the
/// Finance goods-journal port for the Company chart.
pub async fn deliver_sales_order<R, S>(
    actor: &CommerceOrderActor,
    command: &SalesDeliveryCommand,
    repository: &mut R,
    stock: &mut S,
    finance: &mut dyn CommerceSalesDeliveryGoods,
    c: &mut PgConnection,
    now: TimestampMillis,
) -> Result<CommerceReceipt, PgTransactionError<BusinessError>>
where
    R: CommerceSalesDeliveryRepository + ?Sized,
    S: CommerceSalesDeliveryStock + ?Sized,
{
    command.validate()?;
    actor.require_sales_delivery(now, command.mode.is_mixed())?;
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
        // A replayed goods delivery must still carry its goods valuation and
        // journal link; a partial earlier delivery is never silently accepted.
        if prior.total_value_minor > 0
            && repository
                .goods_readback(c, actor.company_id.as_str(), &prior.fulfillment_id)
                .await?
                .is_none()
        {
            return Err(goods_readback_invalid().into());
        }
        return Ok(prior);
    }
    if !repository
        .current_authority(c, actor, now, command.mode.is_mixed())
        .await?
    {
        return Err(permission_required().into());
    }
    stock.lock(c).await?;
    let order = repository
        .order_source(c, actor, command.order_id.as_str())
        .await?
        .ok_or_else(not_found)?;
    order.validate(actor)?;
    if order.version != command.expected_version || order.warehouse_id.is_empty() {
        return Err(source_changed().into());
    }
    let mut values = sales_delivery_values(&order, command)?;
    let request = sales_delivery_stock_request(command, &values)?;
    let source = stock
        .read(
            c,
            actor.company_id.as_str(),
            actor.branch_id.as_ref().map(|value| value.as_str()),
            actor.department_id.as_ref().map(|value| value.as_str()),
            command.order_id.as_str(),
        )
        .await?;
    let prepared = stock.prepare(c, &source, &request).await?;
    apply_prepared_delivery(&order, command, &mut values, &prepared)?;
    let settings = repository
        .delivery_settings(c, actor.company_id.as_str())
        .await?
        .ok_or_else(goods_setup_required)?;
    if settings.inventory_account_id.is_empty() || settings.cogs_account_id.is_empty() {
        return Err(goods_setup_required().into());
    }
    let fulfillment =
        build_sales_delivery_fulfillment(actor, command, &order, &values, &request_hash, now)?;
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
    if applied_facts.fulfillment_id != fulfillment.id
        || applied_facts.lines.len()
            != values
                .iter()
                .filter(|value| value.item_type == "stock")
                .count()
    {
        return Err(source_changed().into());
    }
    let stock_total = fulfillment
        .lines
        .iter()
        .try_fold(0_i64, |total, line| {
            total
                .checked_add(line.value_minor)
                .filter(|total| *total <= crate::domain::receipt::MAX_SAFE)
        })
        .ok_or_else(crate::domain::sales_delivery::money_overflow)?;
    if stock_total > 0 {
        finance.lock(c).await?;
        let valuation = finance
            .resolve(
                c,
                &GoodsValuationRequest {
                    company_id: actor.company_id.as_str().to_owned(),
                    user_id: actor.user_id.as_str().to_owned(),
                    holding_id: actor.holding_id.as_str().to_owned(),
                    assignment_id: actor.assignment_id.as_str().to_owned(),
                    chart_id: settings.chart_id.clone(),
                    business_date: command.date.clone(),
                    document_currency: order.base_currency.clone(),
                },
            )
            .await?;
        valuation.validate()?;
        // A delivery goods journal is valued in the Company functional
        // currency; any other quote means the chart changed under the unit.
        if valuation.stock_currency != order.base_currency || valuation.fx_rate_micros != 1_000_000
        {
            return Err(goods_valuation_invalid().into());
        }
        let journal_id = goods_journal_id(&command.key);
        let posted = finance
            .post(
                c,
                &SalesDeliveryJournalRequest {
                    company_id: actor.company_id.as_str().to_owned(),
                    user_id: actor.user_id.as_str().to_owned(),
                    holding_id: actor.holding_id.as_str().to_owned(),
                    assignment_id: actor.assignment_id.as_str().to_owned(),
                    chart_id: settings.chart_id.clone(),
                    business_date: command.date.clone(),
                    journal_id: journal_id.clone(),
                    fulfillment_id: fulfillment.id.clone(),
                    memo: format!("Delivery {}", fulfillment.id),
                    inventory_account_id: settings.inventory_account_id.clone(),
                    cogs_account_id: settings.cogs_account_id.clone(),
                    currency: order.base_currency.clone(),
                    amount_minor: stock_total,
                    fx_rate_micros: valuation.fx_rate_micros,
                },
                &valuation,
            )
            .await?;
        if posted != journal_id
            || !repository
                .attach_journal(
                    c,
                    &fulfillment.id,
                    command.order_id.as_str(),
                    journal_id.as_str(),
                )
                .await?
        {
            return Err(source_changed().into());
        }
        let plan = sales_delivery_valuation_plan(
            &fulfillment,
            actor.company_id.as_str(),
            settings.chart_id.as_str(),
            journal_id.as_str(),
            order.currency.as_str(),
            order.base_currency.as_str(),
            valuation.stock_currency.as_str(),
            &valuation,
            actor.user_id.as_str(),
            now.get(),
        )?;
        repository.insert_goods_valuation(c, &plan).await?;
        let goods = repository
            .goods_readback(c, actor.company_id.as_str(), &fulfillment.id)
            .await?
            .ok_or_else(goods_readback_invalid)?;
        validate_goods_readback(&plan, &goods)?;
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
            .ok_or_else(crate::domain::sales_delivery::request_invalid)?,
        total_value_minor: stock_total,
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
    validate_sales_delivery_readback(&receipt, &order, &fulfillment, &applied_facts, &current)?;
    let result_json = receipt.to_result_json()?;
    if !repository
        .insert_receipt(
            c,
            &ReceiptEntry {
                key: command.key.as_str(),
                operation: DELIVERY_OPERATION,
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
                operation: DELIVERY_OPERATION,
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
                operation: DELIVERY_OPERATION,
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
    // Receipt/audit/outbox owner effects may invoke database triggers. Retain
    // Inventory evidence until all of them have completed in this unit.
    stock.require_applied(c, &prepared.plan, &applied).await?;
    Ok(receipt)
}
