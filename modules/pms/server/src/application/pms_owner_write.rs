//! Native PMS owner-write use case (wf1a2 full retirement slice).
//!
//! One typed command path executes every retained pms SQL write on a
//! caller-owned serializable Holding unit. The owner domain selects the exact
//! SQL by allow-listed kind; this layer owns idempotency, optimistic replay,
//! the receipt/audit/outbox ledger and command ordering. No other owner's rows
//! are written and no second transaction is opened.
use crate::domain::pms_owner_write::{PmsOwnerStatement, invalid, statement};
use erpx_platform_shared_server::public::{BusinessError, CompanyId, FailureKind};
use serde::Serialize;
use serde_json::{Value, json};
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type PmsOwnerWriteFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StoredOwnerReceipt {
    pub request_hash: String,
    pub result: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct OwnerWriteLedgerEntry<'a> {
    pub key: &'a str,
    pub kind: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub request_hash: &'a str,
    pub result: &'a Value,
    pub state: &'a str,
    pub now: i64,
}

#[derive(Clone, Debug)]
pub struct PmsOwnerWriteActor {
    pub user_id: String,
    pub assignment_id: String,
    pub company_id: String,
    pub authority_json: String,
}

pub trait PmsOwnerWriteRepository: Send {
    type Error: From<BusinessError> + Send;

    fn connection(&mut self) -> Option<&mut PgConnection>;
    fn lock_command<'a>(&'a mut self, key: &'a str) -> PmsOwnerWriteFuture<'a, (), Self::Error>;
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> PmsOwnerWriteFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> PmsOwnerWriteFuture<'a, Option<StoredOwnerReceipt>, Self::Error>;
    fn ledger<'a>(
        &'a mut self,
        entry: &'a OwnerWriteLedgerEntry<'a>,
    ) -> PmsOwnerWriteFuture<'a, (), Self::Error>;
    /// Execute the owner-selected statement on the caller's unit. Returns the
    /// first RETURNING row as JSON when the statement has one.
    fn execute_statement<'a>(
        &'a mut self,
        command: &'a PmsOwnerStatement,
    ) -> PmsOwnerWriteFuture<'a, Option<Value>, Self::Error>;
}

#[derive(Serialize)]
struct CommandIdentity<'a, I: Serialize + ?Sized> {
    user_id: &'a str,
    assignment_id: &'a str,
    company_id: &'a str,
    kind: &'a str,
    record_id: &'a str,
    values: &'a I,
}

fn native_key(key: &str, kind: &str) -> String {
    format!("pms-owner:{key}:{kind}")
}

async fn prepared<R: PmsOwnerWriteRepository>(
    actor: &PmsOwnerWriteActor,
    repository: &mut R,
    key: &str,
    kind: &str,
    record_id: &str,
    values: &[Value],
) -> Result<(String, Option<Value>), R::Error> {
    repository.lock_command(key).await?;
    let identity = serde_json::to_string(&CommandIdentity {
        user_id: &actor.user_id,
        assignment_id: &actor.assignment_id,
        company_id: &actor.company_id,
        kind,
        record_id,
        values,
    })
    .map_err(|_| R::Error::from(invalid()))?;
    let request_hash = repository.request_hash(&identity).await?;
    if let Some(receipt) = repository.receipt(key).await? {
        if receipt.request_hash != request_hash {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command was already used for a different write. Start a new action.",
            )
            .into());
        }
        return match receipt.result {
            Some(result) => Ok((request_hash, Some(result))),
            None => Err(BusinessError::new(
                FailureKind::Conflict,
                "COMMAND_INCOMPLETE",
                "This command did not finish. Retry the same action.",
            )
            .into()),
        };
    }
    Ok((request_hash, None))
}

/// Execute one owner write. The full-parity result is returned to the transport
/// so the retained TypeScript consumer needs no second SQL statement.
pub async fn execute<R: PmsOwnerWriteRepository>(
    actor: &PmsOwnerWriteActor,
    kind: &str,
    values: Vec<Value>,
    now: i64,
    key: &str,
    repository: &mut R,
) -> Result<Value, R::Error> {
    let command = statement(kind, values)?;
    if CompanyId::new(&actor.company_id).is_err() || actor.user_id.is_empty() {
        return Err(invalid().into());
    }
    let record_id = command
        .values
        .first()
        .map(|value| match value {
            Value::String(text) => text.clone(),
            other => other.to_string(),
        })
        .unwrap_or_else(|| kind.to_owned());
    let nk = native_key(key, kind);
    let (request_hash, prior) = prepared(actor, repository, &nk, kind, &record_id, &command.values).await?;
    if let Some(result) = prior {
        return serde_json::from_value(result).map_err(|_| R::Error::from(invalid()));
    }
    let row = repository.execute_statement(&command).await?;
    let result = json!({
        "ok": true,
        "kind": kind,
        "now": now,
        "key": key,
        "row": row,
    });
    repository
        .ledger(&OwnerWriteLedgerEntry {
            key: &nk,
            kind,
            record_id: &record_id,
            version: 1,
            request_hash: &request_hash,
            result: &result,
            state: "applied",
            now: if now > 0 { now } else { 1 },
        })
        .await?;
    Ok(result)
}
