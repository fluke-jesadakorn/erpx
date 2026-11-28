//! Native Commerce wave2 warehouse-master command use case.
//!
//! The caller owns one serializable Holding transaction. This layer validates
//! the retained input, replays an exact idempotency receipt, and orders the
//! owner write together with the command receipt, audit event and outbox event
//! in the same unit. No other owner's rows are written by this use case.
pub use crate::domain::wave2::WarehouseSaveInput;
use crate::domain::wave2::{
    WAREHOUSE_RECORD_TYPE, WAREHOUSE_SAVE_OPERATION, WarehouseSavePlan, invalid,
};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind, TimestampMillis};
use serde_json::Value;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type WarehouseSaveFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StoredWarehouseReceipt {
    pub request_hash: String,
    pub result: Option<Value>,
}

#[derive(Clone, Debug)]
pub struct WarehouseLedgerEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'static str,
    pub record_id: &'a str,
    pub actor_id: &'a str,
    pub request_hash: &'a str,
    pub result: &'a Value,
    pub now: i64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum WarehouseWriteKind {
    Create,
    Update,
}

/// Owner repository port. The adapter is the only place allowed to hold SQL.
pub trait WarehouseSaveRepository: Send {
    type Error: From<BusinessError> + Send;

    fn connection(&mut self) -> Option<&mut PgConnection>;
    fn lock_command<'a>(&'a mut self, key: &'a str) -> WarehouseSaveFuture<'a, (), Self::Error>;
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> WarehouseSaveFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> WarehouseSaveFuture<'a, Option<StoredWarehouseReceipt>, Self::Error>;
    fn ledger<'a>(
        &'a mut self,
        entry: &'a WarehouseLedgerEntry<'a>,
    ) -> WarehouseSaveFuture<'a, (), Self::Error>;
    fn write_warehouse<'a>(
        &'a mut self,
        input: &'a WarehouseSaveInput,
        kind: WarehouseWriteKind,
    ) -> WarehouseSaveFuture<'a, (), Self::Error>;
}

fn command_identity(operation: &str, actor_id: &str, input: &Value) -> String {
    serde_json::json!({
        "operation": operation,
        "actorId": actor_id,
        "input": input,
    })
    .to_string()
}

fn command_key(key: Option<&CommandKey>) -> Result<&CommandKey, BusinessError> {
    key.ok_or_else(|| {
        BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "Use a valid Idempotency-Key for this action.",
        )
    })
}

pub async fn save_warehouse<R: WarehouseSaveRepository>(
    actor_id: &str,
    raw: &Value,
    key: Option<&CommandKey>,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    let key = command_key(key).map_err(R::Error::from)?;
    let input = WarehouseSaveInput::parse(raw).map_err(R::Error::from)?;
    let plan = input.plan();
    let write_kind = match plan {
        WarehouseSavePlan::Create => WarehouseWriteKind::Create,
        WarehouseSavePlan::Update => WarehouseWriteKind::Update,
    };
    repository.lock_command(key.as_str()).await?;
    let identity = command_identity(WAREHOUSE_SAVE_OPERATION, actor_id, raw);
    let request_hash = repository.request_hash(&identity).await?;
    if let Some(receipt) = repository.receipt(key.as_str()).await? {
        if receipt.request_hash != request_hash {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command was already used for a different warehouse write.",
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
    let result = serde_json::json!({
        "id": input.record_id(),
        "companyId": input.company_id,
    });
    repository
        .ledger(&WarehouseLedgerEntry {
            key: key.as_str(),
            operation: WAREHOUSE_SAVE_OPERATION,
            record_type: WAREHOUSE_RECORD_TYPE,
            record_id: input.record_id(),
            actor_id,
            request_hash: &request_hash,
            result: &result,
            now: now.get(),
        })
        .await?;
    repository.write_warehouse(&input, write_kind).await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_key_is_required() {
        assert!(command_key(None).is_err());
        let key = CommandKey::new("warehouse-save-key-1").unwrap();
        assert!(command_key(Some(&key)).is_ok());
    }
}
