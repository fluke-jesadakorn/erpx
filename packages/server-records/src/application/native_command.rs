//! Application use case for the native Records command path.
use crate::adapters::native_command_sql::{NativeStatement, statement};
use crate::domain::native_command::{self, CommandPlan, NativeCommand};
use erpx_server_shared::public::BusinessError;
use sqlx::PgConnection;
use std::future::Future;
use std::pin::Pin;

pub type NativeCommandFuture<'a, T> =
    Pin<Box<dyn Future<Output = Result<T, BusinessError>> + Send + 'a>>;

#[derive(Clone, Debug, Default)]
pub struct NativeCommandOutcome {
    pub kind: String,
    pub rows: u64,
}

pub trait NativeCommandRepository: Send {
    fn connection(&mut self) -> Option<&mut PgConnection>;
    fn execute<'a>(
        &'a mut self,
        sql: &'static str,
        values: Vec<serde_json::Value>,
        key: &'a str,
    ) -> NativeCommandFuture<'a, u64>;
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
