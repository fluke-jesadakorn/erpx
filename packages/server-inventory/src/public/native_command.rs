//! Public surface for the native Inventory command stack.
pub use crate::adapters::native_command_postgres::PgNativeCommandRepository;
pub use crate::adapters::native_command_sql::{
    NativeStatement, RD_ACTIVE_RESERVATIONS_TOTAL, RD_ATTACHMENT_DESCRIPTOR_LANDED_COST,
    RD_ATTACHMENT_DESCRIPTOR_LOT, RD_ATTACHMENT_DESCRIPTOR_STOCK_COUNT,
    RD_ATTACHMENT_DESCRIPTOR_TRANSFER, RD_ATTACHMENT_DESCRIPTOR_TRANSIT,
    RD_ATTACHMENT_OPERATIONAL_TRANSFER, RD_COMMERCE_ATTACHMENT_STOCK_IMAGE,
    RD_FULFILLMENT_LOT_ALLOCATION, RD_IN_TRANSIT_CURRENCY_TOTALS, RD_LEDGER_MOVEMENT_AFTER,
    RD_POS_STARTUP_INVENTORY_STATE,
    RD_LEDGER_SUPPLEMENT_ROW, RD_LEDGER_SUPPLEMENT_ROWS, RD_LOT_BALANCE_SUPPLEMENT_ROWS,
    RD_LOT_SUPPLEMENT_ROW, RD_STOCK_COUNT_DELTAS, RD_STOCK_COUNT_ROW, RD_STOCK_SUPPLEMENT_ROW,
    RD_TRANSIT_ATTACHMENT_DESCRIPTOR, is_read, read_statement, statement, supported,
};
pub use crate::application::native_command::{
    NativeCommandFuture, NativeCommandOutcome, NativeCommandRepository, admit,
    execute_native_command, plan,
};
pub use crate::domain::native_command::{NativeCommand, invalid, unavailable};
