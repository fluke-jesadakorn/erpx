//! Native Billing target evidence commands (`record-operating-cost`,
//! `review-target-capacity`). The caller owns a serializable Control unit; the
//! command locks and CASes the account, validates available evidence, inserts
//! the immutable evidence row, then writes audit/receipt and verifies readback.
use super::BillingFuture;
use super::cost::{AccountSnapshot, AuditEntry, ReceiptSnapshot};
use crate::domain::actor::BillingActor;
use crate::domain::command::{
    CommandReceipt, account_required, holding_not_found, idempotency_conflict, js_trim,
    parse_receipt_result, readback_invalid, receipt_replay, source_invalid, valid_hash,
    version_conflict,
};
use crate::domain::forecast_read::billing_month;
use crate::domain::target_evidence_command::{
    BillingTargetEvidenceCommand, ValidatedCapacity, ValidatedOperatingCost,
    ValidatedTargetEvidence, capacity_already_reviewed, evidence_required, future_month_required,
    month_not_closed, operating_cost_already_verified,
};
use crate::domain::target_pricing::month_bounds;
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};

pub trait TargetEvidenceRepository: Send {
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

    fn available_evidence<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn insert_operating_cost<'a>(
        &'a mut self,
        draft: &'a ValidatedOperatingCost,
        holding: &'a HoldingId,
        actor: &'a BillingActor,
        reason: &'a str,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn insert_capacity<'a>(
        &'a mut self,
        draft: &'a ValidatedCapacity,
        holding: &'a HoldingId,
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

pub async fn execute_target_evidence<R: TargetEvidenceRepository>(
    actor: &BillingActor,
    target: &HoldingId,
    command: &BillingTargetEvidenceCommand,
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
    repository.lock_command(command.key().as_str()).await?;
    if let Some(receipt) = repository.receipt(command.key().as_str()).await? {
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
    if account.version != command.expected_version() {
        return Err(version_conflict().into());
    }
    let current_month = billing_month(now.get())?;
    match &validated {
        ValidatedTargetEvidence::OperatingCost(draft) => {
            if now.get() < month_bounds(&draft.target_month)?.1 {
                return Err(month_not_closed().into());
            }
            if !repository
                .available_evidence(&draft.evidence_id, target)
                .await?
            {
                return Err(evidence_required().into());
            }
            if !repository
                .insert_operating_cost(draft, target, actor, js_trim(command.reason()), now)
                .await?
            {
                return Err(operating_cost_already_verified().into());
            }
        }
        ValidatedTargetEvidence::Capacity(draft) => {
            if draft.target_month <= current_month {
                return Err(future_month_required().into());
            }
            if !repository
                .available_evidence(&draft.evidence_id, target)
                .await?
            {
                return Err(evidence_required().into());
            }
            if !repository
                .insert_capacity(draft, target, actor, js_trim(command.reason()), now)
                .await?
            {
                return Err(capacity_already_reviewed().into());
            }
        }
    }
    let version = repository
        .increment_account_version(target, command.expected_version())
        .await?
        .ok_or_else(version_conflict)?;
    if version != command.expected_version() + 1 {
        return Err(readback_invalid().into());
    }
    let details = command.details();
    if !repository
        .insert_audit(
            &AuditEntry {
                key: command.key().as_str(),
                actor,
                target,
                action: command.action(),
                reason: js_trim(command.reason()),
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
        .insert_receipt(command.key().as_str(), &request_hash, result, now)
        .await?
    {
        return Err(idempotency_conflict().into());
    }
    let receipt = repository
        .serialized_receipt(command.key().as_str())
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
