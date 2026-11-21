//! Public surface for the native runtime command stack.
pub use crate::adapters::native_command_postgres::PgNativeCommandRepository;
pub use crate::adapters::native_command_sql::{
    NativeStatement, is_dynamic_statement, statement, supported,
};
pub use crate::application::native_command::{
    NativeCommandFuture, NativeCommandOutcome, NativeCommandRepository, admit,
    execute_dynamic_command, execute_native_command, plan,
};
pub use crate::domain::feature_predicate::{
    FeaturePredicateConfig, FeaturePredicatePort, interruption_sql, lease_claim_sql,
    scheduled_predicate,
};
pub use crate::domain::native_command::{NativeCommand, invalid, unavailable};
