//! Public surface for the native crm command stack.
pub use crate::adapters::native_command_postgres::PgNativeCommandRepository;
pub use crate::adapters::native_command_sql::{NativeStatement, statement, supported};
pub use crate::application::native_command::{
    NativeCommandFuture, NativeCommandOutcome, NativeCommandRepository, admit,
    execute_native_command, plan,
};
pub use crate::domain::native_command::{NativeCommand, invalid, unavailable};
