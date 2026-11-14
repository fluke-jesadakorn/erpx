//! Same-unit Commerce master primitives. These carry no authority by themselves.
pub use crate::adapters::service_stock_masters::{
    CurrentServiceStockMapping, lock_service_stock_mapping, read_service_stock_mapping,
};
pub use crate::adapters::service_stock_masters::{
    CurrentServiceStockMaster, PgServiceStockMasterSource, PreparedServiceStockMaster,
    lock_service_stock_masters, read_service_stock_master,
};
pub use crate::domain::service_stock_masters::{ServiceStockMaster, ServiceStockMasterIntent};
pub use crate::domain::service_stock_masters::{
    ServiceStockWorkspacePolicy, service_stock_workspace_policy,
};
