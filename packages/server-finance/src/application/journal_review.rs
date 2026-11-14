//! Short, caller-owned Finance review unit. No port may independently commit,
//! retry, obtain another connection, call a provider, or treat actor data as a grant.
use std::future::Future;

use erpx_server_shared::public::{CommandKey, FailureKind, RecordId, TimestampMillis};
use num_bigint::BigInt;
use serde::Serialize;

use super::journal::FinanceActor;
use crate::domain::{
    journal::JournalError,
    journal_reversal_money::{
        JournalMoneyAncestor, JournalReversalMoneyPlan, RetainedJournalMoney,
        plan_journal_reversal_money,
    },
    journal_review::{
        JournalLedgerLine, JournalReversalFacts, JournalReviewerFacts, ReviewJournal,
        inverse_journal_lines, journal_ledger_balanced, require_post_reviewer,
        require_reversal_reviewer, review_error, review_journal_reversal,
    },
};

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(untagged)]
pub enum JournalReviewInput {
    Post {
        #[serde(rename = "expectedVersion")]
        #[cfg_attr(feature = "contracts", ts(type = "number"))]
        #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
        expected_version: i64,
    },
    Reverse {
        #[serde(rename = "periodId")]
        period_id: String,
        #[serde(rename = "entryDate")]
        entry_date: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        memo: Option<String>,
    },
}
#[derive(Clone, Debug)]
pub struct JournalReviewCommand {
    pub key: CommandKey,
    pub journal_id: RecordId,
    pub input: JournalReviewInput,
}
impl JournalReviewCommand {
    pub fn operation(&self) -> &'static str {
        match self.input {
            JournalReviewInput::Post { .. } => "finance.journal.post",
            JournalReviewInput::Reverse { .. } => "finance.journal.reverse",
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(untagged)]
pub enum JournalReviewResult {
    Posted {
        id: String,
        status: String,
        #[cfg_attr(feature = "contracts", ts(type = "number"))]
        #[cfg_attr(feature = "contracts", schema(value_type = i64, minimum = 1, maximum = 9_007_199_254_740_991_i64))]
        version: i64,
    },
    Reversed {
        #[serde(rename = "reversalJournalId")]
        reversal_journal_id: String,
        #[serde(rename = "reversedJournalId")]
        reversed_journal_id: String,
    },
}
#[derive(Clone, Debug)]
pub struct JournalPostingFacts {
    /// Real open same-Company period covering the retained entry date, with no
    /// overlapping closed/closing Company period (the existing DB constraint).
    pub period_accepts_date: bool,
    /// Actual line→active same-Company/chart account relation and account currency
    /// equal to the transaction or chart functional currency, never request labels.
    pub active_accounts: bool,
    pub opening_subledgers_ready: bool,
    pub lines: Vec<JournalLedgerLine>,
}
#[derive(Clone, Debug)]
pub struct JournalReversalSourceFacts {
    pub decision: JournalReversalFacts,
    pub period_accepts_date: bool,
    pub lines: Vec<JournalLedgerLine>,
    pub ancestry: Vec<JournalMoneyAncestor>,
    pub money: Vec<RetainedJournalMoney>,
}

/// Only this application constructs a plan. It is neither a permission nor a
/// reusable transaction witness; the original opaque source remains with ports.
pub struct JournalReviewPlan {
    journal: ReviewJournal,
    now: TimestampMillis,
    opening: bool,
    action: JournalReviewAction,
    result: JournalReviewResult,
}
pub enum JournalReviewAction {
    Post {
        expected_version: i64,
        debit_total_units: String,
        lines: Vec<JournalLedgerLine>,
    },
    Reverse {
        period_id: String,
        entry_date: String,
        memo: String,
        reversal_id: String,
        number: String,
        lines: Vec<JournalLedgerLine>,
        money: Box<JournalReversalMoneyPlan>,
    },
}
impl JournalReviewPlan {
    pub fn journal(&self) -> &ReviewJournal {
        &self.journal
    }
    pub fn now(&self) -> TimestampMillis {
        self.now
    }
    pub fn opening(&self) -> bool {
        self.opening
    }
    pub fn action(&self) -> &JournalReviewAction {
        &self.action
    }
    pub fn result(&self) -> &JournalReviewResult {
        &self.result
    }
}

/// All methods are mandatory. No permissive adapter, serialized authority,
/// guessed source capability, or bypass for disabled purchase modes is supplied.
pub trait JournalReviewPorts {
    type Error: From<JournalError> + Send;
    type Authority: Send + Sync;
    type Source: Send + Sync;
    type MoneyWitness: Send + Sync;
    /// Utility fences before the first Holding snapshot, including absent source
    /// relations, periods, invoices/allocations, immutable Money and book mappings.
    /// Assert the caller's exact Holding/Company SERIALIZABLE transaction here.
    fn lock_sources(
        &mut self,
        actor: &FinanceActor,
        command: &JournalReviewCommand,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Actual retained Control lease and current Identity/People/Org authority;
    /// require finance.write. `FinanceActor` alone proves nothing.
    fn authorize_current(
        &mut self,
        actor: &FinanceActor,
        command: &JournalReviewCommand,
    ) -> impl Future<Output = Result<Self::Authority, Self::Error>> + Send;
    fn reviewer(
        &mut self,
        authority: &Self::Authority,
        source: Option<&Self::Source>,
    ) -> impl Future<Output = Result<JournalReviewerFacts, Self::Error>> + Send;
    fn ready(
        &mut self,
        authority: &Self::Authority,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Original Workflow hash/terminology/current-authority checks FIRST. Only an
    /// exact receipt may select History; an unrelated key must conflict. Return
    /// no effects. Preserve input property order and omitted-vs-null semantics.
    fn replay_exact(
        &mut self,
        authority: &Self::Authority,
        command: &JournalReviewCommand,
    ) -> impl Future<Output = Result<Option<JournalReviewResult>, Self::Error>> + Send;
    fn load_source(
        &mut self,
        authority: &Self::Authority,
        command: &JournalReviewCommand,
        posted_only: bool,
    ) -> impl Future<Output = Result<Option<Self::Source>, Self::Error>> + Send;
    fn journal(source: &Self::Source) -> &ReviewJournal;
    fn posting_facts(
        &mut self,
        authority: &Self::Authority,
        source: &Self::Source,
    ) -> impl Future<Output = Result<JournalPostingFacts, Self::Error>> + Send;
    fn reversal_facts(
        &mut self,
        authority: &Self::Authority,
        source: &Self::Source,
        command: &JournalReviewCommand,
    ) -> impl Future<Output = Result<JournalReversalSourceFacts, Self::Error>> + Send;
    fn now(&self) -> TimestampMillis;
    /// Inspect real owner-source links (including actual finance_book_postings
    /// primary identity), scheduled release and explicit Commercial mode. A new
    /// post/reversal uses SettleExisting; labels/source_type cannot grant it.
    fn require_source_purchase(
        &mut self,
        authority: &Self::Authority,
        source: &Self::Source,
        command: &JournalReviewCommand,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Post uses legacyMoneySnapshot semantics; reversal follows the exact plan.
    /// CaptureNew resolves current scale/FX for only the new reversal business
    /// date; retain existing recorded rates/units and their positive-origin guards.
    fn prepare_money(
        &mut self,
        authority: &Self::Authority,
        source: &Self::Source,
        plan: &JournalReviewPlan,
    ) -> impl Future<Output = Result<Self::MoneyWitness, Self::Error>> + Send;
    /// Same borrowed transaction: real Workflow receipt BEFORE owner writes,
    /// exact version/status/period/account/ledger/opening assertions, source
    /// mutation+Money, then original audit/outbox. Never independently commit.
    fn apply_command(
        &mut self,
        authority: &Self::Authority,
        source: &Self::Source,
        command: &JournalReviewCommand,
        plan: &JournalReviewPlan,
        money: &Self::MoneyWitness,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Preserve the three-step B0 order: finance_book_projection IMMEDIATE,
    /// projection DEFERRED, then ALL IMMEDIATE. Materialize complete secondary
    /// journals before financial-event/final-line constraints inspect them.
    fn flush_secondary_books(
        &mut self,
        authority: &Self::Authority,
        source: &Self::Source,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Re-read exact current source/lineage and allowed post-transition images,
    /// immutable original Money, newly persisted Money, opening subledgers,
    /// secondary books/financial events and exact Workflow receipt/audit/outbox.
    /// No owner writes may follow this successful final retention.
    fn retain_source(
        &mut self,
        authority: &Self::Authority,
        source: &Self::Source,
        command: &JournalReviewCommand,
        plan: &JournalReviewPlan,
        money: &Self::MoneyWitness,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
    /// Fresh current personal/selection/role/HR/Company/release/source-purchase
    /// policy and deadlines; do not merely compare cached actor JSON.
    fn require_final_authority(
        &mut self,
        authority: &Self::Authority,
        command: &JournalReviewCommand,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

pub async fn review_journal<P: JournalReviewPorts>(
    ports: &mut P,
    actor: &FinanceActor,
    command: &JournalReviewCommand,
) -> Result<JournalReviewResult, P::Error> {
    ports.lock_sources(actor, command).await?;
    let authority = ports.authorize_current(actor, command).await?;
    let reversal = matches!(command.input, JournalReviewInput::Reverse { .. });
    if reversal {
        require_reversal_reviewer(ports.reviewer(&authority, None).await?)?;
    }
    ports.ready(&authority).await?;
    if let Some(result) = ports.replay_exact(&authority, command).await? {
        ports.require_final_authority(&authority, command).await?;
        return Ok(result);
    }
    let source = ports
        .load_source(&authority, command, reversal)
        .await?
        .ok_or_else(|| {
            if reversal {
                review_error(
                    FailureKind::NotFound,
                    "POSTED_JOURNAL_NOT_FOUND",
                    "Posted journal not found.",
                )
            } else {
                review_error(
                    FailureKind::NotFound,
                    "JOURNAL_NOT_FOUND",
                    "Journal not found.",
                )
            }
        })?;
    let journal = P::journal(&source).clone();
    if journal.id != command.journal_id.as_str()
        || journal.company_id != actor.company_id.as_str()
        || journal.version < 1
        || journal.version >= 9_007_199_254_740_991
    {
        return Err(source_changed().into());
    }
    if reversal && journal.status != "posted" {
        return Err(review_error(
            FailureKind::NotFound,
            "POSTED_JOURNAL_NOT_FOUND",
            "Posted journal not found.",
        )
        .into());
    }
    let (opening, action, result) = match &command.input {
        JournalReviewInput::Post { expected_version } => {
            require_post_reviewer(ports.reviewer(&authority, Some(&source)).await?)?;
            let facts = ports.posting_facts(&authority, &source).await?;
            if *expected_version != journal.version
                || !matches!(journal.status.as_str(), "draft" | "in_review")
                || !facts.period_accepts_date
                || !facts.active_accounts
                || !journal_ledger_balanced(&facts.lines)
                || facts.lines.iter().any(|l| l.journal_id != journal.id)
                || (journal.source_type == "opening_import" && !facts.opening_subledgers_ready)
            {
                return Err(source_changed().into());
            }
            let total: BigInt = facts
                .lines
                .iter()
                .map(|l| BigInt::from(l.debit_minor))
                .sum();
            let result = JournalReviewResult::Posted {
                id: journal.id.clone(),
                status: "posted".into(),
                version: expected_version + 1,
            };
            (
                journal.source_type == "opening_import",
                JournalReviewAction::Post {
                    expected_version: *expected_version,
                    debit_total_units: total.to_string(),
                    lines: facts.lines,
                },
                result,
            )
        }
        JournalReviewInput::Reverse {
            period_id,
            entry_date,
            memo,
        } => {
            let facts = ports.reversal_facts(&authority, &source, command).await?;
            let opening = review_journal_reversal(&journal, entry_date, &facts.decision)?;
            if !facts.period_accepts_date
                || !journal_ledger_balanced(&facts.lines)
                || facts.lines.iter().any(|l| l.journal_id != journal.id)
            {
                return Err(source_changed().into());
            }
            let first = facts.ancestry.first().ok_or_else(source_changed)?;
            let debit_total: BigInt = facts
                .lines
                .iter()
                .map(|line| BigInt::from(line.debit_minor))
                .sum();
            if first.id != journal.id
                || first.currency != journal.currency
                || first.entry_date != journal.entry_date
                || first.source_type != journal.source_type
                || first.source_id != journal.source_id
                || first.total != debit_total.to_string()
            {
                return Err(source_changed().into());
            }
            let money = plan_journal_reversal_money(
                actor.company_id.as_str(),
                &facts.ancestry,
                &facts.money,
            )?;
            let id = format!("reversal-{}", command.key.as_str());
            let result = JournalReviewResult::Reversed {
                reversal_journal_id: id.clone(),
                reversed_journal_id: journal.id.clone(),
            };
            (
                opening,
                JournalReviewAction::Reverse {
                    period_id: period_id.clone(),
                    entry_date: entry_date.clone(),
                    memo: memo
                        .clone()
                        .unwrap_or_else(|| format!("Reversal of {}", journal.journal_number)),
                    number: format!("REV-{}", command.key.as_str()),
                    lines: inverse_journal_lines(&facts.lines, &id),
                    reversal_id: id,
                    money: Box::new(money),
                },
                result,
            )
        }
    };
    let plan = JournalReviewPlan {
        journal,
        opening,
        action,
        result,
        now: ports.now(),
    };
    ports
        .require_source_purchase(&authority, &source, command)
        .await?;
    let money = ports.prepare_money(&authority, &source, &plan).await?;
    ports
        .apply_command(&authority, &source, command, &plan, &money)
        .await?;
    ports.flush_secondary_books(&authority, &source).await?;
    ports
        .retain_source(&authority, &source, command, &plan, &money)
        .await?;
    ports.require_final_authority(&authority, command).await?;
    Ok(plan.result)
}
fn source_changed() -> JournalError {
    review_error(
        FailureKind::Conflict,
        "JOURNAL_REVIEW_SOURCE_CHANGED",
        "The protected journal, period, accounts or retained ledger changed.",
    )
}
