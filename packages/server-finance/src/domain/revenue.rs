//! Revenue-obligation recognition policy shared by the owner command and tests.
//! Amounts are signed minor units; the adapter owns SQL and transaction scope.

use erpx_server_shared::public::BusinessError;

/// Next persisted obligation facts after one accepted recognition.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct RevenueRecognitionOutcome {
    pub recognized_minor: i64,
    pub status: &'static str,
}

/// Validate a recognition against the remaining obligation and derive its
/// persisted status. The rule is fail-closed: a missing, zero, negative or
/// over-allocated amount never reaches the owner journals.
pub fn recognize_revenue_amount(
    allocated_minor: i64,
    recognized_minor: i64,
    amount_minor: i64,
) -> Result<RevenueRecognitionOutcome, BusinessError> {
    let remaining = allocated_minor
        .checked_sub(recognized_minor)
        .filter(|remaining| *remaining >= 0)
        .ok_or_else(exceeds)?;
    if amount_minor <= 0 || amount_minor > remaining {
        return Err(exceeds());
    }
    let recognized_minor = recognized_minor
        .checked_add(amount_minor)
        .ok_or_else(|| BusinessError::invalid("MONEY_OVERFLOW", "Money amount is too large."))?;
    Ok(RevenueRecognitionOutcome {
        recognized_minor,
        status: if recognized_minor == allocated_minor {
            "complete"
        } else {
            "active"
        },
    })
}

fn exceeds() -> BusinessError {
    BusinessError::invalid(
        "REVENUE_EXCEEDS_ALLOCATION",
        "Recognition exceeds the remaining obligation.",
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn partial_recognition_advances_an_active_obligation() {
        let outcome = recognize_revenue_amount(100_00, 40_00, 25_00).unwrap();
        assert_eq!(
            outcome,
            RevenueRecognitionOutcome {
                recognized_minor: 65_00,
                status: "active",
            }
        );
    }

    #[test]
    fn exact_remaining_recognizes_the_full_obligation() {
        let outcome = recognize_revenue_amount(100_00, 40_00, 60_00).unwrap();
        assert_eq!(
            outcome,
            RevenueRecognitionOutcome {
                recognized_minor: 100_00,
                status: "complete",
            }
        );
    }

    #[test]
    fn zero_negative_and_over_allocated_amounts_are_refused() {
        for amount in [0, -1, 100_01] {
            assert_eq!(
                recognize_revenue_amount(100_00, 0, amount)
                    .unwrap_err()
                    .code,
                "REVENUE_EXCEEDS_ALLOCATION"
            );
        }
        assert_eq!(
            recognize_revenue_amount(100_00, 100_00, 1)
                .unwrap_err()
                .code,
            "REVENUE_EXCEEDS_ALLOCATION"
        );
    }

    #[test]
    fn corrupted_or_overflowing_facts_are_refused() {
        assert_eq!(
            recognize_revenue_amount(100_00, 101_00, 1)
                .unwrap_err()
                .code,
            "REVENUE_EXCEEDS_ALLOCATION"
        );
        assert_eq!(
            recognize_revenue_amount(i64::MAX, i64::MAX - 1, 1)
                .unwrap()
                .status,
            "complete"
        );
    }
}
