use erpx_server_shared::public::{BusinessError, FailureKind};

pub const MAX_SAFE_UNITS: i64 = 9_007_199_254_740_991;

pub(crate) fn safe(value: i64) -> Result<i64, BusinessError> {
    if !(-MAX_SAFE_UNITS..=MAX_SAFE_UNITS).contains(&value) {
        return Err(BusinessError::invalid(
            "INVENTORY_AMOUNT_OUT_OF_RANGE",
            "Inventory values must fit exact safe integer units.",
        ));
    }
    Ok(value)
}

/// Match the existing positive-denominator cumulative allocation, including its
/// truncation toward zero for signed values. Do not substitute Finance's
/// largest-remainder allocation or half-away rounding here.
pub fn scaled_inventory_value(
    amount: i64,
    numerator: i64,
    denominator: i64,
) -> Result<i64, BusinessError> {
    safe(amount)?;
    safe(numerator)?;
    safe(denominator)?;
    if numerator < 0 || denominator <= 0 {
        return Err(BusinessError::invalid(
            "INVENTORY_ALLOCATION_INVALID",
            "Use a nonnegative quantity and positive allocation basis.",
        ));
    }
    let value = (i128::from(amount) * i128::from(numerator) + i128::from(denominator / 2))
        / i128::from(denominator);
    let value = i64::try_from(value).map_err(|_| {
        BusinessError::invalid(
            "INVENTORY_AMOUNT_OUT_OF_RANGE",
            "Inventory values must fit exact safe integer units.",
        )
    })?;
    safe(value)
}

/// Ordered cumulative allocation; the last source receives the exact residual.
/// A source row's position is material and is never sorted by identifier.
pub fn allocate_landed_cost(total: i64, weights: &[i64]) -> Result<Vec<i64>, BusinessError> {
    safe(total)?;
    let mut sum = 0_i64;
    for &weight in weights {
        safe(weight)?;
        if weight < 0 {
            return Err(BusinessError::invalid(
                "INVENTORY_ALLOCATION_INVALID",
                "Use nonnegative allocation weights.",
            ));
        }
        sum = sum.checked_add(weight).ok_or_else(|| {
            BusinessError::invalid(
                "INVENTORY_AMOUNT_OUT_OF_RANGE",
                "Inventory values must fit exact safe integer units.",
            )
        })?;
        safe(sum)?;
    }
    if sum <= 0 {
        return Err(BusinessError::invalid(
            "LANDED_COST_ALLOCATION_UNAVAILABLE",
            "The selected allocation basis has no positive quantity or receipt value.",
        ));
    }
    let mut prior = 0;
    let mut allocated = 0;
    weights
        .iter()
        .enumerate()
        .map(|(index, &weight)| {
            prior += weight;
            let cumulative = if index + 1 == weights.len() {
                total
            } else {
                scaled_inventory_value(total, prior, sum)?
            };
            let part = cumulative - allocated;
            allocated = cumulative;
            safe(part)
        })
        .collect()
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct TransitValuation {
    pub quantity_milli: i64,
    pub received_milli: i64,
    pub lost_milli: i64,
    pub pending_loss_milli: i64,
    pub value_minor: i64,
    pub settled_value_minor: i64,
}

/// Pending losses reserve availability but do not enter settled cumulative cost.
pub fn transit_quantity_value(
    line: &TransitValuation,
    quantity_milli: i64,
) -> Result<i64, BusinessError> {
    for v in [
        line.quantity_milli,
        line.received_milli,
        line.lost_milli,
        line.pending_loss_milli,
        line.value_minor,
        line.settled_value_minor,
        quantity_milli,
    ] {
        safe(v)?;
    }
    if [
        line.quantity_milli,
        line.received_milli,
        line.lost_milli,
        line.pending_loss_milli,
    ]
    .iter()
    .any(|v| *v < 0)
    {
        return Err(BusinessError::invalid(
            "INVENTORY_ALLOCATION_INVALID",
            "Use nonnegative inventory quantities.",
        ));
    }
    let remaining = i128::from(line.quantity_milli)
        - i128::from(line.received_milli)
        - i128::from(line.lost_milli)
        - i128::from(line.pending_loss_milli);
    if quantity_milli <= 0 || i128::from(quantity_milli) > remaining {
        return Err(BusinessError::invalid(
            "TRANSIT_QUANTITY_EXCEEDED",
            "Use a positive quantity within the unreceived, unreserved transport balance.",
        ));
    }
    let cumulative = safe(line.received_milli + line.lost_milli + quantity_milli)?;
    let amount = scaled_inventory_value(line.value_minor, cumulative, line.quantity_milli)?;
    safe(amount - line.settled_value_minor)
}

pub fn require_dispatch_date(
    status: &str,
    dispatch_date: &str,
    date: &str,
) -> Result<(), BusinessError> {
    if status != "in_transit" {
        return Err(BusinessError::new(
            FailureKind::Conflict,
            "TRANSIT_NOT_DISPATCHED",
            "Record outcomes against goods currently in transit.",
        ));
    }
    if date < dispatch_date {
        return Err(BusinessError::invalid(
            "RECEIPT_DATE_INVALID",
            "Receipt or discrepancy cannot precede dispatch.",
        ));
    }
    Ok(())
}

/// The application schema admits 1–12 ASCII whole digits and up to three
/// fractional digits; unlike the old internal helper this rejects extra input
/// rather than silently truncating a fourth decimal or a second dot.
pub fn reorder_point_milli(value: &str) -> Result<i64, BusinessError> {
    let invalid = || {
        BusinessError::invalid(
            "REORDER_POINT_INVALID",
            "The reorder point must fit a positive inventory quantity.",
        )
    };
    let mut parts = value.split('.');
    let whole = parts.next().unwrap_or_default();
    let fraction = parts.next();
    if whole.is_empty()
        || whole.len() > 12
        || !whole.bytes().all(|b| b.is_ascii_digit())
        || parts.next().is_some()
        || fraction
            .is_some_and(|f| f.is_empty() || f.len() > 3 || !f.bytes().all(|b| b.is_ascii_digit()))
    {
        return Err(invalid());
    }
    let fraction = fraction.unwrap_or_default();
    let whole: i64 = whole.parse().map_err(|_| invalid())?;
    let fraction: i64 = if fraction.is_empty() {
        0
    } else {
        fraction.parse::<i64>().map_err(|_| invalid())? * 10_i64.pow(3 - fraction.len() as u32)
    };
    let quantity = whole * 1000 + fraction;
    if quantity <= 0 {
        return Err(invalid());
    }
    Ok(quantity)
}
