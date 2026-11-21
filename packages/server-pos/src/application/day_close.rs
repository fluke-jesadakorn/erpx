//! Native POS day-close use case for `day.prepare` and `day.confirm`.
//!
//! Portable orchestration only. The pure decision rules live in
//! [`crate::domain::day_close`]; owner SQL is supplied through
//! [`DayCloseStore`] (implemented by `PgPosDayClose` on the caller's
//! serializable transaction) and the cross-owner report, IAM identity, clock
//! and identifier facts are injected through [`DayCloseWitness`]. Nothing here
//! mints authority or invents a foreign owner's facts. A prior receipt is
//! resolved on the lock before any fresh write, and the acknowledgement and
//! receipt are committed in that one transaction; the caller owns audit/outbox.
use crate::adapters::day_close::NewDayClose;
use crate::domain::commands::PosActor;
use crate::domain::day_close::{
    DayReportFacts, PreparedDayClose, validate_confirm, validate_prepare,
};
use crate::domain::day_receipt::PosDayReceiptIdentity;
use erpx_server_shared::public::{BusinessError, FailureKind, TimestampMillis};
use std::future::Future;

pub type DayCloseFuture<'a, T, E> =
    std::pin::Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

fn business(kind: FailureKind, code: &'static str, message: &'static str) -> BusinessError {
    BusinessError::new(kind, code, message)
}

/// Typed, bounded command input. Unknown fields are rejected at the HTTP edge.
#[derive(Clone, Debug)]
pub enum DayCloseCommand {
    Prepare {
        branch_id: String,
        business_date: String,
    },
    Confirm {
        id: String,
        expected_version: i64,
    },
}

impl DayCloseCommand {
    pub fn operation(&self) -> &'static str {
        match self {
            Self::Prepare { .. } => "day.prepare",
            Self::Confirm { .. } => "day.confirm",
        }
    }
}

/// Report facts plus the currency and snapshot needed for the prepared row.
/// Supplied by composition from the current Company transaction.
#[derive(Clone, Debug)]
pub struct DayCloseReport {
    pub facts: DayReportFacts,
    pub currency: String,
    /// Serialized ordered snapshot JSON for `pos_day_closes.snapshot_json`.
    pub snapshot_json: String,
}

/// Cross-owner facts and identifier/clock ports held by composition.
pub trait DayCloseWitness: Send {
    type Error: From<BusinessError> + Send;

    fn report<'a>(
        &'a self,
        company_id: &'a str,
        branch_id: &'a str,
        business_date: &'a str,
    ) -> DayCloseFuture<'a, DayCloseReport, Self::Error>;

    fn same_person<'a>(
        &'a self,
        left: &'a str,
        right: &'a str,
    ) -> DayCloseFuture<'a, bool, Self::Error>;

    fn new_id<'a>(&'a self) -> DayCloseFuture<'a, String, Self::Error>;

    fn today<'a>(&'a self) -> DayCloseFuture<'a, String, Self::Error>;
}

/// Owner SQL on the caller's serializable transaction. `PgPosDayClose`
/// implements this adapter in the owner crate.
pub trait DayCloseStore: Send {
    type Error: From<BusinessError> + Send;

    fn replay_receipt<'a>(
        &'a mut self,
        identity: &'a PosDayReceiptIdentity,
    ) -> DayCloseFuture<'a, Option<String>, Self::Error>;

    fn insert_receipt<'a>(
        &'a mut self,
        identity: &'a PosDayReceiptIdentity,
        result_json: &'a str,
        now: i64,
    ) -> DayCloseFuture<'a, (), Self::Error>;

    fn lock_company<'a>(&'a mut self, company_id: &'a str) -> DayCloseFuture<'a, (), Self::Error>;

    fn prepared_exists<'a>(
        &'a mut self,
        company_id: &'a str,
        branch_id: &'a str,
        business_date: &'a str,
    ) -> DayCloseFuture<'a, bool, Self::Error>;

    fn lock_close<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
    ) -> DayCloseFuture<'a, Option<PreparedDayClose>, Self::Error>;

    fn prepare<'a>(
        &'a mut self,
        close: &'a NewDayClose<'a>,
    ) -> DayCloseFuture<'a, bool, Self::Error>;

    fn confirm<'a>(
        &'a mut self,
        company_id: &'a str,
        id: &'a str,
        expected_version: i64,
        reviewer: &'a str,
        now: i64,
    ) -> DayCloseFuture<'a, bool, Self::Error>;
}

fn receipt(id: &str, state: &str) -> Result<String, BusinessError> {
    serde_json::to_string(&serde_json::json!({ "id": id, "state": state }))
        .map_err(|_| BusinessError::invalid("POS_COMMAND_INVALID", "Review the POS command."))
}

fn receipt_id(result_json: &str) -> Result<String, BusinessError> {
    let value: serde_json::Value = serde_json::from_str(result_json).map_err(|_| {
        business(
            FailureKind::Unavailable,
            "POS_SOURCE_INVALID",
            "The POS day close could not be read back.",
        )
    })?;
    value
        .get("id")
        .and_then(|id| id.as_str())
        .filter(|id| !id.is_empty() && id.len() <= 200)
        .map(str::to_owned)
        .ok_or_else(|| {
            business(
                FailureKind::Unavailable,
                "POS_SOURCE_INVALID",
                "The POS day close could not be read back.",
            )
        })
}

/// Execute one day-close command on the locked, authority-checked transaction.
/// `identity` binds the canonical command bytes and idempotency key. The caller
/// must recheck current authority and open the serializable transaction.
pub async fn execute_day_close<S, W>(
    actor: &PosActor,
    identity: &PosDayReceiptIdentity,
    command: &DayCloseCommand,
    store: &mut S,
    witness: &W,
    now: TimestampMillis,
) -> Result<(String, String), S::Error>
where
    S: DayCloseStore,
    W: DayCloseWitness<Error = S::Error>,
{
    store.lock_company(actor.company_id.as_str()).await?;
    if let Some(stored) = store.replay_receipt(identity).await? {
        return Ok((receipt_id(&stored)?, stored));
    }

    let result_json = match command {
        DayCloseCommand::Prepare {
            branch_id,
            business_date,
        } => {
            let report = witness
                .report(actor.company_id.as_str(), branch_id, business_date)
                .await?;
            let today = witness.today().await?;
            let already_closed = store
                .prepared_exists(actor.company_id.as_str(), branch_id, business_date)
                .await?;
            validate_prepare(
                actor,
                now,
                branch_id,
                business_date,
                &today,
                already_closed,
                &report.facts,
            )?;
            let id = witness.new_id().await?;
            let snapshot: serde_json::Value =
                serde_json::from_str(&report.snapshot_json).map_err(|_| {
                    business(
                        FailureKind::Unavailable,
                        "POS_SOURCE_INVALID",
                        "The POS day report snapshot is invalid.",
                    )
                })?;
            let close = NewDayClose {
                id: &id,
                company_id: actor.company_id.as_str(),
                branch_id,
                business_date,
                currency: &report.currency,
                snapshot: &snapshot,
                digest: &report.facts.digest,
                prepared_by: actor.user_id.as_str(),
                prepared_at: now.get(),
            };
            if !store.prepare(&close).await? {
                return Err(business(
                    FailureKind::Conflict,
                    "POS_DAY_CLOSED",
                    "This business day is already prepared.",
                )
                .into());
            }
            receipt(&id, "prepared")?
        }
        DayCloseCommand::Confirm {
            id,
            expected_version,
        } => {
            let close = store
                .lock_close(actor.company_id.as_str(), id)
                .await?
                .ok_or_else(|| {
                    business(
                        FailureKind::NotFound,
                        "POS_NOT_FOUND",
                        "Day close not found.",
                    )
                })?;
            let report = witness
                .report(
                    actor.company_id.as_str(),
                    &close.branch_id,
                    &close.business_date,
                )
                .await?;
            let same_person = witness
                .same_person(actor.user_id.as_str(), &close.prepared_by)
                .await?;
            validate_confirm(
                actor,
                now,
                &close,
                *expected_version,
                same_person,
                &report.facts,
            )?;
            if !store
                .confirm(
                    actor.company_id.as_str(),
                    id,
                    *expected_version,
                    actor.user_id.as_str(),
                    now.get(),
                )
                .await?
            {
                return Err(business(
                    FailureKind::Conflict,
                    "POS_VERSION_CONFLICT",
                    "Reload the current close.",
                )
                .into());
            }
            receipt(id, "confirmed")?
        }
    };

    store
        .insert_receipt(identity, &result_json, now.get())
        .await?;
    let record_id = receipt_id(&result_json)?;
    Ok((record_id, result_json))
}
