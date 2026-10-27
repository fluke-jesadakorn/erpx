//! Real Finance SQL within the caller's retained Control/Holding unit. External
//! current Identity, release, purchases and foreign-owner source proofs remain
//! mandatory ports; this module provides no production authority implementation.
mod closure;
mod image;
mod money;
mod retention;

use std::future::Future;

use erpx_server_runtime::public::postgres::{PersistenceError, PgTransactionError};
use erpx_server_shared::public::{
    BusinessError, Clock, CompanyId, FailureKind, HoldingId, IdentifierSource, TimestampMillis,
};
use erpx_server_workflow::public::commands::*;
use serde_json::Value;
use sqlx::{Connection, PgConnection};
use tokio::sync::Mutex;

use super::postgres_journal_facts::current_transaction_witness;
use crate::application::{
    journal::{FinanceActor, JournalTransactionWitness},
    journal_review::*,
};
use crate::domain::{
    journal::JournalError,
    journal_review::{JournalReviewerFacts, ReviewJournal},
};

type Error = PgTransactionError<JournalError>;
type WorkflowError = PgTransactionError<BusinessError>;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FinanceJournalAccessPurpose {
    BaselineRead,
    ExactReplayHistory,
    SettleExisting,
}

/// Must be assembled from the current public Identity/People/Organization,
/// Commercial and original owner source ports. No default implementation exists.
pub trait PgFinanceJournalReviewPolicy: Send {
    /// Utility locks only, before any Holding data snapshot. Retain the original
    /// Control request lease and every foreign-owner denial/absence fence.
    fn lock_sources(
        &mut self,
        connection: &mut PgConnection,
        actor: &FinanceActor,
        command: &JournalReviewCommand,
    ) -> impl Future<Output = Result<(), WorkflowError>> + Send;
    /// Require current finance.write and real selected grant/role/HR/scope pins.
    fn authorize_current(
        &mut self,
        connection: &mut PgConnection,
        actor: &FinanceActor,
        command: &JournalReviewCommand,
        now: i64,
    ) -> impl Future<Output = Result<CommandActor, WorkflowError>> + Send;
    fn reviewer(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        journal: Option<&ReviewJournal>,
        now: i64,
    ) -> impl Future<Output = Result<JournalReviewerFacts, WorkflowError>> + Send;
    fn ready(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        now: i64,
    ) -> impl Future<Output = Result<(), WorkflowError>> + Send;
    /// Inspect actual owner links, including real book→primary relationships,
    /// scheduled release, explicit Commercial mode and all required capabilities.
    /// History is called only after Workflow matches the entire request hash.
    /// Repeated calls must revalidate the original foreign source/purchase facts,
    /// not accept source_type/source_id or a caller-supplied capability as proof.
    fn require_source_access(
        &mut self,
        connection: &mut PgConnection,
        actor: &CommandActor,
        command: &JournalReviewCommand,
        purpose: FinanceJournalAccessPurpose,
        now: i64,
    ) -> impl Future<Output = Result<(), WorkflowError>> + Send;
}

#[derive(Clone, Debug, PartialEq)]
struct Image {
    chain: Vec<Value>,
    journals: Vec<Value>,
    lines: Vec<Value>,
    money: Vec<Value>,
    invoices: Vec<Value>,
    allocations: Vec<Value>,
    books: Vec<Value>,
    events: Vec<Value>,
    configurations: Vec<(String, Vec<Value>)>,
}
/// Same-transaction source. Neither its constructor nor its images are public.
pub struct PgJournalReviewSource {
    witness: JournalTransactionWitness,
    actor: FinanceActor,
    command: JournalReviewCommand,
    journal: ReviewJournal,
    primary_ids: Vec<String>,
    before: Image,
    database_at: i64,
}
pub struct PgJournalReviewMoney {
    row: Value,
    witness: JournalTransactionWitness,
}

pub struct PgJournalReview<'c, 'p, Policy, ClockSource, Ids> {
    connection: &'c mut PgConnection,
    holding: HoldingId,
    company: CompanyId,
    policy: &'p Mutex<Policy>,
    clock: &'p ClockSource,
    identifiers: &'p mut Ids,
    bound_actor: Option<FinanceActor>,
    replayed: bool,
    retained: Option<(Vec<String>, Image)>,
}
impl<'c, 'p, P, C, I> PgJournalReview<'c, 'p, P, C, I> {
    pub fn new(
        holding: HoldingId,
        company: CompanyId,
        connection: &'c mut PgConnection,
        policy: &'p Mutex<P>,
        clock: &'p C,
        identifiers: &'p mut I,
    ) -> Result<Self, Error> {
        if !connection.is_in_transaction() {
            return Err(PersistenceError::new("JOURNAL_OWNER_TRANSACTION_REQUIRED").into());
        }
        Ok(Self {
            connection,
            holding,
            company,
            policy,
            clock,
            identifiers,
            bound_actor: None,
            replayed: false,
            retained: None,
        })
    }
}

struct Authority<'a, P, C> {
    policy: &'a Mutex<P>,
    clock: &'a C,
    actor: FinanceActor,
    command: &'a JournalReviewCommand,
}
impl<P: PgFinanceJournalReviewPolicy, C: Clock + Sync> PgCommandAuthority for Authority<'_, P, C> {
    async fn lock_sources(
        &mut self,
        _: &mut PgConnection,
        _: &WorkflowRequest,
    ) -> Result<(), WorkflowError> {
        Ok(())
    }
    async fn authorize(
        &mut self,
        c: &mut PgConnection,
        request: &WorkflowRequest,
    ) -> Result<CommandActor, WorkflowError> {
        require_request(request, self.command).map_err(to_workflow)?;
        let actor = self
            .policy
            .lock()
            .await
            .authorize_current(c, &self.actor, self.command, self.clock.now().get())
            .await?;
        require_actor(&self.actor, &actor).map_err(to_workflow)?;
        Ok(actor)
    }
    async fn verify_command(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        request: &WorkflowRequest,
        disposition: CommandDisposition,
    ) -> Result<(), WorkflowError> {
        require_request(request, self.command).map_err(to_workflow)?;
        self.policy
            .lock()
            .await
            .require_source_access(
                c,
                actor,
                self.command,
                match disposition {
                    CommandDisposition::Replay => FinanceJournalAccessPurpose::ExactReplayHistory,
                    CommandDisposition::Execute => FinanceJournalAccessPurpose::SettleExisting,
                },
                self.clock.now().get(),
            )
            .await
    }
}
/// No absent receipt may accidentally prepare owner effects in the replay path.
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
        Err(PersistenceError::new("FINANCE_REPLAY_CANNOT_PREPARE").into())
    }
    async fn apply(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), WorkflowError> {
        Err(PersistenceError::new("FINANCE_REPLAY_CANNOT_APPLY").into())
    }
    async fn retain(
        &mut self,
        _: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), WorkflowError> {
        Err(PersistenceError::new("FINANCE_REPLAY_CANNOT_RETAIN").into())
    }
}
struct Effects<'a> {
    source: &'a PgJournalReviewSource,
    plan: &'a JournalReviewPlan,
    money: &'a PgJournalReviewMoney,
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
        c: &mut PgConnection,
        actor: &CommandActor,
        request: &WorkflowRequest,
    ) -> Result<WorkflowOutput, WorkflowError> {
        require_request(request, &self.source.command).map_err(to_workflow)?;
        require_actor(&self.source.actor, actor).map_err(to_workflow)?;
        self.source.require_before(c).await.map_err(to_workflow)?;
        if self.money.witness != self.source.witness {
            return Err(to_workflow(changed()));
        }
        Ok(WorkflowOutput {
            version: self.plan.journal().version + 1,
            result: CommandJson::from_serializable(self.plan.result())?,
            event_scope: None,
        })
    }
    async fn apply(
        &mut self,
        c: &mut PgConnection,
        actor: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), WorkflowError> {
        self.source.require_before(c).await.map_err(to_workflow)?;
        image::apply(c, actor, self.source, self.plan)
            .await
            .map_err(to_workflow)?;
        money::insert(c, &self.money.row)
            .await
            .map_err(to_workflow)?;
        image::apply_opening(c, actor, self.source, self.plan)
            .await
            .map_err(to_workflow)
    }
    async fn retain(
        &mut self,
        c: &mut PgConnection,
        _: &CommandActor,
        _: &WorkflowRequest,
    ) -> Result<(), WorkflowError> {
        // Workflow's assertion cleanup precedes the final deferred Finance
        // flush. Here prove primary/Money effects; the application then flushes
        // all actual books/events and performs their complete after-image check.
        let after = image::capture(
            c,
            &self.source.actor.company_id,
            &self.source.primary_ids,
            &self.source.command,
        )
        .await
        .map_err(to_workflow)?;
        retention::primary(self.source, self.plan, &self.money.row, &after).map_err(to_workflow)
    }
}

impl PgJournalReviewSource {
    async fn require_transaction(&self, c: &mut PgConnection) -> Result<(), Error> {
        if current_transaction_witness(c).await? != self.witness {
            return Err(changed());
        }
        Ok(())
    }
    async fn require_before(&self, c: &mut PgConnection) -> Result<(), Error> {
        self.require_transaction(c).await?;
        if image::capture(c, &self.actor.company_id, &self.primary_ids, &self.command).await?
            != self.before
        {
            return Err(changed());
        }
        Ok(())
    }
}

impl<P: PgFinanceJournalReviewPolicy, C: Clock + Sync, I: IdentifierSource + Send>
    JournalReviewPorts for PgJournalReview<'_, '_, P, C, I>
{
    type Error = Error;
    type Authority = CommandActor;
    type Source = PgJournalReviewSource;
    type MoneyWitness = PgJournalReviewMoney;
    async fn lock_sources(
        &mut self,
        actor: &FinanceActor,
        command: &JournalReviewCommand,
    ) -> Result<(), Error> {
        if actor.holding_id != self.holding
            || actor.company_id != self.company
            || self.bound_actor.as_ref().is_some_and(|a| a != actor)
        {
            return Err(changed());
        }
        self.policy
            .lock()
            .await
            .lock_sources(self.connection, actor, command)
            .await
            .map_err(from_workflow)?;
        image::lock_sources(self.connection).await?;
        let isolation: String = sqlx::query_scalar("SHOW transaction_isolation")
            .fetch_one(&mut *self.connection)
            .await?;
        if isolation != "serializable" {
            return Err(PersistenceError::new("JOURNAL_SERIALIZABLE_TRANSACTION_REQUIRED").into());
        }
        self.bound_actor = Some(actor.clone());
        Ok(())
    }
    async fn authorize_current(
        &mut self,
        actor: &FinanceActor,
        command: &JournalReviewCommand,
    ) -> Result<CommandActor, Error> {
        let current = self
            .policy
            .lock()
            .await
            .authorize_current(self.connection, actor, command, self.clock.now().get())
            .await
            .map_err(from_workflow)?;
        require_actor(actor, &current)?;
        Ok(current)
    }
    async fn reviewer(
        &mut self,
        actor: &CommandActor,
        source: Option<&PgJournalReviewSource>,
    ) -> Result<JournalReviewerFacts, Error> {
        self.policy
            .lock()
            .await
            .reviewer(
                self.connection,
                actor,
                source.map(|s| &s.journal),
                self.clock.now().get(),
            )
            .await
            .map_err(from_workflow)
    }
    async fn ready(&mut self, actor: &CommandActor) -> Result<(), Error> {
        self.policy
            .lock()
            .await
            .ready(self.connection, actor, self.clock.now().get())
            .await
            .map_err(from_workflow)
    }
    async fn replay_exact(
        &mut self,
        actor: &CommandActor,
        command: &JournalReviewCommand,
    ) -> Result<Option<JournalReviewResult>, Error> {
        let bound = self.bound_actor.as_ref().ok_or_else(changed)?.clone();
        require_actor(&bound, actor)?;
        let mut authority = Authority {
            policy: self.policy,
            clock: self.clock,
            actor: bound,
            command,
        };
        let outcome = PgCommandLedger::new(
            self.company.clone(),
            self.connection,
            &mut authority,
            self.clock,
            self.identifiers,
        )
        .replay(&request(command)?, &mut ReplayOnly)
        .await
        .map_err(from_workflow)?;
        self.replayed = outcome.is_some();
        outcome
            .map(|o| {
                let result = decode_result(&o.result)?;
                if result != expected_result(command)? {
                    return Err(changed());
                }
                Ok(result)
            })
            .transpose()
    }
    async fn load_source(
        &mut self,
        actor: &CommandActor,
        command: &JournalReviewCommand,
        posted_only: bool,
    ) -> Result<Option<PgJournalReviewSource>, Error> {
        self.policy
            .lock()
            .await
            .require_source_access(
                self.connection,
                actor,
                command,
                FinanceJournalAccessPurpose::BaselineRead,
                self.clock.now().get(),
            )
            .await
            .map_err(from_workflow)?;
        image::load(
            self.connection,
            self.bound_actor.as_ref().ok_or_else(changed)?,
            command,
            posted_only,
        )
        .await
    }
    fn journal(source: &PgJournalReviewSource) -> &ReviewJournal {
        &source.journal
    }
    async fn posting_facts(
        &mut self,
        _: &CommandActor,
        source: &PgJournalReviewSource,
    ) -> Result<JournalPostingFacts, Error> {
        source.require_before(self.connection).await?;
        image::posting_facts(source)
    }
    async fn reversal_facts(
        &mut self,
        _: &CommandActor,
        source: &PgJournalReviewSource,
        _: &JournalReviewCommand,
    ) -> Result<JournalReversalSourceFacts, Error> {
        source.require_before(self.connection).await?;
        image::reversal_facts(source)
    }
    fn now(&self) -> TimestampMillis {
        self.clock.now()
    }
    async fn require_source_purchase(
        &mut self,
        actor: &CommandActor,
        _: &PgJournalReviewSource,
        command: &JournalReviewCommand,
    ) -> Result<(), Error> {
        self.policy
            .lock()
            .await
            .require_source_access(
                self.connection,
                actor,
                command,
                FinanceJournalAccessPurpose::SettleExisting,
                self.clock.now().get(),
            )
            .await
            .map_err(from_workflow)
    }
    async fn prepare_money(
        &mut self,
        _: &CommandActor,
        source: &PgJournalReviewSource,
        plan: &JournalReviewPlan,
    ) -> Result<PgJournalReviewMoney, Error> {
        source.require_before(self.connection).await?;
        Ok(PgJournalReviewMoney {
            row: money::prepare(source, plan, self.clock.now().get())?,
            witness: source.witness.clone(),
        })
    }
    async fn apply_command(
        &mut self,
        actor: &CommandActor,
        source: &PgJournalReviewSource,
        command: &JournalReviewCommand,
        plan: &JournalReviewPlan,
        money: &PgJournalReviewMoney,
    ) -> Result<(), Error> {
        source.require_transaction(self.connection).await?;
        require_actor(&source.actor, actor)?;
        let mut authority = Authority {
            policy: self.policy,
            clock: self.clock,
            actor: source.actor.clone(),
            command,
        };
        let mut effects = Effects {
            source,
            plan,
            money,
        };
        let outcome = PgCommandLedger::new(
            self.company.clone(),
            self.connection,
            &mut authority,
            self.clock,
            self.identifiers,
        )
        .execute(&request(command)?, &mut effects)
        .await
        .map_err(from_workflow)?;
        if outcome.replayed || decode_result(&outcome.result)? != *plan.result() {
            return Err(changed());
        }
        Ok(())
    }
    async fn flush_secondary_books(
        &mut self,
        _: &CommandActor,
        source: &PgJournalReviewSource,
    ) -> Result<(), Error> {
        source.require_transaction(self.connection).await?;
        sqlx::query("SET CONSTRAINTS finance_book_projection IMMEDIATE")
            .execute(&mut *self.connection)
            .await?;
        sqlx::query("SET CONSTRAINTS finance_book_projection DEFERRED")
            .execute(&mut *self.connection)
            .await?;
        sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
            .execute(&mut *self.connection)
            .await?;
        Ok(())
    }
    async fn retain_source(
        &mut self,
        _: &CommandActor,
        source: &PgJournalReviewSource,
        _: &JournalReviewCommand,
        plan: &JournalReviewPlan,
        money: &PgJournalReviewMoney,
    ) -> Result<(), Error> {
        source.require_transaction(self.connection).await?;
        let after = image::capture(
            self.connection,
            &self.company,
            &source.primary_ids,
            &source.command,
        )
        .await?;
        let now = image::database_now(self.connection).await?;
        retention::complete(source, plan, &money.row, &after, now)?;
        self.retained = Some((source.primary_ids.clone(), after));
        Ok(())
    }
    async fn require_final_authority(
        &mut self,
        actor: &CommandActor,
        command: &JournalReviewCommand,
    ) -> Result<(), Error> {
        let bound = self.bound_actor.as_ref().ok_or_else(changed)?;
        let current = self
            .policy
            .lock()
            .await
            .authorize_current(self.connection, bound, command, self.clock.now().get())
            .await
            .map_err(from_workflow)?;
        if &current != actor {
            return Err(changed());
        }
        self.policy
            .lock()
            .await
            .require_source_access(
                self.connection,
                &current,
                command,
                if self.replayed {
                    FinanceJournalAccessPurpose::ExactReplayHistory
                } else {
                    FinanceJournalAccessPurpose::SettleExisting
                },
                self.clock.now().get(),
            )
            .await
            .map_err(from_workflow)?;
        if let Some((ids, retained)) = &self.retained
            && &image::capture(self.connection, &self.company, ids, command).await? != retained
        {
            return Err(changed());
        }
        Ok(())
    }
}

fn request(command: &JournalReviewCommand) -> Result<WorkflowRequest, Error> {
    Ok(WorkflowRequest {
        key: command.key.as_str().into(),
        operation: command.operation().into(),
        record_id: command.journal_id.as_str().into(),
        input: CommandJson::from_serializable(&command.input)
            .map_err(|e| from_workflow(e.into()))?,
    })
}
fn require_request(request: &WorkflowRequest, command: &JournalReviewCommand) -> Result<(), Error> {
    let expected = self::request(command)?;
    if request.key != expected.key
        || request.operation != expected.operation
        || request.record_id != expected.record_id
        || request.input != expected.input
    {
        return Err(changed());
    }
    Ok(())
}
fn require_actor(expected: &FinanceActor, actor: &CommandActor) -> Result<(), Error> {
    actor.validate().map_err(|e| from_workflow(e.into()))?;
    if actor.user_id != expected.user_id
        || actor.assignment_id != expected.assignment_id
        || actor.company_id != expected.company_id
    {
        return Err(changed());
    }
    Ok(())
}
fn decode_result(value: &CommandJson) -> Result<JournalReviewResult, Error> {
    let value: Value = serde_json::from_str(value.as_str()).map_err(|_| changed())?;
    let object = value.as_object().ok_or_else(changed)?;
    if object.len() == 3 && value["status"] == "posted" {
        Ok(JournalReviewResult::Posted {
            id: image::text(&value, "id")?.into(),
            status: "posted".into(),
            version: image::integer(&value, "version")?,
        })
    } else if object.len() == 2 {
        Ok(JournalReviewResult::Reversed {
            reversal_journal_id: image::text(&value, "reversalJournalId")?.into(),
            reversed_journal_id: image::text(&value, "reversedJournalId")?.into(),
        })
    } else {
        Err(changed())
    }
}
fn expected_result(command: &JournalReviewCommand) -> Result<JournalReviewResult, Error> {
    match &command.input {
        JournalReviewInput::Post { expected_version }
            if (1..9_007_199_254_740_991).contains(expected_version) =>
        {
            Ok(JournalReviewResult::Posted {
                id: command.journal_id.as_str().into(),
                status: "posted".into(),
                version: expected_version + 1,
            })
        }
        JournalReviewInput::Reverse { .. } => Ok(JournalReviewResult::Reversed {
            reversal_journal_id: format!("reversal-{}", command.key.as_str()),
            reversed_journal_id: command.journal_id.as_str().into(),
        }),
        _ => Err(changed()),
    }
}
fn changed() -> Error {
    JournalError {
        kind: FailureKind::Conflict,
        code: "JOURNAL_REVIEW_SOURCE_CHANGED",
        message: "The protected journal, period, accounts or retained ledger changed.".into(),
    }
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
fn to_workflow(error: Error) -> WorkflowError {
    match error {
        PgTransactionError::Persistence(e) => PgTransactionError::Persistence(e),
        PgTransactionError::Business(e) => PgTransactionError::Business(BusinessError::new(
            e.kind,
            e.code,
            "The protected Finance journal could not be reviewed.",
        )),
    }
}
