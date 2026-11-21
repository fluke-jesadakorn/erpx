//! Native Commerce wave2 warehouse-master command public facade.
pub use crate::adapters::wave2_postgres::PgWarehouseSave;
pub use crate::application::wave2::{
    StoredWarehouseReceipt, WarehouseLedgerEntry, WarehouseSaveFuture, WarehouseSaveRepository,
    WarehouseWriteKind, save_warehouse,
};
pub use crate::domain::wave2::{
    WAREHOUSE_RECORD_TYPE, WAREHOUSE_SAVE_OPERATION, WarehouseSaveInput, WarehouseSavePlan,
};
