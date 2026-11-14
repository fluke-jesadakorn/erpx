//! Native payroll run command use cases.
//!
//! The owner adapter receives one serializable Holding transaction and fresh
//! Account/IAM authority from composition. This layer owns command ordering:
//! actor policy, exact idempotency replay, current owner sources, optimistic
//! run version, independent approval, calculation/readiness, accrual lines and
//! the Finance journal handoff. It never opens another transaction and never
//! writes Finance rows.
use crate::domain::payroll_run::{
    CALCULATE_OPERATION, CANCEL_OPERATION, CREATE_OPERATION, CalculatePayrollRunInput,
    CancelPayrollRunInput, CountryPackFact, CreatePayrollRunInput, POST_OPERATION, PayrollAccrual,
    PayrollCalculationContext, PayrollCommandActor, PayrollJournalRequest, PayrollPostingBasis,
    PayrollRun, PayrollRunMode, PayrollTotals, PostPayrollInput, ResolvedPayrollPost,
    calculate_payroll_item, calculation_digest_payload, cancellable, command_incomplete,
    idempotency_conflict, journal_draft, journal_result_valid, not_found, payroll_accrual,
    require_calculation_ready, resolve_posting,
};
use crate::domain::payroll_run::{PayrollPreparedJournal, calculated_item_id, stored_receipt};
use erpx_server_shared::public::{BusinessError, CommandKey, CompanyId, TimestampMillis};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type PayrollCommandFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

fn request_invalid() -> BusinessError {
    BusinessError::invalid(
        "PAYROLL_REQUEST_INVALID",
        "Review the payroll request details.",
    )
}

fn pay_group_changed() -> BusinessError {
    BusinessError::new(
        erpx_server_shared::public::FailureKind::Conflict,
        "PAY_GROUP_CHANGED",
        "The pay group is no longer active or no longer matches this payroll.",
    )
}

#[derive(Clone, Debug)]
pub struct StoredPayrollReceipt {
    pub request_hash: String,
    pub result: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct CreateRunCommand<'a> {
    pub id: &'a str,
    pub key: &'a str,
    pub request_hash: &'a str,
    pub input: &'a CreatePayrollRunInput,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct SaveCalculationCommand<'a> {
    pub run_id: &'a str,
    pub key: &'a str,
    pub request_hash: &'a str,
    pub items: &'a [crate::domain::payroll_run::CalculatedPayrollItem],
    pub totals: &'a PayrollTotals,
    pub digest: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct CancelRunCommand<'a> {
    pub run: &'a PayrollRun,
    pub key: &'a str,
    pub request_hash: &'a str,
    pub input: &'a CancelPayrollRunInput,
    pub evidence_id: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct CommitPostCommand<'a> {
    pub run: &'a PayrollRun,
    pub command_key: &'a str,
    pub journal_key: &'a str,
    pub request_hash: &'a str,
    pub resolved: &'a ResolvedPayrollPost,
    pub accrual: &'a PayrollAccrual,
    pub journal: &'a PayrollPreparedJournal,
    pub now: i64,
}

pub trait PayrollCommandRepository: Send {
    type Error: From<BusinessError> + Send;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> PayrollCommandFuture<'a, (), Self::Error>;
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> PayrollCommandFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> PayrollCommandFuture<'a, Option<StoredPayrollReceipt>, Self::Error>;
    fn run<'a>(
        &'a mut self,
        company: &'a CompanyId,
        run_id: &'a str,
    ) -> PayrollCommandFuture<'a, Option<PayrollRun>, Self::Error>;
    fn pay_group_fact<'a>(
        &'a mut self,
        company: &'a CompanyId,
        pay_group_id: &'a str,
    ) -> PayrollCommandFuture<'a, Option<crate::domain::payroll_run::PayGroupFact>, Self::Error>;
    fn duplicate_period<'a>(
        &'a mut self,
        run: &'a CreatePayrollRunInput,
        company: &'a CompanyId,
    ) -> PayrollCommandFuture<'a, bool, Self::Error>;
    fn country_pack<'a>(
        &'a mut self,
        company: &'a CompanyId,
        pack_id: &'a str,
    ) -> PayrollCommandFuture<'a, Option<CountryPackFact>, Self::Error>;
    fn create_run<'a>(
        &'a mut self,
        company: &'a CompanyId,
        command: &'a CreateRunCommand<'a>,
    ) -> PayrollCommandFuture<'a, Value, Self::Error>;
    fn calculation_context<'a>(
        &'a mut self,
        run: &'a PayrollRun,
    ) -> PayrollCommandFuture<'a, PayrollCalculationContext, Self::Error>;
    fn save_calculation<'a>(
        &'a mut self,
        company: &'a CompanyId,
        command: &'a SaveCalculationCommand<'a>,
    ) -> PayrollCommandFuture<'a, Value, Self::Error>;
    fn cancel_run<'a>(
        &'a mut self,
        company: &'a CompanyId,
        command: &'a CancelRunCommand<'a>,
    ) -> PayrollCommandFuture<'a, Value, Self::Error>;
    fn posting_basis<'a>(
        &'a mut self,
        company: &'a CompanyId,
        run_id: &'a str,
        mapping_id: Option<&'a str>,
    ) -> PayrollCommandFuture<'a, Option<PayrollPostingBasis>, Self::Error>;
    fn commit_post<'a>(
        &'a mut self,
        company: &'a CompanyId,
        command: &'a CommitPostCommand<'a>,
    ) -> PayrollCommandFuture<'a, Value, Self::Error>;
    fn detail<'a>(
        &'a mut self,
        company: &'a CompanyId,
        run_id: &'a str,
        payment_only: bool,
    ) -> PayrollCommandFuture<'a, Option<Value>, Self::Error>;

    /// Lends the caller's transaction connection to the composed Finance
    /// journal port. The default refuses so an in-memory or non-SQL adapter can
    /// never post payroll without a real Finance journal.
    fn prepare_journal<'a, J: PayrollJournalPort>(
        &'a mut self,
        _journal: &'a mut J,
        _request: &'a PayrollJournalRequest,
    ) -> PayrollCommandFuture<'a, PayrollPreparedJournal, Self::Error> {
        Box::pin(async move {
            Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Unavailable,
                "PAYROLL_FINANCE_JOURNAL_PORT_UNAVAILABLE",
                "The Finance payroll journal port is unavailable.",
            )
            .into())
        })
    }
}

/// Finance journal planning port. Composition implements this by calling
/// `erpx_server_finance::public::prepare_journal` on the caller's transaction
/// and persisting the prepared journal. A missing port fails closed; People
/// never writes `finance_*` rows itself.
pub trait PayrollJournalPort: Send {
    fn prepare<'a>(
        &'a mut self,
        connection: &'a mut PgConnection,
        request: &'a PayrollJournalRequest,
    ) -> PayrollCommandFuture<'a, PayrollPreparedJournal, BusinessError>;
}

/// Explicit fail-closed default for the composed command entrypoint.
pub struct NoPayrollJournalPort;

impl PayrollJournalPort for NoPayrollJournalPort {
    fn prepare<'a>(
        &'a mut self,
        _connection: &'a mut PgConnection,
        _request: &'a PayrollJournalRequest,
    ) -> PayrollCommandFuture<'a, PayrollPreparedJournal, BusinessError> {
        Box::pin(async { Err(crate::domain::payroll_run::journal_port_unavailable()) })
    }
}

#[derive(Serialize)]
struct CommandIdentity<'a, I: Serialize> {
    user_id: &'a str,
    assignment_id: &'a str,
    role: &'a str,
    company_id: &'a str,
    branch_id: Option<&'a str>,
    department_id: Option<&'a str>,
    work_context: &'a str,
    operation: &'a str,
    record_id: &'a str,
    input: &'a I,
}

async fn prepared<R: PayrollCommandRepository, I: Serialize>(
    actor: &PayrollCommandActor,
    repository: &mut R,
    key: &CommandKey,
    operation: &str,
    record_id: &str,
    input: &I,
) -> Result<(String, Option<Value>), R::Error> {
    repository.lock_command(key.as_str()).await?;
    let identity = serde_json::to_string(&CommandIdentity {
        user_id: &actor.user_id,
        assignment_id: &actor.assignment_id,
        role: &actor.role,
        company_id: actor.company_id.as_str(),
        branch_id: actor.branch_id.as_ref().map(|id| id.as_str()),
        department_id: actor.department_id.as_ref().map(|id| id.as_str()),
        work_context: &actor.work_context,
        operation,
        record_id,
        input,
    })
    .map_err(|_| R::Error::from(request_invalid()))?;
    let request_hash = repository.request_hash(&identity).await?;
    stored_receipt(&request_hash)?;
    if let Some(receipt) = repository.receipt(key.as_str()).await? {
        if receipt.request_hash != request_hash {
            return Err(idempotency_conflict().into());
        }
        return match receipt.result {
            Some(result) => Ok((request_hash, Some(result))),
            None => Err(command_incomplete().into()),
        };
    }
    Ok((request_hash, None))
}

fn validate_run_mode(run: &PayrollRun) -> Result<PayrollRunMode, BusinessError> {
    let mode = run.mode()?;
    if !matches!(
        run.status.as_str(),
        "draft" | "calculating" | "preview" | "failed"
    ) || matches!(mode, PayrollRunMode::FinalPay) && run.adjustment_of_run_id.is_some()
    {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "PAYROLL_LOCKED",
            "Only an unapproved payroll preview can be recalculated.",
        ));
    }
    Ok(mode)
}

pub async fn create_payroll_run<R: PayrollCommandRepository>(
    actor: &PayrollCommandActor,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    actor.require_prepare(now)?;
    let input = CreatePayrollRunInput::parse(raw)?;
    let mode = input.mode()?;
    if mode == PayrollRunMode::FinalPay {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::InvalidInput,
            "FINAL_PAY_CLEARANCE_REQUIRED",
            "Prepare final payroll from the assigned HR clearance task.",
        )
        .into());
    }
    let id = format!("payrun-{}", key.as_str());
    let (request_hash, prior) =
        prepared(actor, repository, key, CREATE_OPERATION, &id, raw).await?;
    if let Some(result) = prior {
        return Ok(result);
    }
    let group = repository
        .pay_group_fact(&actor.company_id, &input.pay_group_id)
        .await?
        .ok_or_else(pay_group_changed)?;
    if group.status != "active" || group.currency != input.currency {
        return Err(pay_group_changed().into());
    }
    if repository
        .duplicate_period(&input, &actor.company_id)
        .await?
    {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "PAYROLL_PERIOD_EXISTS",
            "An active payroll run already covers this pay group and period.",
        )
        .into());
    }
    if mode == PayrollRunMode::Production {
        let pack_id = input.country_pack_id.as_deref().ok_or_else(|| {
            BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "PAYROLL_COUNTRY_PACK_REQUIRED",
                "Select an active Thai rule pack and THB payroll currency.",
            )
        })?;
        if group.country_code != "TH" || group.currency != "THB" || group.frequency != "monthly" {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "PAYROLL_FREQUENCY_ADAPTER_UNSUPPORTED",
                "This Thai adapter aggregates wages in a monthly Section 33 pay group.",
            )
            .into());
        }
        let pack = repository
            .country_pack(&actor.company_id, pack_id)
            .await?
            .ok_or_else(|| {
                BusinessError::new(
                    erpx_server_shared::public::FailureKind::Conflict,
                    "PAYROLL_COUNTRY_PACK_INACTIVE",
                    "The selected payroll rule pack is not active.",
                )
            })?;
        validate_pack(&pack, &input.pay_date)?;
    } else if input
        .country_pack_id
        .as_deref()
        .is_some_and(|pack_id| pack_id.is_empty())
    {
        return Err(request_invalid().into());
    }
    repository
        .create_run(
            &actor.company_id,
            &CreateRunCommand {
                id: &id,
                key: key.as_str(),
                request_hash: &request_hash,
                input: &input,
                now: now.get(),
            },
        )
        .await
}

fn validate_pack(pack: &CountryPackFact, pay_date: &str) -> Result<(), BusinessError> {
    if pack.status != "active" {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "PAYROLL_COUNTRY_PACK_INACTIVE",
            "The selected payroll rule pack is not active.",
        ));
    }
    if pack.adapter_id.as_deref() != Some("thai-payroll-v1")
        || pack
            .source_digest
            .as_deref()
            .is_none_or(|value| value.len() != 64)
        || pack
            .effective_from
            .as_deref()
            .is_some_and(|value| pay_date < value)
        || pack
            .effective_to
            .as_deref()
            .is_some_and(|value| pay_date > value)
    {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "PAYROLL_COUNTRY_PACK_NOT_EFFECTIVE",
            "The adapter and immutable rule evidence must be effective on the pay date.",
        ));
    }
    if pack.record_version < 1 {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "PAYROLL_COUNTRY_PACK_NOT_EFFECTIVE",
            "The adapter and immutable rule evidence must be effective on the pay date.",
        ));
    }
    Ok(())
}

pub async fn calculate_payroll_run<R: PayrollCommandRepository>(
    actor: &PayrollCommandActor,
    run_id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    actor.require_prepare(now)?;
    let input = CalculatePayrollRunInput::parse(raw)?;
    let (request_hash, prior) =
        prepared(actor, repository, key, CALCULATE_OPERATION, run_id, &input).await?;
    if let Some(result) = prior {
        return Ok(result);
    }
    let run = repository
        .run(&actor.company_id, run_id)
        .await?
        .ok_or_else(not_found)?;
    run.validate(&actor.company_id)?;
    let mode = validate_run_mode(&run)?;
    if run.adjustment_of_run_id.is_some() {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Unavailable,
            "PAYROLL_ADJUSTMENT_COMMAND_REQUIRED",
            "Additional-pay adjustments are not part of the native run calculation command.",
        )
        .into());
    }
    if !matches!(
        mode,
        PayrollRunMode::Preview | PayrollRunMode::FinalPay | PayrollRunMode::Production
    ) {
        return Err(request_invalid().into());
    }
    let context = repository.calculation_context(&run).await?;
    require_calculation_ready(&run, &context)?;
    let mut items = Vec::with_capacity(context.inputs.len());
    for input in &context.inputs {
        items.push(calculate_payroll_item(input, &run)?);
    }
    let mut totals = PayrollTotals {
        gross_minor: 0,
        deductions_minor: 0,
        employer_cost_minor: 0,
        net_minor: 0,
        headcount: items.len() as i64,
    };
    for item in &items {
        totals.gross_minor = totals
            .gross_minor
            .checked_add(item.gross_minor)
            .ok_or_else(|| {
                BusinessError::invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision.")
            })?;
        totals.deductions_minor = totals
            .deductions_minor
            .checked_add(item.deductions_minor)
            .ok_or_else(|| {
                BusinessError::invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision.")
            })?;
        totals.employer_cost_minor = totals
            .employer_cost_minor
            .checked_add(item.employer_cost_minor)
            .ok_or_else(|| {
                BusinessError::invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision.")
            })?;
        totals.net_minor = totals
            .net_minor
            .checked_add(item.net_minor)
            .ok_or_else(|| {
                BusinessError::invalid("MONEY_OVERFLOW", "Payroll exceeds supported precision.")
            })?;
    }
    totals.validate()?;
    for (index, item) in items.iter_mut().enumerate() {
        item.id = calculated_item_id(key.as_str(), index);
    }
    let digest = repository
        .request_hash(&calculation_digest_payload(&items))
        .await?;
    repository
        .save_calculation(
            &actor.company_id,
            &SaveCalculationCommand {
                run_id,
                key: key.as_str(),
                request_hash: &request_hash,
                items: &items,
                totals: &totals,
                digest: &digest,
                now: now.get(),
            },
        )
        .await
}

pub async fn cancel_payroll_run<R: PayrollCommandRepository>(
    actor: &PayrollCommandActor,
    run_id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    actor.require_prepare(now)?;
    let input = CancelPayrollRunInput::parse(raw)?;
    let (request_hash, prior) =
        prepared(actor, repository, key, CANCEL_OPERATION, run_id, raw).await?;
    if let Some(result) = prior {
        return Ok(result);
    }
    let run = repository
        .run(&actor.company_id, run_id)
        .await?
        .ok_or_else(not_found)?;
    run.validate(&actor.company_id)?;
    if run.version != input.expected_version {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "PAYROLL_RUN_CHANGED",
            "Reload the payroll run before cancelling it.",
        )
        .into());
    }
    cancellable(&run)?;
    if run.created_by != actor.user_id {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Forbidden,
            "PAYROLL_CANCELLATION_OWNER_REQUIRED",
            "The creator or named payroll preparer must cancel this run.",
        )
        .into());
    }
    let evidence_id = format!("payroll-cancellation-{}", key.as_str());
    repository
        .cancel_run(
            &actor.company_id,
            &CancelRunCommand {
                run: &run,
                key: key.as_str(),
                request_hash: &request_hash,
                input: &input,
                evidence_id: &evidence_id,
                now: now.get(),
            },
        )
        .await
}

pub async fn post_payroll<R: PayrollCommandRepository, J: PayrollJournalPort>(
    actor: &PayrollCommandActor,
    run_id: &str,
    raw: &Value,
    key: &CommandKey,
    repository: &mut R,
    journal: &mut J,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    actor.require_post(now)?;
    let input = PostPayrollInput::parse(raw)?;
    let (request_hash, prior) =
        prepared(actor, repository, key, POST_OPERATION, run_id, raw).await?;
    if let Some(result) = prior {
        return Ok(result);
    }
    let basis = repository
        .posting_basis(
            &actor.company_id,
            run_id,
            input.account_mapping_id.as_deref(),
        )
        .await?
        .ok_or_else(not_found)?;
    basis.run.validate(&actor.company_id)?;
    let resolved = resolve_posting(&basis, &input, now)?;
    let digest = repository
        .request_hash(&crate::domain::payroll_run::posting_digest_payload(
            &basis.items,
        ))
        .await?;
    if basis.run.input_digest.as_deref() != Some(digest.as_str()) {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "PAYROLL_SOURCE_CHANGED",
            "Payroll inputs changed. Recalculate and submit this preview again.",
        )
        .into());
    }
    if basis.run.requires_statutory() {
        require_statutory_items(&basis)?;
    }
    let accrual = payroll_accrual(&basis, &resolved)?;
    if accrual.totals.net_minor != basis.run_totals_net()? {
        return Err(BusinessError::new(
            erpx_server_shared::public::FailureKind::Conflict,
            "PAYROLL_TOTALS_INVALID",
            "Approved run totals changed after review.",
        )
        .into());
    }
    let draft = journal_draft(&basis, &resolved, &accrual)?;
    let finance_actor = actor.finance_actor()?;
    let journal_key = format!("{}:journal", key.as_str());
    let journal_key = CommandKey::new(journal_key).map_err(|_| request_invalid())?;
    let journal_request = PayrollJournalRequest {
        actor: finance_actor,
        draft,
        key: journal_key.as_str().to_owned(),
    };
    let prepared_journal = repository
        .prepare_journal(journal, &journal_request)
        .await?;
    journal_result_valid(&prepared_journal)?;
    repository
        .commit_post(
            &actor.company_id,
            &CommitPostCommand {
                run: &basis.run,
                command_key: key.as_str(),
                journal_key: journal_key.as_str(),
                request_hash: &request_hash,
                resolved: &resolved,
                accrual: &accrual,
                journal: &prepared_journal,
                now: now.get(),
            },
        )
        .await
}

fn require_statutory_items(basis: &PayrollPostingBasis) -> Result<(), BusinessError> {
    for item in &basis.items {
        let value: Value = serde_json::from_str(&item.result_json).map_err(|_| {
            BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "PAYROLL_STATUTORY_ADAPTER_REQUIRED",
                "A calculated, source-dated Thai statutory run is required for this payroll action.",
            )
        })?;
        let statutory = value.get("statutory");
        if statutory
            .and_then(|statutory| statutory.get("adapterId"))
            .and_then(Value::as_str)
            != Some("thai-payroll-v1")
            || statutory
                .and_then(|statutory| statutory.get("sourceDigest"))
                .and_then(Value::as_str)
                .is_none_or(|value| value.len() != 64)
        {
            return Err(BusinessError::new(
                erpx_server_shared::public::FailureKind::Conflict,
                "PAYROLL_STATUTORY_ADAPTER_REQUIRED",
                "A calculated, source-dated Thai statutory run is required for this payroll action.",
            ));
        }
    }
    Ok(())
}

pub async fn payroll_run_detail<R: PayrollCommandRepository>(
    actor: &PayrollCommandActor,
    run_id: &str,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    actor.require_read(now)?;
    let payment_only = !actor.can_read_payroll();
    repository
        .detail(&actor.company_id, run_id, payment_only)
        .await?
        .ok_or_else(|| not_found().into())
}

impl PayrollPostingBasis {
    fn run_totals_net(&self) -> Result<i64, BusinessError> {
        let value: Value = serde_json::from_str(&self.run.totals_json).map_err(|_| {
            BusinessError::invalid(
                "PAYROLL_TOTALS_INVALID",
                "Approved run totals changed after review.",
            )
        })?;
        value.get("net").and_then(Value::as_i64).ok_or_else(|| {
            BusinessError::invalid(
                "PAYROLL_TOTALS_INVALID",
                "Approved run totals changed after review.",
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::payroll_run::{
        PayGroupFact, PayrollCalculationInput, PayrollCompensation, PayrollCostAssignment,
        PayrollEngagement, PayrollPayee, PayrollTimeEntry,
    };
    use erpx_server_shared::public::{FailureKind, HoldingId, RecordId};
    use std::collections::BTreeMap;

    struct Repo {
        receipts: BTreeMap<String, StoredPayrollReceipt>,
        runs: BTreeMap<String, PayrollRun>,
        create_calls: usize,
        saved: Option<String>,
    }

    impl Repo {
        fn with_run(run: PayrollRun) -> Self {
            let mut runs = BTreeMap::new();
            runs.insert(run.id.clone(), run);
            Self {
                receipts: BTreeMap::new(),
                runs,
                create_calls: 0,
                saved: None,
            }
        }
    }

    impl PayrollCommandRepository for Repo {
        type Error = BusinessError;

        fn lock_command<'a>(&'a mut self, _: &'a str) -> PayrollCommandFuture<'a, (), Self::Error> {
            Box::pin(async { Ok(()) })
        }
        fn request_hash<'a>(
            &'a mut self,
            identity: &'a str,
        ) -> PayrollCommandFuture<'a, String, Self::Error> {
            Box::pin(async move {
                let mut hash = 0xcbf29ce484222325_u64;
                for byte in identity.bytes() {
                    hash ^= u64::from(byte);
                    hash = hash.wrapping_mul(0x100000001b3);
                }
                Ok(format!("{hash:064x}"))
            })
        }
        fn receipt<'a>(
            &'a mut self,
            key: &'a str,
        ) -> PayrollCommandFuture<'a, Option<StoredPayrollReceipt>, Self::Error> {
            Box::pin(async move { Ok(self.receipts.get(key).cloned()) })
        }
        fn run<'a>(
            &'a mut self,
            _: &'a CompanyId,
            run_id: &'a str,
        ) -> PayrollCommandFuture<'a, Option<PayrollRun>, Self::Error> {
            Box::pin(async move { Ok(self.runs.get(run_id).cloned()) })
        }
        fn pay_group_fact<'a>(
            &'a mut self,
            _: &'a CompanyId,
            _: &'a str,
        ) -> PayrollCommandFuture<'a, Option<PayGroupFact>, Self::Error> {
            Box::pin(async {
                Ok(Some(PayGroupFact {
                    id: "group".into(),
                    currency: "THB".into(),
                    country_code: "TH".into(),
                    frequency: "monthly".into(),
                    status: "active".into(),
                }))
            })
        }
        fn duplicate_period<'a>(
            &'a mut self,
            _: &'a CreatePayrollRunInput,
            _: &'a CompanyId,
        ) -> PayrollCommandFuture<'a, bool, Self::Error> {
            Box::pin(async { Ok(false) })
        }
        fn country_pack<'a>(
            &'a mut self,
            _: &'a CompanyId,
            _: &'a str,
        ) -> PayrollCommandFuture<'a, Option<CountryPackFact>, Self::Error> {
            Box::pin(async { Ok(None) })
        }
        fn create_run<'a>(
            &'a mut self,
            _: &'a CompanyId,
            command: &'a CreateRunCommand<'a>,
        ) -> PayrollCommandFuture<'a, Value, Self::Error> {
            Box::pin(async move {
                self.create_calls += 1;
                let result = serde_json::json!({
                    "id": command.id,
                    "status": "calculating",
                    "mode": command.input.mode,
                    "version": 1,
                });
                self.receipts.insert(
                    command.key.into(),
                    StoredPayrollReceipt {
                        request_hash: command.request_hash.into(),
                        result: Some(result.clone()),
                    },
                );
                Ok(result)
            })
        }
        fn calculation_context<'a>(
            &'a mut self,
            _: &'a PayrollRun,
        ) -> PayrollCommandFuture<'a, PayrollCalculationContext, Self::Error> {
            Box::pin(async {
                Ok(PayrollCalculationContext {
                    inputs: vec![PayrollCalculationInput {
                        engagement: PayrollEngagement {
                            id: "engagement".into(),
                            version: 1,
                            person_id: "person".into(),
                            worker_type: "employee".into(),
                            employment_type: "salaried".into(),
                            start_date: "2026-01-01".into(),
                            end_date: None,
                            job_title: "Engineer".into(),
                            branch_id: Some("branch".into()),
                            department_id: None,
                        },
                        payee: PayrollPayee {
                            id: "person".into(),
                            version: 1,
                            legal_name: "Person".into(),
                            preferred_name: None,
                        },
                        compensation: Some(PayrollCompensation {
                            id: "compensation".into(),
                            version: 1,
                            base_amount_minor: 2_000_000,
                            hourly_rate_minor: 20_000,
                            daily_rate_minor: 0,
                            standard_daily_hours: 8.0,
                            currency: "THB".into(),
                            pay_frequency: "monthly".into(),
                            effective_from: "2026-01-01".into(),
                            effective_to: None,
                            status: "approved".into(),
                            components_json: None,
                        }),
                        time_entries: vec![PayrollTimeEntry {
                            id: "time".into(),
                            version: 1,
                            work_date: "2026-10-01".into(),
                            regular_hours: 160.0,
                            overtime_hours: 0.0,
                            cost_assignment_id: Some("assignment".into()),
                            cost_branch_id: Some("branch".into()),
                            cost_department_id: None,
                        }],
                        handoffs: Vec::new(),
                        advances: Vec::new(),
                        cost_assignments: vec![PayrollCostAssignment {
                            id: "assignment".into(),
                            version: 1,
                            effective_from: "2026-01-01".into(),
                            effective_to: None,
                            branch_id: Some("branch".into()),
                            department_id: None,
                            evidence_status: "verified".into(),
                        }],
                        benefits: Vec::new(),
                        thai_statutory: None,
                    }],
                })
            })
        }
        fn save_calculation<'a>(
            &'a mut self,
            _: &'a CompanyId,
            command: &'a SaveCalculationCommand<'a>,
        ) -> PayrollCommandFuture<'a, Value, Self::Error> {
            Box::pin(async move {
                self.saved = Some(command.digest.into());
                let result = serde_json::json!({
                    "id": command.run_id,
                    "status": "preview",
                    "version": 2,
                    "totals": {
                        "gross": command.totals.gross_minor,
                        "deductions": command.totals.deductions_minor,
                        "employer": command.totals.employer_cost_minor,
                        "net": command.totals.net_minor,
                        "headcount": command.totals.headcount,
                    }
                });
                self.receipts.insert(
                    command.key.into(),
                    StoredPayrollReceipt {
                        request_hash: command.request_hash.into(),
                        result: Some(result.clone()),
                    },
                );
                Ok(result)
            })
        }
        fn cancel_run<'a>(
            &'a mut self,
            _: &'a CompanyId,
            command: &'a CancelRunCommand<'a>,
        ) -> PayrollCommandFuture<'a, Value, Self::Error> {
            Box::pin(async move {
                let result = serde_json::json!({
                    "id": command.run.id,
                    "status": "cancelled",
                    "version": command.input.expected_version + 1,
                    "cancellationId": command.evidence_id,
                });
                self.receipts.insert(
                    command.key.into(),
                    StoredPayrollReceipt {
                        request_hash: command.request_hash.into(),
                        result: Some(result.clone()),
                    },
                );
                Ok(result)
            })
        }
        fn posting_basis<'a>(
            &'a mut self,
            _: &'a CompanyId,
            run_id: &'a str,
            _: Option<&'a str>,
        ) -> PayrollCommandFuture<'a, Option<PayrollPostingBasis>, Self::Error> {
            Box::pin(async move {
                Ok(self
                    .runs
                    .get(run_id)
                    .cloned()
                    .map(|run| PayrollPostingBasis {
                        run,
                        items: Vec::new(),
                        mapping: None,
                    }))
            })
        }
        fn commit_post<'a>(
            &'a mut self,
            _: &'a CompanyId,
            command: &'a CommitPostCommand<'a>,
        ) -> PayrollCommandFuture<'a, Value, Self::Error> {
            Box::pin(async move {
                Ok(serde_json::json!({
                    "id": command.run.id,
                    "status": "posted",
                    "version": command.run.version + 1,
                    "journalId": command.journal.id,
                    "netPayableMinor": command.accrual.totals.net_minor,
                    "payslipIds": [],
                }))
            })
        }
        fn detail<'a>(
            &'a mut self,
            _: &'a CompanyId,
            run_id: &'a str,
            _: bool,
        ) -> PayrollCommandFuture<'a, Option<Value>, Self::Error> {
            Box::pin(async move {
                Ok(self
                    .runs
                    .get(run_id)
                    .map(|run| serde_json::json!({"run": {"id": run.id}, "items": []})))
            })
        }
    }

    fn actor() -> PayrollCommandActor {
        PayrollCommandActor {
            user_id: "owner".into(),
            holding_id: HoldingId::new("holding").unwrap(),
            company_id: CompanyId::new("company").unwrap(),
            branch_id: None,
            department_id: None,
            assignment_id: "assignment".into(),
            role: "payroll".into(),
            permissions: vec![
                "payroll.prepare".into(),
                "payroll.read".into(),
                "finance.approve".into(),
            ],
            work_context: "a".repeat(64),
            expires_at: 1_800_000_100_000,
        }
    }

    fn key(value: &str) -> CommandKey {
        CommandKey::new(value).unwrap()
    }

    fn run() -> PayrollRun {
        PayrollRun {
            id: "run".into(),
            company_id: "company".into(),
            pay_group_id: "group".into(),
            pay_group_currency: "THB".into(),
            pay_group_status: "active".into(),
            overtime_multiplier_bps: 15_000,
            proration_method: "none".into(),
            country_pack_id: None,
            period_start: "2026-10-01".into(),
            period_end: "2026-10-31".into(),
            pay_date: "2026-10-31".into(),
            currency: "THB".into(),
            mode: "preview".into(),
            status: "draft".into(),
            revision: 1,
            totals_json: "{}".into(),
            input_digest: None,
            version: 1,
            created_by: "owner".into(),
            submitted_by: None,
            approved_by: None,
            posted_by: None,
            adjustment_of_run_id: None,
            scope_engagement_id: None,
        }
    }

    const NOW: TimestampMillis = TimestampMillis::new(1_800_000_000_000);

    #[tokio::test]
    async fn create_is_idempotent_and_replays_the_owner_result() {
        let mut repo = Repo::with_run(run());
        let raw = serde_json::json!({
            "payGroupId": "group",
            "periodStart": "2026-10-01",
            "periodEnd": "2026-10-31",
            "payDate": "2026-10-31",
            "currency": "THB",
            "mode": "preview"
        });
        let first = create_payroll_run(&actor(), &raw, &key("create-key-1"), &mut repo, NOW)
            .await
            .unwrap();
        let second = create_payroll_run(&actor(), &raw, &key("create-key-1"), &mut repo, NOW)
            .await
            .unwrap();
        assert_eq!(first, second);
        assert_eq!(repo.create_calls, 1);
    }

    #[tokio::test]
    async fn calculate_requires_current_company_scope_and_writes_preview_totals() {
        let mut repository = Repo::with_run(run());
        let result = calculate_payroll_run(
            &actor(),
            "run",
            &serde_json::json!({}),
            &key("calculate-key-1"),
            &mut repository,
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(result["status"], "preview");
        assert_eq!(result["totals"]["headcount"], 1);
        assert!(repository.saved.is_some());
        let mut scoped = actor();
        scoped.branch_id = Some(RecordId::new("branch").unwrap());
        let error = calculate_payroll_run(
            &scoped,
            "run",
            &serde_json::json!({}),
            &key("calculate-key-2"),
            &mut repository,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "COMPANY_PAYROLL_SCOPE_REQUIRED");
    }

    #[tokio::test]
    async fn cancellation_requires_the_creator_and_current_version() {
        let mut repository = Repo::with_run(run());
        let raw = serde_json::json!({"expectedVersion": 1, "reason": "Wrong pay period"});
        let mut other = actor();
        other.user_id = "other".into();
        let error = cancel_payroll_run(
            &other,
            "run",
            &raw,
            &key("cancel-key-1"),
            &mut repository,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "PAYROLL_CANCELLATION_OWNER_REQUIRED");
        let result = cancel_payroll_run(
            &actor(),
            "run",
            &raw,
            &key("cancel-key-2"),
            &mut repository,
            NOW,
        )
        .await
        .unwrap();
        assert_eq!(result["status"], "cancelled");
    }

    #[tokio::test]
    async fn no_journal_port_fails_closed_with_a_named_code() {
        let mut port = NoPayrollJournalPort;
        let mut repository = Repo::with_run(run());
        let error = repository
            .prepare_journal(&mut port, &PayrollJournalRequest {
                actor: actor().finance_actor().unwrap(),
                draft: crate::domain::payroll_run::PayrollJournalDraft {
                    chart_id: "chart".into(),
                    period_id: "period".into(),
                    entry_date: "2026-10-31".into(),
                    currency: "THB".into(),
                    fx_rate: None,
                    is_opening: false,
                    memo: "Payroll".into(),
                    source_type: "payroll".into(),
                    source_id: Some("run".into()),
                    lines: Vec::new(),
                },
                key: "post-key-1:journal".into(),
            })
            .await
            .unwrap_err();
        assert_eq!(error.code, "PAYROLL_FINANCE_JOURNAL_PORT_UNAVAILABLE");
        assert_eq!(error.kind, FailureKind::Unavailable);
    }

    #[tokio::test]
    async fn post_requires_an_approved_mapping_before_reaching_finance() {
        let mut run = run();
        run.status = "approved".into();
        run.approved_by = Some("reviewer".into());
        run.version = 2;
        run.totals_json = r#"{"net":2000000}"#.into();
        run.input_digest = Some("a".repeat(64));
        let mut repository = Repo::with_run(run);
        let raw = serde_json::json!({"expectedVersion": 2});
        let error = post_payroll(
            &actor(),
            "run",
            &raw,
            &key("post-key-1"),
            &mut repository,
            &mut NoPayrollJournalPort,
            NOW,
        )
        .await
        .unwrap_err();
        assert_eq!(error.code, "PAYROLL_ACCOUNT_MAPPING_REQUIRED");
    }
}
