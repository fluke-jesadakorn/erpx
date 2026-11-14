//! Native POS cash-movement use case.
//!
//! Repository SQL stays on the caller's retained serializable Holding
//! transaction. The application checks current permission/module/context
//! before any source read, resolves the command receipt before a fresh write,
//! applies an exact shift-version CAS, records the movement, receipt and POS
//! audit in one unit, then validates persisted readback before returning.
use crate::domain::cash_movement::{
    CashMovementActor, CashMovementReadback, CashMovementReceipt, CashReceipt, CashShift,
    CreateCashMovement, ValidatedCashMovement, day_closed, idempotency_conflict, not_found,
    parse_receipt_result, readback_invalid, receipt_identity, request_invalid, setup_required,
    source_invalid, version_conflict,
};
use erpx_server_shared::public::{BusinessError, CompanyId, TimestampMillis, UserId};
use std::future::Future;

pub type CashMovementFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

pub trait CashMovementRepository: Send {
    type Error: From<BusinessError> + Send;

    fn digest<'a>(&'a mut self, value: &'a str) -> CashMovementFuture<'a, String, Self::Error>;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> CashMovementFuture<'a, Option<CashReceipt>, Self::Error>;

    fn shift<'a>(
        &'a mut self,
        company: &'a CompanyId,
        shift_id: &'a str,
    ) -> CashMovementFuture<'a, Option<CashShift>, Self::Error>;

    fn setup_exists<'a>(
        &'a mut self,
        company: &'a CompanyId,
    ) -> CashMovementFuture<'a, bool, Self::Error>;

    fn day_writable<'a>(
        &'a mut self,
        company: &'a CompanyId,
        branch_id: &'a str,
        business_date: &'a str,
    ) -> CashMovementFuture<'a, bool, Self::Error>;

    fn insert_movement<'a>(
        &'a mut self,
        movement: &'a ValidatedCashMovement,
        actor: &'a UserId,
        now: TimestampMillis,
    ) -> CashMovementFuture<'a, String, Self::Error>;

    fn bump_shift<'a>(
        &'a mut self,
        shift_id: &'a str,
        expected_version: i64,
    ) -> CashMovementFuture<'a, bool, Self::Error>;

    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        company: &'a CompanyId,
        request_hash: &'a str,
        result_json: &'a str,
        now: TimestampMillis,
    ) -> CashMovementFuture<'a, bool, Self::Error>;

    fn audit<'a>(
        &'a mut self,
        actor: &'a CashMovementActor,
        movement: &'a ValidatedCashMovement,
        receipt: &'a CashMovementReceipt,
        now: TimestampMillis,
    ) -> CashMovementFuture<'a, (), Self::Error>;

    fn readback<'a>(
        &'a mut self,
        movement_id: &'a str,
        shift_id: &'a str,
    ) -> CashMovementFuture<'a, Option<CashMovementReadback>, Self::Error>;
}

pub async fn create_cash_movement<R: CashMovementRepository>(
    actor: &CashMovementActor,
    command: &CreateCashMovement,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<CashMovementReceipt, R::Error> {
    actor.require_cash(now)?;
    let movement = command.validate()?;
    let identity = receipt_identity(actor, &movement);
    let request_hash = repository.digest(&identity).await?;
    if request_hash.len() != 64 || !request_hash.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(source_invalid().into());
    }
    if let Some(receipt) = repository.receipt(command.key().as_str()).await? {
        if receipt.request_hash != request_hash {
            return Err(idempotency_conflict().into());
        }
        return parse_receipt_result(&receipt.result_json).map_err(Into::into);
    }
    let shift = repository
        .shift(&actor.company_id, &movement.shift_id)
        .await?
        .ok_or_else(not_found)?;
    movement.validate_shift(actor, &shift)?;
    if !repository
        .day_writable(&actor.company_id, &shift.branch_id, &shift.business_date)
        .await?
    {
        return Err(day_closed().into());
    }
    if !repository.setup_exists(&actor.company_id).await? {
        return Err(setup_required().into());
    }
    let movement_id = repository
        .insert_movement(&movement, &actor.user_id, now)
        .await?;
    if !crate::domain::cash_movement::valid_uuid(&movement_id) {
        return Err(readback_invalid().into());
    }
    if !repository
        .bump_shift(&movement.shift_id, movement.expected_version)
        .await?
    {
        return Err(version_conflict().into());
    }
    let receipt = CashMovementReceipt {
        id: movement_id.clone(),
    };
    let result_json = serde_json::to_string(&receipt.to_wire()).map_err(|_| request_invalid())?;
    if !repository
        .insert_receipt(
            command.key().as_str(),
            &actor.company_id,
            &request_hash,
            &result_json,
            now,
        )
        .await?
    {
        return Err(idempotency_conflict().into());
    }
    repository.audit(actor, &movement, &receipt, now).await?;
    let readback = repository
        .readback(&movement_id, &movement.shift_id)
        .await?
        .ok_or_else(readback_invalid)?;
    movement.validate_readback(&readback)?;
    Ok(receipt)
}
