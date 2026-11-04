//! Native Procurement supplier promise record use case.
//!
//! Repository SQL stays on the caller's retained SERIALIZABLE Holding
//! transaction. The application checks the current Account/IAM policy before
//! any source read, resolves an exact idempotency receipt before a fresh write,
//! reads the current Commerce purchase source through the owner port, validates
//! the complete promise plan, records the immutable promise, advances the
//! Commerce order version with CAS, and writes the command receipt/audit/outbox
//! evidence in the same unit before validating persisted readback.
use crate::domain::supplier_promise_command::{
    PromiseSource, RecordSupplierPromiseCommand, SupplierPromiseAuthority,
    SupplierPromiseCommandActor, SupplierPromiseReadback, SupplierPromiseRecordReceipt,
    OPERATION, fingerprint, idempotency_conflict, incomplete_receipt, invalid_receipt, line_id,
    not_found, prepare_supplier_promise, promise_id, source_changed, validate_readback,
};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, TimestampMillis};
use std::future::Future;

pub type SupplierPromiseCommandFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StoredSupplierPromiseReceipt {
    pub operation: String,
    pub actor_id: String,
    pub record_id: String,
    pub request_fingerprint: String,
    pub authority_json: Option<String>,
    pub result_json: Option<String>,
}

#[derive(Clone, Debug)]
pub struct SupplierPromiseCommitPlan {
    pub key: CommandKey,
    pub operation: String,
    pub company_id: CompanyId,
    pub order_id: String,
    pub actor_id: String,
    pub assignment_id: String,
    pub branch_id: Option<String>,
    pub department_id: Option<String>,
    pub promise_id: String,
    pub order_version: i64,
    pub kind: String,
    pub supplier_id: String,
    pub currency: String,
    pub required_date: String,
    pub confirmation_date: String,
    pub supplier_reference: String,
    pub evidence: String,
    pub lines: Vec<crate::domain::supplier_promise_command::PreparedPromiseLine>,
    pub now: TimestampMillis,
}

/// Owner persistence and the single retained Commerce order-version effect.
///
/// `advance_order_version` is the same CAS statement the Commerce public
/// supplier-promise version port exports to the retained command. Composition
/// may replace that method with a Commerce-owned port, but it must remain inside
/// this caller-owned transaction and must return `true` only when exactly the
/// current order version advanced.
pub trait SupplierPromiseCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> SupplierPromiseCommandFuture<'a, Option<StoredSupplierPromiseReceipt>, Self::Error>;

    fn source<'a>(
        &'a mut self,
        actor: &'a SupplierPromiseCommandActor,
        order_id: &'a str,
    ) -> SupplierPromiseCommandFuture<'a, Option<PromiseSource>, Self::Error>;

    fn insert_promise<'a>(
        &'a mut self,
        plan: &'a SupplierPromiseCommitPlan,
    ) -> SupplierPromiseCommandFuture<'a, (), Self::Error>;

    fn advance_order_version<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
        expected_version: i64,
        now: TimestampMillis,
    ) -> SupplierPromiseCommandFuture<'a, bool, Self::Error>;

    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        operation: &'a str,
        actor: &'a SupplierPromiseCommandActor,
        request_fingerprint: &'a str,
        record_id: &'a str,
        result_json: &'a str,
        authority_json: &'a str,
        now: TimestampMillis,
    ) -> SupplierPromiseCommandFuture<'a, bool, Self::Error>;

    fn audit<'a>(
        &'a mut self,
        plan: &'a SupplierPromiseCommitPlan,
        receipt: &'a SupplierPromiseRecordReceipt,
        authority_json: &'a str,
    ) -> SupplierPromiseCommandFuture<'a, (), Self::Error>;

    fn outbox<'a>(
        &'a mut self,
        plan: &'a SupplierPromiseCommitPlan,
        receipt: &'a SupplierPromiseRecordReceipt,
        authority_json: &'a str,
    ) -> SupplierPromiseCommandFuture<'a, (), Self::Error>;

    fn readback<'a>(
        &'a mut self,
        company: &'a CompanyId,
        order_id: &'a str,
        promise_id: &'a str,
    ) -> SupplierPromiseCommandFuture<'a, Option<SupplierPromiseReadback>, Self::Error>;
}

fn authority_matches(saved: &str, current: &SupplierPromiseAuthority) -> bool {
    serde_json::from_str::<SupplierPromiseAuthority>(saved)
        .is_ok_and(|saved| {
            saved.user_id == current.user_id
                && saved.assignment_id == current.assignment_id
                && saved.company_id == current.company_id
                && saved.branch_id == current.branch_id
                && saved.department_id == current.department_id
                && saved.revision == current.revision
                && saved.assignment_version == current.assignment_version
        })
}

fn replay(
    stored: Option<StoredSupplierPromiseReceipt>,
    actor: &SupplierPromiseCommandActor,
    record_id: &str,
    request_fingerprint: &str,
) -> Result<Option<SupplierPromiseRecordReceipt>, BusinessError> {
    let Some(stored) = stored else {
        return Ok(None);
    };
    if stored.operation != OPERATION
        || stored.actor_id != actor.user_id
        || stored.record_id != record_id
        || stored.request_fingerprint != request_fingerprint
    {
        return Err(idempotency_conflict());
    }
    let Some(authority) = stored.authority_json else {
        return Err(invalid_receipt());
    };
    if !authority_matches(&authority, &SupplierPromiseAuthority::from_actor(actor)) {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "WORK_CONTEXT_CHANGED",
            "Your work role changed. Reload and review before retrying.",
        ));
    }
    let Some(result) = stored.result_json else {
        return Err(incomplete_receipt());
    };
    let receipt: SupplierPromiseRecordReceipt =
        serde_json::from_str(&result).map_err(|_| invalid_receipt())?;
    if receipt.id != record_id || receipt.promise_id.is_empty() || receipt.version < 1 {
        return Err(invalid_receipt());
    }
    Ok(Some(receipt))
}

pub async fn record_supplier_promise<R: SupplierPromiseCommandRepository>(
    actor: &SupplierPromiseCommandActor,
    repository: &mut R,
    order_id: &str,
    input: &RecordSupplierPromiseCommand,
    key: &str,
    now: TimestampMillis,
) -> Result<SupplierPromiseRecordReceipt, R::Error> {
    actor.require_record(now)?;
    let validated = input.validate()?;
    let key = CommandKey::new(key)?;
    let order_id = erpx_server_shared::public::RecordId::new(order_id)
        .map_err(|_| not_found())?
        .as_str()
        .to_owned();
    let request_fingerprint = fingerprint(actor, &order_id, input)?;
    if let Some(receipt) = replay(
        repository.receipt(key.as_str()).await?,
        actor,
        &order_id,
        &request_fingerprint,
    )? {
        return Ok(receipt);
    }
    let source = repository
        .source(actor, &order_id)
        .await?
        .ok_or_else(not_found)?;
    let prepared = prepare_supplier_promise(actor, &validated, source)?;
    if prepared.order.version != input.expected_version {
        return Err(source_changed().into());
    }
    let promise = promise_id(&key);
    let plan = SupplierPromiseCommitPlan {
        key: key.clone(),
        operation: OPERATION.to_owned(),
        company_id: actor.company_id.clone(),
        order_id: order_id.clone(),
        actor_id: actor.user_id.clone(),
        assignment_id: actor.assignment_id.clone(),
        branch_id: actor
            .branch_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        department_id: actor
            .department_id
            .as_ref()
            .map(|value| value.as_str().to_owned()),
        promise_id: promise.clone(),
        order_version: input.expected_version,
        kind: input.kind.clone(),
        supplier_id: prepared.order.supplier_id.clone(),
        currency: prepared.order.currency.clone(),
        required_date: prepared.order.due_date.clone(),
        confirmation_date: input.confirmation_date.clone(),
        supplier_reference: input.supplier_reference.clone(),
        evidence: input.evidence.clone(),
        lines: prepared.lines.clone(),
        now,
    };
    repository.insert_promise(&plan).await?;
    if !repository
        .advance_order_version(
            &actor.company_id,
            &order_id,
            input.expected_version,
            now,
        )
        .await?
    {
        return Err(source_changed().into());
    }
    let receipt = SupplierPromiseRecordReceipt {
        id: order_id.clone(),
        promise_id: promise.clone(),
        version: input.expected_version + 1,
    };
    let result_json = serde_json::to_string(&receipt).map_err(|_| invalid_receipt())?;
    let authority_json = serde_json::to_string(&SupplierPromiseAuthority::from_actor(actor))
        .map_err(|_| invalid_receipt())?;
    if !repository
        .insert_receipt(
            key.as_str(),
            OPERATION,
            actor,
            &request_fingerprint,
            &order_id,
            &result_json,
            &authority_json,
            now,
        )
        .await?
    {
        return Err(idempotency_conflict().into());
    }
    repository.audit(&plan, &receipt, &authority_json).await?;
    repository.outbox(&plan, &receipt, &authority_json).await?;
    let readback = repository
        .readback(&actor.company_id, &order_id, &promise)
        .await?
        .ok_or_else(invalid_receipt)?;
    validate_readback(&readback, &prepared, &promise, actor, now).map_err(Into::into)?;
    Ok(receipt)
}

/// Public helper for owner HTTP adapters that only need an exact line identity.
pub fn promise_line_id(promise_id: &str, ordinal: i64) -> String {
    line_id(promise_id, ordinal)
}
