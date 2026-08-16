use chrono::{DateTime, Duration, Utc};
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
    grace_expires_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SubscriptionState {
    pub id: SubscriptionId,
    pub version: u64,
    pub scope: TenantScope,
    pub customer_id: Uuid,
    pub plan_id: CatalogPlanId,
    pub status: SubscriptionStatus,
    pub amount_minor: i64,
    pub currency: CurrencyCode,
    pub last_provider_event_id: Option<String>,
    pub last_provider_event_at: Option<DateTime<Utc>>,
    pub grace_expires_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl Subscription {
    pub fn try_rehydrate(state: SubscriptionState) -> Result<Self, CommercialDomainError> {
        let holding_id = holding_id(&state.scope)?;
        require_id(state.customer_id, "customer_id")?;
        if state.version == 0
            || state.amount_minor <= 0
            || state.updated_at < state.created_at
            || state
                .last_provider_event_id
                .as_ref()
                .is_some_and(|value| super::validate_text(value, "provider_event_id").is_err())
            || state.last_provider_event_id.is_some() != state.last_provider_event_at.is_some()
            || state
                .last_provider_event_at
                .is_some_and(|at| at > state.updated_at)
            || (state.status == SubscriptionStatus::PastDue) != state.grace_expires_at.is_some()
        {
            return Err(CommercialDomainError::InvalidTimestamp);
        }
        Ok(Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            holding_id,
            customer_id: state.customer_id,
            plan_id: state.plan_id,
            status: state.status,
            amount_minor: state.amount_minor,
            currency: state.currency,
            last_provider_event_id: state.last_provider_event_id,
            last_provider_event_at: state.last_provider_event_at,
            grace_expires_at: state.grace_expires_at,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        })
    }

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
            grace_expires_at: None,
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
        if signal == BillingSignal::GraceExpired
            && self
                .grace_expires_at
                .is_none_or(|deadline| provider_event_at < deadline)
        {
            return Err(CommercialDomainError::GracePeriodNotExpired);
        }
        self.version = next_version(self.version)?;
        self.status = next;
        self.grace_expires_at = match signal {
            BillingSignal::PaymentFailed => Some(
                provider_event_at
                    .checked_add_signed(Duration::days(7))
                    .ok_or(CommercialDomainError::InvalidTimestamp)?,
            ),
            BillingSignal::PaymentRecovered
            | BillingSignal::GraceExpired
            | BillingSignal::SubscriptionCanceled => None,
            BillingSignal::TrialEnded => self.grace_expires_at,
        };
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
    pub const fn grace_expires_at(&self) -> Option<DateTime<Utc>> {
        self.grace_expires_at
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
