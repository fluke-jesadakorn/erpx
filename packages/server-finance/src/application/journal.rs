use std::future::Future;

use erpx_server_shared::public::{
    AssignmentId, CommandKey, CompanyId, FailureKind, HoldingId, TimestampMillis, UserId,
};
use serde::Serialize;

use crate::domain::journal::{
    JournalAmounts, JournalError, JournalLineInput, functional_fx_micros, prepare_journal_amounts,
};
use crate::domain::money::units_to_decimal;

/// Adapter-owned context, never an inbound request DTO or an authorization grant.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FinanceActor {
    pub user_id: UserId,
    pub holding_id: HoldingId,
    pub company_id: CompanyId,
    pub assignment_id: AssignmentId,
}

#[derive(Clone, Debug)]
pub struct JournalDraft {
    pub chart_id: String,
    pub period_id: String,
    pub entry_date: String,
    pub currency: String,
    pub fx_rate: Option<String>,
    pub is_opening: bool,
    pub memo: String,
    pub source_type: String,
    pub source_id: Option<String>,
    pub lines: Vec<JournalLineInput>,
}

#[derive(Clone, Debug, Serialize)]
pub struct JournalChart {
    pub id: String,
    pub base_currency: String,
}

pub use crate::domain::company_money::FunctionalFxEvidence;

pub trait JournalAuthority {
    type Error: From<JournalError> + Send;
    fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

#[derive(Clone, Copy, Debug, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum JournalStatus {
    Draft,
    Posted,
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PreparedJournalResult {
    pub id: String,
    pub number: String,
    pub status: JournalStatus,
    pub version: u32,
    pub debit: String,
    pub credit: String,
    pub fx_rate_micros: i64,
    pub base_currency: String,
}

/// Opaque adapter proof. It cannot be constructed from an inbound DTO.
#[derive(Clone, Eq, PartialEq)]
pub struct JournalTransactionWitness(pub(crate) String);

pub struct JournalPreparation {
    pub(crate) chart: JournalChart,
    pub(crate) fx: i64,
    pub(crate) amounts: JournalAmounts,
    pub(crate) journal_id: String,
    pub(crate) number: String,
    pub(crate) now: TimestampMillis,
    pub(crate) prepared_fx_evidence: Option<FunctionalFxEvidence>,
    pub(crate) result: PreparedJournalResult,
    pub(crate) actor: FinanceActor,
    pub(crate) draft: JournalDraft,
    pub(crate) transaction_witness: Option<JournalTransactionWitness>,
}

impl JournalPreparation {
    pub fn chart(&self) -> &JournalChart {
        &self.chart
    }
    pub fn fx(&self) -> i64 {
        self.fx
    }
    pub fn amounts(&self) -> &JournalAmounts {
        &self.amounts
    }
    pub fn journal_id(&self) -> &str {
        &self.journal_id
    }
    pub fn number(&self) -> &str {
        &self.number
    }
    pub fn now(&self) -> TimestampMillis {
        self.now
    }
    pub fn prepared_fx_evidence(&self) -> Option<&FunctionalFxEvidence> {
        self.prepared_fx_evidence.as_ref()
    }
    pub fn result(&self) -> &PreparedJournalResult {
        &self.result
    }
    pub fn actor(&self) -> &FinanceActor {
        &self.actor
    }
    pub fn draft(&self) -> &JournalDraft {
        &self.draft
    }
}

/// All facts must use the calling owner's existing transaction and Company scope.
/// Authorization revalidates its source witness; a FinanceActor alone grants no access.
/// Preparation does not commit: the owner adapter atomically persists its journal,
/// source changes, idempotency receipt, audit and outbox through that same transaction.
pub trait JournalPreparationPorts {
    /// Preserve adapter persistence failures so the outer unit of work can retry them.
    type Error: From<JournalError> + Send;
    fn transaction_witness(
        &mut self,
    ) -> impl Future<Output = Result<Option<JournalTransactionWitness>, Self::Error>> + Send;
    fn authorize(
        &mut self,
        actor: &FinanceActor,
        draft: &JournalDraft,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn chart(
        &mut self,
        company: &CompanyId,
        chart: &str,
    ) -> impl Future<Output = Result<Option<JournalChart>, Self::Error>> + Send;
    fn ensure_period(
        &mut self,
        actor: &FinanceActor,
        period: &str,
        date: &str,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    fn money_scale(
        &mut self,
        company: &CompanyId,
        currency: &str,
    ) -> impl Future<Output = Result<u8, Self::Error>> + Send;
    fn functional_fx(
        &mut self,
        actor: &FinanceActor,
        currency: &str,
        base: &str,
        date: &str,
    ) -> impl Future<Output = Result<FunctionalFxEvidence, Self::Error>> + Send;
    fn accounts(
        &mut self,
        company: &CompanyId,
        chart: &str,
        ids: &[String],
    ) -> impl Future<Output = Result<Vec<String>, Self::Error>> + Send;
    fn opening_exists(
        &mut self,
        company: &CompanyId,
        chart: &str,
    ) -> impl Future<Output = Result<bool, Self::Error>> + Send;
    fn now(&mut self) -> TimestampMillis;
}

/// Internal journal identity only: it is never a Workflow command key/receipt.
/// The owned parts caller must independently retain its exact source binding.
pub(crate) struct JournalIdentitySeed(String);
impl JournalIdentitySeed {
    pub(crate) fn owner_parts(key: &CommandKey) -> Self {
        // A valid original owner key is at most180 bytes. The exact suffix makes
        // a186-byte seed, and its194-byte journal ID remains a valid RecordId.
        Self(format!("{}:parts", key.as_str()))
    }
    pub(crate) fn journal_id(&self) -> String {
        format!("journal-{}", self.0)
    }
    pub(crate) fn journal_number(&self) -> String {
        format!("J-{}", self.0)
    }
}

pub async fn prepare_journal<P: JournalPreparationPorts>(
    ports: &mut P,
    actor: &FinanceActor,
    input: &JournalDraft,
    key: &CommandKey,
    status: JournalStatus,
) -> Result<JournalPreparation, P::Error> {
    prepare_journal_with_seed(
        ports,
        actor,
        input,
        &JournalIdentitySeed(key.as_str().into()),
        status,
    )
    .await
}

pub(crate) async fn prepare_journal_with_seed<P: JournalPreparationPorts>(
    ports: &mut P,
    actor: &FinanceActor,
    input: &JournalDraft,
    seed: &JournalIdentitySeed,
    status: JournalStatus,
) -> Result<JournalPreparation, P::Error> {
    ports.authorize(actor, input).await?;
    let chart = ports
        .chart(&actor.company_id, &input.chart_id)
        .await?
        .ok_or_else(|| JournalError {
            kind: FailureKind::NotFound,
            code: "CHART_NOT_FOUND",
            message: "Active chart not found.".to_owned(),
        })?;
    ports
        .ensure_period(actor, &input.period_id, &input.entry_date)
        .await?;
    for currency in [&input.currency, &chart.base_currency] {
        if ports.money_scale(&actor.company_id, currency).await? != 2 {
            return Err(JournalError::new(
                "FINANCE_CURRENCY_SCALE_UNSUPPORTED",
                "This Finance ledger currently accepts currencies with two minor-unit decimals. Historical amounts retain their recorded scale; use a supported currency for a new document.",
            ).into());
        }
    }
    let explicit = input.fx_rate.as_deref().filter(|value| !value.is_empty());
    let prepared_fx_evidence = if explicit.is_none() {
        Some(
            ports
                .functional_fx(
                    actor,
                    &input.currency,
                    &chart.base_currency,
                    &input.entry_date,
                )
                .await?,
        )
    } else {
        None
    };
    let fx = if explicit.is_some() {
        functional_fx_micros(&input.currency, &chart.base_currency, explicit)?
    } else {
        prepared_fx_evidence
            .as_ref()
            .ok_or_else(|| {
                JournalError::new(
                    "FX_RATE_REQUIRED",
                    "Functional exchange evidence is required.",
                )
            })?
            .micros
    };
    if !(1..=1_000_000_000_000).contains(&fx) {
        return Err(JournalError::new(
            "FX_RATE_REQUIRED",
            format!(
                "Provide a positive {} per {} exchange rate.",
                chart.base_currency, input.currency
            ),
        )
        .into());
    }
    let amounts = prepare_journal_amounts(&input.lines, fx)?;
    let valid = ports
        .accounts(&actor.company_id, &input.chart_id, &amounts.account_ids)
        .await?;
    if valid.len() != amounts.account_ids.len()
        || amounts.account_ids.iter().any(|id| !valid.contains(id))
    {
        return Err(JournalError::new(
            "ACCOUNT_INVALID",
            "Every posting account must be active and belong to this chart.",
        )
        .into());
    }
    let journal_id = seed.journal_id();
    let number = seed.journal_number();
    let now = ports.now();
    if input.is_opening
        && ports
            .opening_exists(&actor.company_id, &input.chart_id)
            .await?
    {
        return Err(JournalError {
            kind: FailureKind::Conflict,
            code: "OPENING_BALANCE_EXISTS",
            message: "Reverse or finish the existing opening balance before importing another one."
                .to_owned(),
        }
        .into());
    }
    let total = units_to_decimal(&amounts.total.into(), 2)
        .map_err(|error| JournalError::new("INVALID_MONEY", error.0))?;
    let result = PreparedJournalResult {
        id: journal_id.clone(),
        number: number.clone(),
        status,
        version: 1,
        debit: total.clone(),
        credit: total,
        fx_rate_micros: fx,
        base_currency: chart.base_currency.clone(),
    };
    let transaction_witness = ports.transaction_witness().await?;
    Ok(JournalPreparation {
        chart,
        fx,
        amounts,
        journal_id,
        number,
        now,
        prepared_fx_evidence,
        result,
        actor: actor.clone(),
        draft: input.clone(),
        transaction_witness,
    })
}
