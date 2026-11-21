//! Pure journal direction for physical stock variance. This is not authority.
use erpx_server_shared::public::BusinessError;

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct VarianceLine {
    pub account_id: String,
    pub debit_minor: i64,
    pub credit_minor: i64,
}

/// The value is the Inventory owner's actual-cost delta, never quantity or an
/// independently reconstructed valuation. Zero produces no journal.
pub fn variance_lines(
    inventory: &str,
    variance: &str,
    delta: i64,
) -> Result<Vec<VarianceLine>, BusinessError> {
    const SAFE: i64 = 9_007_199_254_740_991;
    if inventory.is_empty()
        || variance.is_empty()
        || inventory == variance
        || !(-SAFE..=SAFE).contains(&delta)
    {
        return Err(BusinessError::invalid(
            "COUNT_VARIANCE_INVALID",
            "Use distinct posting accounts and a safe actual-cost variance.",
        ));
    }
    if delta == 0 {
        return Ok(vec![]);
    }
    let amount = delta.abs();
    let (debit, credit) = if delta > 0 {
        (inventory, variance)
    } else {
        (variance, inventory)
    };
    Ok(vec![
        VarianceLine {
            account_id: debit.into(),
            debit_minor: amount,
            credit_minor: 0,
        },
        VarianceLine {
            account_id: credit.into(),
            debit_minor: 0,
            credit_minor: amount,
        },
    ])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn exact_direction_zero_and_bounds() {
        assert!(variance_lines("inventory", "5400", 0).unwrap().is_empty());
        let gain = variance_lines("inventory", "5400", 123).unwrap();
        assert_eq!(
            (
                gain[0].account_id.as_str(),
                gain[0].debit_minor,
                gain[1].account_id.as_str(),
                gain[1].credit_minor
            ),
            ("inventory", 123, "5400", 123)
        );
        let loss = variance_lines("inventory", "5400", -123).unwrap();
        assert_eq!(
            (
                loss[0].account_id.as_str(),
                loss[0].debit_minor,
                loss[1].account_id.as_str(),
                loss[1].credit_minor
            ),
            ("5400", 123, "inventory", 123)
        );
        assert!(variance_lines("same", "same", 1).is_err());
        assert!(variance_lines("i", "v", i64::MIN).is_err());
    }
}
