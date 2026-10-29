/// Matches the retained stock projection: whole holds quarantine all stock;
/// otherwise partial holds sum and are capped by physical stock.
pub fn service_available_milli(stock: i64, reserved: i64, whole: bool, partial: i128) -> i64 {
    let quantity = i128::from(stock);
    let quarantine = if whole {
        quantity
    } else {
        quantity.min(partial)
    };
    (quantity - i128::from(reserved) - quarantine).max(0) as i64
}
