//! Native Commercial wave2 grant-save command use case.
//!
//! The caller owns one serializable Holding transaction. This layer validates
//! the retained input, replays an exact idempotency receipt, orders the owner
//! grant write together with the command receipt, audit event and outbox event
//! in the same unit. It never opens a transaction and never touches another
//! owner's rows.
pub use crate::domain::wave2::GrantSaveInput;
use crate::domain::wave2::{GRANT_RECORD_TYPE, GRANT_SAVE_OPERATION, invalid};
use erpx_platform_shared_server::public::{BusinessError, CommandKey, FailureKind, TimestampMillis};
use serde::Serialize;
use serde_json::Value;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type GrantSaveFuture<'a, T, E> = Pin<Box<dyn Future<Output = Result<T, E>> + Send + 'a>>;

#[derive(Clone, Debug)]
pub struct StoredGrantReceipt {
    pub request_hash: String,
    pub result: Option<Value>,
}

/// One command receipt, audit event and outbox event committed inside the
/// caller-owned transaction as the grant row change (CQRS command side).
#[derive(Clone, Debug)]
pub struct GrantLedgerEntry<'a> {
    pub key: &'a str,
    pub operation: &'a str,
    pub record_type: &'a str,
    pub record_id: &'a str,
    pub version: i64,
    pub actor_id: &'a str,
    pub request_hash: &'a str,
    pub result: &'a Value,
    pub now: i64,
}

/// Owner repository port. The adapter is the only place allowed to hold SQL.
pub trait GrantSaveRepository: Send {
    type Error: From<BusinessError> + Send;

    fn connection(&mut self) -> Option<&mut PgConnection>;

    fn lock_command<'a>(&'a mut self, key: &'a str) -> GrantSaveFuture<'a, (), Self::Error>;
    fn request_hash<'a>(
        &'a mut self,
        identity: &'a str,
    ) -> GrantSaveFuture<'a, String, Self::Error>;
    fn receipt<'a>(
        &'a mut self,
        key: &'a str,
    ) -> GrantSaveFuture<'a, Option<StoredGrantReceipt>, Self::Error>;
    fn ledger<'a>(
        &'a mut self,
        entry: &'a GrantLedgerEntry<'a>,
    ) -> GrantSaveFuture<'a, (), Self::Error>;
    fn save_grant<'a>(
        &'a mut self,
        input: &'a GrantSaveInput,
    ) -> GrantSaveFuture<'a, (), Self::Error>;
}

#[derive(Serialize)]
struct CommandIdentity<'a> {
    operation: &'a str,
    actor_id: &'a str,
    record_id: String,
    input: &'a Value,
}

fn command_key(key: Option<&CommandKey>) -> Result<&CommandKey, BusinessError> {
    key.ok_or_else(|| {
        BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "Use a valid Idempotency-Key for this action.",
        )
    })
}

/// Validate and apply one Company package grant save. `actor_id` is the caller-
/// owned identity resolved by composition; it is written on the audit row.
pub async fn save_grant<R: GrantSaveRepository>(
    actor_id: &str,
    raw: &Value,
    key: Option<&CommandKey>,
    repository: &mut R,
    now: TimestampMillis,
) -> Result<Value, R::Error> {
    let key = command_key(key).map_err(R::Error::from)?;
    let input = GrantSaveInput::parse(raw).map_err(R::Error::from)?;
    let plan = input.plan().map_err(R::Error::from)?;
    repository.lock_command(key.as_str()).await?;
    let identity = serde_json::to_string(&CommandIdentity {
        operation: GRANT_SAVE_OPERATION,
        actor_id,
        record_id: input.record_id(),
        input: raw,
    })
    .map_err(|_| R::Error::from(invalid()))?;
    let request_hash = repository.request_hash(&identity).await?;
    if let Some(receipt) = repository.receipt(key.as_str()).await? {
        if receipt.request_hash != request_hash {
            return Err(BusinessError::new(
                FailureKind::Conflict,
                "IDEMPOTENCY_CONFLICT",
                "This command was already used for a different grant write.",
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
        "companyId": input.company_id,
        "packageId": input.package_id,
        "version": plan.next_version,
    });
    repository
        .ledger(&GrantLedgerEntry {
            key: key.as_str(),
            operation: GRANT_SAVE_OPERATION,
            record_type: GRANT_RECORD_TYPE,
            record_id: &input.company_id,
            version: plan.next_version,
            actor_id,
            request_hash: &request_hash,
            result: &result,
            now: now.get(),
        })
        .await?;
    repository.save_grant(&input).await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn command_key_is_required() {
        assert!(command_key(None).is_err());
        let key = CommandKey::new("grant-save-key-1").unwrap();
        assert!(command_key(Some(&key)).is_ok());
    }

    #[test]
    fn identity_is_namespace_bound() {
        let value = json!({"companyId":"c","packageId":"p"});
        let id = serde_json::to_string(&CommandIdentity {
            operation: GRANT_SAVE_OPERATION,
            actor_id: "u",
            record_id: "c:p".to_owned(),
            input: &value,
        })
        .unwrap();
        assert!(id.contains("commercial.wave2.grant.save"));
        assert!(id.contains("c:p"));
    }
}
