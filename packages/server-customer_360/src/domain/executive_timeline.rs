//! Native Executive customer-activity metric source facts.
//!
//! This is the read-only source behind the Commerce `recent-customer-activity`
//! metric. It replaces the retained TypeScript `customerTimelineEvents`
//! subquery: the same federated Customer 360 sources are counted by UTC month or
//! by source type over one bounded window. No source record is written and no
//! customer/order detail leaves the owner.
use erpx_server_shared::public::{BusinessError, FailureKind};
use serde::Serialize;

pub const MAX_GROUPS: usize = 32;
pub const MAX_RANGE_MS: i64 = 366 * 24 * 60 * 60 * 1000;
const MAX_LABEL: usize = 64;
const MAX_VALUE: usize = 20;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum CustomerActivityBreakdown {
    Month,
    SourceType,
}

impl CustomerActivityBreakdown {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Month => "month",
            Self::SourceType => "source_type",
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct CustomerActivityMetricQuery {
    pub from: i64,
    pub to: i64,
    pub breakdown: CustomerActivityBreakdown,
}

impl CustomerActivityMetricQuery {
    pub fn validate(&self) -> Result<(), BusinessError> {
        if self.from < 0 || self.to <= self.from || self.to - self.from > MAX_RANGE_MS {
            return Err(BusinessError::invalid(
                "CUSTOMER_ACTIVITY_RANGE_INVALID",
                "Choose a customer activity range within one year.",
            ));
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CustomerActivityMetricGroup {
    pub label: String,
    pub value: String,
}

pub(crate) fn metric_invalid() -> BusinessError {
    BusinessError::new(
        FailureKind::Unavailable,
        "CUSTOMER_ACTIVITY_METRIC_INVALID",
        "The customer activity metric source is invalid.",
    )
}

pub const SOURCE_TYPES: [&str; 11] = [
    "order",
    "fulfillment",
    "return",
    "contact",
    "followup",
    "support",
    "field_service",
    "legal",
    "invoice",
    "crm",
    "crm_activity",
];

fn valid_month(label: &str) -> bool {
    let bytes = label.as_bytes();
    if bytes.len() != 7
        || !bytes[..4].iter().all(u8::is_ascii_digit)
        || bytes[4] != b'-'
        || !bytes[5..].iter().all(u8::is_ascii_digit)
    {
        return false;
    }
    matches!(label[5..].parse::<u8>(), Ok(1..=12))
}

fn valid_value(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= MAX_VALUE
        && value.bytes().all(|byte| byte.is_ascii_digit())
        && value.parse::<u64>().is_ok()
        && (value == "0" || !value.starts_with('0'))
}

pub(crate) fn validate_groups(
    groups: &[CustomerActivityMetricGroup],
    breakdown: CustomerActivityBreakdown,
) -> Result<(), BusinessError> {
    if groups.len() > MAX_GROUPS {
        return Err(metric_invalid());
    }
    let mut seen = std::collections::BTreeSet::new();
    for group in groups {
        if group.label.len() > MAX_LABEL
            || !valid_value(&group.value)
            || !seen.insert(group.label.as_str())
        {
            return Err(metric_invalid());
        }
        match breakdown {
            CustomerActivityBreakdown::Month => {
                if !valid_month(&group.label) {
                    return Err(metric_invalid());
                }
            }
            CustomerActivityBreakdown::SourceType => {
                if !SOURCE_TYPES.contains(&group.label.as_str()) {
                    return Err(metric_invalid());
                }
            }
        }
    }
    Ok(())
}
