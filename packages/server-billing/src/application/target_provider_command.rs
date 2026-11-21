//! Native Billing target provider evidence commands
//! (`verify-provider-cost`, `verify-statement`). The caller owns a serializable
//! Control unit; the command locks and CASes the account, validates the meter,
//! FX and evidence, inserts the immutable rows, then writes audit/receipt.
use super::BillingFuture;
use super::cost::{AccountSnapshot, AuditEntry, ReceiptSnapshot};
use crate::domain::actor::BillingActor;
use crate::domain::command::{
    CommandReceipt, account_required, holding_not_found, idempotency_conflict, js_trim,
    parse_receipt_result, readback_invalid, receipt_replay, source_invalid, valid_hash,
    version_conflict,
};
use crate::domain::target_pricing::round_satang;
use crate::domain::target_provider_command::{
    BillingTargetProviderCommand, ProviderMeter, ValidatedProviderCost, ValidatedProviderEvidence,
    ValidatedStatement, evidence_required, fx_mismatch, period_frozen,
    provider_cost_already_verified, provider_cost_unverified, statement_already_verified,
    statement_mismatch,
};
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};

pub trait TargetProviderRepository: Send {
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

    fn successful_platform_meter<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        month: &'a str,
        source_key: &'a str,
    ) -> BillingFuture<'a, Option<ProviderMeter>, Self::Error>;

    fn available_evidence<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn approved_period<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        month: &'a str,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn existing_provider_cost<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        month: &'a str,
        source_key: &'a str,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn insert_provider_cost<'a>(
        &'a mut self,
        draft: &'a ValidatedProviderCost,
        holding: &'a HoldingId,
        actor: &'a BillingActor,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn provider_statement_total<'a>(
        &'a mut self,
        holding: &'a HoldingId,
        month: &'a str,
        provider: &'a str,
        statement_id: &'a str,
        currency: &'a str,
    ) -> BillingFuture<'a, (i64, i128), Self::Error>;

    fn insert_statement<'a>(
        &'a mut self,
        draft: &'a ValidatedStatement,
        holding: &'a HoldingId,
        actor: &'a BillingActor,
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

pub async fn execute_target_provider<R: TargetProviderRepository>(
    actor: &BillingActor,
    target: &HoldingId,
    command: &BillingTargetProviderCommand,
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
    match &validated {
        ValidatedProviderEvidence::ProviderCost(draft) => {
            let meter = repository
                .successful_platform_meter(target, &draft.target_month, &draft.source_key)
                .await?
                .ok_or_else(provider_cost_unverified)?;
            let verified = meter
                .verified_cost_satang
                .as_deref()
                .and_then(|value| value.parse::<i64>().ok());
            if meter.provider != draft.provider
                || meter.cost_status.as_deref() != Some("reconciled")
                || meter.cost_reconciled_at.is_none()
                || verified != Some(draft.provider_cost_satang)
            {
                return Err(provider_cost_unverified().into());
            }
            let expected = round_satang(
                i128::from(draft.original_minor) * i128::from(draft.fx_numerator),
                i128::from(draft.fx_denominator),
            )?;
            if expected != draft.provider_cost_satang {
                return Err(fx_mismatch().into());
            }
            if !repository
                .available_evidence(&draft.evidence_id, target)
                .await?
            {
                return Err(evidence_required().into());
            }
            if repository
                .approved_period(target, &draft.target_month)
                .await?
            {
                return Err(period_frozen().into());
            }
            if repository
                .existing_provider_cost(target, &draft.target_month, &draft.source_key)
                .await?
            {
                return Err(provider_cost_already_verified().into());
            }
            if !repository
                .insert_provider_cost(draft, target, actor, now)
                .await?
            {
                return Err(provider_cost_already_verified().into());
            }
        }
        ValidatedProviderEvidence::Statement(draft) => {
            if !repository
                .available_evidence(&draft.evidence_id, target)
                .await?
            {
                return Err(evidence_required().into());
            }
            if repository
                .approved_period(target, &draft.target_month)
                .await?
            {
                return Err(period_frozen().into());
            }
            let (count, total) = repository
                .provider_statement_total(
                    target,
                    &draft.target_month,
                    &draft.provider,
                    &draft.statement_id,
                    &draft.original_currency,
                )
                .await?;
            if count == 0 || total != i128::from(draft.provider_total_minor) {
                return Err(statement_mismatch().into());
            }
            if !repository
                .insert_statement(draft, target, actor, now)
                .await?
            {
                return Err(statement_already_verified().into());
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
