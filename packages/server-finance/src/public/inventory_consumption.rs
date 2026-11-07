//! Finance facts only. Callers retain Commerce mapping, source-event and personal authority separately.
pub use crate::adapters::postgres_inventory_consumption::{
    ConsumptionFinanceConfig, CurrentConsumptionFinance, InventoryConsumptionMapping,
    PreparedConsumptionMoney, RetainedConsumptionMoney, lock_inventory_consumption_finance_sources,
    prepare_consumption_stock_cost_money, read_inventory_consumption_finance,
};
