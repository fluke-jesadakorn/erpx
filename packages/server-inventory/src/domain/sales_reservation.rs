//! Inventory-owned quantity decisions for a Commerce sales-line reservation.
//! The caller must authorize and lock the Commerce order and Inventory sources
//! in the same transaction before using this plan; it grants no authority.
use erpx_server_shared::public::BusinessError;

const MAX_SAFE: i64 = 9_007_199_254_740_991;
const MAX_EXPIRY_MS: i64 = 30 * 86_400_000;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesReservationRow {
    pub id: String,
    pub quantity_milli: i64,
    pub consumed_milli: i64,
    pub released_milli: i64,
    pub expires_at: i64,
    pub created_at: i64,
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SalesReservationRelease {
    pub id: String,
    pub quantity_milli: i64,
}

/// Mirror the legacy release order: unexpired rows, ordered by creation time
/// then ID. Never produce a partial plan when the requested release exceeds
/// the active balance. The database adapter must retain these exact locked rows.
pub fn plan_sales_reservation_release(
    rows: &[SalesReservationRow],
    quantity_milli: i64,
    now: i64,
) -> Result<Vec<SalesReservationRelease>, BusinessError> {
    if !(1..=MAX_SAFE).contains(&quantity_milli) {
        return Err(BusinessError::invalid(
            "RESERVATION_QUANTITY_INVALID",
            "Choose a positive reservation quantity.",
        ));
    }
    let mut active: Vec<_> = rows.iter().filter(|r| r.expires_at > now).collect();
    active.sort_by(|a, b| {
        a.created_at
            .cmp(&b.created_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    let mut remaining = quantity_milli;
    let mut plan = Vec::new();
    for row in active {
        if row.id.is_empty()
            || !(0..=MAX_SAFE).contains(&row.quantity_milli)
            || !(0..=MAX_SAFE).contains(&row.consumed_milli)
            || !(0..=MAX_SAFE).contains(&row.released_milli)
        {
            return Err(BusinessError::invalid(
                "RESERVATION_SOURCE_INVALID",
                "The reservation source is invalid.",
            ));
        }
        let held = row
            .quantity_milli
            .checked_sub(row.consumed_milli)
            .and_then(|n| n.checked_sub(row.released_milli))
            .ok_or_else(|| {
                BusinessError::invalid(
                    "RESERVATION_SOURCE_INVALID",
                    "The reservation source is invalid.",
                )
            })?;
        if held <= 0 {
            continue;
        }
        let amount = held.min(remaining);
        plan.push(SalesReservationRelease {
            id: row.id.clone(),
            quantity_milli: amount,
        });
        remaining -= amount;
        if remaining == 0 {
            return Ok(plan);
        }
    }
    Err(BusinessError::invalid(
        "OVER_RELEASE",
        "Release quantity exceeds the remaining reservation.",
    ))
}

/// The legacy create guard compares the line's unfulfilled quantity with the
/// active reservation view, and accepts expiry only within the next 30 days.
pub fn validate_sales_reservation_create(
    quantity_milli: i64,
    line_quantity_milli: i64,
    fulfilled_milli: i64,
    active_reserved_milli: i64,
    expires_at: i64,
    now: i64,
) -> Result<(), BusinessError> {
    if !(1..=MAX_SAFE).contains(&quantity_milli)
        || [line_quantity_milli, fulfilled_milli, active_reserved_milli]
            .iter()
            .any(|n| !(0..=MAX_SAFE).contains(n))
    {
        return Err(BusinessError::invalid(
            "RESERVATION_QUANTITY_INVALID",
            "Choose a positive reservation quantity.",
        ));
    }
    if expires_at <= now || expires_at > now.checked_add(MAX_EXPIRY_MS).unwrap_or(i64::MAX) {
        return Err(BusinessError::invalid(
            "RESERVATION_EXPIRY_REQUIRED",
            "Choose an expiry within the next 30 days.",
        ));
    }
    let remaining = line_quantity_milli
        .checked_sub(fulfilled_milli)
        .and_then(|n| n.checked_sub(active_reserved_milli))
        .ok_or_else(|| {
            BusinessError::invalid(
                "RESERVATION_SOURCE_INVALID",
                "The reservation source is invalid.",
            )
        })?;
    if remaining < quantity_milli {
        return Err(BusinessError::invalid(
            "RESERVATION_REMAINING_UNAVAILABLE",
            "The order line has insufficient unreserved quantity.",
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    fn row(
        id: &str,
        created_at: i64,
        expiry: i64,
        quantity: i64,
        consumed: i64,
        released: i64,
    ) -> SalesReservationRow {
        SalesReservationRow {
            id: id.into(),
            created_at,
            expires_at: expiry,
            quantity_milli: quantity,
            consumed_milli: consumed,
            released_milli: released,
        }
    }
    #[test]
    fn release_uses_active_fifo_rows_and_rejects_over_release() {
        let rows = vec![
            row("b", 1, 20, 5, 1, 1),
            row("a", 1, 20, 4, 0, 0),
            row("expired", 0, 10, 99, 0, 0),
        ];
        assert_eq!(
            plan_sales_reservation_release(&rows, 6, 10).unwrap(),
            vec![
                SalesReservationRelease {
                    id: "a".into(),
                    quantity_milli: 4
                },
                SalesReservationRelease {
                    id: "b".into(),
                    quantity_milli: 2
                }
            ]
        );
        assert_eq!(
            plan_sales_reservation_release(&rows, 8, 10)
                .unwrap_err()
                .code,
            "OVER_RELEASE"
        );
    }
    #[test]
    fn create_checks_expiry_and_unreserved_line_quantity() {
        assert!(validate_sales_reservation_create(3, 10, 2, 5, 101, 100).is_ok());
        assert!(validate_sales_reservation_create(4, 10, 2, 5, 101, 100).is_err());
        assert_eq!(
            validate_sales_reservation_create(1, 10, 0, 0, 100, 100)
                .unwrap_err()
                .code,
            "RESERVATION_EXPIRY_REQUIRED"
        );
        assert!(validate_sales_reservation_create(1, 10, 0, 0, 100 + MAX_EXPIRY_MS, 100).is_ok());
        assert!(validate_sales_reservation_create(1, 10, 0, 0, 101 + MAX_EXPIRY_MS, 100).is_err());
    }
}
