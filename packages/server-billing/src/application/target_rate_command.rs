//! Native Billing `rate-target` command use case.
//!
//! The command reads the exact target period on the caller's serializable
//! Control transaction, appends a rating run, replaces the current rated period
//! only while it is not approved, then writes audit/receipt and verifies
//! readback. It never approves, invoices or calls Stripe.
use super::BillingFuture;
use super::cost::{AccountSnapshot, AuditEntry, ReceiptSnapshot};
use super::target::read_target_billing_period;
use crate::application::target::TargetBillingRepository;
use crate::domain::actor::BillingActor;
use crate::domain::command::{
    CommandReceipt, account_required, holding_not_found, idempotency_conflict, js_trim,
    parse_receipt_result, readback_invalid, receipt_replay, source_invalid, valid_hash,
    version_conflict,
};
use crate::domain::target::TargetBillingPeriodView;
use crate::domain::target_pricing::month_bounds;
use crate::domain::target_rate_command::{
    RateTarget, ValidatedRating, assignment_required, period_frozen,
};
use erpx_server_shared::public::{BusinessError, HoldingId, TimestampMillis};

#[derive(Clone, Debug)]
pub struct RateTargetDraft {
    pub id: String,
    pub holding_id: String,
    pub month: String,
    pub price_id: String,
    pub assignment_month: String,
    pub mode: String,
    pub snapshot_id: String,
    pub service_start: i64,
    pub service_end: i64,
    pub state: String,
    pub missing_json: String,
    pub lines_json: String,
    pub total_satang: Option<i64>,
}

pub trait RateTargetRepository: TargetBillingRepository {
    fn record_rating_run<'a>(
        &'a mut self,
        draft: &'a RateTargetDraft,
        actor: &'a BillingActor,
        reason: &'a str,
        now: TimestampMillis,
    ) -> BillingFuture<'a, (), Self::Error>;

    fn retain_rated_period<'a>(
        &'a mut self,
        draft: &'a RateTargetDraft,
        now: TimestampMillis,
    ) -> BillingFuture<'a, bool, Self::Error>;

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

fn draft(
    derived_id: &str,
    target: &TargetBillingPeriodView,
) -> Result<(RateTargetDraft, ValidatedRating), BusinessError> {
    let assignment = target.assignment.as_ref().ok_or_else(assignment_required)?;
    let price = target.price.as_ref().ok_or_else(assignment_required)?;
    let bounds = month_bounds(&target.month)?;
    let state = if target.completeness.ready {
        "ready"
    } else {
        "held"
    };
    let missing_json =
        serde_json::to_string(&target.completeness.missing).map_err(|_| source_invalid())?;
    let lines_json = serde_json::to_string(&target.lines).map_err(|_| source_invalid())?;
    Ok((
        RateTargetDraft {
            id: derived_id.to_owned(),
            holding_id: target.holding_id.clone(),
            month: target.month.clone(),
            price_id: price.id.clone(),
            assignment_month: assignment.effective_month.clone(),
            mode: assignment.mode.clone(),
            snapshot_id: target
                .completeness
                .meter_snapshot_id
                .clone()
                .unwrap_or_else(|| "missing".into()),
            service_start: bounds.0.max(assignment.service_started_at),
            service_end: bounds.1,
            state: state.into(),
            missing_json,
            lines_json,
            total_satang: target.total_satang,
        },
        ValidatedRating {
            target_month: target.month.clone(),
        },
    ))
}

pub async fn execute_rate_target<R: RateTargetRepository>(
    actor: &BillingActor,
    target: &HoldingId,
    command: &RateTarget,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<CommandReceipt, R::Error> {
    actor.require_target_command(target, now)?;
    let _validated = command.validate()?;
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
    let period = read_target_billing_period(repository, target, &command.target_month, now).await?;
    if period.state == "approved" {
        return Err(period_frozen().into());
    }
    let derived_id = format!("rating-{}", command.key.as_str());
    let (draft, _) = draft(&derived_id, &period)?;
    repository
        .record_rating_run(&draft, actor, js_trim(&command.reason), now)
        .await?;
    if !repository.retain_rated_period(&draft, now).await? {
        return Err(period_frozen().into());
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
                action: "rate-target",
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
