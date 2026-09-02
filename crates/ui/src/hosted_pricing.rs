use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedSeatStepView {
    pub min_seats: u32,
    pub max_seats: Option<u32>,
    pub price_cents: u32,
    pub label: String,
}

#[allow(dead_code)]
pub fn hosted_steps_view() -> Vec<HostedSeatStepView> {
    vec![
        HostedSeatStepView {
            min_seats: 1,
            max_seats: Some(5),
            price_cents: 900,
            label: "1-5 seats @ $9".into(),
        },
        HostedSeatStepView {
            min_seats: 6,
            max_seats: Some(20),
            price_cents: 700,
            label: "6-20 seats @ $7".into(),
        },
        HostedSeatStepView {
            min_seats: 21,
            max_seats: Some(50),
            price_cents: 500,
            label: "21-50 seats @ $5".into(),
        },
        HostedSeatStepView {
            min_seats: 51,
            max_seats: Some(200),
            price_cents: 350,
            label: "51-200 seats @ $3.5".into(),
        },
    ]
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct HostedQuoteView {
    pub seats: u32,
    pub tier: String,
    pub monthly_cents: u32,
    pub monthly_cents_with_flow: u32,
    pub flow_cents: u32,
}

impl HostedQuoteView {
    pub fn formatted_monthly(&self) -> String {
        format!("${:.2}", self.monthly_cents as f64 / 100.0)
    }

    pub fn formatted_with_flow(&self) -> String {
        format!("${:.2}", self.monthly_cents_with_flow as f64 / 100.0)
    }

    pub fn stepper_label(seats: u32) -> String {
        match seats {
            0 => "no seats".into(),
            1..=5 => format!("{seats} seats @ $9"),
            6..=20 => format!("{seats} seats @ $7 blended"),
            21..=50 => format!("{seats} seats @ $5 blended"),
            51..=200 => format!("{seats} seats @ $3.5 blended"),
            _ => "custom".into(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum HostedPlanView {
    SelfHost,
    Starter,
    Growth,
    Scale,
    Custom,
}

impl HostedPlanView {
    pub const fn from_seats(seats: u32) -> Self {
        match seats {
            0 => Self::SelfHost,
            1..=20 => Self::Starter,
            21..=50 => Self::Growth,
            51..=200 => Self::Scale,
            _ => Self::Custom,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stepper_labels() {
        assert_eq!(HostedQuoteView::stepper_label(3), "3 seats @ $9");
        assert_eq!(HostedQuoteView::stepper_label(10), "10 seats @ $7 blended");
        assert_eq!(HostedQuoteView::stepper_label(30), "30 seats @ $5 blended");
        assert_eq!(
            HostedQuoteView::stepper_label(60),
            "60 seats @ $3.5 blended"
        );
        assert_eq!(HostedQuoteView::stepper_label(300), "custom");
    }

    #[test]
    fn quote_formatting() {
        let view = HostedQuoteView {
            seats: 10,
            tier: "starter".into(),
            monthly_cents: 8000,
            monthly_cents_with_flow: 10900,
            flow_cents: 2900,
        };
        assert_eq!(view.formatted_monthly(), "$80.00");
        assert_eq!(view.formatted_with_flow(), "$109.00");
    }
}
