use serde::{Deserialize, Serialize};

/// Stepper seat pricing — blended, per ADR-018.
/// Steps are inclusive lower bounds; price drops as seat count grows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatStep {
    pub min_seats: u32,
    pub max_seats: Option<u32>,
    pub price_cents_per_seat: u32,
}

pub const HOSTED_SEAT_STEPS: &[SeatStep] = &[
    SeatStep {
        min_seats: 1,
        max_seats: Some(5),
        price_cents_per_seat: 900,
    },
    SeatStep {
        min_seats: 6,
        max_seats: Some(20),
        price_cents_per_seat: 700,
    },
    SeatStep {
        min_seats: 21,
        max_seats: Some(50),
        price_cents_per_seat: 500,
    },
    SeatStep {
        min_seats: 51,
        max_seats: Some(200),
        price_cents_per_seat: 350,
    },
];

/// Flat Flow AI add-on per Holding per month — unlimited seats/flows, customer pays LLM directly.
pub const FLOW_AI_CENTS_PER_HOLDING: u32 = 2900;
/// Topup pass-through: $20 = 2000 cents + 10% fee = 2200 billed, 2000 credit.
pub const TOPUP_FEE_BPS: u32 = 1000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HostedTier {
    Starter,
    Growth,
    Scale,
    Custom,
}

impl HostedTier {
    pub const fn from_seats(seats: u32) -> Self {
        match seats {
            0..=20 => Self::Starter,
            21..=50 => Self::Growth,
            51..=200 => Self::Scale,
            _ => Self::Custom,
        }
    }

    pub const fn label(self) -> &'static str {
        match self {
            Self::Starter => "starter",
            Self::Growth => "growth",
            Self::Scale => "scale",
            Self::Custom => "custom",
        }
    }
}

/// Blended stepper total: 10 seats starter = 5*900 + 5*700 = 8000 cents.
pub fn hosted_monthly_cents(seats: u32) -> Option<u32> {
    if seats == 0 {
        return Some(0);
    }
    if seats > 200 {
        return None;
    }
    let mut remaining = seats;
    let mut total: u32 = 0;
    for step in HOSTED_SEAT_STEPS {
        if remaining == 0 {
            break;
        }
        let step_cap = step.max_seats.map_or(remaining, |max| {
            if seats > max {
                max - step.min_seats + 1
            } else {
                remaining
            }
        });
        let take = remaining.min(step_cap);
        total = total.checked_add(take.checked_mul(step.price_cents_per_seat)?)?;
        remaining = remaining.saturating_sub(take);
    }
    Some(total)
}

pub fn hosted_monthly_cents_with_flow(seats: u32, with_flow: bool) -> Option<u32> {
    let base = hosted_monthly_cents(seats)?;
    if with_flow {
        Some(base.checked_add(FLOW_AI_CENTS_PER_HOLDING)?)
    } else {
        Some(base)
    }
}

pub fn topup_billed_cents(credit_cents: u32) -> Option<u32> {
    let fee = credit_cents
        .checked_mul(TOPUP_FEE_BPS)?
        .checked_div(10_000)?;
    credit_cents.checked_add(fee)
}

pub fn active_seats_from_distinct_principals(principals: &[String]) -> u32 {
    use std::collections::BTreeSet;
    principals
        .iter()
        .filter(|p| !p.trim().is_empty())
        .collect::<BTreeSet<_>>()
        .len() as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepper_examples() {
        assert_eq!(hosted_monthly_cents(0), Some(0));
        assert_eq!(hosted_monthly_cents(3), Some(2700));
        assert_eq!(hosted_monthly_cents(5), Some(4500));
        assert_eq!(hosted_monthly_cents(10), Some(8000));
        assert_eq!(hosted_monthly_cents(30), Some(20_000));
        assert_eq!(hosted_monthly_cents(60), Some(33_500));
        assert_eq!(hosted_monthly_cents(201), None);
    }

    #[test]
    fn with_flow() {
        assert_eq!(hosted_monthly_cents_with_flow(10, true), Some(10_900));
        assert_eq!(hosted_monthly_cents_with_flow(10, false), Some(8_000));
    }

    #[test]
    fn topup_fee() {
        assert_eq!(topup_billed_cents(2000), Some(2200));
        assert_eq!(topup_billed_cents(0), Some(0));
    }

    #[test]
    fn tier_label() {
        assert_eq!(HostedTier::from_seats(5).label(), "starter");
        assert_eq!(HostedTier::from_seats(30).label(), "growth");
        assert_eq!(HostedTier::from_seats(100).label(), "scale");
        assert_eq!(HostedTier::from_seats(300).label(), "custom");
    }
}
