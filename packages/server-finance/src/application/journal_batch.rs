//! Owner-side batch-post use case. It sequences the existing journal review/post
//! port over each selection and finalizes the whole slice as one caller-owned
//! Finance command. No port commits, retries, opens a connection or mints
//! authority on its own.
use std::future::Future;

use erpx_server_shared::public::{CommandKey, FailureKind, RecordId, TimestampMillis};
use serde::Serialize;

use super::journal::FinanceActor;
use super::journal_review::{JournalReviewCommand, JournalReviewInput, JournalReviewResult};
use crate::domain::journal::JournalError;
use crate::domain::journal_batch::{
    JournalBatchInput, JournalBatchResult, JournalBatchSnapshot, PostedJournal, enforce_line_limit,
    line_fingerprint, require_journal, validate_input,
};

pub const BATCH_OPERATION: &str = "finance.batch.post";

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
#[cfg_attr(feature = "contracts", derive(ts_rs::TS, utoipa::ToSchema))]
#[serde(rename_all = "camelCase")]
pub struct JournalBatchCommand {
    pub key: CommandKey,
    #[serde(rename = "batchId")]
    pub batch_id: RecordId,
    pub input: JournalBatchInput,
}

/// One admitted child of the batch. Fingerprint and evidence bind the actual
/// retained rows to the child and to the batch receipt in the same unit.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct BatchItemPlan {
    pub child_key: CommandKey,
    pub journal_id: String,
    pub expected_version: i64,
    pub fingerprint: String,
    pub evidence_digest: String,
    pub now: TimestampMillis,
    pub line_count: usize,
}

pub trait JournalBatchPorts {
    type Error: From<JournalError> + Send;
    type Authority: Send + Sync;

    /// Utility fences before the first Holding snapshot, including every
    /// selected journal and its retained lines under the caller's SERIALIZABLE
    /// transaction.
    fn lock_sources(
        &mut self,
        actor: &FinanceActor,
        command: &JournalBatchCommand,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    fn authorize_current(
        &mut self,
        actor: &FinanceActor,
        command: &JournalBatchCommand,
    ) -> impl Future<Output = Result<Self::Authority, Self::Error>> + Send;

    fn ready(
        &mut self,
        authority: &Self::Authority,
        command: &JournalBatchCommand,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    /// Only an exact normalized receipt may short-circuit; an unrelated key,
    /// body or operation must conflict rather than return another batch result.
    fn replay_exact(
        &mut self,
        authority: &Self::Authority,
        command: &JournalBatchCommand,
    ) -> impl Future<Output = Result<Option<JournalBatchResult>, Self::Error>> + Send;

    fn load_journal(
        &mut self,
        authority: &Self::Authority,
        journal_id: &str,
    ) -> impl Future<Output = Result<Option<JournalBatchSnapshot>, Self::Error>> + Send;

    fn digest(&self, payload: &str) -> String;
    fn now(&self) -> TimestampMillis;

    /// Delegate to the existing owner journal review/post port on the same
    /// connection. The port enforces independent approval, current authority,
    /// version, period, account, ledger balance, Money and retention for each
    /// child; the batch never re-implements those decisions.
    fn post(
        &mut self,
        authority: &Self::Authority,
        child: JournalReviewCommand,
    ) -> impl Future<Output = Result<JournalReviewResult, Self::Error>> + Send;

    /// Stage fingerprint assertions, the retained owner evidence rows and the
    /// batch receipt/audit/outbox in the same borrowed transaction.
    fn commit(
        &mut self,
        authority: &Self::Authority,
        command: &JournalBatchCommand,
        result: &JournalBatchResult,
        plans: &[BatchItemPlan],
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;

    fn require_final_authority(
        &mut self,
        authority: &Self::Authority,
        command: &JournalBatchCommand,
    ) -> impl Future<Output = Result<(), Self::Error>> + Send;
}

pub async fn post_journal_batch<P: JournalBatchPorts>(
    ports: &mut P,
    actor: &FinanceActor,
    command: &JournalBatchCommand,
) -> Result<JournalBatchResult, P::Error> {
    validate_input(&command.input)?;
    ports.lock_sources(actor, command).await?;
    let authority = ports.authorize_current(actor, command).await?;
    ports.ready(&authority, command).await?;
    if let Some(previous) = ports.replay_exact(&authority, command).await? {
        ports.require_final_authority(&authority, command).await?;
        return Ok(previous);
    }

    let mut first: Option<JournalBatchSnapshot> = None;
    let mut plans = Vec::with_capacity(command.input.journals.len());
    let mut line_count = 0usize;
    for (index, item) in command.input.journals.iter().enumerate() {
        let snapshot = ports
            .load_journal(&authority, item.id.as_str())
            .await?
            .ok_or_else(missing)?;
        require_journal(&snapshot, item, first.as_ref())?;
        if first.is_none() {
            first = Some(snapshot.clone());
        }
        let fingerprint = line_fingerprint(&snapshot.lines);
        line_count += snapshot.lines.len();
        enforce_line_limit(line_count)?;
        let payload = serde_json::json!({
            "batchId": command.batch_id.as_str(),
            "reason": command.input.reason,
            "lines": Vec::<serde_json::Value>::from_iter(snapshot.lines.iter().map(|line| {
                serde_json::json!({
                    "id": line.id,
                    "account_id": line.account_id,
                    "debit_minor": line.debit_minor,
                    "credit_minor": line.credit_minor,
                    "base_debit_minor": line.base_debit_minor,
                    "base_credit_minor": line.base_credit_minor,
                    "dimension_json": line.dimension_json,
                    "ordinal": line.ordinal,
                })
            })),
        })
        .to_string();
        let child_key = CommandKey::new(format!("{}:{}", command.key.as_str(), index))
            .map_err(|_| invalid("BATCH_CHILD_KEY_INVALID"))?;
        plans.push(BatchItemPlan {
            child_key,
            journal_id: item.id.clone(),
            expected_version: item.expected_version,
            fingerprint,
            evidence_digest: ports.digest(&payload),
            now: ports.now(),
            line_count: snapshot.lines.len(),
        });
    }

    let mut posted = Vec::with_capacity(plans.len());
    for plan in &plans {
        let child = JournalReviewCommand {
            key: plan.child_key.clone(),
            journal_id: RecordId::new(plan.journal_id.clone())
                .map_err(|_| invalid("BATCH_JOURNAL_ID_INVALID"))?,
            input: JournalReviewInput::Post {
                expected_version: plan.expected_version,
            },
        };
        match ports.post(&authority, child).await? {
            JournalReviewResult::Posted {
                id,
                status,
                version,
            } if status == "posted"
                && id == plan.journal_id
                && version == plan.expected_version + 1 =>
            {
                posted.push(PostedJournal { id, version });
            }
            _ => return Err(crate::domain::journal_batch::changed().into()),
        }
    }
    let result = JournalBatchResult {
        id: command.batch_id.as_str().into(),
        status: "posted".into(),
        journals: posted,
        line_count: i64::try_from(line_count).unwrap_or(i64::MAX),
    };
    ports.commit(&authority, command, &result, &plans).await?;
    ports.require_final_authority(&authority, command).await?;
    Ok(result)
}

fn missing() -> JournalError {
    batch_error(
        FailureKind::NotFound,
        "JOURNAL_NOT_FOUND",
        "A selected journal is not in this Company.",
    )
}

fn invalid(code: &'static str) -> JournalError {
    JournalError::new(code, "The batch command is malformed.")
}

pub(crate) fn batch_error(
    kind: FailureKind,
    code: &'static str,
    message: &'static str,
) -> JournalError {
    JournalError {
        kind,
        code,
        message: message.into(),
    }
}
