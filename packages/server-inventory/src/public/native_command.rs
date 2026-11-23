//! Public surface for the native Inventory command stack.
pub use crate::adapters::native_command_postgres::PgNativeCommandRepository;
pub use crate::adapters::native_command_sql::{
    NativeStatement, RD_LEDGER_SUPPLEMENT_ROW, RD_LEDGER_SUPPLEMENT_ROWS,
    RD_LOT_BALANCE_SUPPLEMENT_ROWS, RD_LOT_SUPPLEMENT_ROW, RD_STOCK_COUNT_DELTAS,
    RD_STOCK_COUNT_ROW, RD_STOCK_SUPPLEMENT_ROW, is_read, read_statement, statement, supported,
};
pub use crate::application::native_command::{
    NativeCommandFuture, NativeCommandOutcome, NativeCommandRepository, admit,
    execute_native_command, plan,
};
pub use crate::domain::native_command::{NativeCommand, invalid, unavailable};
