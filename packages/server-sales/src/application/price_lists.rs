//! Native Sales price-list create, draft update and independent decision.
//!
//! The caller owns the retained serializable Holding transaction. Every
//! command resolves the current IAM assignment, serializes its idempotency key,
//! replays an exact prior receipt, rechecks the current Company currency and
//! active customer/product sources, applies one expected-version write, then
//! writes receipt/audit/outbox and validates persisted readback before
//! returning. No transaction is opened, committed or retried here.
use crate::domain::price_lists::{
    PriceListActor, PriceListCommandReceipt, PriceListDecision, PriceListDocument, PriceListInput,
    PriceListItemRow, PriceListProductSource, PriceListReceipt, PriceListRow, PriceListUpdate,
    command_identity, expected_create_list, expected_decision_list, expected_items,
    expected_update_list, idempotency_conflict, independent_approver_required, locked, not_found,
    permission_required, readback_invalid, replay, request_invalid, source_invalid, validate_hash,
    validate_readback, version_conflict,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, TimestampMillis};
use serde_json::Value;
use std::future::Future;

pub type PriceListFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

/// A price-list command key is also used to derive the durable record id. Keep
/// the generated list/item ids inside the shared identifier bound.
pub const COMMAND_KEY_MAX: usize = 100;

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
    pub status: &'a str,
    pub authority_json: &'a str,
}

pub struct ReceiptEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub actor: &'a PriceListActor,
    pub request_hash: &'a str,
    pub record_id: &'a str,
    pub result_json: &'a str,
}

pub struct PriceListWritePlan<'a> {
    pub input: &'a PriceListInput,
    pub row: &'a PriceListRow,
    pub items: &'a [PriceListItemRow],
    pub item_replace: bool,
    pub now: TimestampMillis,
}

pub trait PriceListCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn current_authority<'a>(
        &'a mut self,
        actor: &'a PriceListActor,
        approve: bool,
        now: TimestampMillis,
    ) -> PriceListFuture<'a, bool, Self::Error>;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> PriceListFuture<'a, (), Self::Error>;

    fn digest<'a>(&'a mut self, value: &'a str) -> PriceListFuture<'a, String, Self::Error>;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> PriceListFuture<'a, Option<PriceListCommandReceipt>, Self::Error>;

    fn currency<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> PriceListFuture<'a, Option<String>, Self::Error>;

    fn customer_valid<'a>(
        &'a mut self,
        company: &'a CompanyId,
        customer_id: &'a str,
        currency: &'a str,
    ) -> PriceListFuture<'a, bool, Self::Error>;

    fn product<'a>(
        &'a mut self,
        company: &'a CompanyId,
        product_id: &'a str,
    ) -> PriceListFuture<'a, Option<PriceListProductSource>, Self::Error>;

    fn product_unchanged<'a>(
        &'a mut self,
        company: &'a CompanyId,
        product_id: &'a str,
        version: i64,
    ) -> PriceListFuture<'a, bool, Self::Error>;

    fn list<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
        lock: bool,
    ) -> PriceListFuture<'a, Option<PriceListRow>, Self::Error>;

    fn items<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> PriceListFuture<'a, Vec<PriceListItemRow>, Self::Error>;

    fn insert_list<'a>(
        &'a mut self,
        plan: &'a PriceListWritePlan<'a>,
    ) -> PriceListFuture<'a, bool, Self::Error>;

    fn update_list<'a>(
        &'a mut self,
        plan: &'a PriceListWritePlan<'a>,
    ) -> PriceListFuture<'a, bool, Self::Error>;

    fn replace_items<'a>(
        &'a mut self,
        plan: &'a PriceListWritePlan<'a>,
    ) -> PriceListFuture<'a, bool, Self::Error>;

    fn decide_list<'a>(
        &'a mut self,
        row: &'a PriceListRow,
        expected_version: i64,
    ) -> PriceListFuture<'a, bool, Self::Error>;

    fn readback<'a>(
        &'a mut self,
        company: &'a CompanyId,
        id: &'a str,
    ) -> PriceListFuture<'a, Option<PriceListDocument>, Self::Error>;

    fn insert_receipt<'a>(
        &'a mut self,
        entry: &'a ReceiptEntry<'a>,
        now: TimestampMillis,
    ) -> PriceListFuture<'a, bool, Self::Error>;

    fn audit<'a>(
        &'a mut self,
        actor: &'a PriceListActor,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> PriceListFuture<'a, (), Self::Error>;

    fn outbox<'a>(
        &'a mut self,
        entry: &'a OutboxEntry<'a>,
        now: TimestampMillis,
    ) -> PriceListFuture<'a, (), Self::Error>;
}

fn key_valid(key: &CommandKey) -> bool {
    key.as_str().len() <= COMMAND_KEY_MAX
}

async fn verify_sources<R: PriceListCommandRepository>(
    actor: &PriceListActor,
    input: &PriceListInput,
    repository: &mut R,
) -> Result<Vec<PriceListProductSource>, R::Error> {
    let currency = repository
        .currency(&actor.company_id)
        .await?
        .ok_or_else(source_invalid)?;
    if currency != input.currency {
        return Err(crate::domain::price_lists::currency_invalid().into());
    }
    if let Some(customer) = &input.customer_id
        && !repository
            .customer_valid(&actor.company_id, customer, &input.currency)
            .await?
    {
        return Err(crate::domain::price_lists::customer_invalid().into());
    }
    let mut products = Vec::with_capacity(input.items.len());
    for item in &input.items {
        let product = repository
            .product(&actor.company_id, &item.product_id)
            .await?
            .ok_or_else(crate::domain::price_lists::product_invalid)?;
        products.push(product);
    }
    Ok(products)
}

async fn recheck_products<R: PriceListCommandRepository>(
    actor: &PriceListActor,
    products: &[PriceListProductSource],
    repository: &mut R,
) -> Result<(), R::Error> {
    for product in products {
        if !repository
            .product_unchanged(&actor.company_id, &product.id, product.version)
            .await?
        {
            return Err(crate::domain::price_lists::product_invalid().into());
        }
    }
    Ok(())
}

async fn recheck_sources<R: PriceListCommandRepository>(
    actor: &PriceListActor,
    input: &PriceListInput,
    products: &[PriceListProductSource],
    repository: &mut R,
) -> Result<(), R::Error> {
    let currency = repository
        .currency(&actor.company_id)
        .await?
        .ok_or_else(source_invalid)?;
    if currency != input.currency {
        return Err(crate::domain::price_lists::currency_invalid().into());
    }
    if let Some(customer) = &input.customer_id
        && !repository
            .customer_valid(&actor.company_id, customer, &input.currency)
            .await?
    {
        return Err(crate::domain::price_lists::customer_invalid().into());
    }
    recheck_products(actor, products, repository).await
}

/// Create one draft price list. The generated id is derived from the exact
/// idempotency key so a replay addresses the same record.
pub async fn create_price_list<R: PriceListCommandRepository>(
    actor: &PriceListActor,
    input: PriceListInput,
    key: &CommandKey,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PriceListReceipt, R::Error> {
    actor.require_manage(now)?;
    if !key_valid(key) {
        return Err(request_invalid().into());
    }
    let input = input.normalized()?;
    repository.lock_command(key.as_str()).await?;
    let record_id = format!("price-list-{}", key.as_str());
    let operation = "sales.price-list.create";
    let input_json = serde_json::to_value(&input).map_err(|_| request_invalid())?;
    let identity = command_identity(actor, operation, &record_id, &input_json);
    let request_hash = repository.digest(&identity).await?;
    validate_hash(&request_hash)?;
    if let Some(prior) = replay(
        repository.receipt(key.as_str()).await?,
        &request_hash,
        operation,
        &record_id,
        "draft",
        1,
    )? {
        return Ok(prior);
    }
    if !repository.current_authority(actor, false, now).await? {
        return Err(permission_required().into());
    }
    let products = verify_sources(actor, &input, repository).await?;
    let row = expected_create_list(actor, &input, &record_id, now.get());
    let items = expected_items(&record_id, 1, false, &input);
    let plan = PriceListWritePlan {
        input: &input,
        row: &row,
        items: &items,
        item_replace: false,
        now,
    };
    if !repository.insert_list(&plan).await? {
        return Err(idempotency_conflict().into());
    }
    if !repository.replace_items(&plan).await? {
        return Err(readback_invalid().into());
    }
    recheck_sources(actor, &input, &products, repository).await?;
    let actual = repository
        .readback(&actor.company_id, &record_id)
        .await?
        .ok_or_else(readback_invalid)?;
    validate_readback(&row, &items, &actual)?;
    let receipt = PriceListReceipt {
        id: record_id.clone(),
        status: row.status.clone(),
        version: row.version,
    };
    let result_json = serde_json::to_string(&receipt.to_wire()).map_err(|_| request_invalid())?;
    if !repository
        .insert_receipt(
            &ReceiptEntry {
                key: key.as_str(),
                operation,
                actor,
                request_hash: &request_hash,
                record_id: &record_id,
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
                key: key.as_str(),
                operation,
                record_type: "sales_price_list",
                record_id: &record_id,
                version: receipt.version,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    repository
        .outbox(
            &OutboxEntry {
                key: key.as_str(),
                operation,
                record_type: "sales_price_list",
                record_id: &record_id,
                version: receipt.version,
                status: &receipt.status,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    Ok(receipt)
}

/// Replace a draft price list header and its complete item set under one
/// expected-version guard. `prepared_by` advances so activation remains
/// independent after every draft edit.
pub async fn update_price_list<R: PriceListCommandRepository>(
    actor: &PriceListActor,
    id: &str,
    input: PriceListUpdate,
    key: &CommandKey,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PriceListReceipt, R::Error> {
    actor.require_manage(now)?;
    if !key_valid(key) {
        return Err(request_invalid().into());
    }
    let input = input.normalized()?;
    repository.lock_command(key.as_str()).await?;
    let operation = "sales.price-list.update";
    let input_json = serde_json::to_value(&input).map_err(|_| request_invalid())?;
    let identity = command_identity(actor, operation, id, &input_json);
    let request_hash = repository.digest(&identity).await?;
    validate_hash(&request_hash)?;
    if let Some(prior) = replay(
        repository.receipt(key.as_str()).await?,
        &request_hash,
        operation,
        id,
        "draft",
        input.expected_version + 1,
    )? {
        return Ok(prior);
    }
    if !repository.current_authority(actor, false, now).await? {
        return Err(permission_required().into());
    }
    let existing = repository
        .list(&actor.company_id, id, true)
        .await?
        .ok_or_else(not_found)?;
    existing.validate(&actor.company_id)?;
    if existing.status() != Some(crate::domain::price_lists::PriceListStatus::Draft) {
        return Err(locked().into());
    }
    if existing.version != input.expected_version {
        return Err(version_conflict().into());
    }
    let products = verify_sources(actor, &input.input, repository).await?;
    let row = expected_update_list(&existing, actor, &input.input, now.get());
    let items = expected_items(id, row.version, true, &input.input);
    let plan = PriceListWritePlan {
        input: &input.input,
        row: &row,
        items: &items,
        item_replace: true,
        now,
    };
    if !repository.update_list(&plan).await? {
        return Err(version_conflict().into());
    }
    if !repository.replace_items(&plan).await? {
        return Err(readback_invalid().into());
    }
    recheck_sources(actor, &input.input, &products, repository).await?;
    let actual = repository
        .readback(&actor.company_id, id)
        .await?
        .ok_or_else(readback_invalid)?;
    validate_readback(&row, &items, &actual)?;
    let receipt = PriceListReceipt {
        id: id.to_owned(),
        status: row.status.clone(),
        version: row.version,
    };
    let result_json = serde_json::to_string(&receipt.to_wire()).map_err(|_| request_invalid())?;
    if !repository
        .insert_receipt(
            &ReceiptEntry {
                key: key.as_str(),
                operation,
                actor,
                request_hash: &request_hash,
                record_id: id,
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
                key: key.as_str(),
                operation,
                record_type: "sales_price_list",
                record_id: id,
                version: receipt.version,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    repository
        .outbox(
            &OutboxEntry {
                key: key.as_str(),
                operation,
                record_type: "sales_price_list",
                record_id: id,
                version: receipt.version,
                status: &receipt.status,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    Ok(receipt)
}

/// Activate or cancel one draft list. Activation requires a Company sales
/// manager who is neither the creator nor the latest draft preparer.
pub async fn decide_price_list<R: PriceListCommandRepository>(
    actor: &PriceListActor,
    id: &str,
    decision: PriceListDecision,
    key: &CommandKey,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<PriceListReceipt, R::Error> {
    actor.require_approve(now)?;
    if !key_valid(key) {
        return Err(request_invalid().into());
    }
    let decision = decision.normalized()?;
    repository.lock_command(key.as_str()).await?;
    let operation = decision.operation();
    let input_json = serde_json::to_value(&decision).map_err(|_| request_invalid())?;
    let identity = command_identity(actor, operation, id, &input_json);
    let request_hash = repository.digest(&identity).await?;
    validate_hash(&request_hash)?;
    let expected_status = match decision.action {
        crate::domain::price_lists::PriceListDecisionAction::Activate => "active",
        crate::domain::price_lists::PriceListDecisionAction::Cancel => "cancelled",
    };
    if let Some(prior) = replay(
        repository.receipt(key.as_str()).await?,
        &request_hash,
        operation,
        id,
        expected_status,
        decision.expected_version + 1,
    )? {
        return Ok(prior);
    }
    if !repository.current_authority(actor, true, now).await? {
        return Err(permission_required().into());
    }
    let existing = repository
        .list(&actor.company_id, id, true)
        .await?
        .ok_or_else(not_found)?;
    existing.validate(&actor.company_id)?;
    if existing.status() != Some(crate::domain::price_lists::PriceListStatus::Draft) {
        return Err(locked().into());
    }
    if existing.version != decision.expected_version {
        return Err(version_conflict().into());
    }
    if decision.action == crate::domain::price_lists::PriceListDecisionAction::Activate
        && (existing.created_by == actor.user_id.as_str()
            || existing.prepared_by == actor.user_id.as_str())
    {
        return Err(independent_approver_required().into());
    }
    let existing_items = repository.items(&actor.company_id, id).await?;
    let row = expected_decision_list(&existing, actor, &decision, now.get());
    if !repository
        .decide_list(&row, decision.expected_version)
        .await?
    {
        return Err(version_conflict().into());
    }
    let actual = repository
        .readback(&actor.company_id, id)
        .await?
        .ok_or_else(readback_invalid)?;
    if actual.list != row || actual.items != existing_items {
        return Err(readback_invalid().into());
    }
    for item in &actual.items {
        item.validate(id)?;
    }
    let receipt = PriceListReceipt {
        id: id.to_owned(),
        status: row.status.clone(),
        version: row.version,
    };
    let result_json = serde_json::to_string(&receipt.to_wire()).map_err(|_| request_invalid())?;
    if !repository
        .insert_receipt(
            &ReceiptEntry {
                key: key.as_str(),
                operation,
                actor,
                request_hash: &request_hash,
                record_id: id,
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
                key: key.as_str(),
                operation,
                record_type: "sales_price_list",
                record_id: id,
                version: receipt.version,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    repository
        .outbox(
            &OutboxEntry {
                key: key.as_str(),
                operation,
                record_type: "sales_price_list",
                record_id: id,
                version: receipt.version,
                status: &receipt.status,
                authority_json: &authority,
            },
            now,
        )
        .await?;
    Ok(receipt)
}

/// Serialize one workspace price-list row for retained callers. This helper is
/// intentionally owner-read only; the command readback uses its exact rows.
pub fn row_to_wire(row: &PriceListRow) -> Value {
    serde_json::json!({
        "id": row.id,
        "company_id": row.company_id,
        "name": row.name,
        "currency": row.currency,
        "customer_id": row.customer_id,
        "valid_from": row.valid_from,
        "valid_until": row.valid_until,
        "max_auto_discount_bps": row.max_auto_discount_bps,
        "status": row.status,
        "version": row.version,
        "created_by": row.created_by,
        "prepared_by": row.prepared_by,
        "approved_by": row.approved_by,
        "approval_reason": row.approval_reason,
        "activated_at": row.activated_at,
        "created_at": row.created_at,
        "updated_at": row.updated_at,
    })
}
