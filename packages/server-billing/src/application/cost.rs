//! Native Billing actual-cost review command use case.
//!
//! The caller's serializable Control transaction supplies the owner unit. The
//! application locks the Holding and command key, resolves the canonical
//! receipt, applies the exact account-version CAS, writes the cost/period,
//! audit and receipt in that one transaction and validates persisted readback.
//! It never calls Stripe and never issues an invoice.
use super::BillingFuture;
use crate::domain::actor::BillingActor;
use crate::domain::command::{
    BillingCostCommand, CommandReceipt, ValidatedCostCommand, ValidatedCostDraft, account_required,
    command_identity, cost_frozen, holding_not_found, idempotency_conflict, invalid_evidence,
    invalid_original, js_trim, parse_receipt_result, period_required, readback_invalid,
    receipt_replay, result_json, source_invalid, valid_hash, validate_complete_readback,
    validate_period_dates, version_conflict,
};
use crate::domain::workspace::{BillingCostView, BillingPeriodView};
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};

#[derive(Clone, Debug)]
pub struct AccountSnapshot {
    pub version: i64,
}

#[derive(Clone, Debug)]
pub struct ReceiptSnapshot {
    pub digest: String,
    pub result_json: String,
}

pub struct AuditEntry<'a> {
    pub key: &'a str,
    pub actor: &'a BillingActor,
    pub target: &'a HoldingId,
    pub action: &'a str,
    pub reason: &'a str,
    pub details_json: &'a str,
}

pub trait BillingCostCommandRepository: Send {
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

    fn period<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingPeriodView>, Self::Error>;

    fn available_evidence<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn original_invoice<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
        period_id: &'a str,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn existing_cost<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
        period_id: &'a str,
    ) -> BillingFuture<'a, Option<BillingCostView>, Self::Error>;

    fn insert_cost<'a>(
        &'a mut self,
        cost: &'a ValidatedCostDraft,
        holding: &'a HoldingId,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn cost_readback<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
        period_id: &'a str,
    ) -> BillingFuture<'a, Option<BillingCostView>, Self::Error>;

    fn delete_unissued_cost<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<String>, Self::Error>;

    fn cost_exists<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn mark_costs_incomplete<'a>(
        &'a mut self,
        period_id: &'a str,
    ) -> BillingFuture<'a, (), Self::Error>;

    fn set_costs_complete<'a>(
        &'a mut self,
        period_id: &'a str,
        complete: bool,
    ) -> BillingFuture<'a, (), Self::Error>;

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

async fn replay<R: BillingCostCommandRepository>(
    repository: &mut R,
    key: &str,
    request_hash: &str,
) -> Result<Option<CommandReceipt>, R::Error> {
    let Some(receipt) = repository.receipt(key).await? else {
        return Ok(None);
    };
    receipt_replay(
        Some(CommandReceipt {
            digest: receipt.digest.clone(),
            result_json: receipt.result_json.clone(),
        }),
        request_hash,
    )
    .map_err(R::Error::from)?;
    Ok(Some(CommandReceipt {
        digest: receipt.digest,
        result_json: receipt.result_json,
    }))
}

pub async fn execute_cost_command<R: BillingCostCommandRepository>(
    actor: &BillingActor,
    target: &HoldingId,
    command: &BillingCostCommand,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<CommandReceipt, R::Error> {
    actor.require_cost_command(target, now)?;
    let validated = command.validate()?;
    let identity = command_identity(actor, target, command);
    let request_hash = repository.digest(&identity).await?;
    if !valid_hash(&request_hash) {
        return Err(source_invalid().into());
    }
    repository.lock_holding(target).await?;
    repository.lock_command(command.key().as_str()).await?;
    if let Some(prior) = replay(repository, command.key().as_str(), &request_hash).await? {
        return Ok(prior);
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
        ValidatedCostCommand::Record(draft) => {
            let period = repository
                .period(&draft.period_id, target)
                .await?
                .ok_or_else(period_required)?;
            validate_period_dates(&validated, &period)?;
            if let Some(original) = &draft.original_invoice_id
                && !repository
                    .original_invoice(original, target, &draft.period_id)
                    .await?
            {
                return Err(invalid_original().into());
            }
            if let Some(evidence) = &draft.evidence_id
                && !repository.available_evidence(evidence, target).await?
            {
                return Err(invalid_evidence().into());
            }
            if let Some(cost_id) = command_cost_id(command) {
                let Some(existing) = repository
                    .existing_cost(cost_id, target, &draft.period_id)
                    .await?
                else {
                    return Err(cost_frozen().into());
                };
                if existing.invoice_id.is_some() {
                    return Err(cost_frozen().into());
                }
            }
            if !repository.insert_cost(draft, target, now).await? {
                return Err(idempotency_conflict().into());
            }
            repository.mark_costs_incomplete(&draft.period_id).await?;
            let cost = repository
                .cost_readback(&draft.id, target, &draft.period_id)
                .await?
                .ok_or_else(readback_invalid)?;
            crate::domain::command::RecordCost::validate_readback(draft, &cost)?;
        }
        ValidatedCostCommand::Delete { cost_id } => {
            let period_id = repository
                .delete_unissued_cost(cost_id, target)
                .await?
                .ok_or_else(cost_frozen)?;
            repository.mark_costs_incomplete(&period_id).await?;
            if repository.cost_exists(cost_id, target).await? {
                return Err(readback_invalid().into());
            }
        }
        ValidatedCostCommand::Complete {
            period_id,
            complete,
        } => {
            let period = repository
                .period(period_id, target)
                .await?
                .ok_or_else(period_required)?;
            repository.set_costs_complete(&period.id, *complete).await?;
            let readback = repository
                .period(&period.id, target)
                .await?
                .ok_or_else(readback_invalid)?;
            validate_complete_readback(&validated, &readback)?;
        }
    }

    let version = repository
        .increment_account_version(target, command.expected_version())
        .await?
        .ok_or_else(version_conflict)?;
    if version != command.expected_version() + 1 {
        return Err(readback_invalid().into());
    }
    let details = command_details(command);
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
    let result = result_json();
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

fn command_cost_id(command: &BillingCostCommand) -> Option<&str> {
    match command {
        BillingCostCommand::Record(command) => command.cost_id.as_ref().map(|id| id.as_str()),
        _ => None,
    }
}

fn command_details(command: &BillingCostCommand) -> String {
    match command {
        BillingCostCommand::Record(command) => serde_json::json!({
            "version": command.expected_version,
            "reason": js_trim(&command.reason),
            "periodId": command.period_id.as_str(),
            "costId": command.cost_id.as_ref().map(|id| id.as_str()),
            "category": command.category.as_str(),
            "amountSatang": command.amount_satang,
            "serviceStart": command.service_start,
            "serviceEnd": command.service_end,
            "note": js_trim(&command.note),
            "simulated": command.simulated,
            "evidenceId": command.evidence_id.as_ref().map(|id| id.as_str()),
            "originalInvoiceId": command.original_invoice_id.as_ref().map(|id| id.as_str()),
        }),
        BillingCostCommand::Delete(command) => serde_json::json!({
            "version": command.expected_version,
            "reason": js_trim(&command.reason),
            "costId": command.cost_id.as_str(),
        }),
        BillingCostCommand::Complete(command) => serde_json::json!({
            "version": command.expected_version,
            "reason": js_trim(&command.reason),
            "periodId": command.period_id.as_str(),
            "complete": command.complete,
        }),
    }
    .to_string()
}
