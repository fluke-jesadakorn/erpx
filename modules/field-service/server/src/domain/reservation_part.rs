//! Pure Field Service part-reservation facts and invariants.
//!
//! No SQL, no HTTP and no I/O here. The eight fields are exactly the closed
//! field set the retired TypeScript reservation readback returned for its
//! `reservation` member, so the native payload can be byte-for-byte identical.

pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// Exact closed field set of the retired TypeScript reservation object.
pub const RESERVATION_KEYS: [&str; 8] = [
    "id",
    "orderId",
    "warehouseId",
    "productId",
    "quantityMilli",
    "consumedMilli",
    "releasedMilli",
    "expiresAt",
];

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct FieldPartReservation {
    pub id: String,
    pub order_id: String,
    pub warehouse_id: String,
    pub product_id: String,
    pub quantity_milli: i64,
    pub consumed_milli: i64,
    pub released_milli: i64,
    pub expires_at: i64,
}

fn bounded_text(value: &str) -> bool {
    !value.is_empty() && value.len() <= 1024
}

impl FieldPartReservation {
    /// The same admission test the TypeScript readback applied to its
    /// `reservation` object: closed bounds plus the no-overuse invariant.
    pub fn is_valid(&self) -> bool {
        bounded_text(&self.id)
            && bounded_text(&self.order_id)
            && bounded_text(&self.warehouse_id)
            && bounded_text(&self.product_id)
            && (1..=MAX_SAFE_INTEGER).contains(&self.quantity_milli)
            && (0..=MAX_SAFE_INTEGER).contains(&self.consumed_milli)
            && (0..=MAX_SAFE_INTEGER).contains(&self.released_milli)
            && (1..=MAX_SAFE_INTEGER).contains(&self.expires_at)
            && self
                .consumed_milli
                .checked_add(self.released_milli)
                .is_some_and(|used| used <= self.quantity_milli)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid() -> FieldPartReservation {
        FieldPartReservation {
            id: "reservation".into(),
            order_id: "order".into(),
            warehouse_id: "warehouse".into(),
            product_id: "product".into(),
            quantity_milli: 1000,
            consumed_milli: 400,
            released_milli: 100,
            expires_at: 1_800_000_010_100,
        }
    }

    #[test]
    fn exact_reservation_is_admitted() {
        assert!(valid().is_valid());
        assert_eq!(RESERVATION_KEYS.len(), 8);
    }

    #[test]
    fn overuse_and_empty_identity_are_rejected() {
        let mut row = valid();
        row.released_milli = 700;
        assert!(!row.is_valid());
        let mut row = valid();
        row.product_id.clear();
        assert!(!row.is_valid());
        let mut row = valid();
        row.quantity_milli = 0;
        assert!(!row.is_valid());
    }
}
