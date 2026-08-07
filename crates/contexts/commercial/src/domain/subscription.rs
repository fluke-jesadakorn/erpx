use chrono::{DateTime, Utc};
use erpx_integration_contracts::CurrencyCode;
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    CatalogPlanId, CommercialDomainError, DomainEvent, SubscriptionId, holding_id, next_version,
    require_id,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SubscriptionStatus {
    Trial,
    Active,
    PastDue,
    Suspended,
    Ended,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BillingSignal {
    TrialEnded,
    PaymentFailed,
    GraceExpired,
    PaymentRecovered,
    SubscriptionCanceled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Subscription {
    id: SubscriptionId,
    version: u64,
    scope: TenantScope,
    holding_id: Uuid,
    customer_id: Uuid,
    plan_id: CatalogPlanId,
    status: SubscriptionStatus,
    amount_minor: i64,
    currency: CurrencyCode,
    last_provider_event_id: Option<String>,
    last_provider_event_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl Subscription {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: SubscriptionId,
        scope: TenantScope,
        customer_id: Uuid,
        plan_id: CatalogPlanId,
        amount_minor: i64,
        currency: CurrencyCode,
        starts_in_trial: bool,
        at: DateTime<Utc>,
    ) -> Result<Self, CommercialDomainError> {
        let holding_id = holding_id(&scope)?;
        require_id(customer_id, "customer_id")?;
        if amount_minor <= 0 {
            return Err(CommercialDomainError::NonPositiveAmount);
        }
        let status = if starts_in_trial {
            SubscriptionStatus::Trial
        } else {
            SubscriptionStatus::Active
        };
        let mut value = Self {
            id,
            version: 1,
            scope,
            holding_id,
            customer_id,
            plan_id,
            status,
            amount_minor,
            currency,
            last_provider_event_id: None,
            last_provider_event_at: None,
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.emit_changed(at);
        Ok(value)
    }

    pub fn apply_verified_billing_event(
        &mut self,
        provider_event_id: String,
        provider_event_at: DateTime<Utc>,
        signal: BillingSignal,
    ) -> Result<(), CommercialDomainError> {
        super::validate_text(&provider_event_id, "provider_event_id")?;
        if self
            .last_provider_event_at
            .is_some_and(|accepted| provider_event_at <= accepted)
            || provider_event_at < self.updated_at
        {
            return Err(CommercialDomainError::StaleProviderEvent);
        }
        let next = match (self.status, signal) {
            (SubscriptionStatus::Trial, BillingSignal::TrialEnded)
            | (
                SubscriptionStatus::PastDue | SubscriptionStatus::Suspended,
                BillingSignal::PaymentRecovered,
            ) => SubscriptionStatus::Active,
            (SubscriptionStatus::Active, BillingSignal::PaymentFailed) => {
                SubscriptionStatus::PastDue
            }
            (SubscriptionStatus::PastDue, BillingSignal::GraceExpired) => {
                SubscriptionStatus::Suspended
            }
            (
                SubscriptionStatus::Trial
                | SubscriptionStatus::Active
                | SubscriptionStatus::PastDue
                | SubscriptionStatus::Suspended,
                BillingSignal::SubscriptionCanceled,
            ) => SubscriptionStatus::Ended,
            (from, signal) => {
                return Err(CommercialDomainError::InvalidSubscriptionTransition { from, signal });
            }
        };
        self.version = next_version(self.version)?;
        self.status = next;
        self.last_provider_event_id = Some(provider_event_id);
        self.last_provider_event_at = Some(provider_event_at);
        self.updated_at = provider_event_at;
        self.emit_changed(provider_event_at);
        Ok(())
    }

    fn emit_changed(&mut self, effective_at: DateTime<Utc>) {
        self.uncommitted_events
            .push(DomainEvent::SubscriptionChanged {
                subscription_id: self.id,
                customer_id: self.customer_id,
                plan_id: self.plan_id,
                status: self.status,
                amount_minor: self.amount_minor,
                currency: self.currency,
                effective_at,
            });
    }

    pub const fn id(&self) -> SubscriptionId {
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
    pub const fn customer_id(&self) -> Uuid {
        self.customer_id
    }
    pub const fn plan_id(&self) -> CatalogPlanId {
        self.plan_id
    }
    pub const fn status(&self) -> SubscriptionStatus {
        self.status
    }
    pub const fn amount_minor(&self) -> i64 {
        self.amount_minor
    }
    pub const fn currency(&self) -> CurrencyCode {
        self.currency
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
