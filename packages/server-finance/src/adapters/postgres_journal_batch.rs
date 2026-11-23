//! Real owner SQL for the Finance journal batch-post port. It runs only on the
//! caller-owned SERIALIZABLE connection: protected source locks, exact replay,
//! child posts through the existing journal review/post port, line fingerprints,
//! owner evidence, receipt/audit/outbox and final authority share one unit.
//! No production authority is minted here; current policy remains a port.
use std::future::Future;

use erpx_server_runtime::public::postgres::{DatabasePool, PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource, RecordId,
};
use erpx_server_workflow::public::commands::*;
use sha2::{Digest, Sha256};
use sqlx::{Connection, PgConnection, Row};
use tokio::sync::Mutex;

use super::journal_batch_wire::NormalizedJournalBatchCommand;
use super::postgres_journal_review::PgJournalReview;
use super::postgres_journal_review::{FinanceJournalAccessPurpose, PgFinanceJournalReviewPolicy};
use crate::application::journal::FinanceActor;
use crate::application::journal_batch::{
    BATCH_OPERATION, BatchItemPlan, JournalBatchCommand, JournalBatchPorts, post_journal_batch,
};
use crate::application::journal_review::{
    JournalReviewCommand, JournalReviewInput, review_journal,
};
use crate::domain::journal::JournalError;
use crate::domain::journal_batch::JournalBatchResult;

type Error = PgTransactionError<JournalError>;
type WorkflowError = PgTransactionError<BusinessError>;

/// Current authority and foreign-source policy for a batch. The review policy
/// remains mandatory because each child is posted by the existing review port.
pub trait PgJournalBatchPolicy: PgFinanceJournalReviewPolicy + Send {
    /// Revalidate the actor's current Control grant and return its workflow
    /// identity. The default delegates to the review authority probe so one
    /// assembled policy can serve both ports; composition may override it.
    fn batch_authorize(
        &mut self,
        connection: &mut PgConnection,
        actor: &FinanceActor,
        command: &JournalBatchCommand,
        now: i64,
    ) -> impl Future<Output = Result<CommandActor, WorkflowError>> + Send {
        let key = command.key.clone();
        let first = command
            .input
            .journals
            .first()
            .map(|item| item.id.clone())
            .unwrap_or_default();
        async move {
            let probe = JournalReviewCommand {
                key,
                journal_id: RecordId::new(first).map_err(|e| WorkflowError::Business(e))?,
                input: JournalReviewInput::Post {
                    expected_version: 1,
                },
            };
            self.authorize_current(connection, actor, &probe, now).await
        }
    }

    fn batch_ready(
        &mut self,
        _connection: &mut PgConnection,
        _actor: &CommandActor,
        _now: i64,
    ) -> impl Future<Output = Result<(), WorkflowError>> + Send {
        async { Ok(()) }
    }

    fn batch_source_access(
        &mut self,
        _connection: &mut PgConnection,
        _actor: &CommandActor,
        _command: &JournalBatchCommand,
        _purpose: FinanceJournalAccessPurpose,
        _now: i64,
    ) -> impl Future<Output = Result<(), WorkflowError>> + Send {
        async { Ok(()) }
    }
}

pub struct PgJournalBatch<'c, 'p, P, C, I> {
    connection: &'c mut PgConnection,
    holding: HoldingId,
    company: CompanyId,
    policy: &'p Mutex<P>,
    clock: &'p C,
    identifiers: &'p mut I,
    normalized: &'p NormalizedJournalBatchCommand,
    actor: Option<FinanceActor>,
    authority: Option<CommandActor>,
    replayed: bool,
    committed: bool,
}

impl<'c, 'p, P, C, I> PgJournalBatch<'c, 'p, P, C, I> {
    pub fn new(
        holding: HoldingId,
        company: CompanyId,
        connection: &'c mut PgConnection,
        policy: &'p Mutex<P>,
        clock: &'p C,
        identifiers: &'p mut I,
        normalized: &'p NormalizedJournalBatchCommand,
    ) -> Result<Self, Error> {
        if !connection.is_in_transaction() {
            return Err(PersistenceError::new("JOURNAL_BATCH_OWNER_TRANSACTION_REQUIRED").into());
        }
        Ok(Self {
            connection,
            holding,
            company,
            policy,
            clock,
            identifiers,
            normalized,
            actor: None,
            authority: None,
            replayed: false,
            committed: false,
        })
    }
}

struct Authority<'a, P> {
    policy: &'a Mutex<P>,
    actor: &'a CommandActor,
    normalized: &'a NormalizedJournalBatchCommand,
    now: i64,
}

impl<P: PgJournalBatchPolicy> PgCommandAuthority for Authority<'_, P> {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), WorkflowError> {
        Ok(())
    }

    async fn authorize(
        &mut self,
        _: &mut PgConnection,
        request: &WorkflowRequest,
    ) -> Result<CommandActor, WorkflowError> {
        require_request(request, self.normalized).map_err(to_workflow)?;
        Ok(self.actor.clone())
    }

    async fn verify_command(
        &mut self,
        connection: &mut PgConnection,
        _: &CommandActor,
        request: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), WorkflowError> {
        require_request(request, self.normalized).map_err(to_workflow)?;
        self.policy
            .lock()
            .await
            .batch_source_access(
                connection,
                self.actor,
                self.normalized.command(),
                match disposition {
                    CommandDisposition::Replay => FinanceJournalAccessPurpose::ExactReplayHistory,
                    CommandDisposition::Execute => FinanceJournalAccessPurpose::SettleExisting,
                },
                self.now,
            )
            .await
    }
}

struct ReplayOnly;
impl PgOwnerCommandEffects for ReplayOnly {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), WorkflowError> {
        Ok(())
    }
    async fn prepare(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<WorkflowOutput, WorkflowError> {
        Err(PersistenceError::new("FINANCE_BATCH_REPLAY_CANNOT_PREPARE").into())
    }
    async fn apply(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), WorkflowError> {
        Err(PersistenceError::new("FINANCE_BATCH_REPLAY_CANNOT_APPLY").into())
    }
    async fn retain(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), WorkflowError> {
        Err(PersistenceError::new("FINANCE_BATCH_REPLAY_CANNOT_RETAIN").into())
    }
}

struct Effects<'a> {
    command: &'a JournalBatchCommand,
    normalized: &'a NormalizedJournalBatchCommand,
    result: &'a JournalBatchResult,
    plans: &'a [BatchItemPlan],
    actor: &'a CommandActor,
}

impl PgOwnerCommandEffects for Effects<'_> {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), WorkflowError> {
        Ok(())
    }

    async fn prepare(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<WorkflowOutput, WorkflowError> {
        require_request(request, self.normalized).map_err(to_workflow)?;
        validate_authority(self.actor, actor)?;
        // Stage every retained-line fingerprint as a transaction-local guard.
        // A differing line image raises 23514 before the batch receipt exists.
        for plan in self.plans {
            let step = format!("batch-lines:{}", plan.journal_id);
            sqlx::query(
                "INSERT INTO command_assertions(command_id,step,ok) VALUES($1,$2,CASE WHEN (SELECT jsonb_agg(jsonb_build_array(id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal) ORDER BY ordinal) FROM finance_journal_lines WHERE journal_id=$3)=$4::jsonb THEN 1 ELSE 0 END)",
            )
            .bind(self.command.key.as_str())
            .bind(step.as_str())
            .bind(plan.journal_id.as_str())
            .bind(plan.fingerprint.as_str())
            .execute(&mut *connection)
            .await?;
        }
        Ok(WorkflowOutput {
            version: 1,
            result: CommandJson::from_serializable(self.result)?,
            event_scope: None,
        })
    }

    async fn apply(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), WorkflowError> {
        for plan in self.plans {
            // Retained finance_stmt_15 owner evidence projection.
            sqlx::query(
                "INSERT INTO finance_batch_review_items(command_id,journal_id,company_id,reviewed_version,reviewed_by,assignment_id,evidence_digest,reason,reviewed_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)",
            )
            .bind(self.command.key.as_str())
            .bind(plan.journal_id.as_str())
            .bind(self.actor.company_id.as_str())
            .bind(plan.expected_version)
            .bind(actor.user_id.as_str())
            .bind(actor.assignment_id.as_str())
            .bind(plan.evidence_digest.as_str())
            .bind(self.command.input.reason.as_str())
            .bind(plan.now.get())
            .execute(&mut *connection)
            .await?;
        }
        Ok(())
    }

    async fn retain(
        &mut self,
        connection: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), WorkflowError> {
        let rows: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM finance_batch_review_items WHERE command_id=$1",
        )
        .bind(self.command.key.as_str())
        .fetch_one(&mut *connection)
        .await?;
        if rows != self.plans.len() as i64 {
            return Err(PersistenceError::new("JOURNAL_BATCH_EVIDENCE_RETAIN_FAILED").into());
        }
        Ok(())
    }
}

impl<P: PgJournalBatchPolicy, C: Clock + Sync, I: IdentifierSource + Send> JournalBatchPorts
    for PgJournalBatch<'_, '_, P, C, I>
{
    type Error = Error;
    type Authority = CommandActor;

    async fn lock_sources(
        &mut self,
        actor: &FinanceActor,
        command: &JournalBatchCommand,
    ) -> Result<(), Error> {
        if actor.holding_id != self.holding
            || actor.company_id != self.company
            || self.actor.as_ref().is_some_and(|bound| bound != actor)
        {
            return Err(changed().into());
        }
        let ids: Vec<String> = command
            .input
            .journals
            .iter()
            .map(|item| item.id.clone())
            .collect();
        let locked: Vec<String> = sqlx::query_scalar(
            "SELECT id FROM finance_journals WHERE company_id=$1 AND id=ANY($2) ORDER BY id FOR UPDATE",
        )
        .bind(actor.company_id.as_str())
        .bind(&ids)
        .fetch_all(&mut *self.connection)
        .await?;
        if locked.len() != ids.len() {
            return Err(crate::application::journal_batch::batch_error(
                FailureKind::NotFound,
                "JOURNAL_NOT_FOUND",
                "A selected journal is not in this Company.",
            )
            .into());
        }
        sqlx::query(
            "SELECT id FROM finance_journal_lines WHERE journal_id=ANY($1) ORDER BY journal_id,ordinal FOR UPDATE",
        )
        .bind(&ids)
        .fetch_all(&mut *self.connection)
        .await?;
        let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?;
        if isolation != "serializable" {
            return Err(
                PersistenceError::new("JOURNAL_BATCH_SERIALIZABLE_TRANSACTION_REQUIRED").into(),
            );
        }
        self.actor = Some(actor.clone());
        Ok(())
    }

    async fn authorize_current(
        &mut self,
        actor: &FinanceActor,
        command: &JournalBatchCommand,
    ) -> Result<CommandActor, Error> {
        let bound = self.actor.as_ref().ok_or_else(|| changed())?.clone();
        if &bound != actor {
            return Err(changed().into());
        }
        let current = self
            .policy
            .lock()
            .await
            .batch_authorize(self.connection, actor, command, self.clock.now().get())
            .await
            .map_err(from_workflow)?;
        require_actor(actor, &current).map_err(from_workflow)?;
        self.authority = Some(current.clone());
        Ok(current)
    }

    async fn ready(
        &mut self,
        authority: &CommandActor,
        _: &JournalBatchCommand,
    ) -> Result<(), Error> {
        self.policy
            .lock()
            .await
            .batch_ready(self.connection, authority, self.clock.now().get())
            .await
            .map_err(from_workflow)
    }

    async fn replay_exact(
        &mut self,
        authority: &CommandActor,
        command: &JournalBatchCommand,
    ) -> Result<Option<JournalBatchResult>, Error> {
        let bound = self.authority.as_ref().ok_or_else(|| changed())?;
        require_actor_bound(bound, authority)?;
        let normalized = self
            .normalized
            .input_for(command)
            .map_err(|e| from_workflow(e.into()))?;
        let mut leader = Authority {
            policy: self.policy,
            actor: bound,
            normalized: self.normalized,
            now: self.clock.now().get(),
        };
        let outcome = PgCommandLedger::new(
            self.company.clone(),
            self.connection,
            &mut leader,
            self.clock,
            self.identifiers,
        )
        .replay(
            &WorkflowRequest {
                key: command.key.as_str().into(),
                operation: BATCH_OPERATION.into(),
                record_id: command.batch_id.as_str().into(),
                input: normalized,
            },
            &mut ReplayOnly,
        )
        .await
        .map_err(from_workflow)?;
        self.replayed = outcome.is_some();
        outcome.map(|outcome| decode(&outcome, command)).transpose()
    }

    async fn load_journal(
        &mut self,
        authority: &CommandActor,
        journal_id: &str,
    ) -> Result<Option<crate::domain::journal_batch::JournalBatchSnapshot>, Error> {
        let bound = self.authority.as_ref().ok_or_else(|| changed())?;
        require_actor_bound(bound, authority)?;
        let row = sqlx::query(
            "SELECT id,company_id,chart_id,period_id,currency,status,version,source_type FROM finance_journals WHERE id=$1 AND company_id=$2",
        )
        .bind(journal_id)
        .bind(self.actor.as_ref().ok_or_else(|| changed())?.company_id.as_str())
        .fetch_optional(&mut *self.connection)
        .await?;
        let Some(row) = row else { return Ok(None) };
        let lines = sqlx::query_as::<_, (String, String, i64, i64, i64, i64, Option<String>, i64)>(
            "SELECT id,account_id,debit_minor,credit_minor,base_debit_minor,base_credit_minor,dimension_json,ordinal FROM finance_journal_lines WHERE journal_id=$1 ORDER BY ordinal",
        )
        .bind(journal_id)
        .fetch_all(&mut *self.connection)
        .await?
        .into_iter()
        .map(|(id, account_id, debit_minor, credit_minor, base_debit_minor, base_credit_minor, dimension_json, ordinal)| {
            crate::domain::journal_review::JournalLedgerLine {
                id,
                journal_id: journal_id.into(),
                account_id,
                description: String::new(),
                debit_minor,
                credit_minor,
                base_debit_minor,
                base_credit_minor,
                dimension_json: dimension_json.unwrap_or_default(),
                ordinal,
            }
        })
        .collect();
        Ok(Some(crate::domain::journal_batch::JournalBatchSnapshot {
            id: row.try_get("id")?,
            company_id: row.try_get("company_id")?,
            chart_id: row.try_get("chart_id")?,
            period_id: row.try_get("period_id")?,
            currency: row.try_get("currency")?,
            status: row.try_get("status")?,
            version: row.try_get("version")?,
            source_type: row.try_get("source_type")?,
            lines,
        }))
    }

    fn digest(&self, payload: &str) -> String {
        format!("{:x}", Sha256::digest(payload.as_bytes()))
    }

    fn now(&self) -> erpx_server_shared::public::TimestampMillis {
        self.clock.now()
    }

    async fn post(
        &mut self,
        _: &CommandActor,
        child: JournalReviewCommand,
    ) -> Result<crate::application::journal_review::JournalReviewResult, Error> {
        let actor = self.actor.as_ref().ok_or_else(|| changed())?.clone();
        let mut ports = PgJournalReview::new(
            self.holding.clone(),
            self.company.clone(),
            &mut *self.connection,
            self.policy,
            self.clock,
            &mut *self.identifiers,
        )?;
        review_journal(&mut ports, &actor, &child).await
    }

    async fn commit(
        &mut self,
        authority: &CommandActor,
        command: &JournalBatchCommand,
        result: &JournalBatchResult,
        plans: &[BatchItemPlan],
    ) -> Result<(), Error> {
        if self.replayed || self.committed {
            return Err(changed().into());
        }
        let bound = self.authority.as_ref().ok_or_else(|| changed())?.clone();
        require_actor_bound(&bound, authority)?;
        let input = self
            .normalized
            .input_for(command)
            .map_err(|e| from_workflow(e.into()))?;
        let mut leader = Authority {
            policy: self.policy,
            actor: &bound,
            normalized: self.normalized,
            now: self.clock.now().get(),
        };
        let mut effects = Effects {
            command,
            normalized: self.normalized,
            result,
            plans,
            actor: &bound,
        };
        let outcome = PgCommandLedger::new(
            self.company.clone(),
            self.connection,
            &mut leader,
            self.clock,
            self.identifiers,
        )
        .execute(
            &WorkflowRequest {
                key: command.key.as_str().into(),
                operation: BATCH_OPERATION.into(),
                record_id: command.batch_id.as_str().into(),
                input,
            },
            &mut effects,
        )
        .await
        .map_err(from_workflow)?;
        if outcome.replayed || decode(&outcome, command)? != *result {
            return Err(changed().into());
        }
        self.committed = true;
        Ok(())
    }

    async fn require_final_authority(
        &mut self,
        authority: &CommandActor,
        command: &JournalBatchCommand,
    ) -> Result<(), Error> {
        let bound = self.authority.as_ref().ok_or_else(|| changed())?;
        require_actor_bound(bound, authority)?;
        let actor = self.actor.as_ref().ok_or_else(|| changed())?.clone();
        let current = self
            .policy
            .lock()
            .await
            .batch_authorize(self.connection, &actor, command, self.clock.now().get())
            .await
            .map_err(from_workflow)?;
        if &current != authority {
            return Err(changed().into());
        }
        self.policy
            .lock()
            .await
            .batch_source_access(
                self.connection,
                authority,
                command,
                if self.replayed {
                    FinanceJournalAccessPurpose::ExactReplayHistory
                } else {
                    FinanceJournalAccessPurpose::SettleExisting
                },
                self.clock.now().get(),
            )
            .await
            .map_err(from_workflow)
    }
}

/// One caller-owned SERIALIZABLE unit: no independent commit, retry or pool
/// checkout inside the batch. Any failure rolls the whole slice back.
pub async fn run_journal_batch<
    P: PgJournalBatchPolicy,
    C: Clock + Sync,
    I: IdentifierSource + Send,
>(
    pool: &DatabasePool,
    policy: &Mutex<P>,
    clock: &C,
    identifiers: &mut I,
    actor: &FinanceActor,
    normalized: &NormalizedJournalBatchCommand,
) -> Result<JournalBatchResult, Error> {
    let mut transaction = pool
        .raw()
        .begin_with("BEGIN ISOLATION LEVEL SERIALIZABLE")
        .await?;
    let outcome = async {
        let mut ports = PgJournalBatch::new(
            actor.holding_id.clone(),
            actor.company_id.clone(),
            &mut transaction,
            policy,
            clock,
            identifiers,
            normalized,
        )?;
        post_journal_batch(&mut ports, actor, normalized.command()).await
    }
    .await;
    match outcome {
        Ok(value) => {
            transaction.commit().await?;
            Ok(value)
        }
        Err(error) => {
            transaction.rollback().await?;
            Err(error)
        }
    }
}

fn require_request(
    request: &WorkflowRequest,
    normalized: &NormalizedJournalBatchCommand,
) -> Result<(), BusinessError> {
    let command = normalized.command();
    if request.key != command.key.as_str()
        || request.operation != BATCH_OPERATION
        || request.record_id != command.batch_id.as_str()
        || request.input != normalized.input_for(command)?
    {
        return Err(BusinessError::invalid(
            "JOURNAL_BATCH_NORMALIZED_MISMATCH",
            "Use the exact normalized Finance batch command.",
        ));
    }
    Ok(())
}

fn decode(
    outcome: &CommandOutcome,
    command: &JournalBatchCommand,
) -> Result<JournalBatchResult, Error> {
    let result: JournalBatchResult =
        serde_json::from_str(outcome.result.as_str()).map_err(|_| {
            JournalError::new(
                "JOURNAL_BATCH_SOURCE_CHANGED",
                "The retained batch result changed.",
            )
        })?;
    if result.id != command.batch_id.as_str() || result.status != "posted" {
        return Err(changed().into());
    }
    Ok(result)
}

fn require_actor(expected: &FinanceActor, actor: &CommandActor) -> Result<(), WorkflowError> {
    actor.validate()?;
    if actor.user_id != expected.user_id
        || actor.assignment_id != expected.assignment_id
        || actor.company_id != expected.company_id
    {
        return Err(actor_changed().into());
    }
    Ok(())
}

fn validate_authority(expected: &CommandActor, actor: &CommandActor) -> Result<(), WorkflowError> {
    actor.validate()?;
    if expected.user_id != actor.user_id
        || expected.assignment_id != actor.assignment_id
        || expected.company_id != actor.company_id
    {
        return Err(actor_changed().into());
    }
    Ok(())
}

fn actor_changed() -> BusinessError {
    BusinessError::new(
        FailureKind::Conflict,
        "JOURNAL_BATCH_ACTOR_CHANGED",
        "The current Finance actor differs from the admitted actor.",
    )
}

fn require_actor_bound(bound: &CommandActor, actor: &CommandActor) -> Result<(), Error> {
    validate_authority(bound, actor).map_err(from_workflow)
}

fn changed() -> Error {
    JournalError::new(
        "JOURNAL_BATCH_SOURCE_CHANGED",
        "The protected journals, ledger lines or retained Finance authority changed.",
    )
    .into()
}

fn from_workflow(error: WorkflowError) -> Error {
    match error {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => PgTransactionError::Business(JournalError {
            kind: e.kind,
            code: e.code,
            message: e.message.into(),
        }),
    }
}

fn to_workflow(error: BusinessError) -> WorkflowError {
    PgTransactionError::Business(error)
}
