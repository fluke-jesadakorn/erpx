mod catalog;
mod entitlement;
pub mod hosted_pricing;
mod records;
mod subscription;

use chrono::{DateTime, Utc};
use erpx_integration_contracts::CurrencyCode;
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub use catalog::{CatalogPlan, CatalogPlanStatus, catalog_plan_publish_decision_ref};
pub use entitlement::{Entitlement, EntitlementCapacity, EntitlementState};
pub use records::{
    CommercialLifecycleRecord, CommercialLifecycleRecordError, CommercialLifecycleRecordKind,
    CommercialLifecycleRecordStatus,
};
pub use subscription::{BillingSignal, Subscription, SubscriptionState, SubscriptionStatus};

pub const MAX_COMMERCIAL_TEXT_BYTES: usize = 255;
pub const FLOW_AI_CAPABILITY: &str = "flow_ai";

macro_rules! commercial_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, CommercialDomainError> {
                require_id(value, $field)?;
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = CommercialDomainError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

commercial_id!(CatalogPlanId, "catalog_plan_id");
commercial_id!(SubscriptionId, "subscription_id");
commercial_id!(EntitlementId, "entitlement_id");

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommercialDomainError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {maximum} bytes")]
    TooLong { field: &'static str, maximum: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("commercial holding records require an exact holding scope")]
    InvalidHoldingScope,
    #[error("catalog plans are platform scoped")]
    InvalidCatalogScope,
    #[error("commercial amount must be positive")]
    NonPositiveAmount,
    #[error("commercial capacity must be positive")]
    NonPositiveCapacity,
    #[error("catalog plan lifecycle transition is not allowed")]
    InvalidCatalogTransition,
    #[error("subscription lifecycle transition is not allowed: {from:?} with {signal:?}")]
    InvalidSubscriptionTransition {
        from: SubscriptionStatus,
        signal: BillingSignal,
    },
    #[error("subscription grace period has not expired")]
    GracePeriodNotExpired,
    #[error("provider event is duplicate or older than the accepted event")]
    StaleProviderEvent,
    #[error("commercial timestamp cannot move backwards")]
    InvalidTimestamp,
    #[error("commercial aggregate version overflowed")]
    VersionOverflow,
    #[error("entitlement capacity cannot be lower than allocated usage")]
    CapacityBelowAllocated,
    #[error("entitlement allocation exceeds purchased capacity")]
    CapacityExceeded,
    #[error("disabled entitlements cannot accept allocations")]
    EntitlementDisabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "state", rename_all = "snake_case")]
pub enum CommercialAggregate {
    CatalogPlan(CatalogPlan),
    Subscription(Subscription),
    Entitlement(Entitlement),
}

impl CommercialAggregate {
    pub const fn version(&self) -> u64 {
        match self {
            Self::CatalogPlan(value) => value.version(),
            Self::Subscription(value) => value.version(),
            Self::Entitlement(value) => value.version(),
        }
    }

    pub const fn scope(&self) -> &AuthorityScope {
        match self {
            Self::CatalogPlan(value) => value.scope(),
            Self::Subscription(value) => value.scope(),
            Self::Entitlement(value) => value.scope(),
        }
    }

    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::CatalogPlan(value) => value.id().as_uuid(),
            Self::Subscription(value) => value.id().as_uuid(),
            Self::Entitlement(value) => value.id().as_uuid(),
        }
    }

    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::CatalogPlan(_) => "catalog_plan",
            Self::Subscription(_) => "subscription",
            Self::Entitlement(_) => "entitlement",
        }
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        match self {
            Self::CatalogPlan(value) => value.take_uncommitted_events(),
            Self::Subscription(value) => value.take_uncommitted_events(),
            Self::Entitlement(value) => value.take_uncommitted_events(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    CatalogPlanCreated,
    CatalogPlanPublished,
    SubscriptionChanged {
        subscription_id: SubscriptionId,
        customer_id: Uuid,
        plan_id: CatalogPlanId,
        status: SubscriptionStatus,
        amount_minor: i64,
        currency: CurrencyCode,
        effective_at: DateTime<Utc>,
    },
    EntitlementChanged {
        entitlement_id: EntitlementId,
        subscription_id: SubscriptionId,
        capability: String,
        enabled: bool,
    },
}

pub(crate) fn require_id(value: Uuid, field: &'static str) -> Result<(), CommercialDomainError> {
    if value.is_nil() {
        Err(CommercialDomainError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

pub(crate) fn validate_text(value: &str, field: &'static str) -> Result<(), CommercialDomainError> {
    if value.trim().is_empty() {
        return Err(CommercialDomainError::Required { field });
    }
    if value.len() > MAX_COMMERCIAL_TEXT_BYTES {
        return Err(CommercialDomainError::TooLong {
            field,
            maximum: MAX_COMMERCIAL_TEXT_BYTES,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(CommercialDomainError::ControlCharacter { field });
    }
    Ok(())
}

pub(crate) const fn next_version(version: u64) -> Result<u64, CommercialDomainError> {
    match version.checked_add(1) {
        Some(value) => Ok(value),
        None => Err(CommercialDomainError::VersionOverflow),
    }
}

pub(crate) fn holding_id(scope: &AuthorityScope) -> Result<Uuid, CommercialDomainError> {
    match scope {
        AuthorityScope::Holding { holding_id } if !holding_id.is_nil() => Ok(*holding_id),
        _ => Err(CommercialDomainError::InvalidHoldingScope),
    }
}

#[cfg(test)]
mod tests;
