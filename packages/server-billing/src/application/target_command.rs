//! Native Billing `assign-target` command use case.
//!
//! The caller's serializable Control transaction supplies the owner unit. The
//! application locks the Holding and command key, resolves the canonical
//! receipt, applies the account-version CAS, validates the price and Sandbox
//! designation, inserts the immutable assignment, then writes audit and
//! receipt and verifies persisted readback. It never rates, invoices or calls
//! Stripe.
use super::BillingFuture;
use super::cost::{AccountSnapshot, AuditEntry, ReceiptSnapshot};
use crate::domain::actor::BillingActor;
use crate::domain::command::{
    CommandReceipt, account_required, holding_not_found, idempotency_conflict, js_trim,
    parse_receipt_result, readback_invalid, receipt_replay, source_invalid, valid_hash,
    version_conflict,
};
use crate::domain::forecast_read::billing_month;
use crate::domain::target_command::{
    AssignTarget, TargetCommandPrice, ValidatedAssignment, assignment_exists, backdate_forbidden,
    rollout_not_ready, target_price_missing,
};
use crate::domain::target_pricing::month_bounds;
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};

pub trait AssignTargetRepository: Send {
    type Error: From<BusinessError> + Send;

    fn digest<'a>(&'a mut self, value: &'a str) -> BillingFuture<'a, String, Self::Error>;

    fn lock_holding<'a>(&'a mut self, holding: &'a HoldingId)
    -> BillingFuture<'a, (), Self::Error>;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> BillingFuture<'a, (), Self::Error>;

    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> BillingFuture<'a, Option<ReceiptSnapshot>, Self::Error>;

    fn holding_exists<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn account_for_update<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<AccountSnapshot>, Self::Error>;

    fn current_price<'a>(
        &'a mut self,
        month: &'a str,
    ) -> BillingFuture<'a, Option<TargetCommandPrice>, Self::Error>;

    fn selected_price<'a>(
        &'a mut self,
        id: &'a str,
        month: &'a str,
    ) -> BillingFuture<'a, Option<TargetCommandPrice>, Self::Error>;

    fn sandbox_designated<'a>(
        &'a mut self,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn approved_rollout<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        month: &'a str,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn insert_assignment<'a>(
        &'a mut self,
        assignment: &'a ValidatedAssignment,
        holding: &'a HoldingId,
        price_id: &'a str,
        service_started_at: i64,
        actor: &'a BillingActor,
        reason: &'a str,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn increment_account_version<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        expected_version: i64,
    ) -> BillingFuture<'a, Option<i64>, Self::Error>;

    fn insert_audit<'a>(
        &'a mut self,
        entry: &'a AuditEntry<'a>,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn insert_receipt<'a>(
        &'a mut self,
        key: &'a str,
        digest: &'a str,
        result_json: &'a str,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn serialized_receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> BillingFuture<'a, Option<ReceiptSnapshot>, Self::Error>;
}

pub async fn execute_assign_target<R: AssignTargetRepository>(
    actor: &BillingActor,
    target: &HoldingId,
    command: &AssignTarget,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<CommandReceipt, R::Error> {
    actor.require_target_command(target, now)?;
    let validated = command.validate()?;
    let identity = command.identity(actor, target);
    let request_hash = repository.digest(&identity).await?;
    if !valid_hash(&request_hash) {
        return Err(source_invalid().into());
    }
    repository.lock_holding(target).await?;
    repository.lock_command(command.key.as_str()).await?;
    if let Some(receipt) = repository.receipt(command.key.as_str()).await? {
        receipt_replay(
            Some(CommandReceipt {
                digest: receipt.digest.clone(),
                result_json: receipt.result_json.clone(),
            }),
            &request_hash,
        )
        .map_err(R::Error::from)?;
        return Ok(CommandReceipt {
            digest: receipt.digest,
            result_json: receipt.result_json,
        });
    }
    if !repository.holding_exists(target).await? {
        return Err(holding_not_found().into());
    }
    let account = repository
        .account_for_update(target)
        .await?
        .ok_or_else(account_required)?;
    if account.version != command.expected_version {
        return Err(version_conflict().into());
    }
    let current_month = billing_month(now.get())?;
    let price = match &validated.price_id {
        Some(id) => {
            repository
                .selected_price(id, &validated.target_month)
                .await?
        }
        None => repository.current_price(&validated.target_month).await?,
    }
    .ok_or_else(target_price_missing)?;
    if validated.target_mode == "sandbox_invoice" {
        if validated.target_month < current_month {
            return Err(backdate_forbidden().into());
        }
        let designated = repository.sandbox_designated(target).await?;
        let rollout = validated.target_month > current_month
            && repository
                .approved_rollout(target, &validated.target_month)
                .await?;
        if !designated && !rollout {
            return Err(rollout_not_ready().into());
        }
    }
    let bounds = month_bounds(&validated.target_month)?;
    let service_started_at = if validated.target_month < current_month {
        bounds.0
    } else {
        now.get().max(bounds.0)
    };
    if !repository
        .insert_assignment(
            &validated,
            target,
            &price.id,
            service_started_at,
            actor,
            js_trim(&command.reason),
            now,
        )
        .await?
    {
        return Err(assignment_exists().into());
    }
    let version = repository
        .increment_account_version(target, command.expected_version)
        .await?
        .ok_or_else(version_conflict)?;
    if version != command.expected_version + 1 {
        return Err(readback_invalid().into());
    }
    let details = command.details();
    if !repository
        .insert_audit(
            &AuditEntry {
                key: command.key.as_str(),
                actor,
                target,
                action: "assign-target",
                reason: js_trim(&command.reason),
                details_json: &details,
            },
            now,
        )
        .await?
    {
        return Err(idempotency_conflict().into());
    }
    let result = command.result_json();
    if !repository
        .insert_receipt(command.key.as_str(), &request_hash, result, now)
        .await?
    {
        return Err(idempotency_conflict().into());
    }
    let receipt = repository
        .serialized_receipt(command.key.as_str())
        .await?
        .ok_or_else(readback_invalid)?;
    if receipt.digest != request_hash || receipt.result_json != result {
        return Err(readback_invalid().into());
    }
    parse_receipt_result(&receipt.result_json)?;
    Ok(CommandReceipt {
        digest: receipt.digest,
        result_json: receipt.result_json,
    })
}
