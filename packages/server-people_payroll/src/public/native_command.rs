//! Public surface of the native people-payroll command slice for composition.
//! Composition owns the transaction, current actor and ledger; this crate owns
//! the domain, use case and the only SQL adapter.
pub use crate::adapters::native_command_postgres::PgNativeCommandRepository;
pub use crate::adapters::native_command_sql::{NativeStatement, statement, statement_count};
pub use crate::application::native_command::{
    NativeCommandFuture, NativeCommandOutcome, NativeCommandRepository, admit,
    execute_native_command, plan,
};
pub use crate::domain::native_command::{NativeCommand, context_invalid, invalid, unavailable};
