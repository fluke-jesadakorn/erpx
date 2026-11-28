//! Current stock-register projection and its source fence.
pub use crate::adapters::stock_register_read::{lock_stock_register_sources, read_stock_register};
pub use crate::domain::stock_register::{
    MAX_COUNT_ROWS, MAX_LOT_ROWS, MAX_STOCK_ROWS, StockRegister, StockRegisterCount,
    StockRegisterLot, StockRegisterRow, StockRegisterTotals, validate_count, validate_lot,
    validate_register, validate_row,
};
