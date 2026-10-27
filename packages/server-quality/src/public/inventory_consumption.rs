pub use crate::adapters::postgres_inventory_consumption::{
    ConsumptionHoldRequest, CurrentConsumptionHolds, lock_consumption_sources,
    read_consumption_holds,
};

pub use crate::adapters::postgres_inventory_consumption::{
    CurrentRawConsumptionHolds, RawConsumptionHoldRequest, read_consumption_holds_raw,
};
