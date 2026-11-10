//! Native Billing invoice lifecycle use case.
//!
//! The caller's serializable Control transaction supplies the owner unit. The
//! application locks the Holding and command key, resolves the canonical
//! receipt, applies the exact account-version CAS, loads the period, plan,
//! invoice and still-unissued cost facts, derives or freezes the reviewed
//! lines, writes the invoice/cost attachments/outbox job, audit and receipt in
//! that one transaction and validates persisted readback. It never calls
//! Stripe, creates a provider object or settles money.
use super::BillingFuture;
use super::cost::{AuditEntry, ReceiptSnapshot};
use crate::domain::actor::BillingActor;
use crate::domain::command::CommandReceipt;
use crate::domain::invoice::{
    BillingAccountFacts, BillingCostFacts, BillingInvoiceCommand, BillingInvoiceRecord,
    BillingPeriodFacts, ValidatedInvoiceWrite, command_identity, conflict_key, conflict_version,
    invalid_readback, missing_account, missing_holding, prepare_create, prepare_finalize,
    prepare_update, prepare_void, readback_result, replay_receipt, source_invalid,
    valid_request_hash, verify_readback, void_job_required,
};
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};

pub trait BillingInvoiceRepository: Send {
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
    ) -> BillingFuture<'a, Option<BillingAccountFacts>, Self::Error>;

    fn period_for_update<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingPeriodFacts>, Self::Error>;

    fn invoice_for_update<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingInvoiceRecord>, Self::Error>;

    fn current_main_invoice<'a>(
        &'a mut self,
        period_id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingInvoiceRecord>, Self::Error>;

    fn pending_invoice<'a>(
        &'a mut self,
        period_id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<String>, Self::Error>;

    fn unissued_costs<'a>(
        &'a mut self,
        period_id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Vec<BillingCostFacts>, Self::Error>;

    fn insert_invoice<'a>(
        &'a mut self,
        invoice: &'a ValidatedInvoiceWrite,
        holding: &'a HoldingId,
        actor: &'a BillingActor,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn update_invoice<'a>(
        &'a mut self,
        invoice_id: &'a str,
        invoice: &'a ValidatedInvoiceWrite,
        holding: &'a HoldingId,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn finalize_invoice<'a>(
        &'a mut self,
        invoice_id: &'a str,
        invoice: &'a ValidatedInvoiceWrite,
        holding: &'a HoldingId,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn void_invoice<'a>(
        &'a mut self,
        invoice_id: &'a str,
        invoice: &'a ValidatedInvoiceWrite,
        holding: &'a HoldingId,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn attach_cost<'a>(
        &'a mut self,
        cost_id: &'a str,
        invoice_id: &'a str,
        holding: &'a HoldingId,
        period_id: &'a str,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn release_costs<'a>(
        &'a mut self,
        invoice_id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, u64, Self::Error>;

    fn enqueue_job<'a>(
        &'a mut self,
        id: &'a str,
        invoice_id: &'a str,
        kind: &'a str,
        holding: &'a HoldingId,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error>;

    fn attached_cost_count<'a>(
        &'a mut self,
        invoice_id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, i64, Self::Error>;

    fn invoice_readback<'a>(
        &'a mut self,
        id: &'a str,
        holding: &'a HoldingId,
    ) -> BillingFuture<'a, Option<BillingInvoiceRecord>, Self::Error>;

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

async fn replay<R: BillingInvoiceRepository>(
    repository: &mut R,
    key: &str,
    request_hash: &str,
) -> Result<Option<CommandReceipt>, R::Error> {
    let Some(receipt) = repository.receipt(key).await? else {
        return Ok(None);
    };
    replay_receipt(
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

async fn verify<R: BillingInvoiceRepository>(
    repository: &mut R,
    invoice: &ValidatedInvoiceWrite,
    holding: &HoldingId,
) -> Result<(), R::Error> {
    let readback = repository
        .invoice_readback(&invoice.id, holding)
        .await?
        .ok_or_else(invalid_readback)?;
    verify_readback(invoice, &readback).map_err(R::Error::from)
}

pub async fn execute_invoice_command<R: BillingInvoiceRepository>(
    actor: &BillingActor,
    target: &HoldingId,
    command: &BillingInvoiceCommand,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<CommandReceipt, R::Error> {
    actor.require_cost_command(target, now)?;
    command.validate()?;
    let identity = command_identity(actor, target, command);
    let request_hash = repository.digest(&identity).await?;
    if !valid_request_hash(&request_hash) {
        return Err(source_invalid().into());
    }
    repository.lock_holding(target).await?;
    repository.lock_command(command.key().as_str()).await?;
    if let Some(receipt) = replay(repository, command.key().as_str(), &request_hash).await? {
        return Ok(receipt);
    }
    if !repository.holding_exists(target).await? {
        return Err(missing_holding().into());
    }
    let account = repository
        .account_for_update(target)
        .await?
        .ok_or_else(missing_account)?;
    if account.version != command.expected_version() {
        return Err(conflict_version().into());
    }

    let invoice = match command {
        BillingInvoiceCommand::Create(command) => {
            let period = repository
                .period_for_update(command.period_id.as_str(), target)
                .await?
                .ok_or_else(crate::domain::command::period_required)?;
            if repository
                .pending_invoice(&period.id, target)
                .await?
                .is_some()
            {
                return Err(crate::domain::invoice::invoice_pending().into());
            }
            let previous = repository.current_main_invoice(&period.id, target).await?;
            let costs = repository.unissued_costs(&period.id, target).await?;
            let write = prepare_create(
                command,
                &account,
                &period,
                previous.as_ref(),
                &costs,
                now.get(),
            )?;
            if !repository
                .insert_invoice(&write, target, actor, now)
                .await?
            {
                return Err(conflict_key().into());
            }
            if write.attach_costs {
                for cost_id in &write.cost_ids {
                    if !repository
                        .attach_cost(cost_id, &write.id, target, &write.period_id)
                        .await?
                    {
                        return Err(crate::domain::invoice::cost_frozen().into());
                    }
                }
            }
            if write.enqueue_issue {
                let job = format!("issue-{}", write.id);
                if !repository
                    .enqueue_job(&job, &write.id, "issue", target, now)
                    .await?
                {
                    return Err(conflict_key().into());
                }
            }
            write
        }
        BillingInvoiceCommand::Update(command) => {
            let record = repository
                .invoice_for_update(command.invoice_id.as_str(), target)
                .await?
                .ok_or_else(crate::domain::invoice::invoice_not_found)?;
            let period = repository
                .period_for_update(&record.period_id, target)
                .await?
                .ok_or_else(crate::domain::invoice::invoice_source_changed)?;
            let previous = repository.current_main_invoice(&period.id, target).await?;
            let costs = repository.unissued_costs(&period.id, target).await?;
            let write = prepare_update(
                command,
                &record,
                &period,
                previous.as_ref(),
                &costs,
                now.get(),
            )?;
            if !repository
                .update_invoice(&record.id, &write, target, now)
                .await?
            {
                return Err(crate::domain::invoice::invoice_source_changed().into());
            }
            write
        }
        BillingInvoiceCommand::Finalize(command) => {
            let record = repository
                .invoice_for_update(command.invoice_id.as_str(), target)
                .await?
                .ok_or_else(crate::domain::invoice::invoice_not_found)?;
            let period = repository
                .period_for_update(&record.period_id, target)
                .await?
                .ok_or_else(crate::domain::invoice::invoice_source_changed)?;
            let costs = repository.unissued_costs(&period.id, target).await?;
            let write = prepare_finalize(command, &record, &period, &account, &costs, now.get())?;
            if !repository
                .finalize_invoice(&record.id, &write, target, now)
                .await?
            {
                return Err(conflict_version().into());
            }
            for cost_id in &write.cost_ids {
                if !repository
                    .attach_cost(cost_id, &write.id, target, &write.period_id)
                    .await?
                {
                    return Err(crate::domain::invoice::cost_frozen().into());
                }
            }
            let job = format!("issue-{}", write.id);
            if !repository
                .enqueue_job(&job, &write.id, "issue", target, now)
                .await?
            {
                return Err(conflict_key().into());
            }
            write
        }
        BillingInvoiceCommand::Void(command) => {
            let record = repository
                .invoice_for_update(command.invoice_id.as_str(), target)
                .await?
                .ok_or_else(crate::domain::invoice::invoice_not_found)?;
            let write = prepare_void(command, &record)?;
            if !repository
                .void_invoice(&record.id, &write, target, now)
                .await?
            {
                return Err(crate::domain::invoice::invoice_not_voidable().into());
            }
            if void_job_required(&record) {
                repository.release_costs(&record.id, target).await?;
                let job = format!("void-{}", record.id);
                if !repository
                    .enqueue_job(&job, &record.id, "void", target, now)
                    .await?
                {
                    return Err(conflict_key().into());
                }
            }
            write
        }
    };

    verify(repository, &invoice, target).await?;
    if invoice.attach_costs {
        let count = repository.attached_cost_count(&invoice.id, target).await?;
        if count != invoice.cost_ids.len() as i64 {
            return Err(invalid_readback().into());
        }
    }
    if invoice.status == "void" {
        let count = repository.attached_cost_count(&invoice.id, target).await?;
        if count != 0 {
            return Err(invalid_readback().into());
        }
    }
    let version = repository
        .increment_account_version(target, command.expected_version())
        .await?
        .ok_or_else(conflict_version)?;
    if version != command.expected_version() + 1 {
        return Err(invalid_readback().into());
    }
    if !repository
        .insert_audit(
            &AuditEntry {
                key: command.key().as_str(),
                actor,
                target,
                action: command.action(),
                reason: crate::domain::command::js_trim(command.reason()),
                details_json: &invoice.details_json,
            },
            now,
        )
        .await?
    {
        return Err(conflict_key().into());
    }
    let result = readback_result();
    let result_json = result.to_string();
    if !repository
        .insert_receipt(command.key().as_str(), &request_hash, &result_json, now)
        .await?
    {
        return Err(conflict_key().into());
    }
    let receipt = repository
        .serialized_receipt(command.key().as_str())
        .await?
        .ok_or_else(invalid_readback)?;
    if receipt.digest != request_hash || receipt.result_json != result_json {
        return Err(invalid_readback().into());
    }
    crate::domain::invoice::validate_receipt_result(&receipt.result_json)?;
    Ok(CommandReceipt {
        digest: receipt.digest,
        result_json: receipt.result_json,
    })
}
