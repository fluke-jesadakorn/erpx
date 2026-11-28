//! Application use case for the native people-payroll command path.
//!
//! The use case validates the pure domain plan, admits the kind against the
//! owner allow-list, then delegates persistence to an owner repository port.
//! The serializable transaction, current actor and receipt/audit/outbox ledger
//! are owned by composition and are not duplicated here (CQRS command side:
//! this path only mutates state).
use crate::adapters::native_command_sql::{NativeStatement, statement};
use crate::domain::native_command::{self, CommandPlan, NativeCommand};
use erpx_platform_shared_server::public::BusinessError;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

/// Boxed future returned by the owner repository port.
pub type NativeCommandFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Outcome returned to the composition caller. Row data is intentionally absent:
/// retained native writes are command effects; reads stay on the existing query
/// path. `rows` is the affected-row count reported by the adapter.
#[derive(Clone, Debug, Default)]
pub struct NativeCommandOutcome {
    pub kind: String,
    pub rows: u64,
}

/// Owner repository port, implemented by the Postgres adapter in this crate.
/// It receives only an admitted statement and its positional values; it never
/// sees a raw kind string that has not passed the domain allow-list.
pub trait NativeCommandRepository: Send {
    /// The caller-owned transaction connection. Real adapters return it so the
    /// write and the receipt/audit/outbox ledger share one unit; local fakes
    /// may return `None`.
    fn connection(&mut self) -> Option<&mut PgConnection>;

    fn execute<'a>(
        &'a mut self,
        sql: &'static str,
        values: Vec<serde_json::Value>,
        key: &'a str,
    ) -> NativeCommandFuture<'a, u64>;
}

/// Resolve the allow-listed statement for a command. Shared by the HTTP ingress
/// and tests so there is a single admission decision.
pub fn admit(command: &NativeCommand) -> Result<&'static NativeStatement, BusinessError> {
    statement(&command.kind).ok_or_else(native_command::invalid)
}

/// Plan a command against its admitted statement. Pure domain work, no I/O.
pub fn plan<'a>(command: &'a NativeCommand) -> Result<CommandPlan<'a>, BusinessError> {
    command.plan(admit(command)?)
}

/// Execute one admitted native command on the caller's unit of work.
pub fn execute_native_command<'a, R: NativeCommandRepository>(
    command: &'a NativeCommand,
    key: &'a str,
    repository: &'a mut R,
) -> NativeCommandFuture<'a, NativeCommandOutcome> {
    Box::pin(async move {
        let admitted = admit(command)?;
        // Validate the pure domain plan (kind + bind arity) before any I/O.
        let checked = command.plan(admitted)?;
        let sql = admitted.sql;
        let values = checked.values.to_vec();
        let rows = repository.execute(sql, values, key).await?;
        Ok(NativeCommandOutcome {
            kind: command.kind.clone(),
            rows,
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    struct Counting(u64);
    impl NativeCommandRepository for Counting {
        fn connection(&mut self) -> Option<&mut PgConnection> {
            None
        }
        fn execute<'a>(
            &'a mut self,
            sql: &'static str,
            values: Vec<serde_json::Value>,
            _: &'a str,
        ) -> NativeCommandFuture<'a, u64> {
            let verb = sql.trim_start();
            assert!(
                verb.starts_with("INSERT")
                    || verb.starts_with("UPDATE")
                    || verb.starts_with("DELETE")
            );
            assert_eq!(values.len(), 2);
            self.0 += 1;
            Box::pin(async { Ok(1) })
        }
    }

    #[tokio::test]
    async fn admits_a_full_parity_payload_and_executes_once() {
        // tech_provision_stmt_4: UPDATE people_provision_operations SET steps=$2 WHERE id=$1
        let command = NativeCommand::parse(
            &json!({"kind":"tech_provision_stmt_4","values":["id",[]],"now":1}),
        )
        .unwrap();
        let mut repo = Counting(0);
        let outcome = execute_native_command(&command, "key-0001", &mut repo)
            .await
            .unwrap();
        assert_eq!(outcome.kind, "tech_provision_stmt_4");
        assert_eq!(outcome.rows, 1);
        assert_eq!(repo.0, 1);
    }

    #[tokio::test]
    async fn rejects_unknown_kind_and_wrong_arity() {
        let mut repo = Counting(0);
        let unknown = NativeCommand::parse(&json!({"kind":"not_real","values":[1]})).unwrap();
        assert!(
            execute_native_command(&unknown, "key-0002", &mut repo)
                .await
                .is_err()
        );
        let wrong =
            NativeCommand::parse(&json!({"kind":"tech_provision_stmt_4","values":["id"]})).unwrap();
        assert!(
            execute_native_command(&wrong, "key-0003", &mut repo)
                .await
                .is_err()
        );
        assert_eq!(repo.0, 0);
    }
}
