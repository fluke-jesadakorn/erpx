//! Native Commerce owner-write executor use case.
//! The caller owns one serializable Holding transaction; the owner adapter
//! holds all SQL. Receipt/audit/outbox are written in the same unit.
use crate::domain::owner_write::{OWNER_WRITE_OPERATION, OwnerWrite, invalid, parse_owner_write};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind, TimestampMillis};
use serde_json::Value;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type OwnerWriteFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StoredReceipt {
    pub request_hash: String,
    pub result: Option<Value>,
}
#[derive(Clone, Debug)]
pub struct LedgerEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'static str,
    pub record_id: &'a str,
    pub actor_id: &'a str,
    pub request_hash: &'a str,
    pub result: &'a Value,
    pub now: i64,
}

pub trait OwnerWriteRepository: Send {
    type Error: From<BusinessError> + Send;
    fn connection(&mut self) -> Option<&mut PgConnection>;
    fn lock_command<'a>(&'a mut self, key: &'a str) -> OwnerWriteFuture<'a, (), Self::Error>;
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> OwnerWriteFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> OwnerWriteFuture<'a, Option<StoredReceipt>, Self::Error>;
    fn ledger<'a>(
        &'a mut self,
        entry: &'a LedgerEntry<'a>,
    ) -> OwnerWriteFuture<'a, (), Self::Error>;
    fn apply<'a>(&'a mut self, write: &'a OwnerWrite) -> OwnerWriteFuture<'a, (), Self::Error>;
}

fn identity(operation: &str, actor: &str, input: &Value) -> String {
    serde_json::json!({"operation":operation,"actorId":actor,"input":input}).to_string()
}
fn command_key(key: Option<&CommandKey>) -> Result<&CommandKey, BusinessError> {
    key.ok_or_else(|| {
        BusinessError::invalid("COMMAND_KEY_REQUIRED", "Use a valid Idempotency-Key.")
    })
}

pub async fn execute_owner_write<R: OwnerWriteRepository>(
    actor_id: &str,
    raw: &Value,
    key: Option<&CommandKey>,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    let key = command_key(key).map_err(R::Error::from)?;
    let write = parse_owner_write(raw).map_err(R::Error::from)?;
    repository.lock_command(key.as_str()).await?;
    let request_hash = repository
        .request_hash(&identity(OWNER_WRITE_OPERATION, actor_id, raw))
        .await?;
    if let Some(receipt) = repository.receipt(key.as_str()).await? {
        if receipt.request_hash != request_hash {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command was already used for a different Commerce write.",
            )
            .into());
        }
        return match receipt.result {
            Some(result) => Ok(result),
            None => Err(BusinessError::new(
                FailureKind::Conflict,
                "COMMAND_INCOMPLETE",
                "This command did not finish. Retry the same action.",
            )
            .into()),
        };
    }
    let result = serde_json::json!({ "id": write.record_id, "kind": write.kind });
    repository
        .ledger(&LedgerEntry {
            key: key.as_str(),
            operation: OWNER_WRITE_OPERATION,
            record_type: write.record_type,
            record_id: &write.record_id,
            actor_id,
            request_hash: &request_hash,
            result: &result,
            now: now.get(),
        })
        .await?;
    repository.apply(&write).await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn command_key_is_required() {
        assert!(command_key(None).is_err());
        let key = CommandKey::new("commerce-owner-key-1").unwrap();
        assert!(command_key(Some(&key)).is_ok());
    }
}
