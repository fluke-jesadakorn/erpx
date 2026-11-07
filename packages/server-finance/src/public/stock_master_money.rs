//! Finance-owned settings witness and product price Money receipt for Stock masters.
pub use crate::adapters::stock_master_money::{
    CurrentStockMasterFinance, CurrentStockMasterProductMoney, PreparedStockMasterProductMoney,
    RetainedStockMasterProductMoney, StockMasterProductMoneyEvidence, StockMasterWorkspaceChart,
    lock_stock_master_finance_sources, prepare_stock_master_product_money,
    read_stock_master_finance, read_stock_master_product_money, read_stock_master_workspace_chart,
};
