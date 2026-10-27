use super::valuation::{safe, scaled_inventory_value};
use erpx_server_shared::public::{BusinessError, FailureKind};
use std::collections::BTreeSet;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TrackingMode {
    None,
    Lot,
    Serial,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct StockValuation {
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub version: i64,
    pub tracking_mode: TrackingMode,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LotAllocation {
    pub lot_id: String,
    pub quantity_milli: i64,
    pub balance_version: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct LotValuation {
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub version: i64,
    pub tracking_mode: TrackingMode,
    pub expiry_date: Option<String>,
    pub active: bool,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CostedLot {
    pub allocation: LotAllocation,
    pub value_minor: i64,
    /// Exact original source, retained by the eventual PostgreSQL owner adapter.
    pub before: LotValuation,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsumptionValuation {
    pub quantity_milli: i64,
    pub value_minor: i64,
    pub lots: Vec<CostedLot>,
    pub before: StockValuation,
}

pub fn require_stock(
    stock: Option<&StockValuation>,
    quantity: i64,
) -> Result<&StockValuation, BusinessError> {
    safe(quantity)?;
    if quantity <= 0 {
        return Err(BusinessError::invalid(
            "INVALID_CONSUMPTION_QUANTITY",
            "Use a positive safe consumption quantity.",
        ));
    }
    let Some(stock) = stock.filter(|s| s.quantity_milli >= quantity) else {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "PART_STOCK_UNAVAILABLE",
            "Choose available spare stock from the service branch warehouse.",
        ));
    };
    safe(stock.quantity_milli)?;
    safe(stock.value_minor)?;
    safe(stock.version)?;
    if stock.version <= 0 {
        return Err(BusinessError::invalid(
            "INVALID_STOCK_VERSION",
            "Use a positive current stock version.",
        ));
    }
    Ok(stock)
}

pub fn require_lot_allocations(
    stock: &StockValuation,
    quantity: i64,
    allocations: &[LotAllocation],
) -> Result<(), BusinessError> {
    if allocations.len() > 500 {
        return Err(BusinessError::invalid(
            "PART_LOT_ALLOCATION_REQUIRED",
            "Select the full spare quantity from distinct available lots or serials.",
        ));
    }
    if stock.tracking_mode == TrackingMode::None {
        if !allocations.is_empty() {
            return Err(BusinessError::invalid(
                "PART_LOT_NOT_ALLOWED",
                "This spare does not use lot or serial tracking.",
            ));
        }
        return Ok(());
    }
    let mut ids = BTreeSet::new();
    let mut sum = 0_i64;
    for lot in allocations {
        safe(lot.quantity_milli)?;
        safe(lot.balance_version)?;
        if lot.lot_id.is_empty()
            || lot.lot_id.len() > 200
            || lot.lot_id.chars().any(char::is_control)
            || lot.quantity_milli <= 0
            || lot.balance_version <= 0
            || !ids.insert(&lot.lot_id)
        {
            return Err(allocation_required());
        }
        sum = safe(
            sum.checked_add(lot.quantity_milli)
                .ok_or_else(allocation_required)?,
        )?;
    }
    if allocations.is_empty() || sum != quantity {
        return Err(allocation_required());
    }
    Ok(())
}

pub fn cost_lot(
    allocation: &LotAllocation,
    lot: Option<LotValuation>,
    entry_date: &str,
) -> Result<CostedLot, BusinessError> {
    let changed = || {
        BusinessError::new(
            FailureKind::Conflict,
            "PART_LOT_CHANGED",
            "Reload an available unexpired spare lot or whole serial.",
        )
    };
    let Some(lot) = lot else {
        return Err(changed());
    };
    safe(lot.quantity_milli)?;
    safe(lot.value_minor)?;
    safe(lot.version)?;
    if !lot.active
        || lot.version != allocation.balance_version
        || lot.quantity_milli < allocation.quantity_milli
        || lot
            .expiry_date
            .as_deref()
            .is_some_and(|d| !d.is_empty() && d < entry_date)
        || (lot.tracking_mode == TrackingMode::Serial && allocation.quantity_milli != 1000)
    {
        return Err(changed());
    }
    let value_minor = if allocation.quantity_milli == lot.quantity_milli {
        lot.value_minor
    } else {
        scaled_inventory_value(
            lot.value_minor,
            allocation.quantity_milli,
            lot.quantity_milli,
        )?
    };
    Ok(CostedLot {
        allocation: allocation.clone(),
        value_minor,
        before: lot,
    })
}

pub fn finish_consumption(
    stock: StockValuation,
    quantity: i64,
    lots: Vec<CostedLot>,
) -> Result<ConsumptionValuation, BusinessError> {
    let value = if stock.tracking_mode == TrackingMode::None {
        if quantity == stock.quantity_milli {
            stock.value_minor
        } else {
            scaled_inventory_value(stock.value_minor, quantity, stock.quantity_milli)?
        }
    } else {
        let mut value = 0_i64;
        for lot in &lots {
            value = safe(value.checked_add(lot.value_minor).ok_or_else(|| {
                BusinessError::invalid(
                    "INVENTORY_AMOUNT_OUT_OF_RANGE",
                    "Inventory values must fit exact safe integer units.",
                )
            })?)?;
        }
        value
    };
    if value <= 0 {
        return Err(BusinessError::invalid(
            "PART_VALUATION_REQUIRED",
            "Reconcile positive spare valuation before posting its service cost.",
        ));
    }
    Ok(ConsumptionValuation {
        quantity_milli: quantity,
        value_minor: value,
        lots,
        before: stock,
    })
}
fn allocation_required() -> BusinessError {
    BusinessError::invalid(
        "PART_LOT_ALLOCATION_REQUIRED",
        "Select the full spare quantity from distinct available lots or serials.",
    )
}
