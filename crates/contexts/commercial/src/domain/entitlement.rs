use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    CommercialDomainError, DomainEvent, EntitlementId, SubscriptionId, holding_id, next_version,
    validate_text,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitlementCapacity {
    pub seats: u32,
    pub companies: u32,
    pub branches: u32,
}

impl EntitlementCapacity {
    pub fn validate(self) -> Result<(), CommercialDomainError> {
        if self.seats == 0 || self.companies == 0 || self.branches == 0 {
            Err(CommercialDomainError::NonPositiveCapacity)
        } else {
            Ok(())
        }
    }

    const fn contains(self, allocated: Self) -> bool {
        self.seats >= allocated.seats
            && self.companies >= allocated.companies
            && self.branches >= allocated.branches
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entitlement {
    id: EntitlementId,
    version: u64,
    scope: TenantScope,
    holding_id: Uuid,
    subscription_id: SubscriptionId,
    capability: String,
    enabled: bool,
    capacity: EntitlementCapacity,
    allocated: EntitlementCapacity,
    last_provider_event_id: Option<String>,
    last_provider_event_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntitlementState {
    pub id: EntitlementId,
    pub version: u64,
    pub scope: TenantScope,
    pub subscription_id: SubscriptionId,
    pub capability: String,
    pub enabled: bool,
    pub capacity: EntitlementCapacity,
    pub allocated: EntitlementCapacity,
    pub last_provider_event_id: Option<String>,
    pub last_provider_event_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Entitlement {
    pub fn try_rehydrate(state: EntitlementState) -> Result<Self, CommercialDomainError> {
        let holding_id = holding_id(&state.scope)?;
        validate_text(&state.capability, "capability")?;
        state.capacity.validate()?;
        if state.version == 0
            || !state.capacity.contains(state.allocated)
            || state.updated_at < state.created_at
            || state
                .last_provider_event_id
                .as_ref()
                .is_some_and(|value| validate_text(value, "provider_event_id").is_err())
            || state.last_provider_event_id.is_some() != state.last_provider_event_at.is_some()
            || state
                .last_provider_event_at
                .is_some_and(|at| at > state.updated_at)
        {
            return Err(CommercialDomainError::InvalidTimestamp);
        }
        Ok(Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            holding_id,
            subscription_id: state.subscription_id,
            capability: state.capability,
            enabled: state.enabled,
            capacity: state.capacity,
            allocated: state.allocated,
            last_provider_event_id: state.last_provider_event_id,
            last_provider_event_at: state.last_provider_event_at,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        })
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: EntitlementId,
        scope: TenantScope,
        subscription_id: SubscriptionId,
        capability: String,
        enabled: bool,
        capacity: EntitlementCapacity,
        at: DateTime<Utc>,
    ) -> Result<Self, CommercialDomainError> {
        let holding_id = holding_id(&scope)?;
        validate_text(&capability, "capability")?;
        capacity.validate()?;
        let mut value = Self {
            id,
            version: 1,
            scope,
            holding_id,
            subscription_id,
            capability,
            enabled,
            capacity,
            allocated: EntitlementCapacity {
                seats: 0,
                companies: 0,
                branches: 0,
            },
            last_provider_event_id: None,
            last_provider_event_at: None,
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.emit_changed();
        Ok(value)
    }

    pub fn apply_verified_change(
        &mut self,
        provider_event_id: String,
        provider_event_at: DateTime<Utc>,
        enabled: bool,
        capacity: EntitlementCapacity,
    ) -> Result<(), CommercialDomainError> {
        validate_text(&provider_event_id, "provider_event_id")?;
        capacity.validate()?;
        if self
            .last_provider_event_at
            .is_some_and(|accepted| provider_event_at <= accepted)
            || provider_event_at < self.updated_at
        {
            return Err(CommercialDomainError::StaleProviderEvent);
        }
        if !capacity.contains(self.allocated) {
            return Err(CommercialDomainError::CapacityBelowAllocated);
        }
        self.version = next_version(self.version)?;
        self.enabled = enabled;
        self.capacity = capacity;
        self.last_provider_event_id = Some(provider_event_id);
        self.last_provider_event_at = Some(provider_event_at);
        self.updated_at = provider_event_at;
        self.emit_changed();
        Ok(())
    }

    pub fn allocate(
        &mut self,
        allocation: EntitlementCapacity,
        at: DateTime<Utc>,
    ) -> Result<(), CommercialDomainError> {
        if !self.enabled {
            return Err(CommercialDomainError::EntitlementDisabled);
        }
        if at < self.updated_at {
            return Err(CommercialDomainError::InvalidTimestamp);
        }
        let next = EntitlementCapacity {
            seats: self
                .allocated
                .seats
                .checked_add(allocation.seats)
                .ok_or(CommercialDomainError::CapacityExceeded)?,
            companies: self
                .allocated
                .companies
                .checked_add(allocation.companies)
                .ok_or(CommercialDomainError::CapacityExceeded)?,
            branches: self
                .allocated
                .branches
                .checked_add(allocation.branches)
                .ok_or(CommercialDomainError::CapacityExceeded)?,
        };
        if !self.capacity.contains(next) {
            return Err(CommercialDomainError::CapacityExceeded);
        }
        self.version = next_version(self.version)?;
        self.allocated = next;
        self.updated_at = at;
        Ok(())
    }

    fn emit_changed(&mut self) {
        self.uncommitted_events
            .push(DomainEvent::EntitlementChanged {
                entitlement_id: self.id,
                subscription_id: self.subscription_id,
                capability: self.capability.clone(),
                enabled: self.enabled,
            });
    }

    pub const fn id(&self) -> EntitlementId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn holding_id(&self) -> Uuid {
        self.holding_id
    }
    pub const fn subscription_id(&self) -> SubscriptionId {
        self.subscription_id
    }
    pub fn capability(&self) -> &str {
        &self.capability
    }
    pub const fn enabled(&self) -> bool {
        self.enabled
    }
    pub const fn capacity(&self) -> EntitlementCapacity {
        self.capacity
    }
    pub const fn allocated(&self) -> EntitlementCapacity {
        self.allocated
    }
    pub fn last_provider_event_id(&self) -> Option<&str> {
        self.last_provider_event_id.as_deref()
    }
    pub const fn last_provider_event_at(&self) -> Option<DateTime<Utc>> {
        self.last_provider_event_at
    }
    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}
