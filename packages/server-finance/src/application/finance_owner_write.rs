//! Native Finance owner-write use case.
//!
//! The use case validates the typed kind and positional values against the
//! domain allow-list, admits canonical SQL through the owner port, and delegates
//! execution to the owner repository port on the caller-owned serializable unit.
//! It never accepts SQL text from a caller; a pure `build` helper remains for
//! tooling that must inspect an admitted statement without executing it.
use crate::domain::finance_owner_write::{OwnerStatement, invalid, unsupported_kind, validate};
use erpx_server_runtime::public::http_support::serde_json::Value;
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

/// Boxed future returned by the owner repository port.
pub type OwnerWriteFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Outcome of one executed owner command. Row data is intentionally absent:
/// retained native writes are command effects; reads stay on the query path.
#[derive(Clone, Debug, Default, serde::Serialize)]
pub struct OwnerWriteOutcome {
    pub kind: String,
    pub rows: u64,
}

/// Owner repository port implemented by the Postgres adapter in this crate.
/// It receives an admitted statement and its full-parity positional payload;
/// it never sees raw SQL text from a caller.
pub trait OwnerWriteRepository: Send {
    /// Caller-owned transaction connection so the DML and the
    /// receipt/audit/outbox ledger share one unit. Fakes may return `None`.
    fn connection(&mut self) -> Option<&mut PgConnection>;

    fn execute<'a>(
        &'a mut self,
        kind: &'a str,
        values: Vec<Value>,
        key: &'a str,
        now: i64,
    ) -> OwnerWriteFuture<'a, u64>;
}

/// Owner port supplying canonical SQL for an allow-listed kind.
pub trait OwnerStatementPort: Send + Sync {
    /// Canonical SQL for a kind and an exact positional value count. Returning
    /// `None` fails the command closed; no caller SQL is ever used.
    fn sql(&self, kind: &str, value_count: usize) -> Option<String>;
}

pub struct FinanceOwnerWrite<P: OwnerStatementPort> {
    port: P,
}

impl<P: OwnerStatementPort> FinanceOwnerWrite<P> {
    pub fn new(port: P) -> Self {
        Self { port }
    }

    /// Build the authoritative statement for one validated command.
    pub fn build(&self, kind: &str, values: Vec<Value>) -> Result<OwnerStatement, BusinessError> {
        validate(kind, &values)?;
        let sql = self
            .port
            .sql(kind, values.len())
            .ok_or_else(unsupported_kind)?;
        if sql.matches('?').count() != values.len() {
            return Err(invalid());
        }
        Ok(OwnerStatement {
            sql: sql.to_owned(),
            values,
        })
    }
}

/// Execute one admitted owner command on the caller's unit of work. Admission
/// (kind allow-list, bind arity, value bounds) happens before any I/O.
pub fn execute_command<'a, R: OwnerWriteRepository>(
    kind: &'a str,
    values: Vec<Value>,
    key: &'a str,
    now: i64,
    repository: &'a mut R,
) -> OwnerWriteFuture<'a, OwnerWriteOutcome> {
    Box::pin(async move {
        validate(kind, &values)?;
        let rows = repository.execute(kind, values, key, now).await?;
        Ok(OwnerWriteOutcome {
            kind: kind.to_owned(),
            rows,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_server_runtime::public::http_support::json;

    struct Port;
    impl OwnerStatementPort for Port {
        fn sql(&self, kind: &str, _value_count: usize) -> Option<String> {
            match kind {
                "two" => Some("UPDATE t SET a=? WHERE id=?".to_owned()),
                "zero" => Some("DELETE FROM t".to_owned()),
                _ => None,
            }
        }
    }

    #[test]
    fn builds_binds_in_order_and_rejects_unknown_kinds() {
        let use_case = FinanceOwnerWrite::new(Port);
        let built = use_case.build("two", vec![json!(7), json!("id")]).unwrap();
        assert_eq!(built.values, vec![json!(7), json!("id")]);
        assert!(use_case.build("zero", vec![]).is_ok());
        assert!(use_case.build("nope", vec![json!(1)]).is_err());
        assert!(use_case.build("two", vec![json!(1)]).is_err());
    }
}
