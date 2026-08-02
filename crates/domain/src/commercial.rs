use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use thiserror::Error;
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BillingState {
    Trialing,
    Active,
    PastDue,
    Grace,
    ReadOnly,
    Canceled,
}

/// The only billing lifecycle events that can change a subscription state.
///
/// Payment recovery is deliberately explicit: a webhook or reconciliation job
/// must not silently turn a suspended subscription back into an active one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BillingEvent {
    TrialEnded,
    PaymentFailed,
    GraceStarted,
    GraceExpired,
    PaymentRecovered,
    SubscriptionCanceled,
}

/// The only source that may assert a subscription billing transition.
///
/// The application boundary must construct this value from a verified Stripe
/// event (or a reconciliation result), never from a client-supplied state
/// string. Keeping the provider event separate from [`BillingState`] prevents
/// a caller from treating the state enum as a setter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderBillingEvent {
    pub provider_event_id: String,
    pub created_at: DateTime<Utc>,
    pub event: BillingEvent,
}

impl ProviderBillingEvent {
    pub fn validate(&self) -> Result<(), CommercialCommandValidationError> {
        validate_reference(&self.provider_event_id, "provider_event_id")
    }
}

/// A verified provider entitlement update. Capacity and module activation are
/// payment-authority data; callers cannot construct this from a desired
/// billing state without first crossing the provider adapter boundary.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderEntitlementUpdate {
    pub provider_event_id: String,
    pub created_at: DateTime<Utc>,
    pub module_key: String,
    pub capacity: EntitlementCapacity,
    pub active: bool,
    pub active_until: Option<DateTime<Utc>>,
}

impl ProviderEntitlementUpdate {
    pub fn validate(&self) -> Result<(), CommercialCommandValidationError> {
        validate_reference(&self.provider_event_id, "provider_event_id")?;
        validate_reference(&self.module_key, "module_key")
    }
}

/// Metadata required by every non-provider commercial command. The raw
/// idempotency key is intentionally redacted from `Debug` output because
/// deployments commonly derive it from request credentials or signed jobs.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommercialCommandContext {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub correlation_id: Uuid,
    pub actor_ref: String,
    pub purpose: String,
}

impl std::fmt::Debug for CommercialCommandContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("CommercialCommandContext")
            .field("command_id", &self.command_id)
            .field("idempotency_key", &"<redacted>")
            .field("correlation_id", &self.correlation_id)
            .field("actor_ref", &self.actor_ref)
            .field("purpose", &self.purpose)
            .finish()
    }
}

impl CommercialCommandContext {
    pub fn new(
        command_id: Uuid,
        idempotency_key: impl Into<String>,
        correlation_id: Uuid,
        actor_ref: impl Into<String>,
        purpose: impl Into<String>,
    ) -> Result<Self, CommercialCommandValidationError> {
        let context = Self {
            command_id,
            idempotency_key: idempotency_key.into(),
            correlation_id,
            actor_ref: actor_ref.into(),
            purpose: purpose.into(),
        };
        context.validate()?;
        Ok(context)
    }

    pub fn validate(&self) -> Result<(), CommercialCommandValidationError> {
        if self.command_id.is_nil() || self.correlation_id.is_nil() {
            return Err(CommercialCommandValidationError::NilIdentifier);
        }
        validate_reference(&self.idempotency_key, "idempotency_key")?;
        if self.idempotency_key.len() > 512 {
            return Err(CommercialCommandValidationError::ReferenceTooLong(
                "idempotency_key",
            ));
        }
        validate_reference(&self.actor_ref, "actor_ref")?;
        validate_reference(&self.purpose, "purpose")
    }

    /// Provider commands use an event-derived idempotency key. A random
    /// command ID is safe because replay identity is the provider event key.
    pub fn for_provider(
        provider: &str,
        provider_event_id: &str,
    ) -> Result<Self, CommercialCommandValidationError> {
        validate_reference(provider, "provider")?;
        validate_reference(provider_event_id, "provider_event_id")?;
        Self::new(
            Uuid::new_v4(),
            format!("erpx:{provider}:{provider_event_id}"),
            Uuid::new_v4(),
            provider,
            "authoritative provider commercial event",
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingPolicyCommand {
    pub customer_id: Uuid,
    pub subscription_id: Uuid,
    pub at: DateTime<Utc>,
    pub context: CommercialCommandContext,
}

impl BillingPolicyCommand {
    pub fn validate(&self) -> Result<(), CommercialCommandValidationError> {
        validate_aggregate_ids(self.customer_id, self.subscription_id)?;
        self.context.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AllocateCompanyCommand {
    pub customer_id: Uuid,
    pub module_key: String,
    pub company_id: Uuid,
    pub seats: u32,
    pub branch_limit: u32,
    pub context: CommercialCommandContext,
}

impl AllocateCompanyCommand {
    pub fn validate(&self) -> Result<(), CommercialCommandValidationError> {
        validate_aggregate_ids(self.customer_id, self.company_id)?;
        validate_reference(&self.module_key, "module_key")?;
        if self.seats == 0 || self.branch_limit == 0 {
            return Err(CommercialCommandValidationError::InvalidQuantity);
        }
        self.context.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateCompanyModuleCommand {
    pub customer_id: Uuid,
    pub module_key: String,
    pub company_id: Uuid,
    pub at: DateTime<Utc>,
    pub context: CommercialCommandContext,
}

impl ActivateCompanyModuleCommand {
    pub fn validate(&self) -> Result<(), CommercialCommandValidationError> {
        validate_aggregate_ids(self.customer_id, self.company_id)?;
        validate_reference(&self.module_key, "module_key")?;
        self.context.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ActivateBranchModuleCommand {
    pub customer_id: Uuid,
    pub module_key: String,
    pub company_id: Uuid,
    pub branch_id: Uuid,
    pub at: DateTime<Utc>,
    pub context: CommercialCommandContext,
}

impl ActivateBranchModuleCommand {
    pub fn validate(&self) -> Result<(), CommercialCommandValidationError> {
        validate_aggregate_ids(self.customer_id, self.company_id)?;
        validate_aggregate_ids(self.customer_id, self.branch_id)?;
        validate_reference(&self.module_key, "module_key")?;
        self.context.validate()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum CommercialCommandValidationError {
    #[error("commercial command identifier cannot be nil")]
    NilIdentifier,
    #[error("{0} is required")]
    MissingReference(&'static str),
    #[error("{0} contains a control character")]
    ControlCharacter(&'static str),
    #[error("{0} exceeds its maximum length")]
    ReferenceTooLong(&'static str),
    #[error("command quantities must be positive")]
    InvalidQuantity,
    #[error("provider event is not for this entitlement module")]
    ProviderModuleMismatch,
}

fn validate_reference(
    value: &str,
    name: &'static str,
) -> Result<(), CommercialCommandValidationError> {
    if value.trim().is_empty() {
        return Err(CommercialCommandValidationError::MissingReference(name));
    }
    if value.chars().any(char::is_control) {
        return Err(CommercialCommandValidationError::ControlCharacter(name));
    }
    if value.len() > 255 {
        return Err(CommercialCommandValidationError::ReferenceTooLong(name));
    }
    Ok(())
}

fn validate_aggregate_ids(
    customer_id: Uuid,
    aggregate_id: Uuid,
) -> Result<(), CommercialCommandValidationError> {
    if customer_id.is_nil() || aggregate_id.is_nil() {
        return Err(CommercialCommandValidationError::NilIdentifier);
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommercialCommandChange {
    Applied,
    Unchanged,
    DuplicateOrOutOfOrder,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CommercialCommandResult {
    Billing {
        state: BillingState,
        change: CommercialCommandChange,
    },
    Entitlement {
        module_key: String,
        active: bool,
        capacity: EntitlementCapacity,
        change: CommercialCommandChange,
    },
    CompanyAllocated {
        module_key: String,
        company_id: Uuid,
    },
    CompanyModuleActivated {
        module_key: String,
        company_id: Uuid,
    },
    BranchModuleActivated {
        module_key: String,
        company_id: Uuid,
        branch_id: Uuid,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommercialCommandReceipt {
    pub command_id: Uuid,
    pub idempotency_key: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub result: CommercialCommandResult,
    pub replayed: bool,
}

/// Durable evidence emitted when a verified provider event is applied to a
/// billing lifecycle. This is intentionally a fact about the accepted
/// provider command, not a client-writable billing-state setter. It is safe to
/// place in an outbox payload because it contains no payment instrument,
/// credential, webhook body, or raw idempotency key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingProviderCommandEvidence {
    pub customer_id: Uuid,
    pub subscription_id: Uuid,
    pub provider_event_id: String,
    pub provider_event_at: DateTime<Utc>,
    pub event: BillingEvent,
    pub state: BillingState,
    pub change: CommercialCommandChange,
}

impl BillingProviderCommandEvidence {
    pub fn validate(&self) -> Result<(), CommercialCommandValidationError> {
        validate_aggregate_ids(self.customer_id, self.subscription_id)?;
        validate_reference(&self.provider_event_id, "provider_event_id")
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BillingPolicyChange {
    Unchanged,
    GraceStarted,
    ReadOnlyEntered,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum BillingError {
    #[error("invalid billing transition from {from:?} with event {event:?}")]
    InvalidTransition {
        from: BillingState,
        event: BillingEvent,
    },
    #[error("the billing grace period has not expired")]
    GraceNotExpired,
    #[error("billing timestamp overflow")]
    TimestampOverflow,
}

/// Stripe and reconciliation state is represented as a small state machine,
/// rather than as an arbitrary writable status string.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingLifecycle {
    pub state: BillingState,
    pub grace_started_at: Option<DateTime<Utc>>,
    pub grace_until: Option<DateTime<Utc>>,
    pub last_transition_at: DateTime<Utc>,
    /// Provider ordering metadata is stored in the JSON payload so the
    /// existing control-plane schema remains compatible with older rows.
    #[serde(default)]
    pub last_provider_event_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_provider_event_id: Option<String>,
}

impl BillingLifecycle {
    pub fn new(state: BillingState, at: DateTime<Utc>) -> Self {
        Self {
            state,
            grace_started_at: None,
            grace_until: None,
            last_transition_at: at,
            last_provider_event_at: None,
            last_provider_event_id: None,
        }
    }

    pub fn provider_event_is_stale(&self, created_at: DateTime<Utc>) -> bool {
        self.last_provider_event_at
            .is_some_and(|last| created_at <= last)
    }

    pub fn record_provider_event(
        &mut self,
        event_id: impl Into<String>,
        created_at: DateTime<Utc>,
    ) {
        self.last_provider_event_at = Some(created_at);
        self.last_provider_event_id = Some(event_id.into());
    }

    pub fn transition(
        &mut self,
        event: BillingEvent,
        at: DateTime<Utc>,
    ) -> Result<BillingState, BillingError> {
        let from = self.state;
        let next = match (from, event) {
            (BillingState::Trialing, BillingEvent::TrialEnded) => BillingState::Active,
            (BillingState::Active, BillingEvent::PaymentFailed) => BillingState::PastDue,
            (BillingState::PastDue, BillingEvent::GraceStarted) => BillingState::Grace,
            (BillingState::Grace, BillingEvent::GraceExpired) => {
                if self.grace_until.is_none_or(|until| at < until) {
                    return Err(BillingError::GraceNotExpired);
                }
                BillingState::ReadOnly
            }
            (
                BillingState::PastDue | BillingState::Grace | BillingState::ReadOnly,
                BillingEvent::PaymentRecovered,
            ) => BillingState::Active,
            (
                BillingState::Trialing
                | BillingState::Active
                | BillingState::PastDue
                | BillingState::Grace
                | BillingState::ReadOnly,
                BillingEvent::SubscriptionCanceled,
            ) => BillingState::Canceled,
            _ => {
                return Err(BillingError::InvalidTransition { from, event });
            }
        };

        if next == BillingState::Grace {
            self.grace_started_at = Some(at);
            self.grace_until = Some(
                at.checked_add_signed(DEFAULT_GRACE_PERIOD)
                    .ok_or(BillingError::TimestampOverflow)?,
            );
        } else if next == BillingState::Active || next == BillingState::Canceled {
            self.grace_started_at = None;
            self.grace_until = None;
        }
        self.state = next;
        self.last_transition_at = at;
        Ok(next)
    }

    /// Apply a verified provider event. The caller supplies an event, never a
    /// desired billing state, so payment recovery and cancellation remain
    /// provider-authoritative. Equal or older provider timestamps are safe
    /// replays and do not mutate the lifecycle.
    pub fn apply_provider_event(
        &mut self,
        event: &ProviderBillingEvent,
    ) -> Result<CommercialCommandChange, BillingError> {
        if self.provider_event_is_stale(event.created_at) {
            return Ok(CommercialCommandChange::DuplicateOrOutOfOrder);
        }
        let transition = effective_provider_event(self.state, event.event);
        let change = if let Some(transition) = transition {
            self.transition(transition, event.created_at)?;
            CommercialCommandChange::Applied
        } else {
            CommercialCommandChange::Unchanged
        };
        self.record_provider_event(event.provider_event_id.clone(), event.created_at);
        Ok(change)
    }

    /// Advance the payment-failure policy from the worker/system clock. A
    /// missed worker tick cannot extend grace: PastDue starts grace at the
    /// original failure timestamp and the exact `grace_until` boundary enters
    /// ReadOnly. This method cannot recover payment or choose another state.
    pub fn advance_payment_policy(
        &mut self,
        at: DateTime<Utc>,
    ) -> Result<BillingPolicyChange, BillingError> {
        let mut change = BillingPolicyChange::Unchanged;
        if self.state == BillingState::PastDue {
            self.transition(BillingEvent::GraceStarted, self.last_transition_at)?;
            change = BillingPolicyChange::GraceStarted;
        }
        if self.state == BillingState::Grace && self.grace_until.is_some_and(|until| at >= until) {
            self.transition(BillingEvent::GraceExpired, at)?;
            change = BillingPolicyChange::ReadOnlyEntered;
        }
        Ok(change)
    }
}

fn effective_provider_event(state: BillingState, event: BillingEvent) -> Option<BillingEvent> {
    match (state, event) {
        // Stripe subscription updates are often repeated while the state is
        // already settled. Record their provider watermark, but do not reject
        // the event as an invalid transition.
        (BillingState::Active, BillingEvent::TrialEnded)
        | (BillingState::Active, BillingEvent::PaymentRecovered)
        | (BillingState::Trialing, BillingEvent::PaymentRecovered)
        | (BillingState::PastDue, BillingEvent::PaymentFailed)
        | (BillingState::Grace, BillingEvent::PaymentFailed)
        | (BillingState::ReadOnly, BillingEvent::PaymentFailed)
        | (BillingState::Canceled, BillingEvent::SubscriptionCanceled) => None,
        // An active subscription update after a payment failure is the
        // provider's recovery signal, not another trial completion.
        (
            BillingState::PastDue | BillingState::Grace | BillingState::ReadOnly,
            BillingEvent::TrialEnded,
        ) => Some(BillingEvent::PaymentRecovered),
        _ => Some(event),
    }
}

pub const DEFAULT_GRACE_PERIOD_DAYS: i64 = 7;
pub const DEFAULT_GRACE_PERIOD: chrono::Duration = chrono::Duration::days(7);
pub const MAX_GRACE_EXTENSION: chrono::Duration = chrono::Duration::days(7);
pub const ACTIVE_CATALOG_CURRENCY: &str = "USD";

/// The single, bounded exception available for one delinquency cycle.
/// Persistence enforces uniqueness on `(customer_id, subscription_id,
/// delinquency_cycle_id)`; this value only validates the business rule.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BillingGraceExtension {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub subscription_id: Uuid,
    pub delinquency_cycle_id: Uuid,
    pub previous_grace_until: DateTime<Utc>,
    pub extended_until: DateTime<Utc>,
    pub granted_at: DateTime<Utc>,
    pub granted_by: String,
    pub reason: String,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BillingGraceExtensionGrant {
    pub customer_id: Uuid,
    pub subscription_id: Uuid,
    pub delinquency_cycle_id: Uuid,
    pub previous_grace_until: DateTime<Utc>,
    pub extension: chrono::Duration,
    pub granted_at: DateTime<Utc>,
    pub granted_by: String,
    pub reason: String,
    pub correlation_id: Uuid,
}

impl BillingGraceExtension {
    pub fn grant(request: BillingGraceExtensionGrant) -> Result<Self, BillingGraceExtensionError> {
        if request.customer_id.is_nil()
            || request.subscription_id.is_nil()
            || request.delinquency_cycle_id.is_nil()
            || request.correlation_id.is_nil()
        {
            return Err(BillingGraceExtensionError::NilIdentifier);
        }
        if request.extension <= chrono::Duration::zero() || request.extension > MAX_GRACE_EXTENSION
        {
            return Err(BillingGraceExtensionError::InvalidDuration);
        }
        if request.granted_by.trim().is_empty()
            || request.reason.trim().is_empty()
            || request.granted_by.chars().any(char::is_control)
            || request.reason.chars().any(char::is_control)
            || request.reason.len() > 512
        {
            return Err(BillingGraceExtensionError::MissingEvidence);
        }
        let extended_until = request
            .previous_grace_until
            .checked_add_signed(request.extension)
            .ok_or(BillingGraceExtensionError::TimestampOverflow)?;
        Ok(Self {
            id: Uuid::new_v4(),
            customer_id: request.customer_id,
            subscription_id: request.subscription_id,
            delinquency_cycle_id: request.delinquency_cycle_id,
            previous_grace_until: request.previous_grace_until,
            extended_until,
            granted_at: request.granted_at,
            granted_by: request.granted_by,
            reason: request.reason,
            correlation_id: request.correlation_id,
        })
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum BillingGraceExtensionError {
    #[error("billing grace extension identifier cannot be nil")]
    NilIdentifier,
    #[error("billing grace extension must be positive and at most seven days")]
    InvalidDuration,
    #[error("billing grace extension requires a bounded reason and actor")]
    MissingEvidence,
    #[error("billing grace extension timestamp overflow")]
    TimestampOverflow,
}

/// UI-safe subscription intent. Provider price identifiers and amounts are
/// absent by construction; the server resolves them from the published
/// catalog and its internal provider binding.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SubscriptionItemSelection {
    pub item_key: String,
    pub quantity: u32,
}

/// Resolve and validate the single currency for a set of requested catalog
/// items. ERPX persistence remains ISO-currency aware, while the first active
/// checkout currency is intentionally USD only.
pub fn active_subscription_currency(
    catalog: &PublishedCommercialCatalogRevision,
    selections: &[SubscriptionItemSelection],
) -> Result<String, SubscriptionSelectionError> {
    if selections.is_empty() {
        return Err(SubscriptionSelectionError::MissingItems);
    }
    let mut currency = None::<&str>;
    let mut keys = BTreeSet::new();
    for selection in selections {
        if selection.quantity == 0
            || selection.item_key.trim().is_empty()
            || !keys.insert(selection.item_key.as_str())
        {
            return Err(SubscriptionSelectionError::InvalidItem);
        }
        let price = catalog
            .price(&selection.item_key)
            .ok_or(SubscriptionSelectionError::UnknownItem)?;
        match currency {
            Some(expected) if expected != price.amount.currency => {
                return Err(SubscriptionSelectionError::MixedCurrency);
            }
            None => currency = Some(&price.amount.currency),
            Some(_) => {}
        }
    }
    let currency = currency.expect("non-empty subscription selections have a currency");
    if currency != ACTIVE_CATALOG_CURRENCY {
        return Err(SubscriptionSelectionError::InactiveCurrency);
    }
    Ok(currency.to_owned())
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum SubscriptionSelectionError {
    #[error("subscription must contain at least one catalog item")]
    MissingItems,
    #[error("subscription item key and quantity are invalid or duplicated")]
    InvalidItem,
    #[error("subscription catalog item does not exist")]
    UnknownItem,
    #[error("subscription items cannot mix currencies")]
    MixedCurrency,
    #[error("catalog currency is not active for checkout")]
    InactiveCurrency,
}

/// Capabilities deliberately retained while a subscription is read-only.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BillingCapability {
    Login,
    Billing,
    Export,
    Support,
    Recovery,
    NonMutatingReports,
}

impl BillingState {
    pub const fn allows_read_only_capability(self, capability: BillingCapability) -> bool {
        match self {
            Self::ReadOnly => matches!(
                capability,
                BillingCapability::Login
                    | BillingCapability::Billing
                    | BillingCapability::Export
                    | BillingCapability::Support
                    | BillingCapability::Recovery
                    | BillingCapability::NonMutatingReports
            ),
            _ => false,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BillingModel {
    Flat,
    PerSeat,
    PerCompany,
    PerBranch,
    OneTime,
    Metered,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CatalogPriceKind {
    BaseFee,
    Module,
    SetupFee,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum PricingUnit {
    Flat,
    Seat,
    Company,
    Branch,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum BillingInterval {
    Monthly,
    Yearly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ChargeKind {
    OneTime,
    Recurring(BillingInterval),
    Metered,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    pub amount_minor: u64,
    pub currency: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TrialPeriod {
    pub days: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum CouponDiscount {
    Percentage(u8),
    Fixed(Money),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Coupon {
    pub code: String,
    pub discount: CouponDiscount,
    pub valid_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OveragePricing {
    pub unit_amount: Money,
    pub unit_name: String,
}

/// A fully described commercial price. Keeping the commercial shape separate
/// from the original `CatalogItem` preserves compatibility with the first
/// catalog prototype while making every pricing dimension explicit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogPrice {
    pub item_key: String,
    pub display_name: String,
    pub kind: CatalogPriceKind,
    pub pricing_unit: PricingUnit,
    pub charge: ChargeKind,
    pub amount: Money,
    pub trial: Option<TrialPeriod>,
    pub coupon: Option<Coupon>,
    pub setup_fee: Option<Money>,
    pub included_usage: Option<u64>,
    pub overage: Option<OveragePricing>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommercialCatalogRevisionDraft {
    pub id: Uuid,
    pub revision_number: u32,
    pub prices: Vec<CatalogPrice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PublishedCommercialCatalogRevision {
    id: Uuid,
    revision_number: u32,
    published_at: DateTime<Utc>,
    published_by: String,
    prices: Vec<CatalogPrice>,
}

impl PublishedCommercialCatalogRevision {
    pub fn id(&self) -> Uuid {
        self.id
    }

    pub fn revision_number(&self) -> u32 {
        self.revision_number
    }

    pub fn published_at(&self) -> DateTime<Utc> {
        self.published_at
    }

    pub fn published_by(&self) -> &str {
        &self.published_by
    }

    pub fn prices(&self) -> &[CatalogPrice] {
        &self.prices
    }

    pub fn price(&self, item_key: &str) -> Option<&CatalogPrice> {
        self.prices.iter().find(|price| price.item_key == item_key)
    }

    pub fn validate(&self) -> Result<(), CatalogError> {
        if self.revision_number == 0 {
            return Err(CatalogError::InvalidRevisionNumber);
        }
        if self.published_by.trim().is_empty() {
            return Err(CatalogError::MissingActor);
        }
        validate_prices(&self.prices)
    }
}

impl CommercialCatalogRevisionDraft {
    pub fn publish(
        self,
        at: DateTime<Utc>,
        actor: impl Into<String>,
    ) -> Result<PublishedCommercialCatalogRevision, CatalogError> {
        if self.revision_number == 0 {
            return Err(CatalogError::InvalidRevisionNumber);
        }
        validate_prices(&self.prices)?;
        let actor = actor.into();
        if actor.trim().is_empty() {
            return Err(CatalogError::MissingActor);
        }
        Ok(PublishedCommercialCatalogRevision {
            id: self.id,
            revision_number: self.revision_number,
            published_at: at,
            published_by: actor,
            prices: self.prices,
        })
    }
}

fn validate_prices(prices: &[CatalogPrice]) -> Result<(), CatalogError> {
    let mut keys = BTreeSet::new();
    let mut base_fee_dimensions = BTreeSet::new();
    for price in prices {
        if price.item_key.trim().is_empty() || !keys.insert(price.item_key.as_str()) {
            return Err(CatalogError::DuplicateItemKey);
        }
        validate_price(price)?;
        if matches!(price.kind, CatalogPriceKind::BaseFee) {
            let charge = match price.charge {
                ChargeKind::Recurring(BillingInterval::Monthly) => "monthly",
                ChargeKind::Recurring(BillingInterval::Yearly) => "yearly",
                ChargeKind::OneTime => "one_time",
                ChargeKind::Metered => "metered",
            };
            if !base_fee_dimensions.insert((price.amount.currency.as_str(), charge)) {
                return Err(CatalogError::ExactlyOneBaseFee);
            }
        }
    }
    if base_fee_dimensions.is_empty() {
        return Err(CatalogError::ExactlyOneBaseFee);
    }
    Ok(())
}

fn validate_price(price: &CatalogPrice) -> Result<(), CatalogError> {
    if price.display_name.trim().is_empty() {
        return Err(CatalogError::MissingDisplayName);
    }
    validate_money(&price.amount)?;
    match price.kind {
        CatalogPriceKind::BaseFee if price.pricing_unit != PricingUnit::Flat => {
            return Err(CatalogError::BaseFeeRequiresFlatPricing);
        }
        CatalogPriceKind::Module if price.pricing_unit == PricingUnit::Flat => {
            return Err(CatalogError::ModuleRequiresScopedPricing);
        }
        CatalogPriceKind::SetupFee
            if price.pricing_unit != PricingUnit::Flat || price.charge != ChargeKind::OneTime =>
        {
            return Err(CatalogError::SetupFeeRequiresOneTimeFlatPricing);
        }
        _ => {}
    }
    match price.charge {
        ChargeKind::OneTime if price.trial.is_some() => {
            return Err(CatalogError::TrialRequiresRecurring);
        }
        ChargeKind::Metered if price.trial.is_some() => {
            return Err(CatalogError::TrialRequiresRecurring);
        }
        ChargeKind::Metered if price.pricing_unit == PricingUnit::Flat => {
            return Err(CatalogError::MeteredRequiresScopedPricing);
        }
        ChargeKind::Recurring(_) | ChargeKind::Metered | ChargeKind::OneTime => {}
    }
    if price.included_usage.is_some() && price.charge != ChargeKind::Metered {
        return Err(CatalogError::IncludedUsageRequiresMetered);
    }
    if let Some(trial) = &price.trial
        && trial.days == 0
    {
        return Err(CatalogError::InvalidTrial);
    }
    if let Some(coupon) = &price.coupon {
        if coupon.code.trim().is_empty() {
            return Err(CatalogError::InvalidCoupon);
        }
        match &coupon.discount {
            CouponDiscount::Percentage(value) if *value == 0 || *value > 100 => {
                return Err(CatalogError::InvalidCoupon);
            }
            CouponDiscount::Fixed(money) => {
                validate_money(money)?;
                if money.currency != price.amount.currency {
                    return Err(CatalogError::CurrencyMismatch);
                }
            }
            CouponDiscount::Percentage(_) => {}
        }
    }
    if let Some(setup_fee) = &price.setup_fee {
        validate_money(setup_fee)?;
        if setup_fee.currency != price.amount.currency {
            return Err(CatalogError::CurrencyMismatch);
        }
    }
    if price.charge != ChargeKind::Metered && price.overage.is_some() {
        return Err(CatalogError::OverageRequiresMetered);
    }
    if let Some(overage) = &price.overage {
        validate_money(&overage.unit_amount)?;
        if overage.unit_amount.currency != price.amount.currency
            || overage.unit_name.trim().is_empty()
        {
            return Err(CatalogError::InvalidOverage);
        }
    }
    Ok(())
}

fn validate_money(money: &Money) -> Result<(), CatalogError> {
    if money.amount_minor == 0 {
        return Err(CatalogError::InvalidAmount);
    }
    if money.currency.len() != 3
        || !money
            .currency
            .chars()
            .all(|character| character.is_ascii_uppercase())
    {
        return Err(CatalogError::InvalidCurrency);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrandfatheredPriceMigration {
    pub id: Uuid,
    pub customer_id: Uuid,
    pub subscription_id: Uuid,
    pub item_key: String,
    pub from_revision: u32,
    pub to_revision: u32,
    pub previous_price: Money,
    pub migrated_price: Money,
    pub effective_at: DateTime<Utc>,
    pub reason: String,
    pub recorded_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GrandfatheredPriceMigrationRequest {
    pub customer_id: Uuid,
    pub subscription_id: Uuid,
    pub item_key: String,
    pub effective_at: DateTime<Utc>,
    pub reason: String,
    pub recorded_by: String,
}

impl PublishedCommercialCatalogRevision {
    pub fn record_grandfathered_price_migration(
        &self,
        previous: &PublishedCommercialCatalogRevision,
        request: GrandfatheredPriceMigrationRequest,
    ) -> Result<GrandfatheredPriceMigration, CatalogError> {
        let previous_price = previous
            .price(&request.item_key)
            .ok_or(CatalogError::PriceNotFound)?;
        let migrated_price = self
            .price(&request.item_key)
            .ok_or(CatalogError::PriceNotFound)?;
        if previous.revision_number >= self.revision_number {
            return Err(CatalogError::RevisionOrder);
        }
        if request.reason.trim().is_empty() || request.recorded_by.trim().is_empty() {
            return Err(CatalogError::MissingMigrationMetadata);
        }
        Ok(GrandfatheredPriceMigration {
            id: Uuid::new_v4(),
            customer_id: request.customer_id,
            subscription_id: request.subscription_id,
            item_key: request.item_key,
            from_revision: previous.revision_number,
            to_revision: self.revision_number,
            previous_price: previous_price.amount.clone(),
            migrated_price: migrated_price.amount.clone(),
            effective_at: request.effective_at,
            reason: request.reason,
            recorded_by: request.recorded_by,
        })
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum CatalogError {
    #[error("published catalog revisions are immutable")]
    Immutable,
    #[error("published price must have a positive amount")]
    InvalidAmount,
    #[error("catalog revision number must be positive")]
    InvalidRevisionNumber,
    #[error("catalog revision must contain one base fee per currency and billing interval")]
    ExactlyOneBaseFee,
    #[error("catalog item keys must be non-empty and unique")]
    DuplicateItemKey,
    #[error("catalog price display name is required")]
    MissingDisplayName,
    #[error("base fee pricing must use a flat unit")]
    BaseFeeRequiresFlatPricing,
    #[error("module pricing must use a seat, company, or branch unit")]
    ModuleRequiresScopedPricing,
    #[error("setup fees must be one-time flat prices")]
    SetupFeeRequiresOneTimeFlatPricing,
    #[error("metered pricing must use a seat, company, or branch unit")]
    MeteredRequiresScopedPricing,
    #[error("included usage is only valid for metered pricing")]
    IncludedUsageRequiresMetered,
    #[error("currency must be a three-letter uppercase ISO code")]
    InvalidCurrency,
    #[error("trial periods require recurring pricing")]
    TrialRequiresRecurring,
    #[error("trial period must be at least one day")]
    InvalidTrial,
    #[error("coupon code or discount is invalid")]
    InvalidCoupon,
    #[error("coupon/setup fee currencies must match the catalog price")]
    CurrencyMismatch,
    #[error("overage pricing requires metered pricing")]
    OverageRequiresMetered,
    #[error("overage pricing is invalid")]
    InvalidOverage,
    #[error("an actor is required to publish a catalog revision")]
    MissingActor,
    #[error("the requested catalog price does not exist")]
    PriceNotFound,
    #[error("grandfathered migration revisions must be ordered")]
    RevisionOrder,
    #[error("migration reason and actor are required")]
    MissingMigrationMetadata,
}

/// Normalized catalog state supplied by an external billing provider. The
/// provider adapter owns translation from provider-specific products/prices;
/// this domain type contains no network or SDK dependency.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCatalogRevision {
    pub provider_revision_ref: String,
    pub revision_number: u32,
    pub prices: Vec<ProviderCatalogPrice>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderCatalogPrice {
    pub product_ref: Option<String>,
    pub price_ref: Option<String>,
    pub price: CatalogPrice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum CatalogMismatchKind {
    MissingLocalRevision,
    RevisionNumberMismatch,
    ProviderRevisionReferenceMissing,
    LocalInvariantViolation,
    ProviderInvariantViolation,
    MissingProviderPrice,
    UnexpectedProviderPrice,
    DuplicateProviderPrice,
    ProviderProductReferenceMissing,
    ProviderPriceReferenceMissing,
    PriceFieldMismatch,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogMismatch {
    pub kind: CatalogMismatchKind,
    pub item_key: Option<String>,
    pub field: Option<String>,
    pub local: Option<String>,
    pub provider: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogReconciliationReport {
    pub local_revision_number: Option<u32>,
    pub provider_revision_number: u32,
    pub provider_revision_ref: String,
    pub mismatches: Vec<CatalogMismatch>,
}

impl CatalogReconciliationReport {
    pub fn compare(
        local: Option<&PublishedCommercialCatalogRevision>,
        provider: &ProviderCatalogRevision,
    ) -> Self {
        let mut report = Self {
            local_revision_number: local.map(PublishedCommercialCatalogRevision::revision_number),
            provider_revision_number: provider.revision_number,
            provider_revision_ref: provider.provider_revision_ref.clone(),
            mismatches: Vec::new(),
        };

        let Some(local) = local else {
            report.mismatches.push(CatalogMismatch {
                kind: CatalogMismatchKind::MissingLocalRevision,
                item_key: None,
                field: None,
                local: None,
                provider: Some(provider.provider_revision_ref.clone()),
            });
            report.finish();
            return report;
        };

        if local.revision_number() != provider.revision_number {
            report.mismatches.push(CatalogMismatch {
                kind: CatalogMismatchKind::RevisionNumberMismatch,
                item_key: None,
                field: Some("revision_number".into()),
                local: Some(local.revision_number().to_string()),
                provider: Some(provider.revision_number.to_string()),
            });
        }
        if provider.provider_revision_ref.trim().is_empty() {
            report.mismatches.push(CatalogMismatch {
                kind: CatalogMismatchKind::ProviderRevisionReferenceMissing,
                item_key: None,
                field: Some("provider_revision_ref".into()),
                local: None,
                provider: Some(provider.provider_revision_ref.clone()),
            });
        }
        if let Err(error) = local.validate() {
            report.mismatches.push(CatalogMismatch {
                kind: CatalogMismatchKind::LocalInvariantViolation,
                item_key: None,
                field: None,
                local: Some(error.to_string()),
                provider: None,
            });
        }
        if let Err(error) = validate_provider_prices(provider) {
            report.mismatches.push(CatalogMismatch {
                kind: CatalogMismatchKind::ProviderInvariantViolation,
                item_key: None,
                field: None,
                local: None,
                provider: Some(error.to_string()),
            });
        }

        let local_prices = local
            .prices()
            .iter()
            .map(|price| (price.item_key.clone(), price))
            .collect::<BTreeMap<_, _>>();
        let mut provider_prices = BTreeMap::new();
        for entry in &provider.prices {
            let key = entry.price.item_key.clone();
            if provider_prices.insert(key.clone(), entry).is_some() {
                report.mismatches.push(CatalogMismatch {
                    kind: CatalogMismatchKind::DuplicateProviderPrice,
                    item_key: Some(key),
                    field: Some("item_key".into()),
                    local: None,
                    provider: Some("duplicate".into()),
                });
            }
        }

        for (item_key, local_price) in &local_prices {
            let Some(provider_price) = provider_prices.get(item_key) else {
                report.mismatches.push(CatalogMismatch {
                    kind: CatalogMismatchKind::MissingProviderPrice,
                    item_key: Some(item_key.clone()),
                    field: None,
                    local: Some(canonical_value(local_price)),
                    provider: None,
                });
                continue;
            };
            compare_provider_references(&mut report, item_key, provider_price);
            compare_price_fields(&mut report, item_key, local_price, &provider_price.price);
        }
        for (item_key, provider_price) in &provider_prices {
            if !local_prices.contains_key(item_key) {
                report.mismatches.push(CatalogMismatch {
                    kind: CatalogMismatchKind::UnexpectedProviderPrice,
                    item_key: Some(item_key.clone()),
                    field: None,
                    local: None,
                    provider: Some(canonical_value(&provider_price.price)),
                });
            }
        }
        report.finish();
        report
    }

    pub fn is_in_sync(&self) -> bool {
        self.mismatches.is_empty()
    }

    fn finish(&mut self) {
        self.mismatches.sort_by(|left, right| {
            left.kind
                .cmp(&right.kind)
                .then_with(|| left.item_key.cmp(&right.item_key))
                .then_with(|| left.field.cmp(&right.field))
                .then_with(|| left.local.cmp(&right.local))
                .then_with(|| left.provider.cmp(&right.provider))
        });
    }
}

impl PublishedCommercialCatalogRevision {
    pub fn reconcile_with_provider(
        &self,
        provider: &ProviderCatalogRevision,
    ) -> CatalogReconciliationReport {
        CatalogReconciliationReport::compare(Some(self), provider)
    }
}

fn validate_provider_prices(provider: &ProviderCatalogRevision) -> Result<(), CatalogError> {
    let prices = provider
        .prices
        .iter()
        .map(|entry| entry.price.clone())
        .collect::<Vec<_>>();
    validate_prices(&prices)
}

fn compare_provider_references(
    report: &mut CatalogReconciliationReport,
    item_key: &str,
    provider: &ProviderCatalogPrice,
) {
    if provider
        .product_ref
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        report.mismatches.push(CatalogMismatch {
            kind: CatalogMismatchKind::ProviderProductReferenceMissing,
            item_key: Some(item_key.into()),
            field: Some("product_ref".into()),
            local: None,
            provider: provider.product_ref.clone(),
        });
    }
    if provider
        .price_ref
        .as_deref()
        .is_none_or(|value| value.trim().is_empty())
    {
        report.mismatches.push(CatalogMismatch {
            kind: CatalogMismatchKind::ProviderPriceReferenceMissing,
            item_key: Some(item_key.into()),
            field: Some("price_ref".into()),
            local: None,
            provider: provider.price_ref.clone(),
        });
    }
}

fn compare_price_fields(
    report: &mut CatalogReconciliationReport,
    item_key: &str,
    local: &CatalogPrice,
    provider: &CatalogPrice,
) {
    compare_price_field(
        report,
        item_key,
        "display_name",
        &local.display_name,
        &provider.display_name,
    );
    compare_price_field(report, item_key, "kind", &local.kind, &provider.kind);
    compare_price_field(
        report,
        item_key,
        "pricing_unit",
        &local.pricing_unit,
        &provider.pricing_unit,
    );
    compare_price_field(report, item_key, "charge", &local.charge, &provider.charge);
    compare_price_field(report, item_key, "amount", &local.amount, &provider.amount);
    compare_price_field(report, item_key, "trial", &local.trial, &provider.trial);
    compare_price_field(report, item_key, "coupon", &local.coupon, &provider.coupon);
    compare_price_field(
        report,
        item_key,
        "setup_fee",
        &local.setup_fee,
        &provider.setup_fee,
    );
    compare_price_field(
        report,
        item_key,
        "included_usage",
        &local.included_usage,
        &provider.included_usage,
    );
    compare_price_field(
        report,
        item_key,
        "overage",
        &local.overage,
        &provider.overage,
    );
}

fn compare_price_field<T: PartialEq + Serialize>(
    report: &mut CatalogReconciliationReport,
    item_key: &str,
    field: &str,
    local: &T,
    provider: &T,
) {
    if local != provider {
        report.mismatches.push(CatalogMismatch {
            kind: CatalogMismatchKind::PriceFieldMismatch,
            item_key: Some(item_key.into()),
            field: Some(field.into()),
            local: Some(canonical_value(local)),
            provider: Some(canonical_value(provider)),
        });
    }
}

fn canonical_value<T: Serialize>(value: &T) -> String {
    serde_json::to_string(value).unwrap_or_else(|_| "<serialization-error>".into())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogItem {
    pub item_key: String,
    pub display_name: String,
    pub billing_model: BillingModel,
    pub amount_minor: u64,
    pub currency: String,
    pub interval: Option<String>,
    pub included_quantity: u64,
    pub stripe_product_id: Option<String>,
    pub stripe_price_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CatalogRevision {
    pub id: Uuid,
    pub revision_number: u32,
    pub published_at: Option<DateTime<Utc>>,
    pub published_by: Option<String>,
    pub items: Vec<CatalogItem>,
}

impl CatalogRevision {
    pub fn publish(
        &mut self,
        at: DateTime<Utc>,
        actor: impl Into<String>,
    ) -> Result<(), CatalogError> {
        if self.published_at.is_some() {
            return Err(CatalogError::Immutable);
        }
        if self
            .items
            .iter()
            .any(|item| item.amount_minor == 0 && item.billing_model != BillingModel::Metered)
        {
            return Err(CatalogError::InvalidAmount);
        }
        self.published_at = Some(at);
        self.published_by = Some(actor.into());
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleEntitlement {
    pub module_key: String,
    pub active: bool,
    pub seats: u32,
    pub company_limit: u32,
    pub branch_limit: u32,
    pub active_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitlementCapacity {
    pub seats: u32,
    pub companies: u32,
    pub branches: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CompanyEntitlementAllocation {
    pub company_id: Uuid,
    pub seats: u32,
    pub branch_limit: u32,
    pub active: bool,
    pub active_branches: BTreeSet<Uuid>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EntitlementAllocationBook {
    pub module_key: String,
    pub active: bool,
    pub active_until: Option<DateTime<Utc>>,
    pub capacity: EntitlementCapacity,
    /// Provider ordering metadata is stored in the JSON payload so the
    /// existing control-plane schema remains compatible with older rows.
    #[serde(default)]
    pub last_provider_event_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub last_provider_event_id: Option<String>,
    allocations: BTreeMap<Uuid, CompanyEntitlementAllocation>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum EntitlementAllocationError {
    #[error("company already has an allocation for this module")]
    DuplicateCompany,
    #[error("requested allocation exceeds purchased entitlement capacity")]
    CapacityExceeded,
    #[error("allocation quantities must be positive")]
    InvalidQuantity,
    #[error("company has no allocation for this module")]
    CompanyNotAllocated,
    #[error("company module is not activated")]
    CompanyNotActivated,
    #[error("module is not activated")]
    ModuleNotActivated,
    #[error("branch is already activated")]
    DuplicateBranch,
    #[error("branch allocation exceeds the company allocation")]
    BranchCapacityExceeded,
    #[error("purchased capacity cannot be below existing allocations")]
    CapacityReductionBelowAllocated,
    #[error("provider event is not for this entitlement module")]
    ProviderModuleMismatch,
}

impl EntitlementAllocationBook {
    pub fn new(
        module_key: impl Into<String>,
        capacity: EntitlementCapacity,
        active_until: Option<DateTime<Utc>>,
    ) -> Self {
        Self {
            module_key: module_key.into(),
            active: false,
            active_until,
            capacity,
            last_provider_event_at: None,
            last_provider_event_id: None,
            allocations: BTreeMap::new(),
        }
    }

    pub fn provider_event_is_stale(&self, created_at: DateTime<Utc>) -> bool {
        self.last_provider_event_at
            .is_some_and(|last| created_at <= last)
    }

    pub fn record_provider_event(
        &mut self,
        event_id: impl Into<String>,
        created_at: DateTime<Utc>,
    ) {
        self.last_provider_event_at = Some(created_at);
        self.last_provider_event_id = Some(event_id.into());
    }

    /// Apply capacity and module activation only from a provider-authenticated
    /// update. An older/equal provider timestamp is a replay and cannot roll
    /// back a newer entitlement snapshot.
    pub fn apply_provider_update(
        &mut self,
        update: &ProviderEntitlementUpdate,
    ) -> Result<CommercialCommandChange, EntitlementAllocationError> {
        if self.module_key != update.module_key {
            return Err(EntitlementAllocationError::ProviderModuleMismatch);
        }
        if self.provider_event_is_stale(update.created_at) {
            return Ok(CommercialCommandChange::DuplicateOrOutOfOrder);
        }
        self.apply_purchased_capacity(update.capacity)?;
        self.set_active(update.active, update.active_until);
        self.record_provider_event(update.provider_event_id.clone(), update.created_at);
        Ok(CommercialCommandChange::Applied)
    }

    pub fn apply_purchased_capacity(
        &mut self,
        capacity: EntitlementCapacity,
    ) -> Result<(), EntitlementAllocationError> {
        if self.allocations.len() > capacity.companies as usize
            || self.allocated_seats() > capacity.seats
            || self.allocated_branches() > capacity.branches
        {
            return Err(EntitlementAllocationError::CapacityReductionBelowAllocated);
        }
        self.capacity = capacity;
        Ok(())
    }

    pub fn set_active(&mut self, active: bool, active_until: Option<DateTime<Utc>>) {
        self.active = active;
        self.active_until = active_until;
    }

    pub fn allocations(&self) -> impl Iterator<Item = &CompanyEntitlementAllocation> {
        self.allocations.values()
    }

    pub fn allocation(&self, company_id: Uuid) -> Option<&CompanyEntitlementAllocation> {
        self.allocations.get(&company_id)
    }

    pub fn allocated_seats(&self) -> u32 {
        self.allocations
            .values()
            .map(|allocation| allocation.seats)
            .sum()
    }

    pub fn allocated_branches(&self) -> u32 {
        self.allocations
            .values()
            .map(|allocation| allocation.branch_limit)
            .sum()
    }

    /// Exposes only the purchased module capacity needed by the universal
    /// access evaluator. Company/branch activation remains enforced by the
    /// allocation book itself rather than by client-provided booleans.
    pub fn module_entitlement(&self) -> ModuleEntitlement {
        ModuleEntitlement {
            module_key: self.module_key.clone(),
            active: self.active,
            seats: self.capacity.seats,
            company_limit: self.capacity.companies,
            branch_limit: self.capacity.branches,
            active_until: self.active_until,
        }
    }

    pub fn allocate_company(
        &mut self,
        company_id: Uuid,
        seats: u32,
        branch_limit: u32,
    ) -> Result<(), EntitlementAllocationError> {
        if seats == 0 || branch_limit == 0 {
            return Err(EntitlementAllocationError::InvalidQuantity);
        }
        if self.allocations.contains_key(&company_id) {
            return Err(EntitlementAllocationError::DuplicateCompany);
        }
        let companies_after = self
            .allocations
            .len()
            .checked_add(1)
            .ok_or(EntitlementAllocationError::CapacityExceeded)?;
        let seats_after = self
            .allocated_seats()
            .checked_add(seats)
            .ok_or(EntitlementAllocationError::CapacityExceeded)?;
        let branches_after = self
            .allocated_branches()
            .checked_add(branch_limit)
            .ok_or(EntitlementAllocationError::CapacityExceeded)?;
        if companies_after > self.capacity.companies as usize
            || seats_after > self.capacity.seats
            || branches_after > self.capacity.branches
        {
            return Err(EntitlementAllocationError::CapacityExceeded);
        }
        self.allocations.insert(
            company_id,
            CompanyEntitlementAllocation {
                company_id,
                seats,
                branch_limit,
                active: false,
                active_branches: BTreeSet::new(),
            },
        );
        Ok(())
    }

    pub fn activate_company(&mut self, company_id: Uuid) -> Result<(), EntitlementAllocationError> {
        if !self.active {
            return Err(EntitlementAllocationError::ModuleNotActivated);
        }
        let allocation = self
            .allocations
            .get_mut(&company_id)
            .ok_or(EntitlementAllocationError::CompanyNotAllocated)?;
        allocation.active = true;
        Ok(())
    }

    pub fn activate_company_at(
        &mut self,
        company_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), EntitlementAllocationError> {
        if self.active_until.is_some_and(|until| until <= at) {
            return Err(EntitlementAllocationError::ModuleNotActivated);
        }
        self.activate_company(company_id)
    }

    pub fn activate_branch(
        &mut self,
        company_id: Uuid,
        branch_id: Uuid,
    ) -> Result<(), EntitlementAllocationError> {
        let allocation = self
            .allocations
            .get_mut(&company_id)
            .ok_or(EntitlementAllocationError::CompanyNotAllocated)?;
        if !allocation.active {
            return Err(EntitlementAllocationError::CompanyNotActivated);
        }
        if allocation.active_branches.contains(&branch_id) {
            return Err(EntitlementAllocationError::DuplicateBranch);
        }
        if allocation.active_branches.len() >= allocation.branch_limit as usize {
            return Err(EntitlementAllocationError::BranchCapacityExceeded);
        }
        allocation.active_branches.insert(branch_id);
        Ok(())
    }

    pub fn activate_branch_at(
        &mut self,
        company_id: Uuid,
        branch_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<(), EntitlementAllocationError> {
        if !self.active || self.active_until.is_some_and(|until| until <= at) {
            return Err(EntitlementAllocationError::ModuleNotActivated);
        }
        self.activate_branch(company_id, branch_id)
    }

    pub fn authorize(
        &self,
        billing: BillingState,
        company_id: Uuid,
        branch_id: Option<Uuid>,
        rbac_allows: bool,
        requested_at: DateTime<Utc>,
        is_mutation: bool,
    ) -> EntitlementDecision {
        let entitlement = self.module_entitlement();
        if authorize_module(
            billing,
            Some(&entitlement),
            rbac_allows,
            requested_at,
            is_mutation,
        ) != EntitlementDecision::Allowed
        {
            return authorize_module(
                billing,
                Some(&entitlement),
                rbac_allows,
                requested_at,
                is_mutation,
            );
        }
        let Some(allocation) = self.allocations.get(&company_id) else {
            return EntitlementDecision::Denied("company_not_allocated");
        };
        if !allocation.active {
            return EntitlementDecision::Denied("company_module_not_active");
        }
        if let Some(branch_id) = branch_id
            && !allocation.active_branches.contains(&branch_id)
        {
            return EntitlementDecision::Denied("branch_not_activated");
        }
        EntitlementDecision::Allowed
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EntitlementDecision {
    Allowed,
    Denied(&'static str),
}

pub fn authorize_module(
    billing: BillingState,
    entitlement: Option<&ModuleEntitlement>,
    rbac_allows: bool,
    requested_at: DateTime<Utc>,
    is_mutation: bool,
) -> EntitlementDecision {
    if billing == BillingState::Canceled {
        return EntitlementDecision::Denied("subscription_canceled");
    }
    if is_mutation && billing == BillingState::ReadOnly {
        return EntitlementDecision::Denied("payment_failure_read_only");
    }
    let Some(entitlement) = entitlement else {
        return EntitlementDecision::Denied("module_not_entitled");
    };
    if !entitlement.active
        || entitlement
            .active_until
            .is_some_and(|until| until <= requested_at)
    {
        return EntitlementDecision::Denied("module_not_active");
    }
    if !rbac_allows {
        return EntitlementDecision::Denied("rbac_denied");
    }
    EntitlementDecision::Allowed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn commercial_price(
        item_key: &str,
        kind: CatalogPriceKind,
        pricing_unit: PricingUnit,
        charge: ChargeKind,
        amount_minor: u64,
    ) -> CatalogPrice {
        CatalogPrice {
            item_key: item_key.into(),
            display_name: item_key.into(),
            kind,
            pricing_unit,
            charge,
            amount: Money {
                amount_minor,
                currency: "THB".into(),
            },
            trial: None,
            coupon: None,
            setup_fee: None,
            included_usage: None,
            overage: None,
        }
    }

    fn commercial_draft(
        revision_number: u32,
        prices: Vec<CatalogPrice>,
    ) -> CommercialCatalogRevisionDraft {
        CommercialCatalogRevisionDraft {
            id: Uuid::new_v4(),
            revision_number,
            prices,
        }
    }

    #[test]
    fn read_only_billing_blocks_mutations_but_allows_reads() {
        let entitlement = ModuleEntitlement {
            module_key: "finance".into(),
            active: true,
            seats: 10,
            company_limit: 3,
            branch_limit: 8,
            active_until: None,
        };
        let now = Utc::now();
        assert_eq!(
            authorize_module(BillingState::ReadOnly, Some(&entitlement), true, now, true),
            EntitlementDecision::Denied("payment_failure_read_only")
        );
        assert_eq!(
            authorize_module(BillingState::ReadOnly, Some(&entitlement), true, now, false),
            EntitlementDecision::Allowed
        );
    }

    #[test]
    fn published_revisions_cannot_be_republished_or_changed_by_the_catalog_rule() {
        let mut revision = CatalogRevision {
            id: Uuid::new_v4(),
            revision_number: 1,
            published_at: None,
            published_by: None,
            items: vec![CatalogItem {
                item_key: "base".into(),
                display_name: "Base".into(),
                billing_model: BillingModel::Flat,
                amount_minor: 100,
                currency: "THB".into(),
                interval: Some("month".into()),
                included_quantity: 1,
                stripe_product_id: None,
                stripe_price_id: None,
            }],
        };
        revision.publish(Utc::now(), "root").expect("publish");
        assert_eq!(
            revision.publish(Utc::now(), "root"),
            Err(CatalogError::Immutable)
        );
    }

    #[test]
    fn commercial_revision_covers_pricing_dimensions_and_is_sealed_on_publish() {
        let mut base = commercial_price(
            "base",
            CatalogPriceKind::BaseFee,
            PricingUnit::Flat,
            ChargeKind::Recurring(BillingInterval::Monthly),
            1_000,
        );
        base.trial = Some(TrialPeriod { days: 14 });
        base.coupon = Some(Coupon {
            code: "WELCOME".into(),
            discount: CouponDiscount::Percentage(20),
            valid_until: None,
        });
        base.setup_fee = Some(Money {
            amount_minor: 500,
            currency: "THB".into(),
        });

        let module = commercial_price(
            "marketing",
            CatalogPriceKind::Module,
            PricingUnit::Seat,
            ChargeKind::Recurring(BillingInterval::Yearly),
            2_000,
        );
        let mut metered = commercial_price(
            "storage",
            CatalogPriceKind::Module,
            PricingUnit::Branch,
            ChargeKind::Metered,
            100,
        );
        metered.included_usage = Some(100);
        metered.overage = Some(OveragePricing {
            unit_amount: Money {
                amount_minor: 2,
                currency: "THB".into(),
            },
            unit_name: "GB".into(),
        });

        let published = CommercialCatalogRevisionDraft {
            id: Uuid::new_v4(),
            revision_number: 1,
            prices: vec![base, module, metered],
        }
        .publish(Utc::now(), "platform-root")
        .expect("valid commercial revision");

        assert_eq!(published.prices().len(), 3);
        assert_eq!(
            published.price("storage").unwrap().charge,
            ChargeKind::Metered
        );
        assert_eq!(
            published.price("base").unwrap().trial,
            Some(TrialPeriod { days: 14 })
        );
    }

    #[test]
    fn published_catalog_rejects_invalid_cross_field_combinations() {
        let cases = [
            (
                commercial_draft(
                    0,
                    vec![commercial_price(
                        "base",
                        CatalogPriceKind::BaseFee,
                        PricingUnit::Flat,
                        ChargeKind::Recurring(BillingInterval::Monthly),
                        1_000,
                    )],
                ),
                CatalogError::InvalidRevisionNumber,
            ),
            (
                commercial_draft(
                    1,
                    vec![CatalogPrice {
                        display_name: " ".into(),
                        ..commercial_price(
                            "base",
                            CatalogPriceKind::BaseFee,
                            PricingUnit::Flat,
                            ChargeKind::Recurring(BillingInterval::Monthly),
                            1_000,
                        )
                    }],
                ),
                CatalogError::MissingDisplayName,
            ),
            (
                commercial_draft(
                    1,
                    vec![commercial_price(
                        "base",
                        CatalogPriceKind::BaseFee,
                        PricingUnit::Seat,
                        ChargeKind::Recurring(BillingInterval::Monthly),
                        1_000,
                    )],
                ),
                CatalogError::BaseFeeRequiresFlatPricing,
            ),
            (
                commercial_draft(
                    1,
                    vec![commercial_price(
                        "module",
                        CatalogPriceKind::Module,
                        PricingUnit::Flat,
                        ChargeKind::Recurring(BillingInterval::Monthly),
                        1_000,
                    )],
                ),
                CatalogError::ModuleRequiresScopedPricing,
            ),
            (
                commercial_draft(
                    1,
                    vec![
                        commercial_price(
                            "base",
                            CatalogPriceKind::BaseFee,
                            PricingUnit::Flat,
                            ChargeKind::Recurring(BillingInterval::Monthly),
                            1_000,
                        ),
                        commercial_price(
                            "module",
                            CatalogPriceKind::Module,
                            PricingUnit::Flat,
                            ChargeKind::Recurring(BillingInterval::Monthly),
                            1_000,
                        ),
                    ],
                ),
                CatalogError::ModuleRequiresScopedPricing,
            ),
            (
                commercial_draft(
                    1,
                    vec![commercial_price(
                        "setup",
                        CatalogPriceKind::SetupFee,
                        PricingUnit::Flat,
                        ChargeKind::Recurring(BillingInterval::Monthly),
                        1_000,
                    )],
                ),
                CatalogError::SetupFeeRequiresOneTimeFlatPricing,
            ),
            (
                commercial_draft(
                    1,
                    vec![commercial_price(
                        "base",
                        CatalogPriceKind::BaseFee,
                        PricingUnit::Flat,
                        ChargeKind::Metered,
                        1_000,
                    )],
                ),
                CatalogError::MeteredRequiresScopedPricing,
            ),
            (
                commercial_draft(
                    1,
                    vec![CatalogPrice {
                        included_usage: Some(10),
                        ..commercial_price(
                            "base",
                            CatalogPriceKind::BaseFee,
                            PricingUnit::Flat,
                            ChargeKind::Recurring(BillingInterval::Monthly),
                            1_000,
                        )
                    }],
                ),
                CatalogError::IncludedUsageRequiresMetered,
            ),
            (
                commercial_draft(
                    1,
                    vec![CatalogPrice {
                        trial: Some(TrialPeriod { days: 7 }),
                        ..commercial_price(
                            "base",
                            CatalogPriceKind::BaseFee,
                            PricingUnit::Flat,
                            ChargeKind::Metered,
                            1_000,
                        )
                    }],
                ),
                CatalogError::TrialRequiresRecurring,
            ),
        ];
        for (draft, expected) in cases {
            assert_eq!(draft.publish(Utc::now(), "root"), Err(expected));
        }
    }

    #[test]
    fn catalog_reconciliation_is_deterministic_and_classifies_safe_mismatches() {
        let local = commercial_draft(
            4,
            vec![
                commercial_price(
                    "base",
                    CatalogPriceKind::BaseFee,
                    PricingUnit::Flat,
                    ChargeKind::Recurring(BillingInterval::Monthly),
                    1_000,
                ),
                commercial_price(
                    "finance",
                    CatalogPriceKind::Module,
                    PricingUnit::Company,
                    ChargeKind::Recurring(BillingInterval::Yearly),
                    2_000,
                ),
            ],
        )
        .publish(Utc::now(), "root")
        .expect("local catalog");
        let provider = ProviderCatalogRevision {
            provider_revision_ref: "stripe_catalog_4".into(),
            revision_number: 4,
            prices: vec![
                ProviderCatalogPrice {
                    product_ref: None,
                    price_ref: Some("price_finance".into()),
                    price: local.price("finance").unwrap().clone(),
                },
                ProviderCatalogPrice {
                    product_ref: Some("prod_base".into()),
                    price_ref: Some("price_base".into()),
                    price: CatalogPrice {
                        amount: Money {
                            amount_minor: 1_250,
                            currency: "THB".into(),
                        },
                        ..local.price("base").unwrap().clone()
                    },
                },
                ProviderCatalogPrice {
                    product_ref: Some("prod_extra".into()),
                    price_ref: Some("price_extra".into()),
                    price: commercial_price(
                        "extra",
                        CatalogPriceKind::Module,
                        PricingUnit::Branch,
                        ChargeKind::Recurring(BillingInterval::Monthly),
                        500,
                    ),
                },
            ],
        };
        let report = local.reconcile_with_provider(&provider);
        assert!(!report.is_in_sync());
        assert!(report.mismatches.iter().any(|m| {
            m.kind == CatalogMismatchKind::PriceFieldMismatch
                && m.item_key.as_deref() == Some("base")
                && m.field.as_deref() == Some("amount")
        }));
        assert!(report.mismatches.iter().any(|m| {
            m.kind == CatalogMismatchKind::ProviderProductReferenceMissing
                && m.item_key.as_deref() == Some("finance")
        }));
        assert!(report.mismatches.iter().any(|m| {
            m.kind == CatalogMismatchKind::UnexpectedProviderPrice
                && m.item_key.as_deref() == Some("extra")
        }));
        let mut reversed = provider.clone();
        reversed.prices.reverse();
        assert_eq!(report, local.reconcile_with_provider(&reversed));

        let missing = CatalogReconciliationReport::compare(None, &provider);
        assert_eq!(
            missing.mismatches[0].kind,
            CatalogMismatchKind::MissingLocalRevision
        );
    }

    #[test]
    fn grandfathered_price_migration_is_an_explicit_record_between_published_revisions() {
        let old = CommercialCatalogRevisionDraft {
            id: Uuid::new_v4(),
            revision_number: 1,
            prices: vec![commercial_price(
                "base",
                CatalogPriceKind::BaseFee,
                PricingUnit::Flat,
                ChargeKind::Recurring(BillingInterval::Monthly),
                1_000,
            )],
        }
        .publish(Utc::now(), "root")
        .unwrap();
        let new = CommercialCatalogRevisionDraft {
            id: Uuid::new_v4(),
            revision_number: 2,
            prices: vec![commercial_price(
                "base",
                CatalogPriceKind::BaseFee,
                PricingUnit::Flat,
                ChargeKind::Recurring(BillingInterval::Monthly),
                1_250,
            )],
        }
        .publish(Utc::now(), "root")
        .unwrap();
        let record = new
            .record_grandfathered_price_migration(
                &old,
                GrandfatheredPriceMigrationRequest {
                    customer_id: Uuid::new_v4(),
                    subscription_id: Uuid::new_v4(),
                    item_key: "base".into(),
                    effective_at: Utc::now(),
                    reason: "customer-approved price migration".into(),
                    recorded_by: "billing-admin".into(),
                },
            )
            .unwrap();

        assert_eq!(record.from_revision, 1);
        assert_eq!(record.to_revision, 2);
        assert_eq!(record.previous_price.amount_minor, 1_000);
        assert_eq!(record.migrated_price.amount_minor, 1_250);
    }

    #[test]
    fn billing_lifecycle_enforces_the_seven_day_grace_boundary() {
        let start = Utc::now();
        let mut lifecycle = BillingLifecycle::new(BillingState::Trialing, start);
        assert_eq!(
            lifecycle.transition(BillingEvent::TrialEnded, start),
            Ok(BillingState::Active)
        );
        assert_eq!(
            lifecycle.transition(BillingEvent::PaymentFailed, start),
            Ok(BillingState::PastDue)
        );
        assert_eq!(
            lifecycle.transition(BillingEvent::GraceStarted, start),
            Ok(BillingState::Grace)
        );
        assert_eq!(lifecycle.grace_until, Some(start + DEFAULT_GRACE_PERIOD));
        assert_eq!(
            lifecycle.transition(
                BillingEvent::GraceExpired,
                start + chrono::Duration::days(7) - chrono::Duration::seconds(1),
            ),
            Err(BillingError::GraceNotExpired)
        );
        assert_eq!(
            lifecycle.transition(BillingEvent::GraceExpired, start + DEFAULT_GRACE_PERIOD),
            Ok(BillingState::ReadOnly)
        );
        assert_eq!(
            lifecycle.transition(BillingEvent::PaymentRecovered, start),
            Ok(BillingState::Active)
        );
        assert_eq!(
            lifecycle.transition(BillingEvent::PaymentFailed, start),
            Ok(BillingState::PastDue)
        );
    }

    #[test]
    fn read_only_retains_only_the_declared_recovery_capabilities() {
        let retained = [
            BillingCapability::Login,
            BillingCapability::Billing,
            BillingCapability::Export,
            BillingCapability::Support,
            BillingCapability::Recovery,
            BillingCapability::NonMutatingReports,
        ];
        for capability in retained {
            assert!(BillingState::ReadOnly.allows_read_only_capability(capability));
        }
    }

    #[test]
    fn entitlement_allocation_cannot_exceed_purchased_capacity_and_requires_activation() {
        let company_a = Uuid::new_v4();
        let company_b = Uuid::new_v4();
        let branch_a = Uuid::new_v4();
        let branch_b = Uuid::new_v4();
        let mut book = EntitlementAllocationBook::new(
            "operations",
            EntitlementCapacity {
                seats: 10,
                companies: 2,
                branches: 3,
            },
            None,
        );

        book.allocate_company(company_a, 6, 2).unwrap();
        book.allocate_company(company_b, 4, 1).unwrap();
        assert_eq!(
            book.allocate_company(Uuid::new_v4(), 1, 1),
            Err(EntitlementAllocationError::CapacityExceeded)
        );
        assert_eq!(
            book.authorize(
                BillingState::Active,
                company_a,
                Some(branch_a),
                true,
                Utc::now(),
                true,
            ),
            EntitlementDecision::Denied("module_not_active")
        );
        book.active = true;
        book.activate_company(company_a).unwrap();
        book.activate_branch(company_a, branch_a).unwrap();
        book.activate_branch(company_a, branch_b).unwrap();
        assert_eq!(
            book.authorize(
                BillingState::Active,
                company_a,
                Some(branch_a),
                true,
                Utc::now(),
                true,
            ),
            EntitlementDecision::Allowed
        );
        assert_eq!(
            book.activate_branch(company_a, Uuid::new_v4()),
            Err(EntitlementAllocationError::BranchCapacityExceeded)
        );
    }

    #[test]
    fn provider_billing_events_are_the_only_state_transition_command() {
        let start = Utc::now();
        let mut lifecycle = BillingLifecycle::new(BillingState::Trialing, start);
        let active = ProviderBillingEvent {
            provider_event_id: "evt_trial_end".into(),
            created_at: start,
            event: BillingEvent::TrialEnded,
        };
        assert_eq!(
            lifecycle.apply_provider_event(&active),
            Ok(CommercialCommandChange::Applied)
        );
        assert_eq!(lifecycle.state, BillingState::Active);
        let duplicate = lifecycle
            .apply_provider_event(&active)
            .expect("duplicate provider event");
        assert_eq!(duplicate, CommercialCommandChange::DuplicateOrOutOfOrder);
        assert_eq!(lifecycle.state, BillingState::Active);
    }

    #[test]
    fn payment_policy_starts_grace_at_failure_and_enters_read_only_at_exact_boundary() {
        let failed_at = Utc::now();
        let mut lifecycle = BillingLifecycle::new(BillingState::Active, failed_at);
        lifecycle
            .apply_provider_event(&ProviderBillingEvent {
                provider_event_id: "evt_payment_failed".into(),
                created_at: failed_at,
                event: BillingEvent::PaymentFailed,
            })
            .expect("payment failure");
        assert_eq!(lifecycle.state, BillingState::PastDue);
        assert_eq!(
            lifecycle.advance_payment_policy(failed_at),
            Ok(BillingPolicyChange::GraceStarted)
        );
        assert_eq!(
            lifecycle.grace_until,
            Some(failed_at + DEFAULT_GRACE_PERIOD)
        );
        assert_eq!(
            lifecycle.advance_payment_policy(
                failed_at + DEFAULT_GRACE_PERIOD - chrono::Duration::nanoseconds(1)
            ),
            Ok(BillingPolicyChange::Unchanged)
        );
        assert_eq!(
            lifecycle.advance_payment_policy(failed_at + DEFAULT_GRACE_PERIOD),
            Ok(BillingPolicyChange::ReadOnlyEntered)
        );
        assert_eq!(lifecycle.state, BillingState::ReadOnly);
        assert_eq!(
            lifecycle.apply_provider_event(&ProviderBillingEvent {
                provider_event_id: "evt_recovered".into(),
                created_at: failed_at + DEFAULT_GRACE_PERIOD,
                event: BillingEvent::PaymentRecovered,
            }),
            Ok(CommercialCommandChange::Applied)
        );
        assert_eq!(lifecycle.state, BillingState::Active);
    }

    #[test]
    fn provider_entitlement_update_cannot_overrun_or_bypass_module_activation() {
        let at = Utc::now();
        let mut book = EntitlementAllocationBook::new(
            "finance",
            EntitlementCapacity {
                seats: 4,
                companies: 1,
                branches: 1,
            },
            None,
        );
        let update = ProviderEntitlementUpdate {
            provider_event_id: "evt_entitlement".into(),
            created_at: at,
            module_key: "finance".into(),
            capacity: book.capacity,
            active: true,
            active_until: None,
        };
        assert_eq!(
            book.apply_provider_update(&update),
            Ok(CommercialCommandChange::Applied)
        );
        let company = Uuid::new_v4();
        let branch = Uuid::new_v4();
        book.allocate_company(company, 4, 1).expect("allocation");
        book.activate_company_at(company, at)
            .expect("company activation");
        book.activate_branch_at(company, branch, at)
            .expect("branch activation");
        assert_eq!(
            book.allocate_company(Uuid::new_v4(), 1, 1),
            Err(EntitlementAllocationError::CapacityExceeded)
        );
        assert_eq!(
            book.apply_provider_update(&ProviderEntitlementUpdate {
                provider_event_id: "evt_entitlement_old".into(),
                created_at: at - chrono::Duration::seconds(1),
                ..update
            }),
            Ok(CommercialCommandChange::DuplicateOrOutOfOrder)
        );
    }

    #[test]
    fn commercial_command_debug_redacts_idempotency_key_and_rejects_invalid_context() {
        let context = CommercialCommandContext::new(
            Uuid::new_v4(),
            "sk_live_sensitive-command-key",
            Uuid::new_v4(),
            "billing-admin",
            "allocate module capacity",
        )
        .expect("valid context");
        let debug = format!("{context:?}");
        assert!(!debug.contains("sk_live_sensitive"));
        assert!(debug.contains("<redacted>"));
        assert_eq!(
            CommercialCommandContext::new(
                Uuid::new_v4(),
                "bad\nkey",
                Uuid::new_v4(),
                "billing-admin",
                "purpose",
            )
            .expect_err("control character must fail"),
            CommercialCommandValidationError::ControlCharacter("idempotency_key")
        );
    }

    #[test]
    fn checkout_selection_is_server_catalog_owned_usd_only_and_single_currency() {
        let mut base = commercial_price(
            "tenant-base-monthly",
            CatalogPriceKind::BaseFee,
            PricingUnit::Flat,
            ChargeKind::Recurring(BillingInterval::Monthly),
            10_000,
        );
        base.amount.currency = "USD".into();
        let mut module = commercial_price(
            "module-finance-monthly",
            CatalogPriceKind::Module,
            PricingUnit::Company,
            ChargeKind::Recurring(BillingInterval::Monthly),
            2_500,
        );
        module.amount.currency = "USD".into();
        let catalog = commercial_draft(1, vec![base, module])
            .publish(Utc::now(), "catalog-admin")
            .expect("USD catalog");
        let selection = vec![
            SubscriptionItemSelection {
                item_key: "tenant-base-monthly".into(),
                quantity: 1,
            },
            SubscriptionItemSelection {
                item_key: "module-finance-monthly".into(),
                quantity: 2,
            },
        ];
        assert_eq!(
            active_subscription_currency(&catalog, &selection),
            Ok("USD".into())
        );

        let thb_catalog = commercial_draft(
            2,
            vec![commercial_price(
                "tenant-base-monthly",
                CatalogPriceKind::BaseFee,
                PricingUnit::Flat,
                ChargeKind::Recurring(BillingInterval::Monthly),
                10_000,
            )],
        )
        .publish(Utc::now(), "catalog-admin")
        .expect("currency-aware persistence accepts ISO currencies");
        assert_eq!(
            active_subscription_currency(&thb_catalog, &selection[..1]),
            Err(SubscriptionSelectionError::InactiveCurrency)
        );
    }

    #[test]
    fn catalog_supports_one_monthly_and_one_yearly_tenant_base() {
        let mut monthly = commercial_price(
            "tenant-base-monthly",
            CatalogPriceKind::BaseFee,
            PricingUnit::Flat,
            ChargeKind::Recurring(BillingInterval::Monthly),
            10_000,
        );
        monthly.amount.currency = "USD".into();
        let mut yearly = commercial_price(
            "tenant-base-yearly",
            CatalogPriceKind::BaseFee,
            PricingUnit::Flat,
            ChargeKind::Recurring(BillingInterval::Yearly),
            100_000,
        );
        yearly.amount.currency = "USD".into();
        assert!(
            commercial_draft(3, vec![monthly.clone(), yearly])
                .publish(Utc::now(), "catalog-admin")
                .is_ok()
        );
        let mut duplicate_monthly = monthly.clone();
        duplicate_monthly.item_key = "tenant-base-monthly-duplicate".into();
        assert_eq!(
            commercial_draft(4, vec![monthly, duplicate_monthly])
                .publish(Utc::now(), "catalog-admin"),
            Err(CatalogError::ExactlyOneBaseFee)
        );
    }

    #[test]
    fn delinquency_extension_is_positive_bounded_and_auditable() {
        let grace_until = Utc::now() + DEFAULT_GRACE_PERIOD;
        let extension = BillingGraceExtension::grant(BillingGraceExtensionGrant {
            customer_id: Uuid::new_v4(),
            subscription_id: Uuid::new_v4(),
            delinquency_cycle_id: Uuid::new_v4(),
            previous_grace_until: grace_until,
            extension: MAX_GRACE_EXTENSION,
            granted_at: Utc::now(),
            granted_by: "platform-billing-admin".into(),
            reason: "approved payment-recovery exception".into(),
            correlation_id: Uuid::new_v4(),
        })
        .expect("bounded extension");
        assert_eq!(extension.extended_until, grace_until + MAX_GRACE_EXTENSION);
        assert_eq!(
            BillingGraceExtension::grant(BillingGraceExtensionGrant {
                customer_id: Uuid::new_v4(),
                subscription_id: Uuid::new_v4(),
                delinquency_cycle_id: Uuid::new_v4(),
                previous_grace_until: grace_until,
                extension: MAX_GRACE_EXTENSION + chrono::Duration::seconds(1),
                granted_at: Utc::now(),
                granted_by: "platform-billing-admin".into(),
                reason: "too long".into(),
                correlation_id: Uuid::new_v4(),
            }),
            Err(BillingGraceExtensionError::InvalidDuration)
        );
    }
}
