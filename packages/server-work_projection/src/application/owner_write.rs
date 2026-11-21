//! Owner-wide write use case. The caller owns the transaction; this layer
//! validates the typed command and delegates execution plus the receipt/audit/
//! outbox ledger to the owner port. No SQL and no TS mirror exist here.
use crate::domain::owner_write_registry::{returning, validate};
use erpx_server_shared::public::BusinessError;
use serde_json::{Value, json};
use std::future::Future;

#[derive(Clone, Debug)]
pub struct OwnerWriteCommand<'a> {
    pub kind: &'a str,
    pub values: &'a [Value],
    pub company_id: &'a str,
    pub actor_id: &'a str,
    pub now: i64,
}

pub trait OwnerWritePort: Send {
    type Error: From<BusinessError> + Send;

    fn execute<'a>(
        &'a mut self,
        command: &'a OwnerWriteCommand<'a>,
        key: &'a str,
    ) -> impl Future<Output = Result<Value, Self::Error>> + Send + 'a;
}

pub async fn execute_owner_write<P: OwnerWritePort>(
    repository: &mut P,
    command: &OwnerWriteCommand<'_>,
    key: &str,
) -> Result<Value, P::Error> {
    validate(command.kind, command.values.len()).map_err(P::Error::from)?;
    if key.is_empty() || key.len() > 180 {
        return Err(BusinessError::invalid(
            "COMMAND_KEY_REQUIRED",
            "Use a valid Idempotency-Key for this action.",
        )
        .into());
    }
    let result = repository.execute(command, key).await?;
    Ok(json!({
        "ok": true,
        "kind": command.kind,
        "result": result,
        "returning": returning(command.kind),
    }))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::owner_write_registry::WP_CAL_SERIES_INSERT;

    struct Reject;
    impl OwnerWritePort for Reject {
        type Error = BusinessError;
        async fn execute(&mut self, _: &OwnerWriteCommand<'_>, _: &str) -> Result<Value, Self::Error> {
            panic!("invalid command must not reach the port")
        }
    }

    #[tokio::test]
    async fn rejects_bad_kind_before_port() {
        let command = OwnerWriteCommand {
            kind: "unknown",
            values: &[],
            company_id: "c",
            actor_id: "a",
            now: 1,
        };
        assert!(execute_owner_write(&mut Reject, &command, "key-1").await.is_err());
    }

    #[test]
    fn arity_is_enforced() {
        let values = vec![json!(1); 8];
        let command = OwnerWriteCommand {
            kind: WP_CAL_SERIES_INSERT,
            values: &values,
            company_id: "c",
            actor_id: "a",
            now: 1,
        };
        assert!(validate(command.kind, command.values.len()).is_ok());
    }
}
