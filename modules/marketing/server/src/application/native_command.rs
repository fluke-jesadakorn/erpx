//! Application use case for the native marketing command path.
use crate::adapters::native_command_sql::{NativeStatement, statement};
use crate::domain::native_command::{self, CommandPlan, NativeCommand};
use erpx_platform_shared_server::public::BusinessError;
use serde_json::Value;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type NativeCommandFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

/// Result of one admitted statement. `rows` carries the actual RETURNING rows
/// (empty for statements without RETURNING) so callers can read returned values
/// such as the public-submission rate-limit count.
#[derive(Clone, Debug, Default)]
pub struct NativeCommandOutcome {
    pub kind: String,
    pub rows: Vec<Value>,
}

pub trait NativeCommandRepository: Send {
    fn connection(&mut self) -> Option<&mut PgConnection>;
    fn execute<'a>(
        &'a mut self,
        sql: &'static str,
        values: Vec<serde_json::Value>,
        key: &'a str,
    ) -> NativeCommandFuture<'a, Vec<Value>>;
}

pub fn admit(command: &NativeCommand) -> Result<&'static NativeStatement, BusinessError> {
    statement(&command.kind).ok_or_else(native_command::invalid)
}
pub fn plan<'a>(command: &'a NativeCommand) -> Result<CommandPlan<'a>, BusinessError> {
    command.plan(admit(command)?)
}

pub fn execute_native_command<'a, R: NativeCommandRepository>(
    command: &'a NativeCommand,
    key: &'a str,
    repository: &'a mut R,
) -> NativeCommandFuture<'a, NativeCommandOutcome> {
    Box::pin(async move {
        let admitted = admit(command)?;
        let checked = command.plan(admitted)?;
        let values = checked.values.to_vec();
        let rows = repository.execute(admitted.sql, values, key).await?;
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

    struct ReturningRepository(Vec<Value>);
    impl NativeCommandRepository for ReturningRepository {
        fn connection(&mut self) -> Option<&mut PgConnection> {
            None
        }
        fn execute<'a>(
            &'a mut self,
            _sql: &'static str,
            _values: Vec<Value>,
            _key: &'a str,
        ) -> NativeCommandFuture<'a, Vec<Value>> {
            let rows = self.0.clone();
            Box::pin(async move { Ok(rows) })
        }
    }

    #[tokio::test]
    async fn outcome_carries_statement_returning_rows() {
        let command = NativeCommand {
            kind: "marketing_stmt_1".to_owned(),
            values: vec![json!("rate-key"), json!(6)],
            now: 0,
        };
        let mut repository = ReturningRepository(vec![json!({"count": 6})]);
        let outcome = execute_native_command(&command, "marketing-key", &mut repository)
            .await
            .expect("admitted statement executes");
        assert_eq!(outcome.kind, "marketing_stmt_1");
        assert_eq!(outcome.rows, vec![json!({"count": 6})]);
    }
}
