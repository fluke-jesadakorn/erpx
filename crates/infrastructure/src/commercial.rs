use async_trait::async_trait;
use chrono::{DateTime, Duration, Utc};
use erpx_domain::commercial::{
    ActivateBranchModuleCommand, ActivateCompanyModuleCommand, AllocateCompanyCommand,
    BillingEvent, BillingInterval, BillingLifecycle, BillingPolicyCommand,
    BillingProviderCommandEvidence, BillingState, CatalogError, CatalogPrice, CatalogPriceKind,
    CatalogReconciliationReport, ChargeKind, CommercialCatalogRevisionDraft,
    CommercialCommandChange, CommercialCommandContext, CommercialCommandReceipt,
    CommercialCommandResult, CommercialHumanDecisionEvidence, EntitlementAllocationBook,
    EntitlementCapacity, GrandfatheredPriceMigration, Money, PricingUnit, ProviderBillingEvent,
    ProviderCatalogPrice, ProviderCatalogRevision, ProviderEntitlementUpdate,
    PublishedCommercialCatalogRevision, validate_branch_entitlement_activation_decision,
    validate_company_entitlement_activation_decision,
};
use erpx_domain::customer::{
    Customer360, Customer360DetailReadContext, Customer360Projection, CustomerCommercialReadDto,
    CustomerReadView,
};
use erpx_kernel::HumanDecisionProof;
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration as StdDuration;
use thiserror::Error;
use uuid::Uuid;

use crate::{
    PersistenceError, SqlxControlPlane, StripeHttpClient, StripeHttpError, StripeHttpRequestError,
    StripePriceCreateRequest, StripePriceInterval, StripePriceListRequest, StripePriceRecurring,
    StripePriceUsageType, StripeProductCreateRequest, StripeProductListRequest,
    StripeResponseEnvelope,
};

#[derive(Clone)]
pub struct StripeInboxEvent {
    pub event_id: String,
    pub event_type: String,
    pub stripe_created_at: Option<DateTime<Utc>>,
    pub payload: Value,
    pub raw_body: Vec<u8>,
}

impl std::fmt::Debug for StripeInboxEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StripeInboxEvent")
            .field("event_id", &self.event_id)
            .field("event_type", &self.event_type)
            .field("stripe_created_at", &self.stripe_created_at)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StripeWebhookPersistenceContextError {
    #[error("Stripe raw body cannot be empty")]
    EmptyRawBody,
    #[error("Stripe webhook correlation ID cannot be nil")]
    NilCorrelationId,
}

/// Authenticated webhook persistence metadata. The digest is calculated from
/// the exact verified raw request bytes and is bound to the transport
/// correlation ID before the inbox row is written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeWebhookPersistenceContext {
    raw_body_sha256: [u8; 32],
    correlation_id: Uuid,
}

impl StripeWebhookPersistenceContext {
    pub fn from_raw_body(
        raw_body: &[u8],
        correlation_id: Uuid,
    ) -> Result<Self, StripeWebhookPersistenceContextError> {
        if raw_body.is_empty() {
            return Err(StripeWebhookPersistenceContextError::EmptyRawBody);
        }
        if correlation_id.is_nil() {
            return Err(StripeWebhookPersistenceContextError::NilCorrelationId);
        }
        Ok(Self {
            raw_body_sha256: Sha256::digest(raw_body).into(),
            correlation_id,
        })
    }

    pub fn raw_body_sha256(&self) -> &[u8; 32] {
        &self.raw_body_sha256
    }

    pub fn correlation_id(&self) -> Uuid {
        self.correlation_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripeInboxInsert {
    Inserted,
    Duplicate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripeInboxStatus {
    Received,
    Processing,
    Processed,
    Retryable,
    DeadLetter,
}

impl StripeInboxStatus {
    fn from_database(value: &str) -> Result<Self, PersistenceError> {
        match value {
            "received" => Ok(Self::Received),
            "processing" => Ok(Self::Processing),
            "processed" => Ok(Self::Processed),
            "retryable" => Ok(Self::Retryable),
            "dead_letter" => Ok(Self::DeadLetter),
            other => Err(PersistenceError::InvalidData(format!(
                "unknown Stripe inbox status {other}"
            ))),
        }
    }
}

#[derive(Clone)]
pub struct StripeInboxClaim {
    pub event_id: String,
    pub event_type: String,
    pub stripe_created_at: Option<DateTime<Utc>>,
    pub payload: Value,
    pub attempts: u32,
    pub claimed_by: Uuid,
    pub claim_expires_at: DateTime<Utc>,
}

impl std::fmt::Debug for StripeInboxClaim {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StripeInboxClaim")
            .field("event_id", &self.event_id)
            .field("event_type", &self.event_type)
            .field("stripe_created_at", &self.stripe_created_at)
            .field("attempts", &self.attempts)
            .field("claimed_by", &self.claimed_by)
            .field("claim_expires_at", &self.claim_expires_at)
            .finish_non_exhaustive()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeBillingTransition {
    pub event_id: String,
    pub event_type: String,
    pub created_at: DateTime<Utc>,
    pub customer_id: Uuid,
    pub subscription_id: Uuid,
    pub transition: BillingEvent,
    pub invoice: Option<StripeInvoiceProjection>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeInvoiceProjection {
    pub holding_id: String,
    pub invoice_id: String,
    pub state: String,
    pub currency: String,
    pub amount_due_minor: u64,
    pub amount_paid_minor: u64,
    pub due_at: Option<DateTime<Utc>>,
    pub hosted_invoice_available: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeEntitlementUpdate {
    pub event_id: String,
    pub event_type: String,
    pub created_at: DateTime<Utc>,
    pub customer_id: Uuid,
    pub module_key: String,
    pub capacity: EntitlementCapacity,
    pub active: bool,
    pub active_until: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeSemanticNoOp {
    pub event_id: String,
    pub event_type: String,
    pub created_at: DateTime<Utc>,
    pub reason: String,
}

/// A verified Checkout completion may bind provider identifiers, but it is
/// deliberately not payment authority. Billing and entitlement activation
/// still require subsequent invoice/subscription/entitlement events.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeCheckoutBinding {
    pub event_id: String,
    pub event_type: String,
    pub created_at: DateTime<Utc>,
    pub holding_id: String,
    pub customer_id: Uuid,
    pub subscription_id: Uuid,
    pub stripe_customer_ref: String,
    pub stripe_subscription_ref: String,
    pub currency: String,
    pub item_quantities: BTreeMap<String, u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum StripeSemanticEvent {
    Checkout(StripeCheckoutBinding),
    Billing(StripeBillingTransition),
    Entitlement(StripeEntitlementUpdate),
    NoOp(StripeSemanticNoOp),
}

impl StripeSemanticEvent {
    pub fn event_id(&self) -> &str {
        match self {
            Self::Checkout(event) => &event.event_id,
            Self::Billing(event) => &event.event_id,
            Self::Entitlement(event) => &event.event_id,
            Self::NoOp(event) => &event.event_id,
        }
    }

    pub fn event_type(&self) -> &str {
        match self {
            Self::Checkout(event) => &event.event_type,
            Self::Billing(event) => &event.event_type,
            Self::Entitlement(event) => &event.event_type,
            Self::NoOp(event) => &event.event_type,
        }
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StripeSemanticParseError {
    #[error("Stripe event timestamp is required for semantic ordering")]
    MissingCreatedAt,
    #[error("Stripe event payload is not a data.object JSON object")]
    MalformedPayload,
    #[error("Stripe event field {0} is required")]
    MissingField(&'static str),
    #[error("Stripe event field {0} is not a valid UUID")]
    InvalidUuid(&'static str),
    #[error("Stripe event field {0} is not a valid timestamp")]
    InvalidTimestamp(&'static str),
    #[error("Stripe event field {0} is not a valid number")]
    InvalidNumber(&'static str),
    #[error("Stripe event field {0} is not a valid boolean")]
    InvalidBoolean(&'static str),
    #[error("Stripe event field {0} is not valid bounded JSON metadata")]
    InvalidJsonMetadata(&'static str),
    #[error("unsupported Stripe event type {0}")]
    UnsupportedEventType(String),
    #[error("unsupported Stripe subscription status {0}")]
    UnsupportedSubscriptionStatus(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripeSemanticApplyResult {
    Applied,
    DuplicateOrOutOfOrder,
    NoOp,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StripeSemanticApplyError {
    #[error("retryable Stripe semantic application failure: {0}")]
    Retryable(String),
    #[error("permanent Stripe semantic application failure: {0}")]
    Permanent(String),
}

#[async_trait]
pub trait StripeSemanticApplier: Send + Sync {
    async fn apply_stripe_semantic_event(
        &self,
        event: &StripeSemanticEvent,
    ) -> Result<StripeSemanticApplyResult, StripeSemanticApplyError>;
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StripeSemanticDispatchError {
    #[error(transparent)]
    Parse(#[from] StripeSemanticParseError),
    #[error(transparent)]
    Apply(#[from] StripeSemanticApplyError),
}

pub struct StripeSemanticDispatcher;

impl StripeSemanticDispatcher {
    pub fn decode(
        claim: &StripeInboxClaim,
    ) -> Result<StripeSemanticEvent, StripeSemanticParseError> {
        decode_stripe_semantic_event(claim)
    }

    pub async fn dispatch<A: StripeSemanticApplier + ?Sized>(
        applier: &A,
        claim: &StripeInboxClaim,
    ) -> Result<StripeSemanticApplyResult, StripeSemanticDispatchError> {
        let event = Self::decode(claim)?;
        match event {
            StripeSemanticEvent::NoOp(_) => Ok(StripeSemanticApplyResult::NoOp),
            ref event => Ok(applier.apply_stripe_semantic_event(event).await?),
        }
    }
}

fn decode_stripe_semantic_event(
    claim: &StripeInboxClaim,
) -> Result<StripeSemanticEvent, StripeSemanticParseError> {
    let created_at = claim
        .stripe_created_at
        .ok_or(StripeSemanticParseError::MissingCreatedAt)?;
    let event_id = claim.event_id.clone();
    let event_type = claim.event_type.clone();
    let no_op = |reason: &str| {
        StripeSemanticEvent::NoOp(StripeSemanticNoOp {
            event_id: event_id.clone(),
            event_type: event_type.clone(),
            created_at,
            reason: reason.into(),
        })
    };

    match claim.event_type.as_str() {
        "invoice.paid" | "invoice.payment_succeeded" => {
            Ok(StripeSemanticEvent::Billing(StripeBillingTransition {
                event_id,
                event_type,
                created_at,
                customer_id: required_uuid(&claim.payload, "erpx_customer_id")?,
                subscription_id: required_uuid(&claim.payload, "erpx_subscription_id")?,
                transition: BillingEvent::PaymentRecovered,
                invoice: optional_invoice_projection(&claim.payload)?,
            }))
        }
        "invoice.payment_failed" => Ok(StripeSemanticEvent::Billing(StripeBillingTransition {
            event_id,
            event_type,
            created_at,
            customer_id: required_uuid(&claim.payload, "erpx_customer_id")?,
            subscription_id: required_uuid(&claim.payload, "erpx_subscription_id")?,
            transition: BillingEvent::PaymentFailed,
            invoice: optional_invoice_projection(&claim.payload)?,
        })),
        "customer.subscription.deleted" => {
            Ok(StripeSemanticEvent::Billing(StripeBillingTransition {
                event_id,
                event_type,
                created_at,
                customer_id: required_uuid(&claim.payload, "erpx_customer_id")?,
                subscription_id: required_uuid(&claim.payload, "erpx_subscription_id")?,
                transition: BillingEvent::SubscriptionCanceled,
                invoice: None,
            }))
        }
        "customer.subscription.created" | "customer.subscription.updated" => {
            let object = stripe_object(&claim.payload)?;
            let status = required_string(object, "status")?;
            let transition = match status.as_str() {
                "active" => BillingEvent::TrialEnded,
                "past_due" | "unpaid" => BillingEvent::PaymentFailed,
                "canceled" => BillingEvent::SubscriptionCanceled,
                "trialing" => return Ok(no_op("subscription remains trialing")),
                other => {
                    return Err(StripeSemanticParseError::UnsupportedSubscriptionStatus(
                        other.into(),
                    ));
                }
            };
            Ok(StripeSemanticEvent::Billing(StripeBillingTransition {
                event_id,
                event_type,
                created_at,
                customer_id: required_uuid(&claim.payload, "erpx_customer_id")?,
                subscription_id: required_uuid(&claim.payload, "erpx_subscription_id")?,
                transition,
                invoice: None,
            }))
        }
        "entitlements.active_entitlement_summary.updated" => {
            let object = stripe_object(&claim.payload)?;
            Ok(StripeSemanticEvent::Entitlement(StripeEntitlementUpdate {
                event_id,
                event_type,
                created_at,
                customer_id: required_uuid(&claim.payload, "erpx_customer_id")?,
                module_key: required_string_from(object, "erpx_module_key")?,
                capacity: EntitlementCapacity {
                    seats: required_u32_from(object, "seats")?,
                    companies: required_u32_from(object, "companies")?,
                    branches: required_u32_from(object, "branches")?,
                },
                active: required_bool_from(object, "active")?,
                active_until: optional_timestamp_from(object, "active_until")?,
            }))
        }
        "checkout.session.completed" => {
            let object = stripe_object(&claim.payload)?;
            Ok(StripeSemanticEvent::Checkout(StripeCheckoutBinding {
                event_id,
                event_type,
                created_at,
                holding_id: required_string_from(object, "erpx_holding_id")?,
                customer_id: required_uuid(&claim.payload, "erpx_customer_id")?,
                subscription_id: required_uuid(&claim.payload, "erpx_subscription_id")?,
                stripe_customer_ref: required_string(object, "customer")?,
                stripe_subscription_ref: required_string(object, "subscription")?,
                currency: required_string_from(object, "erpx_currency")?,
                item_quantities: required_item_quantities(object)?,
            }))
        }
        "customer.subscription.trial_will_end" => Ok(no_op("trial reminder")),
        "invoice.finalized" => Ok(no_op("invoice finalization is not a lifecycle transition")),
        other => Err(StripeSemanticParseError::UnsupportedEventType(other.into())),
    }
}

fn stripe_object(
    payload: &Value,
) -> Result<&serde_json::Map<String, Value>, StripeSemanticParseError> {
    payload
        .get("data")
        .and_then(Value::as_object)
        .and_then(|data| data.get("object"))
        .and_then(Value::as_object)
        .ok_or(StripeSemanticParseError::MalformedPayload)
}

fn metadata_value<'a>(object: &'a serde_json::Map<String, Value>, key: &str) -> Option<&'a Value> {
    object
        .get("metadata")
        .and_then(Value::as_object)
        .and_then(|metadata| metadata.get(key))
        .or_else(|| object.get(key))
}

fn required_uuid(payload: &Value, key: &'static str) -> Result<Uuid, StripeSemanticParseError> {
    let object = stripe_object(payload)?;
    let value = metadata_value(object, key)
        .and_then(Value::as_str)
        .ok_or(StripeSemanticParseError::MissingField(key))?;
    Uuid::parse_str(value).map_err(|_| StripeSemanticParseError::InvalidUuid(key))
}

fn required_string(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
) -> Result<String, StripeSemanticParseError> {
    object
        .get(key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or(StripeSemanticParseError::MissingField(key))
}

fn required_string_from(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
) -> Result<String, StripeSemanticParseError> {
    metadata_value(object, key)
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .map(str::to_owned)
        .ok_or(StripeSemanticParseError::MissingField(key))
}

fn required_u32_from(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
) -> Result<u32, StripeSemanticParseError> {
    let value = metadata_value(object, key).ok_or(StripeSemanticParseError::MissingField(key))?;
    if let Some(number) = value.as_u64() {
        return u32::try_from(number).map_err(|_| StripeSemanticParseError::InvalidNumber(key));
    }
    value
        .as_str()
        .ok_or(StripeSemanticParseError::InvalidNumber(key))?
        .parse::<u32>()
        .map_err(|_| StripeSemanticParseError::InvalidNumber(key))
}

fn required_bool_from(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
) -> Result<bool, StripeSemanticParseError> {
    let value = metadata_value(object, key).ok_or(StripeSemanticParseError::MissingField(key))?;
    if let Some(value) = value.as_bool() {
        return Ok(value);
    }
    match value.as_str() {
        Some("true") => Ok(true),
        Some("false") => Ok(false),
        _ => Err(StripeSemanticParseError::InvalidBoolean(key)),
    }
}

fn required_item_quantities(
    object: &serde_json::Map<String, Value>,
) -> Result<BTreeMap<String, u32>, StripeSemanticParseError> {
    let value = metadata_value(object, "erpx_items")
        .and_then(Value::as_str)
        .ok_or(StripeSemanticParseError::MissingField("erpx_items"))?;
    let items: BTreeMap<String, u32> = serde_json::from_str(value)
        .map_err(|_| StripeSemanticParseError::InvalidJsonMetadata("erpx_items"))?;
    if items.is_empty()
        || items.len() > 100
        || items
            .iter()
            .any(|(key, quantity)| key.trim().is_empty() || key.len() > 255 || *quantity == 0)
    {
        return Err(StripeSemanticParseError::InvalidJsonMetadata("erpx_items"));
    }
    Ok(items)
}

fn optional_invoice_projection(
    payload: &Value,
) -> Result<Option<StripeInvoiceProjection>, StripeSemanticParseError> {
    let object = stripe_object(payload)?;
    let Some(invoice_id) = object.get("id").and_then(Value::as_str) else {
        return Ok(None);
    };
    let Some(holding_id) = metadata_value(object, "erpx_holding_id").and_then(Value::as_str) else {
        return Ok(None);
    };
    let currency = object
        .get("currency")
        .and_then(Value::as_str)
        .map(str::to_ascii_uppercase)
        .ok_or(StripeSemanticParseError::MissingField("currency"))?;
    let amount_due_minor = object
        .get("amount_due")
        .and_then(Value::as_u64)
        .ok_or(StripeSemanticParseError::InvalidNumber("amount_due"))?;
    let amount_paid_minor = object
        .get("amount_paid")
        .and_then(Value::as_u64)
        .ok_or(StripeSemanticParseError::InvalidNumber("amount_paid"))?;
    let due_at = object
        .get("due_date")
        .and_then(Value::as_i64)
        .map(|timestamp| {
            DateTime::<Utc>::from_timestamp(timestamp, 0)
                .ok_or(StripeSemanticParseError::InvalidTimestamp("due_date"))
        })
        .transpose()?;
    let state = object
        .get("status")
        .and_then(Value::as_str)
        .unwrap_or("unknown")
        .to_owned();
    Ok(Some(StripeInvoiceProjection {
        holding_id: holding_id.to_owned(),
        invoice_id: invoice_id.to_owned(),
        state,
        currency,
        amount_due_minor,
        amount_paid_minor,
        due_at,
        hosted_invoice_available: object
            .get("hosted_invoice_url")
            .and_then(Value::as_str)
            .is_some_and(|url| !url.is_empty()),
    }))
}

fn optional_timestamp_from(
    object: &serde_json::Map<String, Value>,
    key: &'static str,
) -> Result<Option<DateTime<Utc>>, StripeSemanticParseError> {
    let Some(value) = metadata_value(object, key) else {
        return Ok(None);
    };
    let Some(value) = value.as_str() else {
        return Err(StripeSemanticParseError::InvalidTimestamp(key));
    };
    DateTime::parse_from_rfc3339(value)
        .map(|value| Some(value.with_timezone(&Utc)))
        .map_err(|_| StripeSemanticParseError::InvalidTimestamp(key))
}

/// The Stripe API version used by the future official client adapter.
///
/// This is intentionally a repository constant rather than a value read from
/// an environment variable. A provider adapter must be regenerated and this
/// value must be reviewed together when Stripe changes its API contract.
pub const PINNED_STRIPE_API_VERSION: &str = "2026-04-22.dahlia";
pub const STRIPE_API_BASE_URL: &str = "https://api.stripe.com";
pub const MAX_STRIPE_CATALOG_RECONCILIATION_PAGES: usize = 64;
const STRIPE_CATALOG_RECONCILIATION_PAGE_SIZE: u16 = 100;
const CATALOG_REVISION_AGGREGATE_TYPE: &str = "commercial_catalog_revision";
const CATALOG_REVISION_PUBLISHED_TYPE: &str = "commercial.catalog.revision_published.v1";
const BILLING_LIFECYCLE_AGGREGATE_TYPE: &str = "billing_lifecycle";
const BILLING_PROVIDER_EVENT_APPLIED_TYPE: &str = "commercial.billing.provider_event_applied.v1";
const BILLING_POLICY_ADVANCED_TYPE: &str = "commercial.billing.policy_advanced.v1";
const ENTITLEMENT_ALLOCATION_AGGREGATE_TYPE: &str = "entitlement_allocation";
const ENTITLEMENT_PROVIDER_EVENT_APPLIED_TYPE: &str =
    "commercial.entitlement.provider_event_applied.v1";
const ENTITLEMENT_COMPANY_ALLOCATED_TYPE: &str = "commercial.entitlement.company_allocated.v1";
const ENTITLEMENT_COMPANY_ACTIVATED_TYPE: &str = "commercial.entitlement.company_activated.v1";
const ENTITLEMENT_BRANCH_ACTIVATED_TYPE: &str = "commercial.entitlement.branch_activated.v1";

fn entitlement_aggregate_type(module_key: &str) -> String {
    format!("{ENTITLEMENT_ALLOCATION_AGGREGATE_TYPE}:{module_key}")
}

macro_rules! stripe_identifier {
    ($name:ident, $prefix:literal, $kind:literal) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash, serde::Serialize, serde::Deserialize)]
        #[serde(try_from = "String", into = "String")]
        pub struct $name(String);

        impl $name {
            pub fn parse(value: impl Into<String>) -> Result<Self, StripeIdentifierError> {
                let value = value.into();
                if !value.starts_with($prefix) {
                    return Err(StripeIdentifierError::InvalidPrefix {
                        kind: $kind,
                        expected_prefix: $prefix,
                    });
                }
                if value.len() == $prefix.len()
                    || !value.chars().all(|character| {
                        character.is_ascii_alphanumeric() || "_-".contains(character)
                    })
                {
                    return Err(StripeIdentifierError::InvalidCharacters { kind: $kind });
                }
                Ok(Self(value))
            }

            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<String> for $name {
            type Error = StripeIdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl TryFrom<&str> for $name {
            type Error = StripeIdentifierError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::parse(value)
            }
        }

        impl From<$name> for String {
            fn from(value: $name) -> Self {
                value.0
            }
        }

        impl AsRef<str> for $name {
            fn as_ref(&self) -> &str {
                self.as_str()
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StripeIdentifierError {
    #[error("{kind} identifier must start with {expected_prefix}")]
    InvalidPrefix {
        kind: &'static str,
        expected_prefix: &'static str,
    },
    #[error("{kind} identifier is empty or contains invalid characters")]
    InvalidCharacters { kind: &'static str },
}

stripe_identifier!(StripeAccountId, "acct_", "Stripe account");
stripe_identifier!(StripeEventId, "evt_", "Stripe event");
stripe_identifier!(StripeProductId, "prod_", "Stripe product");
stripe_identifier!(StripePriceId, "price_", "Stripe price");
stripe_identifier!(
    StripeEntitlementFeatureId,
    "feat_",
    "Stripe entitlement feature"
);
stripe_identifier!(StripeMeterId, "meter_", "Stripe meter");

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StripeWebhookEventParseError {
    #[error("Stripe webhook persistence context does not match the raw body")]
    ContextMismatch,
    #[error("Stripe webhook body is not valid JSON")]
    InvalidJson,
    #[error("Stripe webhook body must be a JSON object")]
    InvalidShape,
    #[error("Stripe webhook event ID is required")]
    MissingEventId,
    #[error("Stripe webhook event type is required")]
    MissingEventType,
    #[error("Stripe webhook event timestamp is required")]
    MissingCreatedAt,
    #[error("Stripe webhook event timestamp is invalid")]
    InvalidCreatedAt,
    #[error("Stripe webhook event ID does not match its typed provider ID")]
    InvalidEventId(#[from] StripeIdentifierError),
    #[error("Stripe webhook event type is invalid")]
    InvalidEventType,
    #[error("Stripe webhook data object is required")]
    MissingDataObject,
}

/// Parsed only after the transport has verified the Stripe signature. It is a
/// typed handoff to the SQLx inbox; the raw body remains private and is never
/// included in debug output or semantic worker claims.
#[derive(Clone)]
pub struct StripeWebhookEvent {
    pub event_id: StripeEventId,
    pub event_type: String,
    pub stripe_created_at: DateTime<Utc>,
    pub payload: Value,
    raw_body: Vec<u8>,
    persistence_context: StripeWebhookPersistenceContext,
}

impl std::fmt::Debug for StripeWebhookEvent {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StripeWebhookEvent")
            .field("event_id", &self.event_id)
            .field("event_type", &self.event_type)
            .field("stripe_created_at", &self.stripe_created_at)
            .field("correlation_id", &self.persistence_context.correlation_id())
            .finish_non_exhaustive()
    }
}

impl StripeWebhookEvent {
    pub fn from_verified_raw_body(
        raw_body: &[u8],
        persistence_context: StripeWebhookPersistenceContext,
    ) -> Result<Self, StripeWebhookEventParseError> {
        let expected = StripeWebhookPersistenceContext::from_raw_body(
            raw_body,
            persistence_context.correlation_id(),
        )
        .map_err(|_| StripeWebhookEventParseError::ContextMismatch)?;
        if expected != persistence_context {
            return Err(StripeWebhookEventParseError::ContextMismatch);
        }

        let payload = serde_json::from_slice::<Value>(raw_body)
            .map_err(|_| StripeWebhookEventParseError::InvalidJson)?;
        let object = payload
            .as_object()
            .ok_or(StripeWebhookEventParseError::InvalidShape)?;
        let event_id = object
            .get("id")
            .and_then(Value::as_str)
            .ok_or(StripeWebhookEventParseError::MissingEventId)?
            .try_into()?;
        let event_type = object
            .get("type")
            .and_then(Value::as_str)
            .ok_or(StripeWebhookEventParseError::MissingEventType)?;
        if event_type.trim().is_empty()
            || event_type.len() > 255
            || event_type.chars().any(char::is_control)
        {
            return Err(StripeWebhookEventParseError::InvalidEventType);
        }
        let created = object
            .get("created")
            .and_then(Value::as_i64)
            .ok_or(StripeWebhookEventParseError::MissingCreatedAt)?;
        let stripe_created_at = DateTime::<Utc>::from_timestamp(created, 0)
            .ok_or(StripeWebhookEventParseError::InvalidCreatedAt)?;
        if object
            .get("data")
            .and_then(Value::as_object)
            .and_then(|data| data.get("object"))
            .and_then(Value::as_object)
            .is_none()
        {
            return Err(StripeWebhookEventParseError::MissingDataObject);
        }
        Ok(Self {
            event_id,
            event_type: event_type.into(),
            stripe_created_at,
            payload,
            raw_body: raw_body.to_vec(),
            persistence_context,
        })
    }

    pub fn persistence_context(&self) -> &StripeWebhookPersistenceContext {
        &self.persistence_context
    }

    pub fn into_inbox_event(self) -> StripeInboxEvent {
        StripeInboxEvent {
            event_id: self.event_id.into(),
            event_type: self.event_type,
            stripe_created_at: Some(self.stripe_created_at),
            payload: self.payload,
            raw_body: self.raw_body,
        }
    }
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StripeProviderConfigError {
    #[error("Stripe API version must be pinned to {expected}, got {actual}")]
    UnpinnedApiVersion {
        expected: &'static str,
        actual: String,
    },
    #[error("Stripe API base URL must be exactly {expected}")]
    InvalidApiBaseUrl { expected: &'static str },
    #[error("invalid Stripe account reference: {0}")]
    InvalidAccountReference(#[from] StripeIdentifierError),
}

/// Provider configuration contains no API key or webhook secret. Credentials
/// are intentionally outside this packet and must be injected by the future
/// adapter without becoming part of a serializable catalog DTO.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeProviderConfig {
    api_version: String,
    api_base_url: String,
    account_ref: StripeAccountId,
}

impl StripeProviderConfig {
    pub fn new(
        api_version: impl Into<String>,
        account_ref: impl Into<String>,
        api_base_url: impl Into<String>,
    ) -> Result<Self, StripeProviderConfigError> {
        let config = Self {
            api_version: api_version.into(),
            api_base_url: api_base_url.into(),
            account_ref: StripeAccountId::parse(account_ref)?,
        };
        config.validate()?;
        Ok(config)
    }

    pub fn pinned(account_ref: impl Into<String>) -> Result<Self, StripeProviderConfigError> {
        Self::new(PINNED_STRIPE_API_VERSION, account_ref, STRIPE_API_BASE_URL)
    }

    pub fn validate(&self) -> Result<(), StripeProviderConfigError> {
        if self.api_version != PINNED_STRIPE_API_VERSION {
            return Err(StripeProviderConfigError::UnpinnedApiVersion {
                expected: PINNED_STRIPE_API_VERSION,
                actual: self.api_version.clone(),
            });
        }
        if self.api_base_url != STRIPE_API_BASE_URL {
            return Err(StripeProviderConfigError::InvalidApiBaseUrl {
                expected: STRIPE_API_BASE_URL,
            });
        }
        StripeAccountId::parse(self.account_ref.as_str())
            .map(|_| ())
            .map_err(StripeProviderConfigError::from)
    }

    pub fn api_version(&self) -> &str {
        &self.api_version
    }

    pub fn api_base_url(&self) -> &str {
        &self.api_base_url
    }

    pub fn account_ref(&self) -> &StripeAccountId {
        &self.account_ref
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StripeCatalogItemIdentifiers {
    pub item_key: String,
    pub product: StripeProductId,
    pub price: StripePriceId,
    pub entitlement_feature: Option<StripeEntitlementFeatureId>,
    pub meter: Option<StripeMeterId>,
}

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum StripeCatalogSnapshotError {
    #[error("provider catalog revision reference is required")]
    MissingRevisionReference,
    #[error("provider catalog revision reference is invalid")]
    InvalidRevisionReference,
    #[error("provider catalog revision number must be positive")]
    InvalidRevisionNumber,
    #[error("provider catalog contains duplicate item keys")]
    DuplicateItemKey,
    #[error("provider catalog item identifiers are incomplete")]
    MissingIdentifiers,
    #[error("provider catalog contains identifiers for an unknown item")]
    UnexpectedIdentifiers,
    #[error("provider product identifier does not match the normalized price")]
    ProductReferenceMismatch,
    #[error("provider price identifier does not match the normalized price")]
    PriceReferenceMismatch,
    #[error("provider catalog item key is required")]
    MissingItemKey,
}

/// The provider-facing form of a catalog snapshot. The normalized domain
/// revision remains the comparison input; typed identifiers prevent raw Stripe
/// strings from being passed between provider operations.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StripeCatalogProviderSnapshot {
    pub revision: ProviderCatalogRevision,
    pub identifiers: BTreeMap<String, StripeCatalogItemIdentifiers>,
}

impl StripeCatalogProviderSnapshot {
    pub fn new(
        revision: ProviderCatalogRevision,
        identifiers: BTreeMap<String, StripeCatalogItemIdentifiers>,
    ) -> Result<Self, StripeCatalogSnapshotError> {
        let snapshot = Self {
            revision,
            identifiers,
        };
        snapshot.validate()?;
        Ok(snapshot)
    }

    pub fn validate(&self) -> Result<(), StripeCatalogSnapshotError> {
        if self.revision.provider_revision_ref.trim().is_empty() {
            return Err(StripeCatalogSnapshotError::MissingRevisionReference);
        }
        if !is_valid_reference_text(&self.revision.provider_revision_ref) {
            return Err(StripeCatalogSnapshotError::InvalidRevisionReference);
        }
        if self.revision.revision_number == 0 {
            return Err(StripeCatalogSnapshotError::InvalidRevisionNumber);
        }

        let mut seen = BTreeSet::new();
        for provider_price in &self.revision.prices {
            if !seen.insert(provider_price.price.item_key.as_str()) {
                return Err(StripeCatalogSnapshotError::DuplicateItemKey);
            }
            let Some(identifier) = self.identifiers.get(&provider_price.price.item_key) else {
                return Err(StripeCatalogSnapshotError::MissingIdentifiers);
            };
            if identifier.item_key.trim().is_empty() {
                return Err(StripeCatalogSnapshotError::MissingItemKey);
            }
            if identifier.item_key != provider_price.price.item_key {
                return Err(StripeCatalogSnapshotError::MissingIdentifiers);
            }
            if provider_price.product_ref.as_deref() != Some(identifier.product.as_str()) {
                return Err(StripeCatalogSnapshotError::ProductReferenceMismatch);
            }
            if provider_price.price_ref.as_deref() != Some(identifier.price.as_str()) {
                return Err(StripeCatalogSnapshotError::PriceReferenceMismatch);
            }
        }
        if self
            .identifiers
            .keys()
            .any(|item_key| !seen.contains(item_key.as_str()))
        {
            return Err(StripeCatalogSnapshotError::UnexpectedIdentifiers);
        }
        Ok(())
    }

    pub fn reconciliation_against(
        &self,
        local: &PublishedCommercialCatalogRevision,
    ) -> CatalogReconciliationReport {
        local.reconcile_with_provider(&self.revision)
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StripeCatalogRequestError {
    #[error("published catalog revision is invalid: {0}")]
    InvalidCatalog(#[from] CatalogError),
    #[error("catalog publish idempotency key is required")]
    MissingIdempotencyKey,
    #[error("catalog publish idempotency key is too long")]
    IdempotencyKeyTooLong,
    #[error("catalog publish idempotency key contains control characters")]
    IdempotencyKeyHasControlCharacter,
    #[error("expected provider revision reference cannot be blank")]
    BlankExpectedProviderReference,
    #[error("expected provider revision reference is invalid")]
    InvalidExpectedProviderReference,
}

fn validate_idempotency_key(key: &str) -> Result<(), StripeCatalogRequestError> {
    if key.trim().is_empty() {
        return Err(StripeCatalogRequestError::MissingIdempotencyKey);
    }
    if key.len() > 255 {
        return Err(StripeCatalogRequestError::IdempotencyKeyTooLong);
    }
    if key.chars().any(char::is_control) {
        return Err(StripeCatalogRequestError::IdempotencyKeyHasControlCharacter);
    }
    Ok(())
}

fn is_valid_reference_text(value: &str) -> bool {
    value.len() <= 255 && !value.chars().any(char::is_control)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub enum StripeCatalogOperation {
    Publish,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StripeCatalogIdempotencyBinding {
    pub operation: StripeCatalogOperation,
    pub revision_number: u32,
    pub provider_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StripeCatalogPublishRequest {
    pub local_revision: PublishedCommercialCatalogRevision,
    pub idempotency_key: String,
}

impl StripeCatalogPublishRequest {
    pub fn validate(&self) -> Result<(), StripeCatalogRequestError> {
        self.local_revision.validate()?;
        validate_idempotency_key(&self.idempotency_key)
    }

    /// Derive the provider idempotency key from the operation, account,
    /// revision, and caller key. The adapter receives this bound value rather
    /// than an unscoped caller string, preventing a key from being reused for
    /// a different account or catalog revision.
    pub fn bind_idempotency_key(
        &self,
        config: &StripeProviderConfig,
    ) -> Result<StripeCatalogIdempotencyBinding, StripeCatalogRequestError> {
        self.validate()?;
        let revision_number = self.local_revision.revision_number();
        let mut digest = Sha256::new();
        digest.update(b"erpx-stripe-catalog-publish");
        digest.update([0]);
        digest.update(config.account_ref().as_str().as_bytes());
        digest.update([0]);
        digest.update(revision_number.to_string().as_bytes());
        digest.update([0]);
        digest.update(self.idempotency_key.as_bytes());
        Ok(StripeCatalogIdempotencyBinding {
            operation: StripeCatalogOperation::Publish,
            revision_number,
            provider_key: format!(
                "erpx-catalog-publish-{revision_number}-{}",
                hex::encode(digest.finalize())
            ),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct StripeCatalogReconcileRequest {
    pub local_revision: PublishedCommercialCatalogRevision,
    pub expected_provider_revision_ref: Option<String>,
}

impl StripeCatalogReconcileRequest {
    pub fn validate(&self) -> Result<(), StripeCatalogRequestError> {
        self.local_revision.validate()?;
        if self
            .expected_provider_revision_ref
            .as_deref()
            .is_some_and(|reference| reference.trim().is_empty())
        {
            return Err(StripeCatalogRequestError::BlankExpectedProviderReference);
        }
        if self
            .expected_provider_revision_ref
            .as_deref()
            .is_some_and(|reference| !is_valid_reference_text(reference))
        {
            return Err(StripeCatalogRequestError::InvalidExpectedProviderReference);
        }
        Ok(())
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StripeCatalogPublicationPlanError {
    #[error(transparent)]
    Configuration(#[from] StripeProviderConfigError),
    #[error(transparent)]
    Request(#[from] StripeCatalogRequestError),
    #[error("catalog item key {item_key} cannot be carried in Stripe metadata")]
    InvalidItemKey { item_key: String },
    #[error("catalog display name for {item_key} exceeds the Stripe product limit")]
    DisplayNameTooLong { item_key: String },
    #[error("catalog amount for {item_key} exceeds the Stripe integer amount range")]
    AmountOutOfRange { item_key: String },
    #[error("metered catalog item {item_key} has no provider billing interval")]
    MeteredIntervalRequired { item_key: String },
}

/// The typed, deterministic work list consumed by a catalog adapter. Product
/// creation is intentionally separate from Price creation because Stripe
/// requires the Product identifier before a Price request can be formed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeCatalogPublicationStep {
    pub item_key: String,
    pub product_request: StripeProductCreateRequest,
    pub product_idempotency_key: String,
    pub price_idempotency_key: String,
    price_currency: String,
    price_amount_minor: i64,
    price_recurring: Option<StripePriceRecurring>,
    price_metadata: BTreeMap<String, String>,
}

impl StripeCatalogPublicationStep {
    pub fn price_request(&self, product: crate::StripeProductId) -> StripePriceCreateRequest {
        StripePriceCreateRequest {
            product,
            currency: self.price_currency.clone(),
            unit_amount_minor: self.price_amount_minor,
            active: Some(true),
            recurring: self.price_recurring,
            metadata: self.price_metadata.clone(),
        }
    }

    pub fn price_currency(&self) -> &str {
        &self.price_currency
    }

    pub fn price_amount_minor(&self) -> i64 {
        self.price_amount_minor
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeCatalogPublicationPlan {
    account_ref: crate::StripeAccountId,
    api_base_url: String,
    api_version: String,
    binding: StripeCatalogIdempotencyBinding,
    revision_number: u32,
    steps: Vec<StripeCatalogPublicationStep>,
}

impl StripeCatalogPublicationPlan {
    pub fn build(
        config: &StripeProviderConfig,
        request: &StripeCatalogPublishRequest,
    ) -> Result<Self, StripeCatalogPublicationPlanError> {
        config.validate()?;
        request.validate()?;
        let binding = request.bind_idempotency_key(config)?;
        let mut prices = request.local_revision.prices().to_vec();
        prices.sort_by(|left, right| left.item_key.cmp(&right.item_key));
        let mut steps = Vec::with_capacity(prices.len());
        for price in &prices {
            validate_publication_item(price)?;
            let mut product_metadata = BTreeMap::new();
            product_metadata.insert(
                "erpx_catalog_revision".into(),
                request.local_revision.revision_number().to_string(),
            );
            product_metadata.insert("erpx_catalog_item".into(), price.item_key.clone());
            product_metadata.insert(
                "erpx_catalog_provider_ref".into(),
                format!("stripe_catalog_{}", binding.provider_key),
            );
            product_metadata.insert(
                "erpx_pricing_unit".into(),
                pricing_unit_key(price.pricing_unit),
            );
            let mut price_metadata = product_metadata.clone();
            price_metadata.insert("erpx_price_kind".into(), price_kind_key(price.kind));

            let step_key = |operation: &str| {
                let mut digest = Sha256::new();
                digest.update(b"erpx-stripe-catalog-step");
                digest.update([0]);
                digest.update(binding.provider_key.as_bytes());
                digest.update([0]);
                digest.update(operation.as_bytes());
                digest.update([0]);
                digest.update(price.item_key.as_bytes());
                format!(
                    "erpx-catalog-{operation}-v1-{}",
                    hex::encode(digest.finalize())
                )
            };

            steps.push(StripeCatalogPublicationStep {
                item_key: price.item_key.clone(),
                product_request: StripeProductCreateRequest {
                    name: price.display_name.clone(),
                    active: Some(true),
                    description: Some(format!(
                        "ERPX catalog revision {} item {}",
                        request.local_revision.revision_number(),
                        price.item_key
                    )),
                    metadata: product_metadata,
                },
                product_idempotency_key: step_key("product"),
                price_idempotency_key: step_key("price"),
                price_currency: price.amount.currency.to_ascii_lowercase(),
                price_amount_minor: i64::try_from(price.amount.amount_minor).map_err(|_| {
                    StripeCatalogPublicationPlanError::AmountOutOfRange {
                        item_key: price.item_key.clone(),
                    }
                })?,
                price_recurring: stripe_recurring(price)?,
                price_metadata,
            });
        }
        Ok(Self {
            account_ref: config.account_ref().clone(),
            api_base_url: config.api_base_url().into(),
            api_version: config.api_version().into(),
            binding,
            revision_number: request.local_revision.revision_number(),
            steps,
        })
    }

    pub fn account_ref(&self) -> &crate::StripeAccountId {
        &self.account_ref
    }

    pub fn api_version(&self) -> &str {
        &self.api_version
    }

    pub fn api_base_url(&self) -> &str {
        &self.api_base_url
    }

    pub fn binding(&self) -> &StripeCatalogIdempotencyBinding {
        &self.binding
    }

    pub fn revision_number(&self) -> u32 {
        self.revision_number
    }

    pub fn steps(&self) -> &[StripeCatalogPublicationStep] {
        &self.steps
    }
}

fn validate_publication_item(
    price: &erpx_domain::commercial::CatalogPrice,
) -> Result<(), StripeCatalogPublicationPlanError> {
    if price.item_key.is_empty()
        || price.item_key.len() > 500
        || price.item_key.chars().any(char::is_control)
    {
        return Err(StripeCatalogPublicationPlanError::InvalidItemKey {
            item_key: price.item_key.clone(),
        });
    }
    if price.display_name.len() > 255 || price.display_name.chars().any(char::is_control) {
        return Err(StripeCatalogPublicationPlanError::DisplayNameTooLong {
            item_key: price.item_key.clone(),
        });
    }
    Ok(())
}

fn stripe_recurring(
    price: &erpx_domain::commercial::CatalogPrice,
) -> Result<Option<StripePriceRecurring>, StripeCatalogPublicationPlanError> {
    match price.charge {
        ChargeKind::OneTime => Ok(None),
        ChargeKind::Recurring(interval) => Ok(Some(StripePriceRecurring {
            interval: match interval {
                erpx_domain::commercial::BillingInterval::Monthly => StripePriceInterval::Month,
                erpx_domain::commercial::BillingInterval::Yearly => StripePriceInterval::Year,
            },
            usage_type: StripePriceUsageType::Licensed,
        })),
        ChargeKind::Metered => Err(StripeCatalogPublicationPlanError::MeteredIntervalRequired {
            item_key: price.item_key.clone(),
        }),
    }
}

fn pricing_unit_key(unit: PricingUnit) -> String {
    match unit {
        PricingUnit::Flat => "flat",
        PricingUnit::Seat => "seat",
        PricingUnit::Company => "company",
        PricingUnit::Branch => "branch",
    }
    .into()
}

fn price_kind_key(kind: erpx_domain::commercial::CatalogPriceKind) -> String {
    match kind {
        erpx_domain::commercial::CatalogPriceKind::BaseFee => "base_fee",
        erpx_domain::commercial::CatalogPriceKind::Module => "module",
        erpx_domain::commercial::CatalogPriceKind::SetupFee => "setup_fee",
    }
    .into()
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeCatalogPublicationResponse {
    pub provider_revision_ref: String,
    pub revision_number: u32,
    pub items: BTreeMap<String, StripeCatalogItemIdentifiers>,
}

impl StripeCatalogPublicationResponse {
    pub fn into_provider_snapshot(
        self,
        local_revision: &PublishedCommercialCatalogRevision,
    ) -> Result<StripeCatalogProviderSnapshot, StripeCatalogSnapshotError> {
        let prices = local_revision
            .prices()
            .iter()
            .map(|price| {
                let identifiers = self
                    .items
                    .get(&price.item_key)
                    .ok_or(StripeCatalogSnapshotError::MissingIdentifiers)?;
                Ok(ProviderCatalogPrice {
                    product_ref: Some(identifiers.product.as_str().into()),
                    price_ref: Some(identifiers.price.as_str().into()),
                    price: price.clone(),
                })
            })
            .collect::<Result<Vec<_>, StripeCatalogSnapshotError>>()?;
        StripeCatalogProviderSnapshot::new(
            ProviderCatalogRevision {
                provider_revision_ref: self.provider_revision_ref,
                revision_number: self.revision_number,
                prices,
            },
            self.items,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeCatalogOperationResult {
    pub snapshot: StripeCatalogProviderSnapshot,
    pub reconciliation: CatalogReconciliationReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripeProviderErrorKind {
    Authentication,
    InvalidRequest,
    Conflict,
    RateLimited,
    Unavailable,
    Unknown,
}

impl StripeProviderErrorKind {
    pub const fn is_retryable(self) -> bool {
        matches!(self, Self::RateLimited | Self::Unavailable)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StripeProviderError {
    kind: StripeProviderErrorKind,
    status: u16,
    provider_code: Option<String>,
    request_id: Option<String>,
    redacted_detail: String,
}

impl StripeProviderError {
    /// Translate a provider response into a safe error. The body is consumed
    /// only to extract an allow-listed error type/code; raw provider details,
    /// URLs, headers, and payment secrets are never retained.
    pub fn from_remote_response(
        status: u16,
        provider_code: Option<&str>,
        request_id: Option<&str>,
        raw_body: &str,
    ) -> Self {
        let kind = match status {
            401 | 403 => StripeProviderErrorKind::Authentication,
            409 => StripeProviderErrorKind::Conflict,
            429 => StripeProviderErrorKind::RateLimited,
            400..=499 => StripeProviderErrorKind::InvalidRequest,
            500..=599 => StripeProviderErrorKind::Unavailable,
            _ => StripeProviderErrorKind::Unknown,
        };
        Self {
            kind,
            status,
            provider_code: safe_provider_token(provider_code),
            request_id: safe_request_id(request_id),
            redacted_detail: redact_provider_error(raw_body),
        }
    }

    pub fn kind(&self) -> StripeProviderErrorKind {
        self.kind
    }

    pub fn status(&self) -> u16 {
        self.status
    }

    pub fn provider_code(&self) -> Option<&str> {
        self.provider_code.as_deref()
    }

    pub fn request_id(&self) -> Option<&str> {
        self.request_id.as_deref()
    }

    pub fn redacted_detail(&self) -> &str {
        &self.redacted_detail
    }
}

pub const MAX_STRIPE_CATALOG_RETRY_ATTEMPTS: u32 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripeCatalogRetryPolicy {
    pub max_attempts: u32,
    pub base_delay: StdDuration,
    pub max_delay: StdDuration,
}

impl Default for StripeCatalogRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 3,
            base_delay: StdDuration::from_millis(100),
            max_delay: StdDuration::from_secs(5),
        }
    }
}

impl StripeCatalogRetryPolicy {
    pub fn new(
        max_attempts: u32,
        base_delay: StdDuration,
        max_delay: StdDuration,
    ) -> Result<Self, PersistenceError> {
        let policy = Self {
            max_attempts,
            base_delay,
            max_delay,
        };
        policy.validate()?;
        Ok(policy)
    }

    pub fn validate(&self) -> Result<(), PersistenceError> {
        if !(1..=MAX_STRIPE_CATALOG_RETRY_ATTEMPTS).contains(&self.max_attempts)
            || self.max_delay < self.base_delay
        {
            return Err(PersistenceError::InvalidData(
                "invalid Stripe catalog retry policy".into(),
            ));
        }
        Ok(())
    }

    pub fn backoff_for_retry(&self, retry_number: u32) -> StdDuration {
        let exponent = retry_number.saturating_sub(1).min(62);
        let multiplier = 1_u128.checked_shl(exponent).unwrap_or(u128::MAX);
        let base_millis = self.base_delay.as_millis();
        let max_millis = self.max_delay.as_millis();
        let millis = base_millis.saturating_mul(multiplier).min(max_millis);
        StdDuration::from_millis(u64::try_from(millis).unwrap_or(u64::MAX))
    }
}

impl std::fmt::Display for StripeProviderError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            formatter,
            "Stripe provider {:?} (status {})",
            self.kind, self.status
        )?;
        if let Some(code) = &self.provider_code {
            write!(formatter, ", code {code}")?;
        }
        if let Some(request_id) = &self.request_id {
            write!(formatter, ", request {request_id}")?;
        }
        write!(formatter, ": {}", self.redacted_detail)
    }
}

impl std::error::Error for StripeProviderError {}

impl StripeProviderError {
    fn local_failure(kind: StripeProviderErrorKind, detail: &'static str) -> Self {
        Self {
            kind,
            status: 0,
            provider_code: None,
            request_id: None,
            redacted_detail: detail.into(),
        }
    }
}

/// The typed Product/Price operations required by the catalog adapter. The
/// production implementation is [`StripeHttpClient`]; tests can inject a
/// recorder without constructing a network client or loading credentials.
#[async_trait]
pub trait StripeCatalogHttpClient: Send + Sync {
    fn config(&self) -> &StripeProviderConfig;

    async fn list_products(
        &self,
        request: StripeProductListRequest,
    ) -> Result<
        StripeResponseEnvelope<crate::StripeListResponse<crate::StripeProductResponse>>,
        StripeHttpError,
    >;

    async fn retrieve_product(
        &self,
        product: crate::StripeProductId,
    ) -> Result<StripeResponseEnvelope<crate::StripeProductResponse>, StripeHttpError>;

    async fn list_prices(
        &self,
        request: StripePriceListRequest,
    ) -> Result<
        StripeResponseEnvelope<crate::StripeListResponse<crate::StripePriceResponse>>,
        StripeHttpError,
    >;

    async fn retrieve_price(
        &self,
        price: crate::StripePriceId,
    ) -> Result<StripeResponseEnvelope<crate::StripePriceResponse>, StripeHttpError>;

    async fn create_product(
        &self,
        request: StripeProductCreateRequest,
        idempotency_key: &str,
    ) -> Result<StripeResponseEnvelope<crate::StripeProductResponse>, StripeHttpError>;

    async fn create_price(
        &self,
        request: StripePriceCreateRequest,
        idempotency_key: &str,
    ) -> Result<StripeResponseEnvelope<crate::StripePriceResponse>, StripeHttpError>;

    /// Compensate a Product created for a publication whose dependent Price
    /// could not be created. Providers that cannot support the typed archive
    /// operation fail closed; they must not report a successful publication
    /// with a partially active catalog.
    async fn archive_product(
        &self,
        product: crate::StripeProductId,
        idempotency_key: &str,
    ) -> Result<StripeResponseEnvelope<crate::StripeProductResponse>, StripeHttpError> {
        let _ = (product, idempotency_key);
        Err(StripeHttpError::Request(
            StripeHttpRequestError::InvalidField {
                field: "catalog_rollback",
            },
        ))
    }
}

#[async_trait]
impl StripeCatalogHttpClient for StripeHttpClient {
    fn config(&self) -> &StripeProviderConfig {
        StripeHttpClient::config(self)
    }

    async fn list_products(
        &self,
        request: StripeProductListRequest,
    ) -> Result<
        StripeResponseEnvelope<crate::StripeListResponse<crate::StripeProductResponse>>,
        StripeHttpError,
    > {
        StripeHttpClient::list_products(self, request).await
    }

    async fn retrieve_product(
        &self,
        product: crate::StripeProductId,
    ) -> Result<StripeResponseEnvelope<crate::StripeProductResponse>, StripeHttpError> {
        StripeHttpClient::retrieve_product(self, product).await
    }

    async fn list_prices(
        &self,
        request: StripePriceListRequest,
    ) -> Result<
        StripeResponseEnvelope<crate::StripeListResponse<crate::StripePriceResponse>>,
        StripeHttpError,
    > {
        StripeHttpClient::list_prices(self, request).await
    }

    async fn retrieve_price(
        &self,
        price: crate::StripePriceId,
    ) -> Result<StripeResponseEnvelope<crate::StripePriceResponse>, StripeHttpError> {
        StripeHttpClient::retrieve_price(self, price).await
    }

    async fn create_product(
        &self,
        request: StripeProductCreateRequest,
        idempotency_key: &str,
    ) -> Result<StripeResponseEnvelope<crate::StripeProductResponse>, StripeHttpError> {
        StripeHttpClient::create_product(self, request, idempotency_key).await
    }

    async fn create_price(
        &self,
        request: StripePriceCreateRequest,
        idempotency_key: &str,
    ) -> Result<StripeResponseEnvelope<crate::StripePriceResponse>, StripeHttpError> {
        StripeHttpClient::create_price(self, request, idempotency_key).await
    }

    async fn archive_product(
        &self,
        product: crate::StripeProductId,
        idempotency_key: &str,
    ) -> Result<StripeResponseEnvelope<crate::StripeProductResponse>, StripeHttpError> {
        StripeHttpClient::archive_product(self, product, idempotency_key).await
    }
}

/// Bounded live catalog provider backed by the repository's typed Stripe HTTP
/// client. A publication executes each validated step as Product then Price,
/// using only the idempotency keys derived by `StripeCatalogPublicationPlan`.
/// It does not claim generated official OpenAPI coverage; that remains an
/// integration prerequisite documented by the packet evidence.
#[derive(Debug)]
pub struct StripeHttpCatalogProvider<C> {
    client: C,
}

impl<C> StripeHttpCatalogProvider<C> {
    pub fn new(client: C) -> Self {
        Self { client }
    }

    pub fn client(&self) -> &C {
        &self.client
    }
}

impl<C> StripeHttpCatalogProvider<C>
where
    C: StripeCatalogHttpClient,
{
    async fn compensate_product(
        &self,
        config: &StripeProviderConfig,
        product: crate::StripeProductId,
        idempotency_key: &str,
    ) -> bool {
        let Ok(response) = self
            .client
            .archive_product(product.clone(), idempotency_key)
            .await
        else {
            return false;
        };
        validate_catalog_response(config, &response).is_ok()
            && response.body.id == product
            && !response.body.active
    }

    async fn compensate_products(
        &self,
        config: &StripeProviderConfig,
        products: &[crate::StripeProductId],
        provider_key: &str,
    ) -> bool {
        let mut seen = BTreeSet::new();
        let mut success = true;
        for product in products.iter().rev() {
            if !seen.insert(product.as_str().to_owned()) {
                continue;
            }
            let rollback_key = catalog_product_rollback_key(provider_key, product);
            if !self
                .compensate_product(config, product.clone(), &rollback_key)
                .await
            {
                success = false;
            }
        }
        success
    }

    async fn rollback_or_preserve_error(
        &self,
        config: &StripeProviderConfig,
        products: &[crate::StripeProductId],
        provider_key: &str,
        error: StripeProviderError,
    ) -> StripeProviderError {
        if self
            .compensate_products(config, products, provider_key)
            .await
        {
            error
        } else {
            local_catalog_failure(StripeProviderErrorKind::Unavailable)
        }
    }
}

#[async_trait]
impl<C> StripeCatalogProvider for StripeHttpCatalogProvider<C>
where
    C: StripeCatalogHttpClient,
{
    async fn publish_catalog(
        &self,
        config: &StripeProviderConfig,
        request: &StripeCatalogPublishRequest,
        idempotency_key: &str,
    ) -> Result<StripeCatalogProviderSnapshot, StripeProviderError> {
        config
            .validate()
            .map_err(|_| local_catalog_failure(StripeProviderErrorKind::InvalidRequest))?;
        if self.client.config() != config {
            return Err(local_catalog_failure(
                StripeProviderErrorKind::InvalidRequest,
            ));
        }
        let plan = StripeCatalogPublicationPlan::build(config, request)
            .map_err(|_| local_catalog_failure(StripeProviderErrorKind::InvalidRequest))?;
        if plan.binding().provider_key != idempotency_key {
            return Err(local_catalog_failure(
                StripeProviderErrorKind::InvalidRequest,
            ));
        }

        let mut items = BTreeMap::new();
        let mut products = BTreeSet::<String>::new();
        let mut prices = BTreeSet::<String>::new();
        let mut published_products = Vec::<crate::StripeProductId>::new();
        for step in plan.steps() {
            let product = match self
                .client
                .create_product(step.product_request.clone(), &step.product_idempotency_key)
                .await
            {
                Ok(product) => product,
                Err(error) => {
                    return Err(self
                        .rollback_or_preserve_error(
                            config,
                            &published_products,
                            plan.binding().provider_key.as_str(),
                            map_stripe_http_error(error),
                        )
                        .await);
                }
            };
            let product_id = product.body.id.clone();
            if validate_catalog_response(config, &product).is_err() || !product.body.active {
                let mut rollback_products = published_products.clone();
                rollback_products.push(product_id);
                return Err(self
                    .rollback_or_preserve_error(
                        config,
                        &rollback_products,
                        plan.binding().provider_key.as_str(),
                        local_catalog_failure(StripeProviderErrorKind::InvalidRequest),
                    )
                    .await);
            }
            if !products.insert(product_id.as_str().into()) {
                let mut rollback_products = published_products.clone();
                rollback_products.push(product_id);
                return Err(self
                    .rollback_or_preserve_error(
                        config,
                        &rollback_products,
                        plan.binding().provider_key.as_str(),
                        local_catalog_failure(StripeProviderErrorKind::InvalidRequest),
                    )
                    .await);
            }
            published_products.push(product_id.clone());

            let price = match self
                .client
                .create_price(
                    step.price_request(product_id.clone()),
                    &step.price_idempotency_key,
                )
                .await
            {
                Ok(price) => price,
                Err(error) => {
                    // Product creation is a provider-side mutation that
                    // precedes Price creation. Compensate with the same
                    // deterministic publication identity before surfacing
                    // the failure, so a partial publication cannot remain
                    // active. If compensation is unavailable or malformed,
                    // return a retryable boundary error instead of hiding
                    // the orphan risk behind the original error.
                    return Err(self
                        .rollback_or_preserve_error(
                            config,
                            &published_products,
                            plan.binding().provider_key.as_str(),
                            map_stripe_http_error(error),
                        )
                        .await);
                }
            };
            if validate_catalog_response(config, &price).is_err() || !price.body.active {
                return Err(self
                    .rollback_or_preserve_error(
                        config,
                        &published_products,
                        plan.binding().provider_key.as_str(),
                        local_catalog_failure(StripeProviderErrorKind::InvalidRequest),
                    )
                    .await);
            }
            if price.body.product != product_id {
                return Err(self
                    .rollback_or_preserve_error(
                        config,
                        &published_products,
                        plan.binding().provider_key.as_str(),
                        local_catalog_failure(StripeProviderErrorKind::InvalidRequest),
                    )
                    .await);
            }
            if !prices.insert(price.body.id.as_str().into()) {
                return Err(self
                    .rollback_or_preserve_error(
                        config,
                        &published_products,
                        plan.binding().provider_key.as_str(),
                        local_catalog_failure(StripeProviderErrorKind::InvalidRequest),
                    )
                    .await);
            }

            if items
                .insert(
                    step.item_key.clone(),
                    StripeCatalogItemIdentifiers {
                        item_key: step.item_key.clone(),
                        product: product_id,
                        price: price.body.id,
                        entitlement_feature: None,
                        meter: None,
                    },
                )
                .is_some()
            {
                return Err(self
                    .rollback_or_preserve_error(
                        config,
                        &published_products,
                        plan.binding().provider_key.as_str(),
                        local_catalog_failure(StripeProviderErrorKind::InvalidRequest),
                    )
                    .await);
            }
        }

        let response = StripeCatalogPublicationResponse {
            provider_revision_ref: format!("stripe_catalog_{}", plan.binding().provider_key),
            revision_number: plan.revision_number(),
            items,
        };
        match response.into_provider_snapshot(&request.local_revision) {
            Ok(snapshot) => Ok(snapshot),
            Err(_) => Err(self
                .rollback_or_preserve_error(
                    config,
                    &published_products,
                    plan.binding().provider_key.as_str(),
                    local_catalog_failure(StripeProviderErrorKind::InvalidRequest),
                )
                .await),
        }
    }

    async fn reconcile_catalog(
        &self,
        config: &StripeProviderConfig,
        request: &StripeCatalogReconcileRequest,
    ) -> Result<StripeCatalogProviderSnapshot, StripeProviderError> {
        config
            .validate()
            .map_err(|_| local_catalog_failure(StripeProviderErrorKind::InvalidRequest))?;
        request
            .validate()
            .map_err(|_| local_catalog_failure(StripeProviderErrorKind::InvalidRequest))?;
        if self.client.config() != config {
            return Err(local_catalog_failure(
                StripeProviderErrorKind::InvalidRequest,
            ));
        }
        let products = list_all_stripe_products(&self.client, config)
            .await
            .map_err(map_stripe_http_error)?;
        let selected = select_catalog_products(
            products,
            request.expected_provider_revision_ref.as_deref(),
            request.local_revision.revision_number(),
        )
        .map_err(|_| local_catalog_failure(StripeProviderErrorKind::InvalidRequest))?;

        let mut provider_prices = Vec::new();
        let mut identifiers = BTreeMap::new();
        let mut seen_items = BTreeSet::new();
        let mut seen_prices = BTreeSet::new();
        for product in &selected.products {
            if !product.body.active {
                continue;
            }
            let prices = list_all_stripe_prices(&self.client, config, product.body.id.clone())
                .await
                .map_err(map_stripe_http_error)?;
            let price = select_catalog_price(&product.body, prices, &selected)
                .map_err(|_| local_catalog_failure(StripeProviderErrorKind::InvalidRequest))?;
            let Some(price) = price else {
                continue;
            };
            if !seen_items.insert(product.item_key.clone())
                || !seen_prices.insert(price.body.id.as_str().to_owned())
            {
                return Err(local_catalog_failure(
                    StripeProviderErrorKind::InvalidRequest,
                ));
            }
            let normalized = normalize_provider_catalog_price(
                &product.body,
                &price.body,
                request.local_revision.price(&product.item_key),
            )
            .map_err(|_| local_catalog_failure(StripeProviderErrorKind::InvalidRequest))?;
            identifiers.insert(
                product.item_key.clone(),
                StripeCatalogItemIdentifiers {
                    item_key: product.item_key.clone(),
                    product: product.body.id.clone(),
                    price: price.body.id.clone(),
                    entitlement_feature: None,
                    meter: None,
                },
            );
            provider_prices.push(normalized);
        }

        StripeCatalogProviderSnapshot::new(
            ProviderCatalogRevision {
                provider_revision_ref: selected.provider_revision_ref,
                revision_number: selected.revision_number,
                prices: provider_prices,
            },
            identifiers,
        )
        .map_err(|_| local_catalog_failure(StripeProviderErrorKind::InvalidRequest))
    }
}

fn local_catalog_failure(kind: StripeProviderErrorKind) -> StripeProviderError {
    StripeProviderError::local_failure(
        kind,
        "Stripe catalog provider boundary rejected the operation",
    )
}

fn catalog_product_rollback_key(provider_key: &str, product: &crate::StripeProductId) -> String {
    let mut digest = Sha256::new();
    digest.update(b"erpx-stripe-catalog-rollback");
    digest.update([0]);
    digest.update(provider_key.as_bytes());
    digest.update([0]);
    digest.update(product.as_str().as_bytes());
    format!("erpx-catalog-rollback-{}", hex::encode(digest.finalize()))
}

fn map_stripe_http_error(error: StripeHttpError) -> StripeProviderError {
    match error {
        StripeHttpError::Provider(error) => error,
        StripeHttpError::Configuration(_)
        | StripeHttpError::Request(_)
        | StripeHttpError::Decode { .. }
        | StripeHttpError::ResponseBodyTooLarge => {
            local_catalog_failure(StripeProviderErrorKind::InvalidRequest)
        }
        StripeHttpError::Credentials(_) => {
            local_catalog_failure(StripeProviderErrorKind::Authentication)
        }
        StripeHttpError::Timeout | StripeHttpError::Transport => {
            local_catalog_failure(StripeProviderErrorKind::Unavailable)
        }
    }
}

fn validate_catalog_response<T>(
    config: &StripeProviderConfig,
    response: &StripeResponseEnvelope<T>,
) -> Result<(), ()> {
    if !(200..=299).contains(&response.status) || response.api_version != config.api_version() {
        return Err(());
    }
    Ok(())
}

#[derive(Debug, Clone)]
struct RemoteCatalogProduct {
    body: crate::StripeProductResponse,
    item_key: String,
    revision_number: u32,
    provider_revision_ref: String,
}

#[derive(Debug)]
struct SelectedCatalogProducts {
    products: Vec<RemoteCatalogProduct>,
    revision_number: u32,
    provider_revision_ref: String,
}

async fn list_all_stripe_products<C: StripeCatalogHttpClient>(
    client: &C,
    config: &StripeProviderConfig,
) -> Result<
    Vec<StripeResponseEnvelope<crate::StripeListResponse<crate::StripeProductResponse>>>,
    StripeHttpError,
> {
    let mut products = Vec::new();
    let mut starting_after = None;
    for page_number in 0..MAX_STRIPE_CATALOG_RECONCILIATION_PAGES {
        let response = client
            .list_products(StripeProductListRequest {
                active: None,
                limit: Some(STRIPE_CATALOG_RECONCILIATION_PAGE_SIZE),
                starting_after: starting_after.clone(),
            })
            .await?;
        validate_catalog_response(config, &response)
            .map_err(|_| invalid_catalog_reconciliation_request())?;
        if response.body.data.len() > usize::from(STRIPE_CATALOG_RECONCILIATION_PAGE_SIZE) {
            return Err(invalid_catalog_reconciliation_request());
        }
        let has_more = response.body.has_more;
        let last_id = response.body.data.last().map(|product| product.id.clone());
        products.push(response);
        if !has_more {
            return Ok(products);
        }
        let Some(last_id) = last_id else {
            return Err(invalid_catalog_reconciliation_request());
        };
        if starting_after.as_ref() == Some(&last_id) {
            return Err(invalid_catalog_reconciliation_request());
        }
        starting_after = Some(last_id);
        if page_number + 1 == MAX_STRIPE_CATALOG_RECONCILIATION_PAGES {
            return Err(invalid_catalog_reconciliation_request());
        }
    }
    Err(invalid_catalog_reconciliation_request())
}

async fn list_all_stripe_prices<C: StripeCatalogHttpClient>(
    client: &C,
    config: &StripeProviderConfig,
    product: crate::StripeProductId,
) -> Result<
    Vec<StripeResponseEnvelope<crate::StripeListResponse<crate::StripePriceResponse>>>,
    StripeHttpError,
> {
    let mut prices = Vec::new();
    let mut starting_after = None;
    for page_number in 0..MAX_STRIPE_CATALOG_RECONCILIATION_PAGES {
        let response = client
            .list_prices(StripePriceListRequest {
                product: Some(product.clone()),
                active: None,
                limit: Some(STRIPE_CATALOG_RECONCILIATION_PAGE_SIZE),
                starting_after: starting_after.clone(),
            })
            .await?;
        validate_catalog_response(config, &response)
            .map_err(|_| invalid_catalog_reconciliation_request())?;
        if response.body.data.len() > usize::from(STRIPE_CATALOG_RECONCILIATION_PAGE_SIZE) {
            return Err(invalid_catalog_reconciliation_request());
        }
        let has_more = response.body.has_more;
        let last_id = response.body.data.last().map(|price| price.id.clone());
        prices.push(response);
        if !has_more {
            return Ok(prices);
        }
        let Some(last_id) = last_id else {
            return Err(invalid_catalog_reconciliation_request());
        };
        if starting_after.as_ref() == Some(&last_id) {
            return Err(invalid_catalog_reconciliation_request());
        }
        starting_after = Some(last_id);
        if page_number + 1 == MAX_STRIPE_CATALOG_RECONCILIATION_PAGES {
            return Err(invalid_catalog_reconciliation_request());
        }
    }
    Err(invalid_catalog_reconciliation_request())
}

fn select_catalog_products(
    pages: Vec<StripeResponseEnvelope<crate::StripeListResponse<crate::StripeProductResponse>>>,
    expected_provider_revision_ref: Option<&str>,
    local_revision_number: u32,
) -> Result<SelectedCatalogProducts, ()> {
    let mut seen_product_ids = BTreeSet::new();
    let mut candidates = Vec::new();
    for page in pages {
        for product in page.body.data {
            if !seen_product_ids.insert(product.id.as_str().to_owned()) {
                return Err(());
            }
            let Some(revision_text) = product.metadata.get("erpx_catalog_revision") else {
                continue;
            };
            let revision_number = parse_revision_number(revision_text)?;
            let item_key = required_catalog_metadata(&product.metadata, "erpx_catalog_item")?;
            let provider_revision_ref = product
                .metadata
                .get("erpx_catalog_provider_ref")
                .map(String::as_str)
                .map(validate_catalog_reference)
                .transpose()?
                .unwrap_or_else(|| format!("stripe_catalog_{revision_number}"));
            candidates.push(RemoteCatalogProduct {
                body: product,
                item_key,
                revision_number,
                provider_revision_ref,
            });
        }
    }

    let (revision_number, provider_revision_ref) =
        if let Some(expected) = expected_provider_revision_ref {
            let matching = candidates
                .iter()
                .filter(|candidate| candidate.provider_revision_ref == expected)
                .collect::<Vec<_>>();
            let revision_number = matching
                .iter()
                .map(|candidate| candidate.revision_number)
                .collect::<BTreeSet<_>>();
            if revision_number.len() > 1 {
                return Err(());
            }
            (
                revision_number
                    .into_iter()
                    .next()
                    .unwrap_or(local_revision_number),
                expected.into(),
            )
        } else if candidates.is_empty() {
            (
                local_revision_number,
                format!("stripe_catalog_empty_{local_revision_number}"),
            )
        } else {
            let revision_number = candidates
                .iter()
                .map(|candidate| candidate.revision_number)
                .max()
                .ok_or(())?;
            let references = candidates
                .iter()
                .filter(|candidate| candidate.revision_number == revision_number)
                .map(|candidate| candidate.provider_revision_ref.clone())
                .collect::<BTreeSet<_>>();
            if references.len() != 1 {
                return Err(());
            }
            (revision_number, references.into_iter().next().ok_or(())?)
        };

    let mut products = candidates
        .into_iter()
        .filter(|candidate| {
            candidate.revision_number == revision_number
                && candidate.provider_revision_ref == provider_revision_ref
        })
        .collect::<Vec<_>>();
    products.sort_by(|left, right| {
        left.item_key
            .cmp(&right.item_key)
            .then_with(|| left.body.id.as_str().cmp(right.body.id.as_str()))
    });
    Ok(SelectedCatalogProducts {
        products,
        revision_number,
        provider_revision_ref,
    })
}

fn select_catalog_price(
    product: &crate::StripeProductResponse,
    pages: Vec<StripeResponseEnvelope<crate::StripeListResponse<crate::StripePriceResponse>>>,
    selected: &SelectedCatalogProducts,
) -> Result<Option<StripeResponseEnvelope<crate::StripePriceResponse>>, ()> {
    let selected_product = selected
        .products
        .iter()
        .find(|candidate| candidate.body.id == product.id)
        .ok_or(())?;
    let mut matches = Vec::new();
    for page in pages {
        for price in page.body.data {
            if price.product != product.id {
                return Err(());
            }
            let Some(revision) = price.metadata.get("erpx_catalog_revision") else {
                continue;
            };
            if parse_revision_number(revision)? != selected_product.revision_number {
                continue;
            }
            let Some(provider_ref) = price.metadata.get("erpx_catalog_provider_ref") else {
                continue;
            };
            if validate_catalog_reference(provider_ref)? != selected_product.provider_revision_ref {
                continue;
            }
            let Some(item_key) = price.metadata.get("erpx_catalog_item") else {
                continue;
            };
            if validate_catalog_reference(item_key)? != selected_product.item_key {
                return Err(());
            }
            if price.active {
                matches.push(StripeResponseEnvelope {
                    status: page.status,
                    request_id: page.request_id.clone(),
                    api_version: page.api_version.clone(),
                    body: price,
                });
            }
        }
    }
    match matches.len() {
        0 => Ok(None),
        1 => Ok(matches.into_iter().next()),
        _ => Err(()),
    }
}

fn normalize_provider_catalog_price(
    product: &crate::StripeProductResponse,
    price: &crate::StripePriceResponse,
    local: Option<&CatalogPrice>,
) -> Result<ProviderCatalogPrice, ()> {
    let amount_minor = price.unit_amount.filter(|amount| *amount > 0).ok_or(())?;
    let currency = price.currency.trim();
    if currency.len() != 3
        || !currency
            .chars()
            .all(|character| character.is_ascii_alphabetic())
    {
        return Err(());
    }
    let metadata = if price.metadata.is_empty() {
        &product.metadata
    } else {
        &price.metadata
    };
    let kind = metadata
        .get("erpx_price_kind")
        .map(|value| parse_price_kind(value))
        .transpose()?
        .or_else(|| local.map(|price| price.kind))
        .ok_or(())?;
    let pricing_unit = product
        .metadata
        .get("erpx_pricing_unit")
        .map(|value| parse_pricing_unit(value))
        .transpose()?
        .or_else(|| local.map(|price| price.pricing_unit))
        .ok_or(())?;
    let charge = match price.recurring {
        None => ChargeKind::OneTime,
        Some(recurring) => match recurring.usage_type {
            crate::StripePriceUsageType::Licensed => {
                ChargeKind::Recurring(match recurring.interval {
                    crate::StripePriceInterval::Month => BillingInterval::Monthly,
                    crate::StripePriceInterval::Year => BillingInterval::Yearly,
                })
            }
            crate::StripePriceUsageType::Metered => ChargeKind::Metered,
        },
    };
    let normalized = local.cloned().unwrap_or_else(|| CatalogPrice {
        item_key: product
            .metadata
            .get("erpx_catalog_item")
            .cloned()
            .unwrap_or_default(),
        display_name: product.name.clone(),
        kind,
        pricing_unit,
        charge,
        amount: Money {
            amount_minor: amount_minor as u64,
            currency: currency.to_ascii_uppercase(),
        },
        trial: None,
        coupon: None,
        setup_fee: None,
        included_usage: None,
        overage: None,
    });
    let mut normalized = normalized;
    normalized.display_name = product.name.clone();
    normalized.kind = kind;
    normalized.pricing_unit = pricing_unit;
    normalized.charge = charge;
    normalized.amount = Money {
        amount_minor: amount_minor as u64,
        currency: currency.to_ascii_uppercase(),
    };
    Ok(ProviderCatalogPrice {
        product_ref: Some(product.id.as_str().into()),
        price_ref: Some(price.id.as_str().into()),
        price: normalized,
    })
}

fn parse_revision_number(value: &str) -> Result<u32, ()> {
    let revision = value.parse::<u32>().map_err(|_| ())?;
    (revision > 0).then_some(revision).ok_or(())
}

fn required_catalog_metadata(metadata: &BTreeMap<String, String>, key: &str) -> Result<String, ()> {
    let value = metadata.get(key).ok_or(())?;
    validate_catalog_reference(value)
}

fn validate_catalog_reference(value: &str) -> Result<String, ()> {
    if value.trim().is_empty() || !is_valid_reference_text(value) {
        return Err(());
    }
    Ok(value.into())
}

fn parse_price_kind(value: &str) -> Result<CatalogPriceKind, ()> {
    match value {
        "base_fee" => Ok(CatalogPriceKind::BaseFee),
        "module" => Ok(CatalogPriceKind::Module),
        "setup_fee" => Ok(CatalogPriceKind::SetupFee),
        _ => Err(()),
    }
}

fn parse_pricing_unit(value: &str) -> Result<PricingUnit, ()> {
    match value {
        "flat" => Ok(PricingUnit::Flat),
        "seat" => Ok(PricingUnit::Seat),
        "company" => Ok(PricingUnit::Company),
        "branch" => Ok(PricingUnit::Branch),
        _ => Err(()),
    }
}

fn invalid_catalog_reconciliation_request() -> StripeHttpError {
    StripeHttpError::Request(StripeHttpRequestError::InvalidField {
        field: "catalog_reconciliation",
    })
}

/// Return a deterministic, allow-listed representation of an untrusted
/// provider error body. Unstructured bodies are intentionally reduced to a
/// fixed string so secrets cannot escape through logs or dead-letter records.
pub fn redact_provider_error(raw_body: &str) -> String {
    const FALLBACK: &str = "provider error details redacted";
    let Ok(value) = serde_json::from_str::<Value>(raw_body) else {
        return FALLBACK.into();
    };
    let object = value
        .get("error")
        .and_then(Value::as_object)
        .or_else(|| value.as_object());
    let Some(object) = object else {
        return FALLBACK.into();
    };

    let mut safe = BTreeMap::new();
    for field in ["code", "decline_code", "type"] {
        if let Some(token) = object.get(field).and_then(safe_error_token) {
            safe.insert(field, token);
        }
    }
    if safe.is_empty() {
        FALLBACK.into()
    } else {
        serde_json::to_string(&safe).unwrap_or_else(|_| FALLBACK.into())
    }
}

fn safe_provider_token(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    let lower = value.to_ascii_lowercase();
    let sensitive_marker = [
        "sk_",
        "rk_",
        "whsec_",
        "secret",
        "token",
        "password",
        "api_key",
        "authorization",
        "client_secret",
    ];
    if value.is_empty()
        || value.len() > 96
        || sensitive_marker.iter().any(|marker| lower.contains(marker))
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "_.-".contains(character))
    {
        None
    } else {
        Some(value.into())
    }
}

fn safe_error_token(value: &Value) -> Option<String> {
    safe_provider_token(value.as_str())
}

fn safe_request_id(value: Option<&str>) -> Option<String> {
    let value = value?.trim();
    if value.starts_with("req_") {
        safe_provider_token(Some(value))
    } else {
        None
    }
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum StripeCatalogBoundaryError {
    #[error(transparent)]
    Configuration(#[from] StripeProviderConfigError),
    #[error(transparent)]
    Request(#[from] StripeCatalogRequestError),
    #[error(transparent)]
    PublicationPlan(#[from] StripeCatalogPublicationPlanError),
    #[error(transparent)]
    Provider(#[from] StripeProviderError),
    #[error(transparent)]
    Snapshot(#[from] StripeCatalogSnapshotError),
    #[error("provider returned an unexpected catalog revision reference")]
    UnexpectedProviderRevisionReference,
    #[error("provider returned a catalog revision that does not match the publication plan")]
    PublicationRevisionMismatch,
    #[error("Stripe catalog retry policy is invalid")]
    InvalidRetryPolicy,
}

/// Production adapters implement this trait after the official Stripe client
/// and credential source are integrated. This packet deliberately supplies no
/// implementation, network fallback, or fake-success adapter.
#[async_trait]
pub trait StripeCatalogProvider: Send + Sync {
    async fn publish_catalog(
        &self,
        config: &StripeProviderConfig,
        request: &StripeCatalogPublishRequest,
        idempotency_key: &str,
    ) -> Result<StripeCatalogProviderSnapshot, StripeProviderError>;

    async fn reconcile_catalog(
        &self,
        config: &StripeProviderConfig,
        request: &StripeCatalogReconcileRequest,
    ) -> Result<StripeCatalogProviderSnapshot, StripeProviderError>;
}

/// Validates configuration and DTOs around an adapter invocation. Keeping the
/// guard separate from the trait prevents an implementation from accidentally
/// running with an unpinned API version while retaining a testable seam.
#[derive(Debug)]
pub struct StripeCatalogProviderBoundary<P> {
    config: StripeProviderConfig,
    provider: P,
}

impl<P> StripeCatalogProviderBoundary<P> {
    pub fn new(
        config: StripeProviderConfig,
        provider: P,
    ) -> Result<Self, StripeCatalogBoundaryError> {
        config.validate()?;
        Ok(Self { config, provider })
    }

    pub fn config(&self) -> &StripeProviderConfig {
        &self.config
    }
}

impl<P: StripeCatalogProvider> StripeCatalogProviderBoundary<P> {
    pub fn publication_plan(
        &self,
        request: &StripeCatalogPublishRequest,
    ) -> Result<StripeCatalogPublicationPlan, StripeCatalogBoundaryError> {
        Ok(StripeCatalogPublicationPlan::build(&self.config, request)?)
    }

    pub async fn publish_catalog(
        &self,
        request: &StripeCatalogPublishRequest,
    ) -> Result<StripeCatalogOperationResult, StripeCatalogBoundaryError> {
        let plan = self.publication_plan(request)?;
        let snapshot = self
            .provider
            .publish_catalog(&self.config, request, &plan.binding.provider_key)
            .await?;
        snapshot.validate()?;
        let expected_provider_revision_ref =
            format!("stripe_catalog_{}", plan.binding.provider_key);
        if snapshot.revision.revision_number != plan.revision_number
            || snapshot.revision.provider_revision_ref != expected_provider_revision_ref
        {
            return Err(StripeCatalogBoundaryError::PublicationRevisionMismatch);
        }
        let reconciliation = snapshot.reconciliation_against(&request.local_revision);
        Ok(StripeCatalogOperationResult {
            snapshot,
            reconciliation,
        })
    }

    /// Retry only provider failures that are safe to retry. Every Product,
    /// Price, and compensation request carries a deterministic idempotency
    /// key, so an ambiguous timeout cannot create a second catalog object.
    /// The bound is deliberately local to the provider operation; callers
    /// still receive the final failure for durable command/DLQ handling.
    pub async fn publish_catalog_with_retry(
        &self,
        request: &StripeCatalogPublishRequest,
        policy: StripeCatalogRetryPolicy,
    ) -> Result<StripeCatalogOperationResult, StripeCatalogBoundaryError> {
        policy
            .validate()
            .map_err(|_| StripeCatalogBoundaryError::InvalidRetryPolicy)?;
        for attempt in 1..=policy.max_attempts {
            match self.publish_catalog(request).await {
                Ok(result) => return Ok(result),
                Err(error) => {
                    let retryable = matches!(
                        &error,
                        StripeCatalogBoundaryError::Provider(provider_error)
                            if provider_error.kind().is_retryable()
                    );
                    if !retryable || attempt == policy.max_attempts {
                        return Err(error);
                    }
                    let delay = policy.backoff_for_retry(attempt);
                    if !delay.is_zero() {
                        tokio::time::sleep(delay).await;
                    }
                }
            }
        }
        unreachable!("Stripe catalog retry policy has at least one attempt")
    }

    pub async fn reconcile_catalog(
        &self,
        request: &StripeCatalogReconcileRequest,
    ) -> Result<StripeCatalogOperationResult, StripeCatalogBoundaryError> {
        self.config.validate()?;
        request.validate()?;
        let snapshot = self
            .provider
            .reconcile_catalog(&self.config, request)
            .await?;
        snapshot.validate()?;
        if request
            .expected_provider_revision_ref
            .as_deref()
            .is_some_and(|expected| expected != snapshot.revision.provider_revision_ref)
        {
            return Err(StripeCatalogBoundaryError::UnexpectedProviderRevisionReference);
        }
        let reconciliation = snapshot.reconciliation_against(&request.local_revision);
        Ok(StripeCatalogOperationResult {
            snapshot,
            reconciliation,
        })
    }
}

#[derive(Debug, Clone)]
pub struct StripeInboxState {
    pub event_id: String,
    pub status: StripeInboxStatus,
    pub attempts: u32,
    pub replay_count: u32,
    pub available_at: DateTime<Utc>,
    pub claimed_by: Option<Uuid>,
    pub claim_expires_at: Option<DateTime<Utc>>,
    pub processed_at: Option<DateTime<Utc>>,
    pub dead_lettered_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StripeInboxTransition {
    Processed {
        attempts: u32,
        processed_at: DateTime<Utc>,
    },
    RetryScheduled {
        attempts: u32,
        available_at: DateTime<Utc>,
    },
    DeadLettered {
        attempts: u32,
        dead_lettered_at: DateTime<Utc>,
    },
}

pub const MAX_STRIPE_INBOX_ATTEMPTS: u32 = 100;
pub const MAX_STRIPE_INBOX_REPLAYS: u32 = 3;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripeInboxReplayPolicy {
    pub max_replays: u32,
}

impl Default for StripeInboxReplayPolicy {
    fn default() -> Self {
        Self {
            max_replays: MAX_STRIPE_INBOX_REPLAYS,
        }
    }
}

impl StripeInboxReplayPolicy {
    pub fn new(max_replays: u32) -> Result<Self, PersistenceError> {
        if !(1..=MAX_STRIPE_INBOX_REPLAYS).contains(&max_replays) {
            return Err(PersistenceError::InvalidData(
                "invalid Stripe inbox replay policy".into(),
            ));
        }
        Ok(Self { max_replays })
    }
}

/// The replay boundary is an administrative mutation, not a background read.
/// Callers must carry the already-authenticated actor, purpose, and
/// correlation binding into the durable audit transaction. No secret or
/// bearer value is stored in this context.
#[derive(Clone, PartialEq, Eq)]
pub struct StripeReplayAuthorizationContext {
    actor_ref: String,
    purpose: String,
    correlation_id: Uuid,
}

impl std::fmt::Debug for StripeReplayAuthorizationContext {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("StripeReplayAuthorizationContext")
            .field("actor_ref", &self.actor_ref)
            .field("purpose", &self.purpose)
            .field("correlation_id", &self.correlation_id)
            .finish()
    }
}

impl StripeReplayAuthorizationContext {
    pub fn new(
        actor_ref: impl Into<String>,
        purpose: impl Into<String>,
        correlation_id: Uuid,
    ) -> Result<Self, PersistenceError> {
        let context = Self {
            actor_ref: actor_ref.into(),
            purpose: purpose.into(),
            correlation_id,
        };
        if context.actor_ref.trim().is_empty()
            || context.purpose.trim().is_empty()
            || context.actor_ref.len() > 255
            || context.purpose.len() > 255
            || context
                .actor_ref
                .chars()
                .chain(context.purpose.chars())
                .any(char::is_control)
            || context.correlation_id.is_nil()
        {
            return Err(PersistenceError::InvalidData(
                "Stripe replay authorization context is invalid".into(),
            ));
        }
        Ok(context)
    }

    pub fn actor_ref(&self) -> &str {
        &self.actor_ref
    }

    pub fn purpose(&self) -> &str {
        &self.purpose
    }

    pub const fn correlation_id(&self) -> Uuid {
        self.correlation_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StripeInboxRetryPolicy {
    pub max_attempts: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
}

impl Default for StripeInboxRetryPolicy {
    fn default() -> Self {
        Self {
            max_attempts: 8,
            base_delay: Duration::seconds(5),
            max_delay: Duration::hours(1),
        }
    }
}

impl StripeInboxRetryPolicy {
    pub fn new(
        max_attempts: u32,
        base_delay: Duration,
        max_delay: Duration,
    ) -> Result<Self, PersistenceError> {
        if !(1..=MAX_STRIPE_INBOX_ATTEMPTS).contains(&max_attempts)
            || base_delay <= Duration::zero()
            || max_delay < base_delay
        {
            return Err(PersistenceError::InvalidData(
                "invalid Stripe inbox retry policy".into(),
            ));
        }
        Ok(Self {
            max_attempts,
            base_delay,
            max_delay,
        })
    }

    pub fn backoff_for_attempt(&self, attempt: u32) -> Duration {
        let exponent = attempt.saturating_sub(1).min(62);
        let multiplier = 1_i64.checked_shl(exponent).unwrap_or(i64::MAX);
        let base_millis = self.base_delay.num_milliseconds();
        let max_millis = self.max_delay.num_milliseconds();
        Duration::milliseconds(base_millis.saturating_mul(multiplier).min(max_millis))
    }
}

#[derive(Debug, Clone)]
pub struct SqlxCommercialStore {
    control: SqlxControlPlane,
    write: PgPool,
    read: PgPool,
}

pub const MAX_BILLING_POLICY_BATCH_SIZE: u32 = 500;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct BillingPolicyAdvanceReport {
    pub considered: u32,
    pub advanced: u32,
}

impl SqlxCommercialStore {
    pub fn new(control: &SqlxControlPlane) -> Self {
        Self {
            control: control.clone(),
            write: control.pools().write.clone(),
            read: control.pools().read.clone(),
        }
    }

    pub async fn publish_catalog_revision(
        &self,
        draft: CommercialCatalogRevisionDraft,
        at: DateTime<Utc>,
        actor: impl Into<String>,
    ) -> Result<PublishedCommercialCatalogRevision, PersistenceError> {
        let revision = draft
            .publish(at, actor)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let payload = serde_json::to_value(&revision)?;
        let mut transaction = self.write.begin().await?;
        let inserted = sqlx::query(
            "INSERT INTO erpx_control.commercial_catalog_revisions
                (id, revision_number, published_at, published_by, payload)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT DO NOTHING",
        )
        .bind(revision.id())
        .bind(i32::try_from(revision.revision_number()).map_err(|_| {
            PersistenceError::InvalidData("catalog revision exceeds database range".into())
        })?)
        .bind(revision.published_at())
        .bind(revision.published_by())
        .bind(&payload)
        .execute(&mut *transaction)
        .await?
        .rows_affected();
        if inserted != 1 {
            return Err(PersistenceError::InvalidData(
                "published catalog revision already exists and is immutable".into(),
            ));
        }
        let version = i64::from(revision.revision_number());
        let correlation_id = Uuid::new_v4();
        let idempotency_key = format!(
            "commercial.catalog.revision.published:{}",
            revision.revision_number()
        );
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, active_scope, actor, correlation_id,
                 idempotency_key, payload, occurred_at, available_at)
             VALUES ($1, $2, 1, $3, $4, $5, NULL, $6, $7, $8, $9, $10, $10)",
        )
        .bind(Uuid::new_v4())
        .bind(CATALOG_REVISION_PUBLISHED_TYPE)
        .bind(CATALOG_REVISION_AGGREGATE_TYPE)
        .bind(revision.id())
        .bind(version)
        .bind(serde_json::json!({
            "actor_ref": revision.published_by(),
            "source": "erpx_commercial_catalog",
        }))
        .bind(correlation_id)
        .bind(&idempotency_key)
        .bind(&payload)
        .bind(revision.published_at())
        .execute(&mut *transaction)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_control.audit_events
                (id, actor_ref, customer_account_id, action, purpose, correlation_id, payload)
             VALUES ($1, $2, NULL, $3, $4, $5, $6)",
        )
        .bind(Uuid::new_v4())
        .bind(revision.published_by())
        .bind(CATALOG_REVISION_PUBLISHED_TYPE)
        .bind("publish commercial catalog revision")
        .bind(correlation_id)
        .bind(serde_json::json!({
            "revision_number": revision.revision_number(),
            "catalog_revision_id": revision.id(),
        }))
        .execute(&mut *transaction)
        .await?;
        transaction.commit().await?;
        Ok(revision)
    }

    pub async fn get_catalog_revision(
        &self,
        revision_number: u32,
    ) -> Result<Option<PublishedCommercialCatalogRevision>, PersistenceError> {
        let row = sqlx::query(
            "SELECT payload FROM erpx_control.commercial_catalog_revisions
             WHERE revision_number = $1",
        )
        .bind(i32::try_from(revision_number).map_err(|_| {
            PersistenceError::InvalidData("catalog revision exceeds database range".into())
        })?)
        .fetch_optional(&self.write)
        .await?;
        row.map(|row| {
            let payload: Value = row.try_get("payload")?;
            let revision: PublishedCommercialCatalogRevision = serde_json::from_value(payload)?;
            revision
                .validate()
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
            Ok(revision)
        })
        .transpose()
    }

    /// Return the latest published catalog that contains at least one price
    /// in the requested ISO currency. Catalog reads remain control-plane
    /// reads and never route through a holding database.
    pub async fn get_latest_catalog_revision(
        &self,
        currency: &str,
    ) -> Result<Option<PublishedCommercialCatalogRevision>, PersistenceError> {
        if currency.len() != 3
            || !currency
                .chars()
                .all(|character| character.is_ascii_uppercase())
        {
            return Err(PersistenceError::InvalidData(
                "catalog currency must be a three-letter uppercase ISO code".into(),
            ));
        }
        let row = sqlx::query(
            "SELECT payload
             FROM erpx_control.commercial_catalog_revisions
             WHERE EXISTS (
                 SELECT 1
                 FROM jsonb_array_elements(payload -> 'prices') AS price
                 WHERE price -> 'amount' ->> 'currency' = $1
             )
             ORDER BY revision_number DESC
             LIMIT 1",
        )
        .bind(currency)
        .fetch_optional(&self.write)
        .await?;
        row.map(|row| {
            let payload: Value = row.try_get("payload")?;
            let revision: PublishedCommercialCatalogRevision = serde_json::from_value(payload)?;
            revision
                .validate()
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
            Ok(revision)
        })
        .transpose()
    }

    /// Persist the provider-owned Product/Price identifiers emitted by a
    /// successful catalog publication. The identifiers stay private to the
    /// server and are resolved by item key during Checkout construction.
    pub async fn persist_stripe_catalog_snapshot(
        &self,
        snapshot: &StripeCatalogProviderSnapshot,
    ) -> Result<(), PersistenceError> {
        snapshot
            .validate()
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let local = self
            .get_catalog_revision(snapshot.revision.revision_number)
            .await?
            .ok_or_else(|| PersistenceError::InvalidData("catalog revision is missing".into()))?;
        if !snapshot.reconciliation_against(&local).is_in_sync() {
            return Err(PersistenceError::InvalidData(
                "Stripe catalog snapshot does not match the published catalog".into(),
            ));
        }
        let mut transaction = self.write.begin().await?;
        for provider_price in &snapshot.revision.prices {
            let identifiers = snapshot
                .identifiers
                .get(&provider_price.price.item_key)
                .ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "Stripe catalog snapshot identifier is missing".into(),
                    )
                })?;
            let persisted = sqlx::query(
                "INSERT INTO erpx_control.stripe_catalog_price_bindings
                    (catalog_revision, item_key, stripe_product_ref,
                     stripe_price_ref, currency, active)
                 VALUES ($1, $2, $3, $4, $5, TRUE)
                 ON CONFLICT (catalog_revision, item_key) DO UPDATE
                 SET active = TRUE
                 WHERE stripe_catalog_price_bindings.stripe_product_ref = EXCLUDED.stripe_product_ref
                   AND stripe_catalog_price_bindings.stripe_price_ref = EXCLUDED.stripe_price_ref
                   AND stripe_catalog_price_bindings.currency = EXCLUDED.currency",
            )
            .bind(
                i32::try_from(snapshot.revision.revision_number).map_err(|_| {
                    PersistenceError::InvalidData("catalog revision exceeds database range".into())
                })?,
            )
            .bind(&provider_price.price.item_key)
            .bind(identifiers.product.as_str())
            .bind(identifiers.price.as_str())
            .bind(&provider_price.price.amount.currency)
            .execute(&mut *transaction)
            .await?;
            if persisted.rows_affected() != 1 {
                return Err(PersistenceError::InvalidData(
                    "Stripe catalog binding conflicts with immutable provider identifiers".into(),
                ));
            }
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Resolve UI-safe item keys to typed, private Stripe Price identifiers.
    pub async fn resolve_stripe_checkout_prices(
        &self,
        revision_number: u32,
        item_keys: &[String],
    ) -> Result<BTreeMap<String, StripePriceId>, PersistenceError> {
        if item_keys.is_empty() || item_keys.len() > 100 {
            return Err(PersistenceError::InvalidData(
                "checkout item selection is outside its bound".into(),
            ));
        }
        let rows = sqlx::query(
            "SELECT item_key, stripe_price_ref
             FROM erpx_control.stripe_catalog_price_bindings
             WHERE catalog_revision = $1
               AND item_key = ANY($2)
               AND active = TRUE",
        )
        .bind(i32::try_from(revision_number).map_err(|_| {
            PersistenceError::InvalidData("catalog revision exceeds database range".into())
        })?)
        .bind(item_keys)
        .fetch_all(&self.write)
        .await?;
        let mut prices = BTreeMap::new();
        for row in rows {
            let item_key: String = row.try_get("item_key")?;
            let price_ref: String = row.try_get("stripe_price_ref")?;
            let price = StripePriceId::parse(price_ref)
                .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
            prices.insert(item_key, price);
        }
        if prices.len() != item_keys.len() {
            return Err(PersistenceError::InvalidData(
                "one or more catalog items have no active Stripe price binding".into(),
            ));
        }
        Ok(prices)
    }

    pub async fn reconcile_catalog_revision(
        &self,
        revision_number: u32,
        provider: &ProviderCatalogRevision,
    ) -> Result<CatalogReconciliationReport, PersistenceError> {
        let local = self.get_catalog_revision(revision_number).await?;
        Ok(CatalogReconciliationReport::compare(
            local.as_ref(),
            provider,
        ))
    }

    pub async fn get_billing_lifecycle(
        &self,
        customer_id: Uuid,
        subscription_id: Uuid,
    ) -> Result<Option<(BillingLifecycle, u64)>, PersistenceError> {
        let row = sqlx::query(
            "SELECT payload, version FROM erpx_control.billing_lifecycles
             WHERE customer_id = $1 AND subscription_id = $2",
        )
        .bind(customer_id)
        .bind(subscription_id)
        .fetch_optional(&self.write)
        .await?;
        row.map(|row| {
            let lifecycle: BillingLifecycle = serde_json::from_value(row.try_get("payload")?)?;
            let version = u64::try_from(row.try_get::<i64, _>("version")?)
                .map_err(|_| PersistenceError::InvalidData("negative billing version".into()))?;
            Ok((lifecycle, version))
        })
        .transpose()
    }

    /// Return the authoritative billing subscription identities owned by one
    /// customer. Callers that require an exact workspace binding must reject
    /// zero or multiple rows rather than selecting one implicitly.
    pub async fn customer_subscription_ids(
        &self,
        customer_id: Uuid,
    ) -> Result<Vec<Uuid>, PersistenceError> {
        if customer_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "customer subscription lookup requires a non-nil customer".into(),
            ));
        }
        sqlx::query_scalar(
            "SELECT subscription_id
             FROM erpx_control.billing_lifecycles
             WHERE customer_id = $1
             ORDER BY subscription_id",
        )
        .bind(customer_id)
        .fetch_all(&self.write)
        .await
        .map_err(Into::into)
    }

    pub async fn get_entitlement_book(
        &self,
        customer_id: Uuid,
        module_key: &str,
    ) -> Result<Option<(EntitlementAllocationBook, u64)>, PersistenceError> {
        let row = sqlx::query(
            "SELECT payload, version FROM erpx_control.entitlement_allocation_books
             WHERE customer_id = $1 AND module_key = $2",
        )
        .bind(customer_id)
        .bind(module_key)
        .fetch_optional(&self.write)
        .await?;
        row.map(|row| {
            let book: EntitlementAllocationBook = serde_json::from_value(row.try_get("payload")?)?;
            let version = u64::try_from(row.try_get::<i64, _>("version")?).map_err(|_| {
                PersistenceError::InvalidData("negative entitlement version".into())
            })?;
            Ok((book, version))
        })
        .transpose()
    }

    pub async fn record_grandfathered_migration(
        &self,
        migration: &GrandfatheredPriceMigration,
    ) -> Result<(), PersistenceError> {
        if migration.id.is_nil()
            || migration.customer_id.is_nil()
            || migration.subscription_id.is_nil()
            || migration.recorded_by.trim().is_empty()
        {
            return Err(commercial_invalid(
                "grandfathered price migration identity is required",
            ));
        }
        let payload = serde_json::to_value(migration)?;
        let mut transaction = self.write.begin().await?;
        let result = sqlx::query(
            "INSERT INTO erpx_control.grandfathered_price_migrations
                (id, customer_id, subscription_id, item_key, from_revision, to_revision,
                 effective_at, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (customer_id, subscription_id, item_key, from_revision, to_revision)
             DO NOTHING",
        )
        .bind(migration.id)
        .bind(migration.customer_id)
        .bind(migration.subscription_id)
        .bind(&migration.item_key)
        .bind(i32::try_from(migration.from_revision).map_err(|_| {
            PersistenceError::InvalidData("catalog revision exceeds database range".into())
        })?)
        .bind(i32::try_from(migration.to_revision).map_err(|_| {
            PersistenceError::InvalidData("catalog revision exceeds database range".into())
        })?)
        .bind(migration.effective_at)
        .bind(&payload)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(PersistenceError::InvalidData(
                "grandfathered price migration already exists".into(),
            ));
        }
        let event_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!("commercial.grandfathered-price-migration:{}", migration.id).as_bytes(),
        );
        append_commercial_state_event(
            &mut transaction,
            "commercial_grandfathered_price_migration",
            migration.id,
            1,
            "commercial.billing.grandfathered_price_migrated.v1",
            serde_json::json!({ "migration": migration }),
            migration.effective_at,
            &migration.recorded_by,
            event_id,
            &format!("commercial.grandfathered-price-migration:{}", migration.id),
            Some(migration.customer_id),
            "commercial state bootstrap",
            serde_json::json!({ "migration_id": migration.id }),
        )
        .await?;
        transaction.commit().await?;
        Ok(())
    }

    pub async fn upsert_billing_lifecycle(
        &self,
        customer_id: Uuid,
        subscription_id: Uuid,
        lifecycle: &BillingLifecycle,
        expected_version: Option<u64>,
    ) -> Result<u64, PersistenceError> {
        if customer_id.is_nil() || subscription_id.is_nil() {
            return Err(commercial_invalid("commercial aggregate IDs are required"));
        }
        let payload = serde_json::to_value(lifecycle)?;
        let next_version = match expected_version {
            Some(version) => version
                .checked_add(1)
                .ok_or_else(|| PersistenceError::InvalidData("billing version overflow".into()))?,
            None => 1,
        };
        let mut transaction = self.write.begin().await?;
        let result = if let Some(expected) = expected_version {
            sqlx::query(
                "UPDATE erpx_control.billing_lifecycles
                 SET state = $3, grace_started_at = $4, grace_until = $5,
                     last_transition_at = $6, version = $7, payload = $8, updated_at = now()
                 WHERE customer_id = $1 AND subscription_id = $2 AND version = $9",
            )
            .bind(customer_id)
            .bind(subscription_id)
            .bind(billing_state(lifecycle.state))
            .bind(lifecycle.grace_started_at)
            .bind(lifecycle.grace_until)
            .bind(lifecycle.last_transition_at)
            .bind(i64::try_from(next_version).map_err(|_| {
                PersistenceError::InvalidData("billing version exceeds database range".into())
            })?)
            .bind(&payload)
            .bind(i64::try_from(expected).map_err(|_| {
                PersistenceError::InvalidData("billing version exceeds database range".into())
            })?)
            .execute(&mut *transaction)
            .await?
            .rows_affected()
        } else {
            sqlx::query(
                "INSERT INTO erpx_control.billing_lifecycles
                    (customer_id, subscription_id, state, grace_started_at, grace_until,
                     last_transition_at, version, payload)
                 VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
                 ON CONFLICT DO NOTHING",
            )
            .bind(customer_id)
            .bind(subscription_id)
            .bind(billing_state(lifecycle.state))
            .bind(lifecycle.grace_started_at)
            .bind(lifecycle.grace_until)
            .bind(lifecycle.last_transition_at)
            .bind(1_i64)
            .bind(&payload)
            .execute(&mut *transaction)
            .await?
            .rows_affected()
        };
        if result != 1 {
            let actual = sqlx::query_scalar::<_, i64>(
                "SELECT version FROM erpx_control.billing_lifecycles
                 WHERE customer_id = $1 AND subscription_id = $2",
            )
            .bind(customer_id)
            .bind(subscription_id)
            .fetch_optional(&mut *transaction)
            .await?
            .map(|version| u64::try_from(version).unwrap_or(u64::MAX));
            let _ = transaction.rollback().await;
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "billing_lifecycle".into(),
                aggregate_id: subscription_id,
                expected: expected_version,
                actual,
            });
        }
        let event_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "commercial.billing.lifecycle.bootstrap:{customer_id}:{subscription_id}:{next_version}"
            )
            .as_bytes(),
        );
        append_commercial_state_event(
            &mut transaction,
            BILLING_LIFECYCLE_AGGREGATE_TYPE,
            subscription_id,
            next_version,
            "commercial.billing.lifecycle.upserted.v1",
            serde_json::json!({
                "customer_id": customer_id,
                "subscription_id": subscription_id,
                "lifecycle": payload,
            }),
            lifecycle.last_transition_at,
            "system.commercial.bootstrap",
            event_id,
            &format!(
                "commercial.billing.lifecycle.bootstrap:{customer_id}:{subscription_id}:{next_version}"
            ),
            Some(customer_id),
            "commercial state bootstrap",
            serde_json::json!({
                "customer_id": customer_id,
                "subscription_id": subscription_id,
                "aggregate_version": next_version,
            }),
        )
        .await?;
        transaction.commit().await?;
        Ok(next_version)
    }

    pub async fn upsert_entitlement_book(
        &self,
        customer_id: Uuid,
        book: &EntitlementAllocationBook,
        expected_version: Option<u64>,
    ) -> Result<u64, PersistenceError> {
        if customer_id.is_nil() || book.module_key.trim().is_empty() {
            return Err(commercial_invalid(
                "commercial entitlement identity is required",
            ));
        }
        let payload = serde_json::to_value(book)?;
        let next_version = expected_version
            .unwrap_or(0)
            .checked_add(1)
            .ok_or_else(|| PersistenceError::InvalidData("entitlement version overflow".into()))?;
        let mut transaction = self.write.begin().await?;
        lock_commercial_aggregate(
            &mut transaction,
            &format!("entitlement:{}:{}", customer_id, book.module_key),
        )
        .await?;
        let result = sqlx::query(
            "INSERT INTO erpx_control.entitlement_allocation_books
                (customer_id, module_key, active, active_until, version, payload)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (customer_id, module_key) DO UPDATE SET
                active = EXCLUDED.active, active_until = EXCLUDED.active_until,
                version = EXCLUDED.version, payload = EXCLUDED.payload, updated_at = now()
             WHERE erpx_control.entitlement_allocation_books.version = $7",
        )
        .bind(customer_id)
        .bind(&book.module_key)
        .bind(book.active)
        .bind(book.active_until)
        .bind(i64::try_from(next_version).map_err(|_| {
            PersistenceError::InvalidData("entitlement version exceeds database range".into())
        })?)
        .bind(&payload)
        .bind(i64::try_from(expected_version.unwrap_or(0)).map_err(|_| {
            PersistenceError::InvalidData("entitlement version exceeds database range".into())
        })?)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() != 1 {
            let _ = transaction.rollback().await;
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: "entitlement_allocation".into(),
                aggregate_id: customer_id,
                expected: expected_version,
                actual: None,
            });
        }
        let event_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "commercial.entitlement.book.bootstrap:{customer_id}:{}:{next_version}",
                book.module_key
            )
            .as_bytes(),
        );
        let aggregate_type = entitlement_aggregate_type(&book.module_key);
        append_commercial_state_event(
            &mut transaction,
            &aggregate_type,
            customer_id,
            next_version,
            "commercial.entitlement.book.upserted.v1",
            serde_json::json!({
                "customer_id": customer_id,
                "module_key": book.module_key,
                "book": book,
            }),
            Utc::now(),
            "system.commercial.bootstrap",
            event_id,
            &format!(
                "commercial.entitlement.book.bootstrap:{customer_id}:{}:{next_version}",
                book.module_key
            ),
            Some(customer_id),
            "commercial state bootstrap",
            serde_json::json!({
                "customer_id": customer_id,
                "module_key": book.module_key,
                "aggregate_version": next_version,
            }),
        )
        .await?;
        transaction.commit().await?;
        Ok(next_version)
    }

    /// Apply a provider-authenticated billing event as one control-plane
    /// command. The provider event, not a requested `BillingState`, is the
    /// only input that can change the lifecycle. Idempotency reservation,
    /// aggregate locking, state persistence, audit, and receipt completion
    /// share one transaction.
    pub async fn apply_provider_billing_event(
        &self,
        command: BillingPolicyCommand,
        event: ProviderBillingEvent,
    ) -> Result<CommercialCommandReceipt, PersistenceError> {
        command
            .validate()
            .map_err(|_| commercial_invalid("invalid commercial billing command"))?;
        event
            .validate()
            .map_err(|_| commercial_invalid("invalid provider billing event"))?;
        // `command_id` and `correlation_id` are transport-attempt metadata;
        // they are intentionally regenerated by `CommercialCommandContext::for_provider`
        // on each delivery of the same provider event.  Hash only the
        // authoritative command payload plus the stable authorization
        // context so a provider replay can use its event-derived key.
        let request_hash = commercial_command_request_hash(
            &(&command.customer_id, &command.subscription_id, &event),
            &command.context,
        )?;
        let mut transaction = self.write.begin().await?;
        match reserve_commercial_idempotency(&mut transaction, &command.context, &request_hash)
            .await?
        {
            CommercialIdempotencyReservation::Replayed(receipt) => {
                transaction.commit().await?;
                return Ok(replayed_receipt(receipt));
            }
            CommercialIdempotencyReservation::Reserved => {}
        }

        lock_commercial_aggregate(
            &mut transaction,
            &format!(
                "billing:{}:{}",
                command.customer_id, command.subscription_id
            ),
        )
        .await?;
        let existing = load_billing_lifecycle(
            &mut transaction,
            command.customer_id,
            command.subscription_id,
        )
        .await?;
        let (mut lifecycle, expected_version) = match existing {
            Some((lifecycle, version)) => (lifecycle, Some(version)),
            None => (seed_lifecycle(event.event, event.created_at), None),
        };
        let change = lifecycle
            .apply_provider_event(&event)
            .map_err(|_| commercial_invalid("provider billing event cannot advance lifecycle"))?;
        let version = if change == CommercialCommandChange::DuplicateOrOutOfOrder {
            expected_version.ok_or_else(|| {
                commercial_invalid("stale provider billing event has no persisted lifecycle")
            })?
        } else {
            let version = persist_billing_lifecycle(
                &mut transaction,
                command.customer_id,
                command.subscription_id,
                &lifecycle,
                expected_version,
            )
            .await?;
            let evidence = BillingProviderCommandEvidence {
                customer_id: command.customer_id,
                subscription_id: command.subscription_id,
                provider_event_id: event.provider_event_id.clone(),
                provider_event_at: event.created_at,
                event: event.event,
                state: lifecycle.state,
                change,
            };
            persist_billing_provider_evidence(
                &mut transaction,
                &command.context,
                version,
                &evidence,
                event.created_at,
            )
            .await?;
            version
        };
        let result = CommercialCommandResult::Billing {
            state: lifecycle.state,
            change,
        };
        append_commercial_audit(
            &mut transaction,
            command.customer_id,
            &command.context,
            "commercial.billing.provider_event_applied",
            serde_json::json!({
                "command_id": command.context.command_id,
                "provider_event_id": event.provider_event_id,
                "provider_event_at": event.created_at,
                "subscription_id": command.subscription_id,
                "state": billing_state(lifecycle.state),
                "change": change,
            }),
        )
        .await?;
        let receipt = CommercialCommandReceipt {
            command_id: command.context.command_id,
            idempotency_key: command.context.idempotency_key.clone(),
            aggregate_type: "billing_lifecycle".into(),
            aggregate_id: command.subscription_id,
            aggregate_version: version,
            result,
            replayed: false,
        };
        complete_commercial_idempotency(&mut transaction, &command.context, &receipt).await?;
        transaction.commit().await?;
        Ok(receipt)
    }

    /// Apply a verified provider entitlement snapshot atomically. The
    /// provider event timestamp is the ordering watermark; stale/equal
    /// snapshots are acknowledged as safe replays and cannot roll back
    /// capacity or activation.
    pub async fn apply_provider_entitlement_update(
        &self,
        customer_id: Uuid,
        context: CommercialCommandContext,
        update: ProviderEntitlementUpdate,
    ) -> Result<CommercialCommandReceipt, PersistenceError> {
        if customer_id.is_nil() {
            return Err(commercial_invalid("commercial customer ID is required"));
        }
        context
            .validate()
            .map_err(|_| commercial_invalid("invalid commercial command context"))?;
        update
            .validate()
            .map_err(|_| commercial_invalid("invalid provider entitlement update"))?;
        let request_hash = commercial_command_request_hash(&(&customer_id, &update), &context)?;
        let mut transaction = self.write.begin().await?;
        match reserve_commercial_idempotency(&mut transaction, &context, &request_hash).await? {
            CommercialIdempotencyReservation::Replayed(receipt) => {
                transaction.commit().await?;
                return Ok(replayed_receipt(receipt));
            }
            CommercialIdempotencyReservation::Reserved => {}
        }

        lock_commercial_aggregate(
            &mut transaction,
            &format!("entitlement:{}:{}", customer_id, update.module_key),
        )
        .await?;
        let existing =
            load_entitlement_book(&mut transaction, customer_id, &update.module_key).await?;
        let (mut book, expected_version) = match existing {
            Some((book, version)) => (book, Some(version)),
            None => (
                EntitlementAllocationBook::new(
                    update.module_key.clone(),
                    update.capacity,
                    update.active_until,
                ),
                None,
            ),
        };
        let change = book
            .apply_provider_update(&update)
            .map_err(|_| commercial_invalid("provider entitlement update is not applicable"))?;
        let version = if change == CommercialCommandChange::DuplicateOrOutOfOrder {
            expected_version.ok_or_else(|| {
                commercial_invalid("stale provider entitlement has no persisted book")
            })?
        } else {
            let version =
                persist_entitlement_book(&mut transaction, customer_id, &book, expected_version)
                    .await?;
            persist_entitlement_provider_evidence(
                &mut transaction,
                customer_id,
                &context,
                &book,
                version,
                &update,
                change,
            )
            .await?;
            version
        };
        let result = CommercialCommandResult::Entitlement {
            module_key: book.module_key.clone(),
            active: book.active,
            capacity: book.capacity,
            change,
        };
        append_commercial_audit(
            &mut transaction,
            customer_id,
            &context,
            "commercial.entitlement.provider_snapshot_applied",
            serde_json::json!({
                "command_id": context.command_id,
                "provider_event_id": update.provider_event_id,
                "provider_event_at": update.created_at,
                "module_key": book.module_key,
                "active": book.active,
                "change": change,
            }),
        )
        .await?;
        let receipt = CommercialCommandReceipt {
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
            aggregate_type: "entitlement_allocation".into(),
            aggregate_id: customer_id,
            aggregate_version: version,
            result,
            replayed: false,
        };
        complete_commercial_idempotency(&mut transaction, &context, &receipt).await?;
        transaction.commit().await?;
        Ok(receipt)
    }

    /// Advance only the worker/system billing policy. This command can enter
    /// grace or read-only at the exact boundary, but cannot recover payment
    /// or choose an arbitrary billing state.
    pub async fn advance_billing_policy(
        &self,
        command: BillingPolicyCommand,
    ) -> Result<CommercialCommandReceipt, PersistenceError> {
        let mut transaction = self.write.begin().await?;
        let receipt = advance_billing_policy_in_transaction(&mut transaction, command).await?;
        transaction.commit().await?;
        Ok(receipt)
    }

    /// Advance due billing policy rows in one bounded, lock-scoped pass. The
    /// lifecycle row lock prevents two policy workers from advancing the same
    /// aggregate, while the command idempotency key makes a crash/retry safe
    /// after a committed transition. Provider events remain the only source
    /// of payment recovery and cancellation.
    pub async fn advance_due_billing_policies(
        &self,
        actor_ref: &str,
        at: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<BillingPolicyAdvanceReport, PersistenceError> {
        if actor_ref.trim().is_empty()
            || actor_ref.len() > 256
            || actor_ref.chars().any(char::is_control)
            || batch_size == 0
        {
            return Err(commercial_invalid("invalid billing policy worker request"));
        }
        let mut transaction = self.write.begin().await?;
        let rows = sqlx::query(
            "SELECT customer_id, subscription_id
             FROM erpx_control.billing_lifecycles
             WHERE state = 'past_due'
                OR (state = 'grace' AND grace_until IS NOT NULL AND grace_until <= $1)
             ORDER BY last_transition_at, subscription_id
             FOR UPDATE SKIP LOCKED
             LIMIT $2",
        )
        .bind(at)
        .bind(i64::from(batch_size.min(MAX_BILLING_POLICY_BATCH_SIZE)))
        .fetch_all(&mut *transaction)
        .await?;

        let mut report = BillingPolicyAdvanceReport {
            considered: rows.len() as u32,
            ..Default::default()
        };
        for row in rows {
            let customer_id = row.try_get::<Uuid, _>("customer_id")?;
            let subscription_id = row.try_get::<Uuid, _>("subscription_id")?;
            let idempotency_key = format!(
                "erpx:commercial-policy:{customer_id}:{subscription_id}:{}",
                at.timestamp_nanos_opt()
                    .unwrap_or_else(|| at.timestamp_micros() * 1_000)
            );
            let context = CommercialCommandContext::new(
                Uuid::new_v5(&Uuid::NAMESPACE_URL, idempotency_key.as_bytes()),
                idempotency_key.clone(),
                Uuid::new_v5(
                    &Uuid::NAMESPACE_OID,
                    format!("{idempotency_key}:correlation").as_bytes(),
                ),
                actor_ref,
                "advance payment policy",
            )
            .map_err(|_| commercial_invalid("invalid billing policy worker context"))?;
            let receipt = advance_billing_policy_in_transaction(
                &mut transaction,
                BillingPolicyCommand {
                    customer_id,
                    subscription_id,
                    at,
                    context,
                },
            )
            .await?;
            if matches!(
                receipt.result,
                CommercialCommandResult::Billing {
                    change: CommercialCommandChange::Applied,
                    ..
                }
            ) {
                report.advanced = report.advanced.saturating_add(1);
            }
        }
        transaction.commit().await?;
        Ok(report)
    }

    pub async fn allocate_entitlement_company(
        &self,
        command: AllocateCompanyCommand,
    ) -> Result<CommercialCommandReceipt, PersistenceError> {
        self.allocate_entitlement_company_with_expected_version(command, None)
            .await
    }

    pub async fn allocate_entitlement_company_with_expected_version(
        &self,
        command: AllocateCompanyCommand,
        expected_version: Option<u64>,
    ) -> Result<CommercialCommandReceipt, PersistenceError> {
        command
            .validate()
            .map_err(|_| commercial_invalid("invalid company entitlement allocation command"))?;
        let request_hash = commercial_command_request_hash(
            &(
                &command.customer_id,
                &command.module_key,
                &command.company_id,
                &command.seats,
                &command.branch_limit,
            ),
            &command.context,
        )?;
        let mut transaction = self.write.begin().await?;
        match reserve_commercial_idempotency(&mut transaction, &command.context, &request_hash)
            .await?
        {
            CommercialIdempotencyReservation::Replayed(receipt) => {
                transaction.commit().await?;
                return Ok(replayed_receipt(receipt));
            }
            CommercialIdempotencyReservation::Reserved => {}
        }
        lock_commercial_aggregate(
            &mut transaction,
            &format!("entitlement:{}:{}", command.customer_id, command.module_key),
        )
        .await?;
        let (mut book, current_version) = load_entitlement_lifecycle_for_command(
            &mut transaction,
            command.customer_id,
            &command.module_key,
        )
        .await?;
        if expected_version.is_some_and(|expected| expected != current_version) {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: entitlement_aggregate_type(&command.module_key),
                aggregate_id: command.customer_id,
                expected: expected_version,
                actual: Some(current_version),
            });
        }
        book.allocate_company(command.company_id, command.seats, command.branch_limit)
            .map_err(|_| commercial_invalid("company entitlement allocation exceeds capacity"))?;
        let version = persist_entitlement_book(
            &mut transaction,
            command.customer_id,
            &book,
            Some(current_version),
        )
        .await?;
        let result = CommercialCommandResult::CompanyAllocated {
            module_key: command.module_key.clone(),
            company_id: command.company_id,
        };
        append_commercial_state_event(
            &mut transaction,
            &entitlement_aggregate_type(&command.module_key),
            command.customer_id,
            version,
            ENTITLEMENT_COMPANY_ALLOCATED_TYPE,
            serde_json::json!({
                "customer_id": command.customer_id,
                "module_key": command.module_key,
                "company_id": command.company_id,
                "seats": command.seats,
                "branch_limit": command.branch_limit,
                "book": book,
            }),
            Utc::now(),
            &command.context.actor_ref,
            command.context.correlation_id,
            &command.context.idempotency_key,
            Some(command.customer_id),
            &command.context.purpose,
            serde_json::json!({
                "command_id": command.context.command_id,
                "module_key": command.module_key,
                "company_id": command.company_id,
                "seats": command.seats,
                "branch_limit": command.branch_limit,
            }),
        )
        .await?;
        let receipt = CommercialCommandReceipt {
            command_id: command.context.command_id,
            idempotency_key: command.context.idempotency_key.clone(),
            aggregate_type: "entitlement_allocation".into(),
            aggregate_id: command.company_id,
            aggregate_version: version,
            result,
            replayed: false,
        };
        complete_commercial_idempotency(&mut transaction, &command.context, &receipt).await?;
        transaction.commit().await?;
        Ok(receipt)
    }

    pub async fn activate_entitlement_company(
        &self,
        command: ActivateCompanyModuleCommand,
    ) -> Result<CommercialCommandReceipt, PersistenceError> {
        self.activate_entitlement_company_with_expected_version(command, None)
            .await
    }

    pub async fn activate_entitlement_company_with_expected_version(
        &self,
        command: ActivateCompanyModuleCommand,
        expected_version: Option<u64>,
    ) -> Result<CommercialCommandReceipt, PersistenceError> {
        command
            .validate()
            .map_err(|_| commercial_invalid("invalid company module activation command"))?;
        let request_hash = commercial_command_request_hash(
            &(
                &command.customer_id,
                &command.module_key,
                &command.company_id,
                &command.at,
            ),
            &command.context,
        )?;
        let mut transaction = self.write.begin().await?;
        match reserve_commercial_idempotency(&mut transaction, &command.context, &request_hash)
            .await?
        {
            CommercialIdempotencyReservation::Replayed(receipt) => {
                transaction.commit().await?;
                return Ok(replayed_receipt(receipt));
            }
            CommercialIdempotencyReservation::Reserved => {}
        }
        lock_commercial_aggregate(
            &mut transaction,
            &format!("entitlement:{}:{}", command.customer_id, command.module_key),
        )
        .await?;
        let (mut book, current_version) = load_entitlement_lifecycle_for_command(
            &mut transaction,
            command.customer_id,
            &command.module_key,
        )
        .await?;
        if expected_version.is_some_and(|expected| expected != current_version) {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: entitlement_aggregate_type(&command.module_key),
                aggregate_id: command.customer_id,
                expected: expected_version,
                actual: Some(current_version),
            });
        }
        book.activate_company_at(command.company_id, command.at)
            .map_err(|_| commercial_invalid("company module activation is not permitted"))?;
        let version = persist_entitlement_book(
            &mut transaction,
            command.customer_id,
            &book,
            Some(current_version),
        )
        .await?;
        let result = CommercialCommandResult::CompanyModuleActivated {
            module_key: command.module_key.clone(),
            company_id: command.company_id,
        };
        append_commercial_state_event(
            &mut transaction,
            &entitlement_aggregate_type(&command.module_key),
            command.customer_id,
            version,
            ENTITLEMENT_COMPANY_ACTIVATED_TYPE,
            serde_json::json!({
                "customer_id": command.customer_id,
                "module_key": command.module_key,
                "company_id": command.company_id,
                "at": command.at,
                "book": book,
                "human_decision": command.context.human_decision.as_ref(),
            }),
            command.at,
            &command.context.actor_ref,
            command.context.correlation_id,
            &command.context.idempotency_key,
            Some(command.customer_id),
            &command.context.purpose,
            serde_json::json!({
                "command_id": command.context.command_id,
                "module_key": command.module_key,
                "company_id": command.company_id,
                "at": command.at,
                "human_decision": command.context.human_decision.as_ref(),
            }),
        )
        .await?;
        let receipt = CommercialCommandReceipt {
            command_id: command.context.command_id,
            idempotency_key: command.context.idempotency_key.clone(),
            aggregate_type: "entitlement_allocation".into(),
            aggregate_id: command.company_id,
            aggregate_version: version,
            result,
            replayed: false,
        };
        complete_commercial_idempotency(&mut transaction, &command.context, &receipt).await?;
        transaction.commit().await?;
        Ok(receipt)
    }

    /// Authenticated-human ingress. Proof validation happens before opening a
    /// transaction or consulting idempotency receipts, and the redacted proof
    /// material is retained in audit/outbox JSON through the command context.
    pub async fn activate_entitlement_company_with_human_decision(
        &self,
        mut command: ActivateCompanyModuleCommand,
        expected_version: Option<u64>,
        decision: &HumanDecisionProof,
    ) -> Result<CommercialCommandReceipt, PersistenceError> {
        validate_company_entitlement_activation_decision(
            &command,
            expected_version.unwrap_or(0),
            decision,
        )
        .map_err(|_| commercial_invalid("company activation human decision is invalid"))?;
        command.context.human_decision = Some(
            CommercialHumanDecisionEvidence::from_proof(decision)
                .map_err(|_| commercial_invalid("company activation human decision is invalid"))?,
        );
        self.activate_entitlement_company_with_expected_version(command, expected_version)
            .await
    }

    pub async fn activate_entitlement_branch(
        &self,
        command: ActivateBranchModuleCommand,
    ) -> Result<CommercialCommandReceipt, PersistenceError> {
        self.activate_entitlement_branch_with_expected_version(command, None)
            .await
    }

    pub async fn activate_entitlement_branch_with_expected_version(
        &self,
        command: ActivateBranchModuleCommand,
        expected_version: Option<u64>,
    ) -> Result<CommercialCommandReceipt, PersistenceError> {
        command
            .validate()
            .map_err(|_| commercial_invalid("invalid branch module activation command"))?;
        let request_hash = commercial_command_request_hash(
            &(
                &command.customer_id,
                &command.module_key,
                &command.company_id,
                &command.branch_id,
                &command.at,
            ),
            &command.context,
        )?;
        let mut transaction = self.write.begin().await?;
        match reserve_commercial_idempotency(&mut transaction, &command.context, &request_hash)
            .await?
        {
            CommercialIdempotencyReservation::Replayed(receipt) => {
                transaction.commit().await?;
                return Ok(replayed_receipt(receipt));
            }
            CommercialIdempotencyReservation::Reserved => {}
        }
        lock_commercial_aggregate(
            &mut transaction,
            &format!("entitlement:{}:{}", command.customer_id, command.module_key),
        )
        .await?;
        let (mut book, current_version) = load_entitlement_lifecycle_for_command(
            &mut transaction,
            command.customer_id,
            &command.module_key,
        )
        .await?;
        if expected_version.is_some_and(|expected| expected != current_version) {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: entitlement_aggregate_type(&command.module_key),
                aggregate_id: command.customer_id,
                expected: expected_version,
                actual: Some(current_version),
            });
        }
        book.activate_branch_at(command.company_id, command.branch_id, command.at)
            .map_err(|_| commercial_invalid("branch module activation is not permitted"))?;
        let version = persist_entitlement_book(
            &mut transaction,
            command.customer_id,
            &book,
            Some(current_version),
        )
        .await?;
        let result = CommercialCommandResult::BranchModuleActivated {
            module_key: command.module_key.clone(),
            company_id: command.company_id,
            branch_id: command.branch_id,
        };
        append_commercial_state_event(
            &mut transaction,
            &entitlement_aggregate_type(&command.module_key),
            command.customer_id,
            version,
            ENTITLEMENT_BRANCH_ACTIVATED_TYPE,
            serde_json::json!({
                "customer_id": command.customer_id,
                "module_key": command.module_key,
                "company_id": command.company_id,
                "branch_id": command.branch_id,
                "at": command.at,
                "book": book,
                "human_decision": command.context.human_decision.as_ref(),
            }),
            command.at,
            &command.context.actor_ref,
            command.context.correlation_id,
            &command.context.idempotency_key,
            Some(command.customer_id),
            &command.context.purpose,
            serde_json::json!({
                "command_id": command.context.command_id,
                "module_key": command.module_key,
                "company_id": command.company_id,
                "branch_id": command.branch_id,
                "at": command.at,
                "human_decision": command.context.human_decision.as_ref(),
            }),
        )
        .await?;
        let receipt = CommercialCommandReceipt {
            command_id: command.context.command_id,
            idempotency_key: command.context.idempotency_key.clone(),
            aggregate_type: "entitlement_allocation".into(),
            aggregate_id: command.branch_id,
            aggregate_version: version,
            result,
            replayed: false,
        };
        complete_commercial_idempotency(&mut transaction, &command.context, &receipt).await?;
        transaction.commit().await?;
        Ok(receipt)
    }

    /// Branch counterpart to `activate_entitlement_company_with_human_decision`.
    pub async fn activate_entitlement_branch_with_human_decision(
        &self,
        mut command: ActivateBranchModuleCommand,
        expected_version: Option<u64>,
        decision: &HumanDecisionProof,
    ) -> Result<CommercialCommandReceipt, PersistenceError> {
        validate_branch_entitlement_activation_decision(
            &command,
            expected_version.unwrap_or(0),
            decision,
        )
        .map_err(|_| commercial_invalid("branch activation human decision is invalid"))?;
        command.context.human_decision = Some(
            CommercialHumanDecisionEvidence::from_proof(decision)
                .map_err(|_| commercial_invalid("branch activation human decision is invalid"))?,
        );
        self.activate_entitlement_branch_with_expected_version(command, expected_version)
            .await
    }

    pub async fn insert_stripe_event(
        &self,
        event: &StripeInboxEvent,
    ) -> Result<StripeInboxInsert, PersistenceError> {
        self.insert_stripe_event_with_context(event, None).await
    }

    /// Persist the authenticated raw-body digest and request correlation
    /// without exposing the raw body to workers. The optional context keeps
    /// older internal callers wire-compatible while HTTP webhook ingestion
    /// supplies the correlation identifier.
    pub async fn insert_stripe_event_with_context(
        &self,
        event: &StripeInboxEvent,
        correlation_id: Option<Uuid>,
    ) -> Result<StripeInboxInsert, PersistenceError> {
        if event.event_id.trim().is_empty() || event.event_type.trim().is_empty() {
            return Err(PersistenceError::InvalidData(
                "Stripe event ID and type are required".into(),
            ));
        }
        if event.raw_body.is_empty() {
            return Err(PersistenceError::InvalidData(
                "Stripe raw body cannot be empty".into(),
            ));
        }
        let raw_body_sha256: [u8; 32] = correlation_id
            .map(|correlation_id| {
                StripeWebhookPersistenceContext::from_raw_body(&event.raw_body, correlation_id)
                    .map(|context| *context.raw_body_sha256())
                    .map_err(|error| PersistenceError::InvalidData(error.to_string()))
            })
            .transpose()?
            .unwrap_or_else(|| Sha256::digest(&event.raw_body).into());
        self.insert_stripe_event_with_metadata(event, &raw_body_sha256, correlation_id)
            .await
    }

    /// Strict authenticated-webhook insertion contract. The caller must bind
    /// the context to the exact raw body before persistence; a context from a
    /// different request body is rejected even if its correlation ID is valid.
    pub async fn insert_stripe_webhook(
        &self,
        event: &StripeInboxEvent,
        context: &StripeWebhookPersistenceContext,
    ) -> Result<StripeInboxInsert, PersistenceError> {
        let expected = StripeWebhookPersistenceContext::from_raw_body(
            &event.raw_body,
            context.correlation_id(),
        )
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        if expected != *context {
            return Err(PersistenceError::InvalidData(
                "Stripe raw-body persistence context does not match event".into(),
            ));
        }
        self.insert_stripe_event_with_metadata(
            event,
            expected.raw_body_sha256(),
            Some(expected.correlation_id()),
        )
        .await
    }

    pub async fn insert_verified_stripe_webhook(
        &self,
        webhook: StripeWebhookEvent,
    ) -> Result<StripeInboxInsert, PersistenceError> {
        let context = webhook.persistence_context().clone();
        self.insert_stripe_webhook(&webhook.into_inbox_event(), &context)
            .await
    }

    async fn insert_stripe_event_with_metadata(
        &self,
        event: &StripeInboxEvent,
        raw_body_sha256: &[u8; 32],
        correlation_id: Option<Uuid>,
    ) -> Result<StripeInboxInsert, PersistenceError> {
        if event.event_id.trim().is_empty() || event.event_type.trim().is_empty() {
            return Err(PersistenceError::InvalidData(
                "Stripe event ID and type are required".into(),
            ));
        }
        if event.raw_body.is_empty() {
            return Err(PersistenceError::InvalidData(
                "Stripe raw body cannot be empty".into(),
            ));
        }
        let result = sqlx::query(
            "INSERT INTO erpx_control.stripe_event_inbox
                (event_id, event_type, stripe_created_at, payload, raw_body,
                 raw_body_sha256, correlation_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (event_id) DO NOTHING",
        )
        .bind(&event.event_id)
        .bind(&event.event_type)
        .bind(event.stripe_created_at)
        .bind(&event.payload)
        .bind(&event.raw_body)
        .bind(raw_body_sha256.as_slice())
        .bind(correlation_id)
        .execute(&self.write)
        .await?;
        if result.rows_affected() == 1 {
            return Ok(StripeInboxInsert::Inserted);
        }

        // Stripe event IDs are the replay identity, but they are not enough
        // on their own: accepting a different body under an existing ID
        // would turn a signed webhook into an ambiguous inbox record. Treat
        // only an exact event/type/payload/body replay as idempotent.
        let existing = sqlx::query(
            "SELECT event_type, payload, raw_body, raw_body_sha256
             FROM erpx_control.stripe_event_inbox
             WHERE event_id = $1",
        )
        .bind(&event.event_id)
        .fetch_optional(&self.write)
        .await?
        .ok_or_else(|| {
            PersistenceError::InvalidData("Stripe inbox duplicate disappeared".into())
        })?;
        let existing_event_type: String = existing.try_get("event_type")?;
        let existing_payload: Value = existing.try_get("payload")?;
        let existing_raw_body: Vec<u8> = existing.try_get("raw_body")?;
        let existing_digest: Option<Vec<u8>> = existing.try_get("raw_body_sha256")?;
        let body_matches = existing_digest
            .as_deref()
            .is_some_and(|digest| digest == raw_body_sha256.as_slice())
            || (existing_digest.is_none() && existing_raw_body == event.raw_body);
        if existing_event_type != event.event_type
            || existing_payload != event.payload
            || !body_matches
        {
            return Err(PersistenceError::IdempotencyConflict {
                key: event.event_id.clone(),
            });
        }
        Ok(StripeInboxInsert::Duplicate)
    }

    /// Claims ready events with PostgreSQL row locking. The returned claim is
    /// deliberately parsed metadata only; the authenticated raw request body
    /// is never selected, returned, or exposed to a worker.
    pub async fn claim_stripe_events(
        &self,
        worker_id: Uuid,
        limit: u32,
        now: DateTime<Utc>,
        claim_timeout: Duration,
        retry_policy: StripeInboxRetryPolicy,
    ) -> Result<Vec<StripeInboxClaim>, PersistenceError> {
        if limit == 0 || claim_timeout <= Duration::zero() {
            return Err(PersistenceError::InvalidData(
                "Stripe inbox claim limit and timeout must be positive".into(),
            ));
        }
        let claim_expires_at = now
            .checked_add_signed(claim_timeout)
            .ok_or_else(|| PersistenceError::InvalidData("Stripe claim timeout overflow".into()))?;
        let max_attempts = i32::try_from(retry_policy.max_attempts).map_err(|_| {
            PersistenceError::InvalidData("Stripe inbox max attempts exceeds database range".into())
        })?;
        let mut transaction = self.write.begin().await?;

        // A worker that dies after acquiring the final permitted attempt must
        // not strand the event in `processing` forever.
        sqlx::query(
            "UPDATE erpx_control.stripe_event_inbox
             SET status = 'dead_letter', dead_lettered_at = $1,
                 claimed_by = NULL, claimed_at = NULL, claim_expires_at = NULL,
                 last_error = COALESCE(last_error, 'claim lease expired at attempt limit')
             WHERE status = 'processing'
               AND claim_expires_at <= $1
               AND attempts >= $2",
        )
        .bind(now)
        .bind(max_attempts)
        .execute(&mut *transaction)
        .await?;

        let rows = sqlx::query(
            "WITH candidates AS (
                SELECT event_id
                FROM erpx_control.stripe_event_inbox
                WHERE attempts < $4
                  AND (
                    (status IN ('received', 'retryable') AND available_at <= $1)
                    OR (status = 'processing' AND claim_expires_at <= $1)
                  )
                ORDER BY available_at, received_at, event_id
                FOR UPDATE SKIP LOCKED
                LIMIT $2
             )
             UPDATE erpx_control.stripe_event_inbox e
             SET status = 'processing', claimed_by = $3, claimed_at = $1,
                 claim_expires_at = $5, attempts = e.attempts + 1
             FROM candidates c
             WHERE e.event_id = c.event_id
             RETURNING e.event_id, e.event_type, e.stripe_created_at, e.payload,
                       e.attempts, e.claimed_by, e.claim_expires_at",
        )
        .bind(now)
        .bind(i64::from(limit))
        .bind(worker_id)
        .bind(max_attempts)
        .bind(claim_expires_at)
        .fetch_all(&mut *transaction)
        .await?;
        transaction.commit().await?;
        rows.into_iter().map(stripe_claim_from_row).collect()
    }

    pub async fn get_stripe_event_state(
        &self,
        event_id: &str,
    ) -> Result<Option<StripeInboxState>, PersistenceError> {
        let row = sqlx::query(
            "SELECT event_id, status, attempts, replay_count, available_at, claimed_by,
                    claim_expires_at, processed_at, dead_lettered_at
             FROM erpx_control.stripe_event_inbox
             WHERE event_id = $1",
        )
        .bind(event_id)
        .fetch_optional(&self.write)
        .await?;
        row.map(stripe_state_from_row).transpose()
    }

    pub async fn mark_stripe_event_processed(
        &self,
        event_id: &str,
        worker_id: Uuid,
        at: DateTime<Utc>,
    ) -> Result<StripeInboxTransition, PersistenceError> {
        let row = sqlx::query(
            "UPDATE erpx_control.stripe_event_inbox
             SET status = 'processed', processed_at = $3,
                 claimed_by = NULL, claimed_at = NULL, claim_expires_at = NULL,
                 last_error = NULL
             WHERE event_id = $1 AND status = 'processing' AND claimed_by = $2
               AND claim_expires_at > $3
             RETURNING attempts",
        )
        .bind(event_id)
        .bind(worker_id)
        .bind(at)
        .fetch_optional(&self.write)
        .await?
        .ok_or_else(|| stripe_claim_lost(event_id, worker_id))?;
        Ok(StripeInboxTransition::Processed {
            attempts: attempts_from_row(&row)?,
            processed_at: at,
        })
    }

    pub async fn retry_stripe_event(
        &self,
        event_id: &str,
        worker_id: Uuid,
        error: &str,
        at: DateTime<Utc>,
        retry_policy: StripeInboxRetryPolicy,
    ) -> Result<StripeInboxTransition, PersistenceError> {
        let mut transaction = self.write.begin().await?;
        let row = sqlx::query(
            "SELECT attempts
             FROM erpx_control.stripe_event_inbox
             WHERE event_id = $1 AND status = 'processing' AND claimed_by = $2
               AND claim_expires_at > $3
             FOR UPDATE",
        )
        .bind(event_id)
        .bind(worker_id)
        .bind(at)
        .fetch_optional(&mut *transaction)
        .await?
        .ok_or_else(|| stripe_claim_lost(event_id, worker_id))?;
        let attempts = attempts_from_row(&row)?;
        let error = bounded_inbox_error(error);
        let transition = if attempts >= retry_policy.max_attempts {
            sqlx::query(
                "UPDATE erpx_control.stripe_event_inbox
                 SET status = 'dead_letter', dead_lettered_at = $3,
                     claimed_by = NULL, claimed_at = NULL, claim_expires_at = NULL,
                     last_error = $4
                 WHERE event_id = $1 AND status = 'processing' AND claimed_by = $2",
            )
            .bind(event_id)
            .bind(worker_id)
            .bind(at)
            .bind(error)
            .execute(&mut *transaction)
            .await?;
            StripeInboxTransition::DeadLettered {
                attempts,
                dead_lettered_at: at,
            }
        } else {
            let available_at = at
                .checked_add_signed(retry_policy.backoff_for_attempt(attempts))
                .ok_or_else(|| {
                    PersistenceError::InvalidData("Stripe retry backoff overflow".into())
                })?;
            sqlx::query(
                "UPDATE erpx_control.stripe_event_inbox
                 SET status = 'retryable', available_at = $3,
                     claimed_by = NULL, claimed_at = NULL, claim_expires_at = NULL,
                     last_error = $4
                 WHERE event_id = $1 AND status = 'processing' AND claimed_by = $2",
            )
            .bind(event_id)
            .bind(worker_id)
            .bind(available_at)
            .bind(error)
            .execute(&mut *transaction)
            .await?;
            StripeInboxTransition::RetryScheduled {
                attempts,
                available_at,
            }
        };
        transaction.commit().await?;
        Ok(transition)
    }

    pub async fn dead_letter_stripe_event(
        &self,
        event_id: &str,
        worker_id: Uuid,
        error: &str,
        at: DateTime<Utc>,
    ) -> Result<StripeInboxTransition, PersistenceError> {
        let row = sqlx::query(
            "UPDATE erpx_control.stripe_event_inbox
             SET status = 'dead_letter', dead_lettered_at = $3,
                 claimed_by = NULL, claimed_at = NULL, claim_expires_at = NULL,
                 last_error = $4
             WHERE event_id = $1 AND status = 'processing' AND claimed_by = $2
               AND claim_expires_at > $3
             RETURNING attempts",
        )
        .bind(event_id)
        .bind(worker_id)
        .bind(at)
        .bind(bounded_inbox_error(error))
        .fetch_optional(&self.write)
        .await?
        .ok_or_else(|| stripe_claim_lost(event_id, worker_id))?;
        Ok(StripeInboxTransition::DeadLettered {
            attempts: attempts_from_row(&row)?,
            dead_lettered_at: at,
        })
    }

    /// Replay a dead-lettered Stripe event only with an authenticated actor,
    /// purpose, and correlation binding. The state transition and audit row
    /// commit atomically, so a failed replay leaves the DLQ record untouched.
    pub async fn replay_stripe_dead_letter_authenticated(
        &self,
        event_id: &str,
        available_at: DateTime<Utc>,
        replay_policy: StripeInboxReplayPolicy,
        authorization: &StripeReplayAuthorizationContext,
    ) -> Result<(), PersistenceError> {
        self.replay_stripe_dead_letter_internal(
            event_id,
            available_at,
            replay_policy,
            Some(authorization),
        )
        .await
    }

    async fn replay_stripe_dead_letter_internal(
        &self,
        event_id: &str,
        available_at: DateTime<Utc>,
        replay_policy: StripeInboxReplayPolicy,
        authorization: Option<&StripeReplayAuthorizationContext>,
    ) -> Result<(), PersistenceError> {
        let max_replays = i32::try_from(replay_policy.max_replays).map_err(|_| {
            PersistenceError::InvalidData("Stripe inbox replay bound exceeds database range".into())
        })?;
        let mut transaction = self.write.begin().await?;
        let result = sqlx::query(
            "UPDATE erpx_control.stripe_event_inbox
             SET status = 'retryable', attempts = 0, available_at = $2,
                 claimed_by = NULL, claimed_at = NULL, claim_expires_at = NULL,
                 dead_lettered_at = NULL, processed_at = NULL, last_error = NULL,
                 replay_count = replay_count + 1
             WHERE event_id = $1 AND status = 'dead_letter'
               AND replay_count < $3",
        )
        .bind(event_id)
        .bind(available_at)
        .bind(max_replays)
        .execute(&mut *transaction)
        .await?;
        if result.rows_affected() == 0 {
            let state = sqlx::query(
                "SELECT status, replay_count
                 FROM erpx_control.stripe_event_inbox
                 WHERE event_id = $1",
            )
            .bind(event_id)
            .fetch_optional(&mut *transaction)
            .await?;
            if let Some(state) = state {
                let status: String = state.try_get("status")?;
                let replay_count: i32 = state.try_get("replay_count")?;
                if status == "dead_letter" && replay_count >= max_replays {
                    return Err(PersistenceError::InvalidData(
                        "Stripe event replay limit has been reached".into(),
                    ));
                }
            }
            let _ = transaction.rollback().await;
            return Err(PersistenceError::InvalidData(
                "Stripe event is not dead-lettered or does not exist".into(),
            ));
        }
        if let Some(authorization) = authorization {
            sqlx::query(
                "INSERT INTO erpx_control.audit_events
                    (id, actor_ref, customer_account_id, action, purpose, correlation_id, payload)
                 VALUES ($1, $2, NULL, $3, $4, $5, $6)",
            )
            .bind(Uuid::new_v4())
            .bind(authorization.actor_ref())
            .bind("commercial.stripe.dead_letter_replayed")
            .bind(authorization.purpose())
            .bind(authorization.correlation_id())
            .bind(serde_json::json!({
                "event_id": event_id,
                "available_at": available_at,
                "replay_limit": replay_policy.max_replays,
            }))
            .execute(&mut *transaction)
            .await?;
        }
        transaction.commit().await?;
        Ok(())
    }

    /// Read only the commercial section from the rebuildable control read
    /// model after checking the authenticated context against authoritative
    /// customer scope bindings on the write database. The read payload never
    /// becomes an authorization substitute and malformed cross-customer
    /// projections fail closed.
    pub async fn read_authenticated_commercial(
        &self,
        context: &Customer360DetailReadContext,
    ) -> Result<CustomerReadView<CustomerCommercialReadDto>, PersistenceError> {
        context.validate().map_err(|_| {
            PersistenceError::InvalidData("commercial read context is invalid".into())
        })?;
        if !self
            .control
            .customer_scope_contains(context.customer_id, &context.scope)
            .await?
        {
            return Err(PersistenceError::InvalidData(
                "commercial read scope is not authorized".into(),
            ));
        }
        let row = sqlx::query(
            "SELECT id, payload, updated_at
             FROM erpx_control.customers
             WHERE id = $1",
        )
        .bind(context.customer_id)
        .fetch_optional(&self.read)
        .await?;
        let Some(row) = row else {
            return Ok(CustomerReadView::absent(context.customer_id));
        };
        let row_id: Uuid = row.try_get("id")?;
        let payload: Value = row.try_get("payload")?;
        let projection = match serde_json::from_value::<Customer360Projection>(payload.clone()) {
            Ok(projection) => projection,
            Err(_) => {
                let legacy: Customer360 = serde_json::from_value(payload).map_err(|_| {
                    PersistenceError::InvalidData("commercial read projection is invalid".into())
                })?;
                Customer360Projection::from_legacy(&legacy)
            }
        };
        if row_id != context.customer_id || projection.account.id != context.customer_id {
            return Err(PersistenceError::InvalidData(
                "commercial read projection identity is invalid".into(),
            ));
        }
        let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
        Ok(CustomerReadView::present(
            CustomerCommercialReadDto::from(&projection),
            context.customer_id,
            updated_at,
            Utc::now(),
        ))
    }
}

#[derive(Debug)]
enum CommercialIdempotencyReservation {
    Reserved,
    Replayed(CommercialCommandReceipt),
}

fn commercial_invalid(message: &'static str) -> PersistenceError {
    // Keep command errors deliberately opaque: idempotency keys, provider
    // payloads, and database diagnostics must not be reflected in an error
    // that can cross an application or worker boundary.
    PersistenceError::InvalidData(message.into())
}

fn commercial_request_hash<T: serde::Serialize>(value: &T) -> Result<String, PersistenceError> {
    let bytes = serde_json::to_vec(value)?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

/// Hash the durable meaning of a commercial command, not per-attempt
/// transport metadata. A caller may retry with a new command/correlation ID,
/// while the idempotency key remains bound to the same payload and
/// authorization purpose. Provider commands rely on this because their
/// contexts are deliberately regenerated for each inbox delivery.
fn commercial_command_request_hash<T: serde::Serialize>(
    payload: &T,
    context: &CommercialCommandContext,
) -> Result<String, PersistenceError> {
    commercial_request_hash(&(
        payload,
        serde_json::json!({
            "actor_ref": context.actor_ref,
            "purpose": context.purpose,
        }),
    ))
}

fn replayed_receipt(mut receipt: CommercialCommandReceipt) -> CommercialCommandReceipt {
    receipt.replayed = true;
    receipt
}

async fn lock_commercial_aggregate(
    transaction: &mut Transaction<'_, Postgres>,
    lock_key: &str,
) -> Result<(), PersistenceError> {
    // The row may not exist yet. A transaction-scoped advisory lock closes
    // the first-write race without adding a schema object or relying on a
    // client-side mutex.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1)::bigint)")
        .bind(lock_key)
        .execute(&mut **transaction)
        .await?;
    Ok(())
}

async fn reserve_commercial_idempotency(
    transaction: &mut Transaction<'_, Postgres>,
    context: &CommercialCommandContext,
    request_hash: &str,
) -> Result<CommercialIdempotencyReservation, PersistenceError> {
    let inserted = sqlx::query(
        "INSERT INTO erpx_control.idempotency_keys
            (idempotency_key, request_hash, command_id, status)
         VALUES ($1, $2, $3, 'in_progress')
         ON CONFLICT (idempotency_key) DO NOTHING",
    )
    .bind(&context.idempotency_key)
    .bind(request_hash)
    .bind(context.command_id)
    .execute(&mut **transaction)
    .await?
    .rows_affected();
    if inserted == 1 {
        return Ok(CommercialIdempotencyReservation::Reserved);
    }

    let row = sqlx::query(
        "SELECT request_hash, status, response
         FROM erpx_control.idempotency_keys
         WHERE idempotency_key = $1
         FOR UPDATE",
    )
    .bind(&context.idempotency_key)
    .fetch_optional(&mut **transaction)
    .await?
    .ok_or_else(|| commercial_invalid("commercial idempotency record is unavailable"))?;
    let stored_hash: String = row.try_get("request_hash")?;
    if stored_hash != request_hash {
        return Err(commercial_invalid(
            "commercial idempotency request hash mismatch",
        ));
    }
    let status: String = row.try_get("status")?;
    match status.as_str() {
        "completed" => {
            let response: Option<Value> = row.try_get("response")?;
            let receipt = response
                .ok_or_else(|| commercial_invalid("commercial receipt is missing"))
                .and_then(|response| {
                    serde_json::from_value(response)
                        .map_err(|_| commercial_invalid("commercial receipt is invalid"))
                })?;
            Ok(CommercialIdempotencyReservation::Replayed(receipt))
        }
        "failed" => {
            let reset = sqlx::query(
                "UPDATE erpx_control.idempotency_keys
                 SET status = 'in_progress', command_id = $2,
                     response = NULL, completed_at = NULL
                 WHERE idempotency_key = $1 AND status = 'failed'",
            )
            .bind(&context.idempotency_key)
            .bind(context.command_id)
            .execute(&mut **transaction)
            .await?
            .rows_affected();
            if reset != 1 {
                return Err(commercial_invalid("commercial idempotency state changed"));
            }
            Ok(CommercialIdempotencyReservation::Reserved)
        }
        "in_progress" => Err(commercial_invalid(
            "commercial command is already in progress",
        )),
        _ => Err(commercial_invalid(
            "commercial idempotency state is invalid",
        )),
    }
}

async fn complete_commercial_idempotency(
    transaction: &mut Transaction<'_, Postgres>,
    context: &CommercialCommandContext,
    receipt: &CommercialCommandReceipt,
) -> Result<(), PersistenceError> {
    let response = serde_json::to_value(receipt)?;
    let aggregate_version = i64::try_from(receipt.aggregate_version)
        .map_err(|_| commercial_invalid("commercial aggregate version exceeds database range"))?;
    let updated = sqlx::query(
        "UPDATE erpx_control.idempotency_keys
         SET status = 'completed', response = $2, completed_at = now(),
             aggregate_type = $3, aggregate_id = $4, aggregate_version = $5
         WHERE idempotency_key = $1 AND status = 'in_progress'",
    )
    .bind(&context.idempotency_key)
    .bind(response)
    .bind(&receipt.aggregate_type)
    .bind(receipt.aggregate_id)
    .bind(aggregate_version)
    .execute(&mut **transaction)
    .await?
    .rows_affected();
    if updated != 1 {
        return Err(commercial_invalid(
            "commercial idempotency completion was not reserved",
        ));
    }
    Ok(())
}

async fn append_commercial_audit(
    transaction: &mut Transaction<'_, Postgres>,
    customer_id: Uuid,
    context: &CommercialCommandContext,
    action: &str,
    payload: Value,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_control.audit_events
            (id, actor_ref, customer_account_id, action, purpose, correlation_id, payload)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(&context.actor_ref)
    .bind(customer_id)
    .bind(action)
    .bind(&context.purpose)
    .bind(context.correlation_id)
    .bind(payload)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

/// Keep low-level commercial bootstrap writes on the same rebuild boundary as
/// provider and worker commands. The normalized owner row is the sole version
/// authority; this helper appends only the rebuild fact and audit evidence.
#[allow(clippy::too_many_arguments)]
async fn append_commercial_state_event(
    transaction: &mut Transaction<'_, Postgres>,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    event_type: &str,
    payload: Value,
    occurred_at: DateTime<Utc>,
    actor_ref: &str,
    correlation_id: Uuid,
    idempotency_key: &str,
    customer_id: Option<Uuid>,
    audit_purpose: &str,
    audit_payload: Value,
) -> Result<(), PersistenceError> {
    if aggregate_id.is_nil()
        || aggregate_version == 0
        || actor_ref.trim().is_empty()
        || correlation_id.is_nil()
        || idempotency_key.trim().is_empty()
    {
        return Err(commercial_invalid(
            "commercial state event metadata is invalid",
        ));
    }
    let version = i64::try_from(aggregate_version)
        .map_err(|_| commercial_invalid("commercial aggregate version exceeds database range"))?;
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, active_scope, actor, correlation_id,
             idempotency_key, payload, occurred_at, available_at)
         VALUES ($1, $2, 1, $3, $4, $5, NULL, $6, $7, $8, $9, $10, $10)",
    )
    .bind(Uuid::new_v4())
    .bind(event_type)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(version)
    .bind(serde_json::json!({ "actor_ref": actor_ref }))
    .bind(correlation_id)
    .bind(idempotency_key)
    .bind(payload)
    .bind(occurred_at)
    .execute(&mut **transaction)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_control.audit_events
            (id, actor_ref, customer_account_id, action, purpose,
             correlation_id, payload)
         VALUES ($1, $2, $3, $4, $5, $6, $7)",
    )
    .bind(Uuid::new_v4())
    .bind(actor_ref)
    .bind(customer_id)
    .bind(event_type)
    .bind(audit_purpose)
    .bind(correlation_id)
    .bind(audit_payload)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn load_billing_lifecycle(
    transaction: &mut Transaction<'_, Postgres>,
    customer_id: Uuid,
    subscription_id: Uuid,
) -> Result<Option<(BillingLifecycle, u64)>, PersistenceError> {
    let row = sqlx::query(
        "SELECT payload, version
         FROM erpx_control.billing_lifecycles
         WHERE customer_id = $1 AND subscription_id = $2
         FOR UPDATE",
    )
    .bind(customer_id)
    .bind(subscription_id)
    .fetch_optional(&mut **transaction)
    .await?;
    row.map(|row| {
        let lifecycle: BillingLifecycle = serde_json::from_value(row.try_get("payload")?)?;
        let version = u64::try_from(row.try_get::<i64, _>("version")?)
            .map_err(|_| PersistenceError::InvalidData("negative billing version".into()))?;
        Ok((lifecycle, version))
    })
    .transpose()
}

async fn advance_billing_policy_in_transaction(
    transaction: &mut Transaction<'_, Postgres>,
    command: BillingPolicyCommand,
) -> Result<CommercialCommandReceipt, PersistenceError> {
    command
        .validate()
        .map_err(|_| commercial_invalid("invalid billing policy command"))?;
    let request_hash = commercial_command_request_hash(
        &(&command.customer_id, &command.subscription_id, &command.at),
        &command.context,
    )?;
    match reserve_commercial_idempotency(transaction, &command.context, &request_hash).await? {
        CommercialIdempotencyReservation::Replayed(receipt) => {
            return Ok(replayed_receipt(receipt));
        }
        CommercialIdempotencyReservation::Reserved => {}
    }
    lock_commercial_aggregate(
        transaction,
        &format!(
            "billing:{}:{}",
            command.customer_id, command.subscription_id
        ),
    )
    .await?;
    let (mut lifecycle, expected_version) =
        load_billing_lifecycle(transaction, command.customer_id, command.subscription_id)
            .await?
            .ok_or(PersistenceError::NotFound {
                entity: "billing lifecycle",
                id: command.subscription_id,
            })?;
    let policy_change = lifecycle
        .advance_payment_policy(command.at)
        .map_err(|_| commercial_invalid("billing policy cannot advance lifecycle"))?;
    let change = match policy_change {
        erpx_domain::commercial::BillingPolicyChange::Unchanged => {
            CommercialCommandChange::Unchanged
        }
        erpx_domain::commercial::BillingPolicyChange::GraceStarted
        | erpx_domain::commercial::BillingPolicyChange::ReadOnlyEntered => {
            CommercialCommandChange::Applied
        }
    };
    let version = if change == CommercialCommandChange::Unchanged {
        expected_version
    } else {
        let version = persist_billing_lifecycle(
            transaction,
            command.customer_id,
            command.subscription_id,
            &lifecycle,
            Some(expected_version),
        )
        .await?;
        persist_billing_policy_evidence(transaction, &command, &lifecycle, version, change).await?;
        version
    };
    let result = CommercialCommandResult::Billing {
        state: lifecycle.state,
        change,
    };
    append_commercial_audit(
        transaction,
        command.customer_id,
        &command.context,
        "commercial.billing.policy_advanced",
        serde_json::json!({
            "command_id": command.context.command_id,
            "subscription_id": command.subscription_id,
            "at": command.at,
            "state": billing_state(lifecycle.state),
            "change": change,
        }),
    )
    .await?;
    let receipt = CommercialCommandReceipt {
        command_id: command.context.command_id,
        idempotency_key: command.context.idempotency_key.clone(),
        aggregate_type: "billing_lifecycle".into(),
        aggregate_id: command.subscription_id,
        aggregate_version: version,
        result,
        replayed: false,
    };
    complete_commercial_idempotency(transaction, &command.context, &receipt).await?;
    Ok(receipt)
}

async fn persist_billing_lifecycle(
    transaction: &mut Transaction<'_, Postgres>,
    customer_id: Uuid,
    subscription_id: Uuid,
    lifecycle: &BillingLifecycle,
    expected_version: Option<u64>,
) -> Result<u64, PersistenceError> {
    let next_version = expected_version
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| commercial_invalid("commercial billing version overflow"))?;
    let version = i64::try_from(next_version)
        .map_err(|_| commercial_invalid("commercial billing version exceeds database range"))?;
    let payload = serde_json::to_value(lifecycle)?;
    let rows =
        match expected_version {
            Some(expected) => sqlx::query(
                "UPDATE erpx_control.billing_lifecycles
             SET state = $3, grace_started_at = $4, grace_until = $5,
                 last_transition_at = $6, version = $7, payload = $8, updated_at = now()
             WHERE customer_id = $1 AND subscription_id = $2 AND version = $9",
            )
            .bind(customer_id)
            .bind(subscription_id)
            .bind(billing_state(lifecycle.state))
            .bind(lifecycle.grace_started_at)
            .bind(lifecycle.grace_until)
            .bind(lifecycle.last_transition_at)
            .bind(version)
            .bind(payload)
            .bind(i64::try_from(expected).map_err(|_| {
                commercial_invalid("commercial billing version exceeds database range")
            })?)
            .execute(&mut **transaction)
            .await?
            .rows_affected(),
            None => sqlx::query(
                "INSERT INTO erpx_control.billing_lifecycles
                (customer_id, subscription_id, state, grace_started_at, grace_until,
                 last_transition_at, version, payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (customer_id, subscription_id) DO NOTHING",
            )
            .bind(customer_id)
            .bind(subscription_id)
            .bind(billing_state(lifecycle.state))
            .bind(lifecycle.grace_started_at)
            .bind(lifecycle.grace_until)
            .bind(lifecycle.last_transition_at)
            .bind(version)
            .bind(payload)
            .execute(&mut **transaction)
            .await?
            .rows_affected(),
        };
    if rows != 1 {
        return Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: "billing_lifecycle".into(),
            aggregate_id: subscription_id,
            expected: expected_version,
            actual: None,
        });
    }
    Ok(next_version)
}

/// Persist the immutable outbox fact for a provider billing command in the
/// same transaction as the authoritative lifecycle row. The lifecycle row's
/// exact-version update is the sole concurrency fence; no generic snapshot is
/// maintained as a second authority.
async fn persist_billing_provider_evidence(
    transaction: &mut Transaction<'_, Postgres>,
    context: &CommercialCommandContext,
    version: u64,
    evidence: &BillingProviderCommandEvidence,
    occurred_at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    evidence
        .validate()
        .map_err(|_| commercial_invalid("invalid billing provider evidence"))?;
    let version_i64 = i64::try_from(version)
        .map_err(|_| commercial_invalid("commercial billing version exceeds database range"))?;
    let payload = serde_json::to_value(evidence)?;
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, active_scope, actor, correlation_id,
             idempotency_key, payload, occurred_at, available_at)
         VALUES ($1, $2, 1, $3, $4, $5, NULL, $6, $7, $8, $9, $10, $10)",
    )
    .bind(Uuid::new_v4())
    .bind(BILLING_PROVIDER_EVENT_APPLIED_TYPE)
    .bind(BILLING_LIFECYCLE_AGGREGATE_TYPE)
    .bind(evidence.subscription_id)
    .bind(version_i64)
    .bind(serde_json::json!({
        "actor_ref": context.actor_ref,
        "provider": "stripe",
    }))
    .bind(context.correlation_id)
    .bind(&context.idempotency_key)
    .bind(payload)
    .bind(occurred_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn load_entitlement_book(
    transaction: &mut Transaction<'_, Postgres>,
    customer_id: Uuid,
    module_key: &str,
) -> Result<Option<(EntitlementAllocationBook, u64)>, PersistenceError> {
    let row = sqlx::query(
        "SELECT payload, version
         FROM erpx_control.entitlement_allocation_books
         WHERE customer_id = $1 AND module_key = $2
         FOR UPDATE",
    )
    .bind(customer_id)
    .bind(module_key)
    .fetch_optional(&mut **transaction)
    .await?;
    row.map(|row| {
        let book: EntitlementAllocationBook = serde_json::from_value(row.try_get("payload")?)?;
        let version = u64::try_from(row.try_get::<i64, _>("version")?)
            .map_err(|_| PersistenceError::InvalidData("negative entitlement version".into()))?;
        Ok((book, version))
    })
    .transpose()
}

/// Persist the provider entitlement worker boundary in the same transaction
/// as the authoritative allocation book. The book's exact-version update is
/// the sole concurrency fence. An event that cannot produce a durable outbox
/// fact rolls back the book, audit, and idempotency reservation together.
async fn persist_entitlement_provider_evidence(
    transaction: &mut Transaction<'_, Postgres>,
    customer_id: Uuid,
    context: &CommercialCommandContext,
    book: &EntitlementAllocationBook,
    version: u64,
    update: &ProviderEntitlementUpdate,
    change: CommercialCommandChange,
) -> Result<(), PersistenceError> {
    update
        .validate()
        .map_err(|_| commercial_invalid("invalid entitlement provider evidence"))?;
    let aggregate_type = entitlement_aggregate_type(&book.module_key);
    let version_i64 = i64::try_from(version)
        .map_err(|_| commercial_invalid("commercial entitlement version exceeds database range"))?;
    let payload = serde_json::json!({
        "customer_id": customer_id,
        "provider_event_id": update.provider_event_id,
        "provider_event_at": update.created_at,
        "module_key": update.module_key,
        "capacity": update.capacity,
        "active": update.active,
        "active_until": update.active_until,
        "change": change,
    });
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, active_scope, actor, correlation_id,
             idempotency_key, payload, occurred_at, available_at)
         VALUES ($1, $2, 1, $3, $4, $5, NULL, $6, $7, $8, $9, $10, $10)",
    )
    .bind(Uuid::new_v4())
    .bind(ENTITLEMENT_PROVIDER_EVENT_APPLIED_TYPE)
    .bind(&aggregate_type)
    .bind(customer_id)
    .bind(version_i64)
    .bind(serde_json::json!({
        "actor_ref": context.actor_ref,
        "provider": "stripe",
    }))
    .bind(context.correlation_id)
    .bind(&context.idempotency_key)
    .bind(payload)
    .bind(update.created_at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

/// Persist a worker-owned billing-policy transition's rebuild boundary
/// atomically. Policy transitions do not come from Stripe, but they still
/// publish a durable outbox fact for projections and downstream enforcement.
/// The authoritative lifecycle row already fenced the exact version.
async fn persist_billing_policy_evidence(
    transaction: &mut Transaction<'_, Postgres>,
    command: &BillingPolicyCommand,
    lifecycle: &BillingLifecycle,
    version: u64,
    change: CommercialCommandChange,
) -> Result<(), PersistenceError> {
    if change != CommercialCommandChange::Applied {
        return Err(commercial_invalid(
            "billing policy evidence requires an applied transition",
        ));
    }
    let version_i64 = i64::try_from(version)
        .map_err(|_| commercial_invalid("commercial billing version exceeds database range"))?;
    let payload = serde_json::json!({
        "command_id": command.context.command_id,
        "customer_id": command.customer_id,
        "subscription_id": command.subscription_id,
        "at": command.at,
        "state": billing_state(lifecycle.state),
        "change": change,
    });
    sqlx::query(
        "INSERT INTO erpx_control.outbox_events
            (id, event_type, event_version, aggregate_type, aggregate_id,
             aggregate_version, active_scope, actor, correlation_id,
             idempotency_key, payload, occurred_at, available_at)
         VALUES ($1, $2, 1, $3, $4, $5, NULL, $6, $7, $8, $9, $10, $10)",
    )
    .bind(Uuid::new_v4())
    .bind(BILLING_POLICY_ADVANCED_TYPE)
    .bind(BILLING_LIFECYCLE_AGGREGATE_TYPE)
    .bind(command.subscription_id)
    .bind(version_i64)
    .bind(serde_json::json!({
        "actor_ref": command.context.actor_ref,
        "purpose": command.context.purpose,
    }))
    .bind(command.context.correlation_id)
    .bind(&command.context.idempotency_key)
    .bind(payload)
    .bind(command.at)
    .execute(&mut **transaction)
    .await?;
    Ok(())
}

async fn load_entitlement_lifecycle_for_command(
    transaction: &mut Transaction<'_, Postgres>,
    customer_id: Uuid,
    module_key: &str,
) -> Result<(EntitlementAllocationBook, u64), PersistenceError> {
    load_entitlement_book(transaction, customer_id, module_key)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "entitlement allocation book",
            id: customer_id,
        })
}

async fn persist_entitlement_book(
    transaction: &mut Transaction<'_, Postgres>,
    customer_id: Uuid,
    book: &EntitlementAllocationBook,
    expected_version: Option<u64>,
) -> Result<u64, PersistenceError> {
    let expected = expected_version.unwrap_or(0);
    let next_version = expected
        .checked_add(1)
        .ok_or_else(|| commercial_invalid("commercial entitlement version overflow"))?;
    let version = i64::try_from(next_version)
        .map_err(|_| commercial_invalid("commercial entitlement version exceeds database range"))?;
    let expected = i64::try_from(expected)
        .map_err(|_| commercial_invalid("commercial entitlement version exceeds database range"))?;
    let payload = serde_json::to_value(book)?;
    let rows = match expected_version {
        Some(_) => sqlx::query(
            "UPDATE erpx_control.entitlement_allocation_books
             SET active = $3, active_until = $4, version = $5, payload = $6,
                 updated_at = now()
             WHERE customer_id = $1 AND module_key = $2 AND version = $7",
        )
        .bind(customer_id)
        .bind(&book.module_key)
        .bind(book.active)
        .bind(book.active_until)
        .bind(version)
        .bind(payload)
        .bind(expected)
        .execute(&mut **transaction)
        .await?
        .rows_affected(),
        None => sqlx::query(
            "INSERT INTO erpx_control.entitlement_allocation_books
                (customer_id, module_key, active, active_until, version, payload)
             VALUES ($1, $2, $3, $4, $5, $6)
             ON CONFLICT (customer_id, module_key) DO NOTHING",
        )
        .bind(customer_id)
        .bind(&book.module_key)
        .bind(book.active)
        .bind(book.active_until)
        .bind(version)
        .bind(payload)
        .execute(&mut **transaction)
        .await?
        .rows_affected(),
    };
    if rows != 1 {
        return Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: "entitlement_allocation".into(),
            aggregate_id: customer_id,
            expected: expected_version,
            actual: None,
        });
    }
    Ok(next_version)
}

#[async_trait]
impl StripeSemanticApplier for SqlxCommercialStore {
    async fn apply_stripe_semantic_event(
        &self,
        event: &StripeSemanticEvent,
    ) -> Result<StripeSemanticApplyResult, StripeSemanticApplyError> {
        match event {
            StripeSemanticEvent::NoOp(_) => Ok(StripeSemanticApplyResult::NoOp),
            StripeSemanticEvent::Checkout(event) => self.apply_checkout_binding(event).await,
            StripeSemanticEvent::Billing(event) => self.apply_billing_transition(event).await,
            StripeSemanticEvent::Entitlement(event) => self.apply_entitlement_update(event).await,
        }
    }
}

impl SqlxCommercialStore {
    async fn apply_checkout_binding(
        &self,
        event: &StripeCheckoutBinding,
    ) -> Result<StripeSemanticApplyResult, StripeSemanticApplyError> {
        if event.holding_id.trim().is_empty()
            || event.holding_id.len() > 128
            || event.holding_id.chars().any(char::is_control)
            || !event.stripe_customer_ref.starts_with("cus_")
            || !event.stripe_subscription_ref.starts_with("sub_")
            || event.currency != erpx_domain::commercial::ACTIVE_CATALOG_CURRENCY
        {
            return Err(StripeSemanticApplyError::Permanent(
                "invalid Checkout subscription binding".into(),
            ));
        }
        let mut transaction = self
            .write
            .begin()
            .await
            .map_err(|error| StripeSemanticApplyError::Retryable(error.to_string()))?;
        let exact_binding: bool = sqlx::query_scalar(
            "SELECT EXISTS (
                SELECT 1 FROM erpx_control.customer_holding_bindings
                WHERE customer_id = $1 AND holding_id = $2
                  AND binding_state = 'configured'
             )",
        )
        .bind(event.customer_id)
        .bind(&event.holding_id)
        .fetch_one(&mut *transaction)
        .await
        .map_err(|error| StripeSemanticApplyError::Retryable(error.to_string()))?;
        if !exact_binding {
            return Err(StripeSemanticApplyError::Permanent(
                "Checkout metadata does not match a configured holding".into(),
            ));
        }
        let existing = sqlx::query(
            "SELECT holding_id, customer_id, subscription_id,
                    currency::text AS currency
             FROM erpx_control.stripe_subscription_bindings
             WHERE stripe_customer_ref = $1
                OR stripe_subscription_ref = $2
             FOR UPDATE",
        )
        .bind(&event.stripe_customer_ref)
        .bind(&event.stripe_subscription_ref)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|error| StripeSemanticApplyError::Retryable(error.to_string()))?;
        if let Some(existing) = existing {
            let matches = existing
                .try_get::<String, _>("holding_id")
                .is_ok_and(|holding_id| holding_id == event.holding_id)
                && existing
                    .try_get::<Uuid, _>("customer_id")
                    .is_ok_and(|customer_id| customer_id == event.customer_id)
                && existing
                    .try_get::<Uuid, _>("subscription_id")
                    .is_ok_and(|subscription_id| subscription_id == event.subscription_id)
                && existing
                    .try_get::<String, _>("currency")
                    .is_ok_and(|currency| currency.trim() == event.currency);
            if !matches {
                return Err(StripeSemanticApplyError::Permanent(
                    "Stripe subscription is already bound to another holding".into(),
                ));
            }
            transaction
                .commit()
                .await
                .map_err(|error| StripeSemanticApplyError::Retryable(error.to_string()))?;
            return Ok(StripeSemanticApplyResult::DuplicateOrOutOfOrder);
        }
        sqlx::query(
            "INSERT INTO erpx_control.stripe_subscription_bindings
                (customer_id, holding_id, subscription_id, stripe_customer_ref,
                 stripe_subscription_ref, currency, item_quantities,
                 created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8)",
        )
        .bind(event.customer_id)
        .bind(&event.holding_id)
        .bind(event.subscription_id)
        .bind(&event.stripe_customer_ref)
        .bind(&event.stripe_subscription_ref)
        .bind(&event.currency)
        .bind(
            serde_json::to_value(&event.item_quantities)
                .map_err(|error| StripeSemanticApplyError::Permanent(error.to_string()))?,
        )
        .bind(event.created_at)
        .execute(&mut *transaction)
        .await
        .map_err(|error| StripeSemanticApplyError::Retryable(error.to_string()))?;
        sqlx::query(
            "UPDATE erpx_control.holding_summary_projection
             SET subscription_state = 'pending', projected_at = now()
             WHERE holding_id = $1",
        )
        .bind(&event.holding_id)
        .execute(&mut *transaction)
        .await
        .map_err(|error| StripeSemanticApplyError::Retryable(error.to_string()))?;
        sqlx::query(
            "INSERT INTO erpx_control.audit_events
                (id, actor_ref, customer_account_id, action, purpose,
                 correlation_id, payload)
             VALUES ($1, 'stripe', $2,
                     'commercial.billing.checkout_bound',
                     'verified Stripe Checkout binding', $3, $4)",
        )
        .bind(Uuid::new_v4())
        .bind(event.customer_id)
        .bind(Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            event.event_id.as_bytes(),
        ))
        .bind(serde_json::json!({
            "holding_id": event.holding_id,
            "subscription_id": event.subscription_id,
            "provider_event_id": event.event_id,
        }))
        .execute(&mut *transaction)
        .await
        .map_err(|error| StripeSemanticApplyError::Retryable(error.to_string()))?;
        transaction
            .commit()
            .await
            .map_err(|error| StripeSemanticApplyError::Retryable(error.to_string()))?;
        Ok(StripeSemanticApplyResult::Applied)
    }

    async fn apply_billing_transition(
        &self,
        event: &StripeBillingTransition,
    ) -> Result<StripeSemanticApplyResult, StripeSemanticApplyError> {
        let context = CommercialCommandContext::for_provider("stripe", &event.event_id)
            .map_err(|_| StripeSemanticApplyError::Permanent("invalid provider context".into()))?;
        let command = BillingPolicyCommand {
            customer_id: event.customer_id,
            subscription_id: event.subscription_id,
            at: event.created_at,
            context,
        };
        let provider_event = ProviderBillingEvent {
            provider_event_id: event.event_id.clone(),
            created_at: event.created_at,
            event: event.transition,
        };
        let receipt = self
            .apply_provider_billing_event(command, provider_event)
            .await
            .map_err(classify_semantic_persistence)?;
        let CommercialCommandResult::Billing { state, change } = receipt.result else {
            return Err(StripeSemanticApplyError::Permanent(
                "provider billing receipt has an invalid result".into(),
            ));
        };
        sqlx::query(
            "UPDATE erpx_control.holding_summary_projection AS summary
             SET subscription_state = $3,
                 overdue = $4,
                 projected_at = now()
             FROM erpx_control.stripe_subscription_bindings AS binding
             WHERE binding.customer_id = $1
               AND binding.subscription_id = $2
               AND summary.holding_id = binding.holding_id",
        )
        .bind(event.customer_id)
        .bind(event.subscription_id)
        .bind(billing_state(state))
        .bind(matches!(
            state,
            BillingState::PastDue | BillingState::Grace | BillingState::ReadOnly
        ))
        .execute(&self.write)
        .await
        .map_err(|error| StripeSemanticApplyError::Retryable(error.to_string()))?;
        if let Some(invoice) = &event.invoice {
            if invoice.currency != erpx_domain::commercial::ACTIVE_CATALOG_CURRENCY {
                return Err(StripeSemanticApplyError::Permanent(
                    "invoice currency does not match the active SaaS currency".into(),
                ));
            }
            let projected = sqlx::query(
                "INSERT INTO erpx_control.holding_invoice_summary_projection
                    (holding_id, invoice_id, state, currency,
                     amount_due_minor, amount_paid_minor, due_at,
                     hosted_invoice_available, projected_at)
                 SELECT $3, $4, $5, $6, $7, $8, $9, $10, now()
                 FROM erpx_control.stripe_subscription_bindings AS binding
                 WHERE binding.customer_id = $1
                   AND binding.subscription_id = $2
                   AND binding.holding_id = $3
                 ON CONFLICT (holding_id, invoice_id) DO UPDATE
                 SET state = EXCLUDED.state,
                     currency = EXCLUDED.currency,
                     amount_due_minor = EXCLUDED.amount_due_minor,
                     amount_paid_minor = EXCLUDED.amount_paid_minor,
                     due_at = EXCLUDED.due_at,
                     hosted_invoice_available = EXCLUDED.hosted_invoice_available,
                     projected_at = EXCLUDED.projected_at",
            )
            .bind(event.customer_id)
            .bind(event.subscription_id)
            .bind(&invoice.holding_id)
            .bind(&invoice.invoice_id)
            .bind(&invoice.state)
            .bind(&invoice.currency)
            .bind(i64::try_from(invoice.amount_due_minor).map_err(|_| {
                StripeSemanticApplyError::Permanent("invoice amount is outside range".into())
            })?)
            .bind(i64::try_from(invoice.amount_paid_minor).map_err(|_| {
                StripeSemanticApplyError::Permanent("invoice amount is outside range".into())
            })?)
            .bind(invoice.due_at)
            .bind(invoice.hosted_invoice_available)
            .execute(&self.write)
            .await
            .map_err(|error| StripeSemanticApplyError::Retryable(error.to_string()))?;
            if projected.rows_affected() != 1 {
                return Err(StripeSemanticApplyError::Permanent(
                    "invoice metadata does not match its holding subscription".into(),
                ));
            }
        }
        if receipt.replayed {
            return Ok(StripeSemanticApplyResult::DuplicateOrOutOfOrder);
        }
        Ok(match change {
            CommercialCommandChange::Applied => StripeSemanticApplyResult::Applied,
            CommercialCommandChange::DuplicateOrOutOfOrder => {
                StripeSemanticApplyResult::DuplicateOrOutOfOrder
            }
            CommercialCommandChange::Unchanged => StripeSemanticApplyResult::NoOp,
        })
    }

    async fn apply_entitlement_update(
        &self,
        event: &StripeEntitlementUpdate,
    ) -> Result<StripeSemanticApplyResult, StripeSemanticApplyError> {
        let context = CommercialCommandContext::for_provider("stripe", &event.event_id)
            .map_err(|_| StripeSemanticApplyError::Permanent("invalid provider context".into()))?;
        let update = ProviderEntitlementUpdate {
            provider_event_id: event.event_id.clone(),
            created_at: event.created_at,
            module_key: event.module_key.clone(),
            capacity: event.capacity,
            active: event.active,
            active_until: event.active_until,
        };
        let receipt = self
            .apply_provider_entitlement_update(event.customer_id, context, update)
            .await
            .map_err(classify_semantic_persistence)?;
        if receipt.replayed {
            return Ok(StripeSemanticApplyResult::DuplicateOrOutOfOrder);
        }
        match receipt.result {
            CommercialCommandResult::Entitlement { change, .. } => Ok(match change {
                CommercialCommandChange::Applied => StripeSemanticApplyResult::Applied,
                CommercialCommandChange::DuplicateOrOutOfOrder => {
                    StripeSemanticApplyResult::DuplicateOrOutOfOrder
                }
                CommercialCommandChange::Unchanged => StripeSemanticApplyResult::NoOp,
            }),
            _ => Err(StripeSemanticApplyError::Permanent(
                "provider entitlement receipt has an invalid result".into(),
            )),
        }
    }
}

fn seed_lifecycle(event: BillingEvent, at: DateTime<Utc>) -> BillingLifecycle {
    match event {
        BillingEvent::TrialEnded | BillingEvent::SubscriptionCanceled => {
            BillingLifecycle::new(BillingState::Trialing, at)
        }
        BillingEvent::PaymentFailed | BillingEvent::GraceStarted => {
            BillingLifecycle::new(BillingState::Active, at)
        }
        BillingEvent::PaymentRecovered => BillingLifecycle::new(BillingState::PastDue, at),
        BillingEvent::GraceExpired => {
            let mut lifecycle = BillingLifecycle::new(
                BillingState::Grace,
                at - erpx_domain::commercial::DEFAULT_GRACE_PERIOD,
            );
            lifecycle.grace_started_at = Some(at - erpx_domain::commercial::DEFAULT_GRACE_PERIOD);
            lifecycle.grace_until = Some(at);
            lifecycle
        }
    }
}

fn classify_semantic_persistence(error: PersistenceError) -> StripeSemanticApplyError {
    let message = error.to_string();
    match error {
        PersistenceError::Database(_)
        | PersistenceError::Migration(_)
        | PersistenceError::OptimisticConcurrency { .. } => {
            StripeSemanticApplyError::Retryable(message)
        }
        PersistenceError::Serialization(_)
        | PersistenceError::IdempotencyConflict { .. }
        | PersistenceError::NotFound { .. }
        | PersistenceError::ClaimLost { .. }
        | PersistenceError::InvalidData(_)
        | PersistenceError::Configuration(_) => StripeSemanticApplyError::Permanent(message),
    }
}

fn stripe_claim_lost(event_id: &str, worker_id: Uuid) -> PersistenceError {
    PersistenceError::InvalidData(format!(
        "worker {worker_id} does not own Stripe inbox event {event_id}"
    ))
}

fn bounded_inbox_error(error: &str) -> String {
    let lower = error.to_ascii_lowercase();
    if [
        "password",
        "secret",
        "token",
        "authorization",
        "api_key",
        "client_secret",
        "private_key",
        "sk_",
        "rk_",
        "whsec_",
        "postgres://",
        "raw_body",
        "payment_method",
        "payment_intent",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return "Stripe processing error redacted".into();
    }
    if error.chars().any(char::is_control) {
        return "Stripe processing error redacted".into();
    }
    error.chars().take(4096).collect()
}

fn attempts_from_row(row: &sqlx::postgres::PgRow) -> Result<u32, PersistenceError> {
    u32::try_from(row.try_get::<i32, _>("attempts")?)
        .map_err(|_| PersistenceError::InvalidData("negative Stripe inbox attempts".into()))
}

fn stripe_claim_from_row(row: sqlx::postgres::PgRow) -> Result<StripeInboxClaim, PersistenceError> {
    Ok(StripeInboxClaim {
        event_id: row.try_get("event_id")?,
        event_type: row.try_get("event_type")?,
        stripe_created_at: row.try_get("stripe_created_at")?,
        payload: row.try_get("payload")?,
        attempts: attempts_from_row(&row)?,
        claimed_by: row.try_get("claimed_by")?,
        claim_expires_at: row.try_get("claim_expires_at")?,
    })
}

fn stripe_state_from_row(row: sqlx::postgres::PgRow) -> Result<StripeInboxState, PersistenceError> {
    Ok(StripeInboxState {
        event_id: row.try_get("event_id")?,
        status: StripeInboxStatus::from_database(row.try_get::<String, _>("status")?.as_str())?,
        attempts: attempts_from_row(&row)?,
        replay_count: u32::try_from(row.try_get::<i32, _>("replay_count")?).map_err(|_| {
            PersistenceError::InvalidData("negative Stripe inbox replay count".into())
        })?,
        available_at: row.try_get("available_at")?,
        claimed_by: row.try_get("claimed_by")?,
        claim_expires_at: row.try_get("claim_expires_at")?,
        processed_at: row.try_get("processed_at")?,
        dead_lettered_at: row.try_get("dead_lettered_at")?,
    })
}

fn billing_state(state: erpx_domain::commercial::BillingState) -> &'static str {
    match state {
        erpx_domain::commercial::BillingState::Trialing => "trialing",
        erpx_domain::commercial::BillingState::Active => "active",
        erpx_domain::commercial::BillingState::PastDue => "past_due",
        erpx_domain::commercial::BillingState::Grace => "grace",
        erpx_domain::commercial::BillingState::ReadOnly => "read_only",
        erpx_domain::commercial::BillingState::Canceled => "canceled",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::commercial::ProviderCatalogPrice;

    #[test]
    fn retry_policy_is_positive_bounded_and_exponential() {
        let policy = StripeInboxRetryPolicy::new(4, Duration::seconds(2), Duration::seconds(5))
            .expect("retry policy");
        assert_eq!(policy.backoff_for_attempt(1), Duration::seconds(2));
        assert_eq!(policy.backoff_for_attempt(2), Duration::seconds(4));
        assert_eq!(policy.backoff_for_attempt(3), Duration::seconds(5));
        assert_eq!(policy.backoff_for_attempt(99), Duration::seconds(5));
        assert!(
            StripeInboxRetryPolicy::new(0, Duration::seconds(1), Duration::seconds(1)).is_err()
        );
        assert!(
            StripeInboxRetryPolicy::new(
                MAX_STRIPE_INBOX_ATTEMPTS + 1,
                Duration::seconds(1),
                Duration::seconds(1)
            )
            .is_err()
        );
        assert!(StripeInboxRetryPolicy::new(2, Duration::zero(), Duration::seconds(1)).is_err());
        assert!(
            StripeInboxRetryPolicy::new(2, Duration::seconds(2), Duration::seconds(1)).is_err()
        );
    }

    #[test]
    fn catalog_retry_policy_is_bounded_and_only_transient_provider_failures_retry() {
        let policy = StripeCatalogRetryPolicy::new(
            3,
            StdDuration::from_millis(10),
            StdDuration::from_millis(15),
        )
        .expect("catalog retry policy");
        assert_eq!(policy.backoff_for_retry(1), StdDuration::from_millis(10));
        assert_eq!(policy.backoff_for_retry(2), StdDuration::from_millis(15));
        assert_eq!(policy.backoff_for_retry(100), StdDuration::from_millis(15));
        assert!(
            StripeCatalogRetryPolicy::new(
                MAX_STRIPE_CATALOG_RETRY_ATTEMPTS + 1,
                StdDuration::ZERO,
                StdDuration::ZERO,
            )
            .is_err()
        );
        assert!(StripeProviderErrorKind::RateLimited.is_retryable());
        assert!(StripeProviderErrorKind::Unavailable.is_retryable());
        assert!(!StripeProviderErrorKind::Authentication.is_retryable());
        assert!(!StripeProviderErrorKind::InvalidRequest.is_retryable());
    }

    #[test]
    fn inbox_status_mapping_is_closed_and_errors_are_bounded() {
        assert_eq!(
            StripeInboxStatus::from_database("dead_letter").expect("status"),
            StripeInboxStatus::DeadLetter
        );
        assert!(StripeInboxStatus::from_database("unknown").is_err());
        let raw = "x".repeat(8_192);
        assert_eq!(bounded_inbox_error(&raw).len(), 4_096);
        assert_eq!(
            bounded_inbox_error("provider password=secret token=hidden"),
            "Stripe processing error redacted"
        );
        assert_eq!(
            bounded_inbox_error("Stripe provider returned sk_live_never_log"),
            "Stripe processing error redacted"
        );
    }

    #[test]
    fn commercial_idempotency_hash_ignores_transport_attempt_ids() {
        let payload = (Uuid::new_v4(), "finance", 10_u32);
        let first = CommercialCommandContext::new(
            Uuid::new_v4(),
            "commercial-retry-1",
            Uuid::new_v4(),
            "stripe",
            "authoritative provider commercial event",
        )
        .expect("first context");
        let retry = CommercialCommandContext::new(
            Uuid::new_v4(),
            "commercial-retry-1",
            Uuid::new_v4(),
            "stripe",
            "authoritative provider commercial event",
        )
        .expect("retry context");
        assert_eq!(
            commercial_command_request_hash(&payload, &first).expect("first hash"),
            commercial_command_request_hash(&payload, &retry).expect("retry hash")
        );

        let different_purpose = CommercialCommandContext::new(
            Uuid::new_v4(),
            "commercial-retry-1",
            Uuid::new_v4(),
            "stripe",
            "different purpose",
        )
        .expect("different-purpose context");
        assert_ne!(
            commercial_command_request_hash(&payload, &first).expect("first hash"),
            commercial_command_request_hash(&payload, &different_purpose)
                .expect("different-purpose hash")
        );
    }

    fn sample_published_catalog() -> PublishedCommercialCatalogRevision {
        CommercialCatalogRevisionDraft {
            id: Uuid::new_v4(),
            revision_number: 7,
            prices: vec![erpx_domain::commercial::CatalogPrice {
                item_key: "base".into(),
                display_name: "Base".into(),
                kind: erpx_domain::commercial::CatalogPriceKind::BaseFee,
                pricing_unit: erpx_domain::commercial::PricingUnit::Flat,
                charge: erpx_domain::commercial::ChargeKind::Recurring(
                    erpx_domain::commercial::BillingInterval::Monthly,
                ),
                amount: erpx_domain::commercial::Money {
                    amount_minor: 1_000,
                    currency: "THB".into(),
                },
                trial: None,
                coupon: None,
                setup_fee: None,
                included_usage: None,
                overage: None,
            }],
        }
        .publish(Utc::now(), "catalog-admin")
        .expect("sample catalog")
    }

    fn sample_two_item_catalog() -> PublishedCommercialCatalogRevision {
        CommercialCatalogRevisionDraft {
            id: Uuid::new_v4(),
            revision_number: 8,
            prices: vec![
                erpx_domain::commercial::CatalogPrice {
                    item_key: "base".into(),
                    display_name: "Base".into(),
                    kind: erpx_domain::commercial::CatalogPriceKind::BaseFee,
                    pricing_unit: erpx_domain::commercial::PricingUnit::Flat,
                    charge: erpx_domain::commercial::ChargeKind::Recurring(
                        erpx_domain::commercial::BillingInterval::Monthly,
                    ),
                    amount: erpx_domain::commercial::Money {
                        amount_minor: 1_000,
                        currency: "THB".into(),
                    },
                    trial: None,
                    coupon: None,
                    setup_fee: None,
                    included_usage: None,
                    overage: None,
                },
                erpx_domain::commercial::CatalogPrice {
                    item_key: "module".into(),
                    display_name: "Module".into(),
                    kind: erpx_domain::commercial::CatalogPriceKind::Module,
                    pricing_unit: erpx_domain::commercial::PricingUnit::Company,
                    charge: erpx_domain::commercial::ChargeKind::OneTime,
                    amount: erpx_domain::commercial::Money {
                        amount_minor: 2_000,
                        currency: "THB".into(),
                    },
                    trial: None,
                    coupon: None,
                    setup_fee: None,
                    included_usage: None,
                    overage: None,
                },
            ],
        }
        .publish(Utc::now(), "catalog-admin")
        .expect("sample two-item catalog")
    }

    fn sample_provider_snapshot() -> StripeCatalogProviderSnapshot {
        let local = sample_published_catalog();
        let price = local.price("base").expect("base price").clone();
        let revision = ProviderCatalogRevision {
            provider_revision_ref: "stripe_catalog_7".into(),
            revision_number: 7,
            prices: vec![ProviderCatalogPrice {
                product_ref: Some("prod_base".into()),
                price_ref: Some("price_base".into()),
                price,
            }],
        };
        let identifiers = BTreeMap::from([(
            "base".into(),
            StripeCatalogItemIdentifiers {
                item_key: "base".into(),
                product: StripeProductId::try_from("prod_base").expect("product id"),
                price: StripePriceId::try_from("price_base").expect("price id"),
                entitlement_feature: None,
                meter: None,
            },
        )]);
        StripeCatalogProviderSnapshot::new(revision, identifiers).expect("provider snapshot")
    }

    #[test]
    fn stripe_provider_configuration_requires_the_repository_pin_and_safe_endpoint() {
        let config = StripeProviderConfig::pinned("acct_test").expect("pinned config");
        assert_eq!(config.api_version(), PINNED_STRIPE_API_VERSION);
        assert_eq!(config.api_base_url(), STRIPE_API_BASE_URL);
        assert_eq!(config.account_ref().as_str(), "acct_test");

        assert!(matches!(
            StripeProviderConfig::new("latest", "acct_test", STRIPE_API_BASE_URL),
            Err(StripeProviderConfigError::UnpinnedApiVersion { .. })
        ));
        assert!(matches!(
            StripeProviderConfig::new(
                PINNED_STRIPE_API_VERSION,
                "acct_test",
                "http://localhost:4242"
            ),
            Err(StripeProviderConfigError::InvalidApiBaseUrl { .. })
        ));
        assert!(matches!(
            StripeProviderConfig::pinned("sk_live_not_an_account"),
            Err(StripeProviderConfigError::InvalidAccountReference(_))
        ));
    }

    #[test]
    fn provider_configuration_debug_contains_no_credential_material() {
        let config = StripeProviderConfig::pinned("acct_test").expect("pinned config");
        let debug = format!("{config:?}");
        for secret in [
            "sk_live_not_present",
            "rk_live_not_present",
            "whsec_not_present",
        ] {
            assert!(!debug.contains(secret));
        }
        assert!(!debug.contains("api_key"));
        assert!(!debug.contains("secret"));
    }

    #[test]
    fn typed_provider_identifiers_bind_snapshot_references_and_reconcile_deterministically() {
        assert!(StripeProductId::try_from("price_wrong_prefix").is_err());
        assert!(StripeEntitlementFeatureId::try_from("feat_finance").is_ok());
        assert!(StripeMeterId::try_from("meter_usage").is_ok());

        let snapshot = sample_provider_snapshot();
        let local = sample_published_catalog();
        assert!(snapshot.validate().is_ok());
        assert!(snapshot.reconciliation_against(&local).is_in_sync());

        let mut invalid = snapshot.clone();
        invalid.revision.prices[0].price_ref = Some("price_other".into());
        assert_eq!(
            invalid.validate().expect_err("mismatched price reference"),
            StripeCatalogSnapshotError::PriceReferenceMismatch
        );
    }

    #[test]
    fn catalog_requests_require_idempotency_and_validate_expected_revision_references() {
        let local_revision = sample_published_catalog();
        let request = StripeCatalogPublishRequest {
            local_revision: local_revision.clone(),
            idempotency_key: "catalog-publish-revision-7".into(),
        };
        assert!(request.validate().is_ok());
        assert_eq!(request.idempotency_key, "catalog-publish-revision-7");

        let missing_key = StripeCatalogPublishRequest {
            idempotency_key: "  ".into(),
            ..request.clone()
        };
        assert_eq!(
            missing_key.validate().expect_err("missing idempotency key"),
            StripeCatalogRequestError::MissingIdempotencyKey
        );

        let reconcile = StripeCatalogReconcileRequest {
            local_revision,
            expected_provider_revision_ref: Some("stripe_catalog_7".into()),
        };
        assert!(reconcile.validate().is_ok());
        assert_eq!(
            StripeCatalogReconcileRequest {
                expected_provider_revision_ref: Some("  ".into()),
                ..reconcile
            }
            .validate()
            .expect_err("blank expected reference"),
            StripeCatalogRequestError::BlankExpectedProviderReference
        );
    }

    #[test]
    fn catalog_publication_plan_is_pinned_typed_and_deterministic() {
        let config = StripeProviderConfig::pinned("acct_test").expect("config");
        let local_revision = sample_published_catalog();
        let request = StripeCatalogPublishRequest {
            local_revision: local_revision.clone(),
            idempotency_key: "catalog-publish-revision-7".into(),
        };
        let plan = StripeCatalogPublicationPlan::build(&config, &request).expect("plan");
        assert_eq!(plan.account_ref().as_str(), "acct_test");
        assert_eq!(plan.api_base_url(), STRIPE_API_BASE_URL);
        assert_eq!(plan.api_version(), PINNED_STRIPE_API_VERSION);
        assert_eq!(plan.revision_number(), 7);
        assert_eq!(plan.steps().len(), 1);
        let step = &plan.steps()[0];
        assert_eq!(step.item_key, "base");
        assert_eq!(
            step.product_request.metadata.get("erpx_catalog_revision"),
            Some(&"7".into())
        );
        assert_eq!(
            step.product_request.metadata.get("erpx_catalog_item"),
            Some(&"base".into())
        );
        assert!(
            step.product_idempotency_key
                .starts_with("erpx-catalog-product-")
        );
        assert!(
            step.price_idempotency_key
                .starts_with("erpx-catalog-price-")
        );
        let price_request = step.price_request(StripeProductId::try_from("prod_base").unwrap());
        assert_eq!(price_request.unit_amount_minor, 1_000);
        assert_eq!(price_request.currency, "thb");
        assert_eq!(
            price_request.recurring,
            Some(StripePriceRecurring {
                interval: StripePriceInterval::Month,
                usage_type: StripePriceUsageType::Licensed,
            })
        );
        let repeat = StripeCatalogPublicationPlan::build(&config, &request).expect("repeat plan");
        assert_eq!(plan, repeat);
    }

    #[test]
    fn catalog_publication_response_maps_typed_ids_to_existing_reconciliation() {
        let local = sample_published_catalog();
        let response = StripeCatalogPublicationResponse {
            provider_revision_ref: "stripe_catalog_7".into(),
            revision_number: 7,
            items: BTreeMap::from([(
                "base".into(),
                StripeCatalogItemIdentifiers {
                    item_key: "base".into(),
                    product: StripeProductId::try_from("prod_base").unwrap(),
                    price: StripePriceId::try_from("price_base").unwrap(),
                    entitlement_feature: None,
                    meter: None,
                },
            )]),
        };
        let snapshot = response
            .into_provider_snapshot(&local)
            .expect("provider snapshot");
        assert!(snapshot.reconciliation_against(&local).is_in_sync());
    }

    #[test]
    fn publish_idempotency_key_is_bound_to_account_operation_and_revision() {
        let config = StripeProviderConfig::pinned("acct_test").expect("config");
        let request = StripeCatalogPublishRequest {
            local_revision: sample_published_catalog(),
            idempotency_key: "caller-request-1".into(),
        };
        let binding = request.bind_idempotency_key(&config).expect("binding");
        assert_eq!(binding.operation, StripeCatalogOperation::Publish);
        assert_eq!(binding.revision_number, 7);
        assert!(binding.provider_key.starts_with("erpx-catalog-publish-7-"));
        assert_eq!(
            binding.provider_key,
            request
                .bind_idempotency_key(&config)
                .expect("repeat binding")
                .provider_key
        );

        let other_account = StripeProviderConfig::pinned("acct_other").expect("config");
        assert_ne!(
            binding.provider_key,
            request
                .bind_idempotency_key(&other_account)
                .expect("other account binding")
                .provider_key
        );
        assert!(matches!(
            (StripeCatalogPublishRequest {
                idempotency_key: "bad\nkey".into(),
                ..request
            })
            .validate(),
            Err(StripeCatalogRequestError::IdempotencyKeyHasControlCharacter)
        ));
    }

    #[test]
    fn provider_errors_are_deterministically_redacted_and_do_not_retain_secrets() {
        let raw = serde_json::json!({
            "error": {
                "type": "invalid_request_error",
                "code": "resource_missing",
                "param": "payment_intent",
                "message": "secret sk_live_do_not_log and pi_secret_do_not_log"
            }
        })
        .to_string();
        let redacted = redact_provider_error(&raw);
        assert_eq!(
            redacted,
            r#"{"code":"resource_missing","type":"invalid_request_error"}"#
        );
        assert_eq!(redacted, redact_provider_error(&raw));
        assert!(!redacted.contains("sk_live_do_not_log"));
        assert!(!redacted.contains("pi_secret_do_not_log"));
        assert!(!redacted.contains("payment_intent"));

        let error = StripeProviderError::from_remote_response(
            429,
            Some("sk_live_provider_secret"),
            Some("req_123"),
            &raw,
        );
        assert_eq!(error.kind(), StripeProviderErrorKind::RateLimited);
        assert_eq!(error.provider_code(), None);
        assert_eq!(error.request_id(), Some("req_123"));
        assert!(!error.to_string().contains("sk_live_provider_secret"));
        assert!(!format!("{error:?}").contains("pi_secret_do_not_log"));
    }

    #[test]
    fn raw_body_context_binds_exact_digest_and_correlation() {
        let raw_body = br#"{"id":"evt_context"}"#;
        let correlation_id = Uuid::new_v4();
        let context = StripeWebhookPersistenceContext::from_raw_body(raw_body, correlation_id)
            .expect("persistence context");
        let expected: [u8; 32] = Sha256::digest(raw_body).into();
        assert_eq!(context.raw_body_sha256(), &expected);
        assert_eq!(context.correlation_id(), correlation_id);
        assert_eq!(
            StripeWebhookPersistenceContext::from_raw_body(b"", correlation_id)
                .expect_err("empty raw body"),
            StripeWebhookPersistenceContextError::EmptyRawBody
        );
        assert_eq!(
            StripeWebhookPersistenceContext::from_raw_body(raw_body, Uuid::nil())
                .expect_err("nil correlation"),
            StripeWebhookPersistenceContextError::NilCorrelationId
        );
    }

    #[test]
    fn verified_webhook_parser_binds_context_and_produces_typed_inbox_event() {
        let raw_body = br#"{"id":"evt_test_123","type":"invoice.paid","created":1700000000,"data":{"object":{"metadata":{"erpx_customer_id":"customer"}}},"secret":"not-in-debug"}"#;
        let correlation_id = Uuid::new_v4();
        let context = StripeWebhookPersistenceContext::from_raw_body(raw_body, correlation_id)
            .expect("context");
        let webhook =
            StripeWebhookEvent::from_verified_raw_body(raw_body, context.clone()).expect("webhook");
        assert_eq!(webhook.event_id.as_str(), "evt_test_123");
        assert_eq!(webhook.event_type, "invoice.paid");
        assert_eq!(
            webhook.persistence_context().correlation_id(),
            correlation_id
        );
        assert!(!format!("{webhook:?}").contains("not-in-debug"));
        let inbox = webhook.into_inbox_event();
        assert_eq!(inbox.event_id, "evt_test_123");
        assert_eq!(
            inbox.stripe_created_at.expect("created").timestamp(),
            1_700_000_000
        );

        let other_context = StripeWebhookPersistenceContext::from_raw_body(
            br#"{"id":"evt_other"}"#,
            correlation_id,
        )
        .expect("other context");
        assert_eq!(
            StripeWebhookEvent::from_verified_raw_body(raw_body, other_context)
                .expect_err("context mismatch")
                .to_string(),
            "Stripe webhook persistence context does not match the raw body"
        );
    }

    #[test]
    fn replay_policy_is_positive_and_durably_bounded() {
        assert_eq!(
            StripeInboxReplayPolicy::default().max_replays,
            MAX_STRIPE_INBOX_REPLAYS
        );
        assert!(StripeInboxReplayPolicy::new(1).is_ok());
        assert!(StripeInboxReplayPolicy::new(0).is_err());
        assert!(StripeInboxReplayPolicy::new(MAX_STRIPE_INBOX_REPLAYS + 1).is_err());
        let authorization = StripeReplayAuthorizationContext::new(
            "billing-admin",
            "replay dead-lettered Stripe event",
            Uuid::new_v4(),
        )
        .expect("replay authorization");
        assert_eq!(authorization.actor_ref(), "billing-admin");
        assert!(StripeReplayAuthorizationContext::new("", "purpose", Uuid::new_v4()).is_err());
    }

    struct NeverCalledProvider;

    #[async_trait]
    impl StripeCatalogProvider for NeverCalledProvider {
        async fn publish_catalog(
            &self,
            _config: &StripeProviderConfig,
            _request: &StripeCatalogPublishRequest,
            _idempotency_key: &str,
        ) -> Result<StripeCatalogProviderSnapshot, StripeProviderError> {
            panic!("invalid requests must be rejected before provider invocation");
        }

        async fn reconcile_catalog(
            &self,
            _config: &StripeProviderConfig,
            _request: &StripeCatalogReconcileRequest,
        ) -> Result<StripeCatalogProviderSnapshot, StripeProviderError> {
            panic!("invalid requests must be rejected before provider invocation");
        }
    }

    struct MismatchedRevisionProvider;

    #[async_trait]
    impl StripeCatalogProvider for MismatchedRevisionProvider {
        async fn publish_catalog(
            &self,
            _config: &StripeProviderConfig,
            _request: &StripeCatalogPublishRequest,
            _idempotency_key: &str,
        ) -> Result<StripeCatalogProviderSnapshot, StripeProviderError> {
            panic!("publish is not part of the response-validation test");
        }

        async fn reconcile_catalog(
            &self,
            _config: &StripeProviderConfig,
            _request: &StripeCatalogReconcileRequest,
        ) -> Result<StripeCatalogProviderSnapshot, StripeProviderError> {
            Ok(sample_provider_snapshot())
        }
    }

    struct WrongPublicationRevisionProvider;

    #[async_trait]
    impl StripeCatalogProvider for WrongPublicationRevisionProvider {
        async fn publish_catalog(
            &self,
            _config: &StripeProviderConfig,
            _request: &StripeCatalogPublishRequest,
            _idempotency_key: &str,
        ) -> Result<StripeCatalogProviderSnapshot, StripeProviderError> {
            // The snapshot is structurally valid, but it belongs to an
            // unrelated provider revision and must not cross the publication
            // boundary as if it represented this request.
            Ok(sample_provider_snapshot())
        }

        async fn reconcile_catalog(
            &self,
            _config: &StripeProviderConfig,
            _request: &StripeCatalogReconcileRequest,
        ) -> Result<StripeCatalogProviderSnapshot, StripeProviderError> {
            panic!("reconcile is not part of the publication response test");
        }
    }

    #[tokio::test]
    async fn provider_boundary_rejects_invalid_publish_before_adapter_invocation() {
        let boundary = StripeCatalogProviderBoundary::new(
            StripeProviderConfig::pinned("acct_test").expect("config"),
            NeverCalledProvider,
        )
        .expect("boundary");
        let request = StripeCatalogPublishRequest {
            local_revision: sample_published_catalog(),
            idempotency_key: String::new(),
        };
        assert_eq!(
            boundary
                .publish_catalog(&request)
                .await
                .expect_err("missing idempotency key"),
            StripeCatalogBoundaryError::PublicationPlan(
                StripeCatalogPublicationPlanError::Request(
                    StripeCatalogRequestError::MissingIdempotencyKey,
                ),
            )
        );
    }

    #[tokio::test]
    async fn provider_boundary_rejects_unexpected_reconcile_revision_reference() {
        let boundary = StripeCatalogProviderBoundary::new(
            StripeProviderConfig::pinned("acct_test").expect("config"),
            MismatchedRevisionProvider,
        )
        .expect("boundary");
        let request = StripeCatalogReconcileRequest {
            local_revision: sample_published_catalog(),
            expected_provider_revision_ref: Some("stripe_catalog_expected".into()),
        };
        assert_eq!(
            boundary
                .reconcile_catalog(&request)
                .await
                .expect_err("unexpected provider revision"),
            StripeCatalogBoundaryError::UnexpectedProviderRevisionReference
        );
    }

    #[tokio::test]
    async fn provider_boundary_rejects_publication_snapshot_from_wrong_revision() {
        let config = StripeProviderConfig::pinned("acct_test").expect("config");
        let request = StripeCatalogPublishRequest {
            local_revision: sample_published_catalog(),
            idempotency_key: "catalog-publish-revision-7".into(),
        };
        let boundary = StripeCatalogProviderBoundary::new(config, WrongPublicationRevisionProvider)
            .expect("boundary");

        assert_eq!(
            boundary
                .publish_catalog(&request)
                .await
                .expect_err("wrong provider revision must fail closed"),
            StripeCatalogBoundaryError::PublicationRevisionMismatch
        );
    }

    #[derive(Debug, PartialEq, Eq)]
    enum MockCatalogCall {
        Product {
            item_key: String,
            idempotency_key: String,
        },
        Price {
            product: StripeProductId,
            idempotency_key: String,
        },
        Archive {
            product: StripeProductId,
            idempotency_key: String,
        },
    }

    #[derive(Debug)]
    struct RecordingCatalogHttpClient {
        config: StripeProviderConfig,
        calls: std::sync::Arc<std::sync::Mutex<Vec<MockCatalogCall>>>,
        mismatched_price_product: bool,
        failure: Option<StripeHttpError>,
        price_failure: Option<StripeHttpError>,
        price_failure_item: Option<String>,
        archive_failure: Option<StripeHttpError>,
        products: Vec<crate::StripeProductResponse>,
        prices: Vec<crate::StripePriceResponse>,
    }

    #[async_trait]
    impl StripeCatalogHttpClient for RecordingCatalogHttpClient {
        fn config(&self) -> &StripeProviderConfig {
            &self.config
        }

        async fn list_products(
            &self,
            _request: StripeProductListRequest,
        ) -> Result<
            StripeResponseEnvelope<crate::StripeListResponse<crate::StripeProductResponse>>,
            StripeHttpError,
        > {
            Ok(StripeResponseEnvelope {
                status: 200,
                request_id: Some("req_product_list_test".into()),
                api_version: self.config.api_version().into(),
                body: crate::StripeListResponse {
                    data: self.products.clone(),
                    has_more: false,
                },
            })
        }

        async fn retrieve_product(
            &self,
            _product: crate::StripeProductId,
        ) -> Result<StripeResponseEnvelope<crate::StripeProductResponse>, StripeHttpError> {
            Err(StripeHttpError::Transport)
        }

        async fn list_prices(
            &self,
            request: StripePriceListRequest,
        ) -> Result<
            StripeResponseEnvelope<crate::StripeListResponse<crate::StripePriceResponse>>,
            StripeHttpError,
        > {
            Ok(StripeResponseEnvelope {
                status: 200,
                request_id: Some("req_price_list_test".into()),
                api_version: self.config.api_version().into(),
                body: crate::StripeListResponse {
                    data: self
                        .prices
                        .iter()
                        .filter(|price| {
                            request
                                .product
                                .as_ref()
                                .is_none_or(|product| &price.product == product)
                        })
                        .cloned()
                        .collect(),
                    has_more: false,
                },
            })
        }

        async fn retrieve_price(
            &self,
            _price: crate::StripePriceId,
        ) -> Result<StripeResponseEnvelope<crate::StripePriceResponse>, StripeHttpError> {
            Err(StripeHttpError::Transport)
        }

        async fn create_product(
            &self,
            request: StripeProductCreateRequest,
            idempotency_key: &str,
        ) -> Result<StripeResponseEnvelope<crate::StripeProductResponse>, StripeHttpError> {
            if let Some(error) = &self.failure {
                return Err(error.clone());
            }
            self.calls
                .lock()
                .expect("catalog call lock")
                .push(MockCatalogCall::Product {
                    item_key: request
                        .metadata
                        .get("erpx_catalog_item")
                        .cloned()
                        .expect("catalog item metadata"),
                    idempotency_key: idempotency_key.into(),
                });
            let product_id = match request
                .metadata
                .get("erpx_catalog_item")
                .map(String::as_str)
            {
                Some("module") => StripeProductId::try_from("prod_module").expect("product id"),
                _ => StripeProductId::try_from("prod_base").expect("product id"),
            };
            Ok(StripeResponseEnvelope {
                status: 200,
                request_id: Some("req_product_test".into()),
                api_version: self.config.api_version().into(),
                body: crate::StripeProductResponse {
                    id: product_id,
                    name: request.name,
                    active: true,
                    metadata: request.metadata,
                },
            })
        }

        async fn create_price(
            &self,
            request: StripePriceCreateRequest,
            idempotency_key: &str,
        ) -> Result<StripeResponseEnvelope<crate::StripePriceResponse>, StripeHttpError> {
            let item_key = request.metadata.get("erpx_catalog_item");
            if self
                .price_failure_item
                .as_deref()
                .is_none_or(|expected| item_key.is_some_and(|actual| actual == expected))
                && let Some(error) = &self.price_failure
            {
                return Err(error.clone());
            }
            if let Some(error) = &self.failure {
                return Err(error.clone());
            }
            self.calls
                .lock()
                .expect("catalog call lock")
                .push(MockCatalogCall::Price {
                    product: request.product.clone(),
                    idempotency_key: idempotency_key.into(),
                });
            let response_product = if self.mismatched_price_product {
                StripeProductId::try_from("prod_other").expect("product id")
            } else {
                request.product
            };
            let price_id = match item_key.map(String::as_str) {
                Some("module") => StripePriceId::try_from("price_module").expect("price id"),
                _ => StripePriceId::try_from("price_base").expect("price id"),
            };
            Ok(StripeResponseEnvelope {
                status: 200,
                request_id: Some("req_price_test".into()),
                api_version: self.config.api_version().into(),
                body: crate::StripePriceResponse {
                    id: price_id,
                    product: response_product,
                    active: true,
                    currency: "thb".into(),
                    unit_amount: Some(1_000),
                    recurring: request.recurring.map(|recurring| {
                        crate::StripePriceRecurringResponse {
                            interval: recurring.interval,
                            usage_type: recurring.usage_type,
                        }
                    }),
                    metadata: request.metadata,
                },
            })
        }

        async fn archive_product(
            &self,
            product: StripeProductId,
            idempotency_key: &str,
        ) -> Result<StripeResponseEnvelope<crate::StripeProductResponse>, StripeHttpError> {
            if let Some(error) = &self.archive_failure {
                return Err(error.clone());
            }
            self.calls
                .lock()
                .expect("catalog call lock")
                .push(MockCatalogCall::Archive {
                    product: product.clone(),
                    idempotency_key: idempotency_key.into(),
                });
            Ok(StripeResponseEnvelope {
                status: 200,
                request_id: Some("req_archive_test".into()),
                api_version: self.config.api_version().into(),
                body: crate::StripeProductResponse {
                    id: product,
                    name: "Base".into(),
                    active: false,
                    metadata: BTreeMap::new(),
                },
            })
        }
    }

    fn recording_catalog_client(
        config: StripeProviderConfig,
    ) -> (
        RecordingCatalogHttpClient,
        std::sync::Arc<std::sync::Mutex<Vec<MockCatalogCall>>>,
    ) {
        let calls = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
        (
            RecordingCatalogHttpClient {
                config,
                calls: calls.clone(),
                mismatched_price_product: false,
                failure: None,
                price_failure: None,
                price_failure_item: None,
                archive_failure: None,
                products: Vec::new(),
                prices: Vec::new(),
            },
            calls,
        )
    }

    #[tokio::test]
    async fn live_catalog_provider_executes_product_then_price_with_bound_keys() {
        let config = StripeProviderConfig::pinned("acct_test").expect("config");
        let request = StripeCatalogPublishRequest {
            local_revision: sample_published_catalog(),
            idempotency_key: "caller-request-1".into(),
        };
        let plan = StripeCatalogPublicationPlan::build(&config, &request).expect("plan");
        let (client, calls) = recording_catalog_client(config.clone());
        let provider = StripeHttpCatalogProvider::new(client);
        assert_eq!(provider.client().config(), &config);
        let boundary = StripeCatalogProviderBoundary::new(config, provider).expect("boundary");

        let result = boundary.publish_catalog(&request).await.expect("publish");
        assert!(result.reconciliation.is_in_sync());
        assert_eq!(
            result.snapshot.identifiers["base"].product.as_str(),
            "prod_base"
        );
        assert_eq!(
            result.snapshot.identifiers["base"].price.as_str(),
            "price_base"
        );
        assert_eq!(
            result.snapshot.revision.provider_revision_ref,
            format!("stripe_catalog_{}", plan.binding().provider_key)
        );

        assert_eq!(
            *calls.lock().expect("catalog call lock"),
            vec![
                MockCatalogCall::Product {
                    item_key: "base".into(),
                    idempotency_key: plan.steps()[0].product_idempotency_key.clone(),
                },
                MockCatalogCall::Price {
                    product: StripeProductId::try_from("prod_base").expect("product id"),
                    idempotency_key: plan.steps()[0].price_idempotency_key.clone(),
                },
            ]
        );
    }

    #[tokio::test]
    async fn live_catalog_provider_compensates_product_when_price_creation_fails() {
        let config = StripeProviderConfig::pinned("acct_test").expect("config");
        let request = StripeCatalogPublishRequest {
            local_revision: sample_published_catalog(),
            idempotency_key: "caller-request-compensation".into(),
        };
        let (mut client, calls) = recording_catalog_client(config.clone());
        client.price_failure = Some(StripeHttpError::Provider(
            StripeProviderError::from_remote_response(
                503,
                Some("temporarily_unavailable"),
                Some("req_price_failure"),
                r#"{"error":{"type":"api_error","code":"temporarily_unavailable"}}"#,
            ),
        ));
        let provider = StripeHttpCatalogProvider::new(client);
        let boundary = StripeCatalogProviderBoundary::new(config, provider).expect("boundary");

        let error = boundary
            .publish_catalog(&request)
            .await
            .expect_err("price failure must not publish a partial catalog");
        assert!(matches!(
            error,
            StripeCatalogBoundaryError::Provider(StripeProviderError {
                kind: StripeProviderErrorKind::Unavailable,
                ..
            })
        ));
        let calls = calls.lock().expect("catalog call lock");
        assert!(matches!(
            calls.as_slice(),
            [
                MockCatalogCall::Product { .. },
                MockCatalogCall::Archive { .. }
            ]
        ));
        assert!(
            !calls
                .iter()
                .any(|call| matches!(call, MockCatalogCall::Price { .. }))
        );
    }

    #[tokio::test]
    async fn live_catalog_provider_rolls_back_prior_products_when_later_price_fails() {
        let config = StripeProviderConfig::pinned("acct_test").expect("config");
        let request = StripeCatalogPublishRequest {
            local_revision: sample_two_item_catalog(),
            idempotency_key: "caller-request-multi-item-rollback".into(),
        };
        let (mut client, calls) = recording_catalog_client(config.clone());
        client.price_failure = Some(StripeHttpError::Provider(
            StripeProviderError::from_remote_response(
                503,
                Some("temporarily_unavailable"),
                Some("req_second_price_failure"),
                r#"{"error":{"type":"api_error","code":"temporarily_unavailable"}}"#,
            ),
        ));
        client.price_failure_item = Some("module".into());
        let plan = StripeCatalogPublicationPlan::build(&config, &request).expect("plan");
        let boundary =
            StripeCatalogProviderBoundary::new(config, StripeHttpCatalogProvider::new(client))
                .expect("boundary");

        let error = boundary
            .publish_catalog(&request)
            .await
            .expect_err("later price failure must roll back the publication");
        assert!(matches!(
            error,
            StripeCatalogBoundaryError::Provider(StripeProviderError {
                kind: StripeProviderErrorKind::Unavailable,
                ..
            })
        ));
        let calls = calls.lock().expect("catalog call lock");
        assert_eq!(
            calls.as_slice(),
            [
                MockCatalogCall::Product {
                    item_key: "base".into(),
                    idempotency_key: plan.steps()[0].product_idempotency_key.clone(),
                },
                MockCatalogCall::Price {
                    product: StripeProductId::try_from("prod_base").expect("product id"),
                    idempotency_key: plan.steps()[0].price_idempotency_key.clone(),
                },
                MockCatalogCall::Product {
                    item_key: "module".into(),
                    idempotency_key: plan.steps()[1].product_idempotency_key.clone(),
                },
                MockCatalogCall::Archive {
                    product: StripeProductId::try_from("prod_module").expect("product id"),
                    idempotency_key: catalog_product_rollback_key(
                        &plan.binding().provider_key,
                        &StripeProductId::try_from("prod_module").expect("product id"),
                    ),
                },
                MockCatalogCall::Archive {
                    product: StripeProductId::try_from("prod_base").expect("product id"),
                    idempotency_key: catalog_product_rollback_key(
                        &plan.binding().provider_key,
                        &StripeProductId::try_from("prod_base").expect("product id"),
                    ),
                },
            ]
        );
    }

    #[tokio::test]
    async fn live_catalog_provider_rejects_mismatched_price_product() {
        let config = StripeProviderConfig::pinned("acct_test").expect("config");
        let request = StripeCatalogPublishRequest {
            local_revision: sample_published_catalog(),
            idempotency_key: "caller-request-1".into(),
        };
        let (mut client, calls) = recording_catalog_client(config.clone());
        client.mismatched_price_product = true;
        let provider = StripeHttpCatalogProvider::new(client);
        let boundary = StripeCatalogProviderBoundary::new(config, provider).expect("boundary");

        let error = boundary
            .publish_catalog(&request)
            .await
            .expect_err("mismatched provider mapping");
        assert_eq!(
            error,
            StripeCatalogBoundaryError::Provider(local_catalog_failure(
                StripeProviderErrorKind::InvalidRequest,
            ))
        );
        assert!(matches!(
            calls.lock().expect("catalog call lock").as_slice(),
            [
                MockCatalogCall::Product { .. },
                MockCatalogCall::Price { .. },
                MockCatalogCall::Archive { .. }
            ]
        ));
    }

    #[tokio::test]
    async fn live_catalog_provider_reconciles_bounded_remote_metadata() {
        let config = StripeProviderConfig::pinned("acct_test").expect("config");
        let local = sample_published_catalog();
        let provider_ref = "stripe_catalog_7";
        let product = crate::StripeProductResponse {
            id: StripeProductId::try_from("prod_base").expect("product id"),
            name: "Base".into(),
            active: true,
            metadata: BTreeMap::from([
                ("erpx_catalog_revision".into(), "7".into()),
                ("erpx_catalog_item".into(), "base".into()),
                ("erpx_catalog_provider_ref".into(), provider_ref.into()),
                ("erpx_pricing_unit".into(), "flat".into()),
            ]),
        };
        let price = crate::StripePriceResponse {
            id: StripePriceId::try_from("price_base").expect("price id"),
            product: product.id.clone(),
            active: true,
            currency: "thb".into(),
            unit_amount: Some(1_000),
            recurring: Some(crate::StripePriceRecurringResponse {
                interval: StripePriceInterval::Month,
                usage_type: StripePriceUsageType::Licensed,
            }),
            metadata: BTreeMap::from([
                ("erpx_catalog_revision".into(), "7".into()),
                ("erpx_catalog_item".into(), "base".into()),
                ("erpx_catalog_provider_ref".into(), provider_ref.into()),
                ("erpx_price_kind".into(), "base_fee".into()),
            ]),
        };
        let (mut client, _calls) = recording_catalog_client(config.clone());
        client.products = vec![product];
        client.prices = vec![price];
        let boundary =
            StripeCatalogProviderBoundary::new(config, StripeHttpCatalogProvider::new(client))
                .expect("boundary");
        let result = boundary
            .reconcile_catalog(&StripeCatalogReconcileRequest {
                local_revision: local,
                expected_provider_revision_ref: None,
            })
            .await
            .expect("reconcile");
        assert!(result.reconciliation.is_in_sync());
        assert_eq!(result.snapshot.revision.provider_revision_ref, provider_ref);
        assert_eq!(
            result.snapshot.identifiers["base"].price.as_str(),
            "price_base"
        );
    }

    #[tokio::test]
    async fn live_catalog_provider_ignores_unmanaged_active_prices() {
        let config = StripeProviderConfig::pinned("acct_test").expect("config");
        let local = sample_published_catalog();
        let provider_ref = "stripe_catalog_7";
        let product = crate::StripeProductResponse {
            id: StripeProductId::try_from("prod_base").expect("product id"),
            name: "Base".into(),
            active: true,
            metadata: BTreeMap::from([
                ("erpx_catalog_revision".into(), "7".into()),
                ("erpx_catalog_item".into(), "base".into()),
                ("erpx_catalog_provider_ref".into(), provider_ref.into()),
            ]),
        };
        let unmanaged_price = crate::StripePriceResponse {
            id: StripePriceId::try_from("price_external").expect("price id"),
            product: product.id.clone(),
            active: true,
            currency: "thb".into(),
            unit_amount: Some(1_000),
            recurring: None,
            metadata: BTreeMap::new(),
        };
        let (mut client, _calls) = recording_catalog_client(config.clone());
        client.products = vec![product];
        client.prices = vec![unmanaged_price];
        let boundary =
            StripeCatalogProviderBoundary::new(config, StripeHttpCatalogProvider::new(client))
                .expect("boundary");
        let result = boundary
            .reconcile_catalog(&StripeCatalogReconcileRequest {
                local_revision: local,
                expected_provider_revision_ref: None,
            })
            .await
            .expect("reconcile unmanaged price");
        assert!(!result.reconciliation.is_in_sync());
        assert!(result.snapshot.identifiers.is_empty());
    }

    #[tokio::test]
    async fn live_catalog_provider_preserves_redacted_provider_errors() {
        let config = StripeProviderConfig::pinned("acct_test").expect("config");
        let request = StripeCatalogPublishRequest {
            local_revision: sample_published_catalog(),
            idempotency_key: "caller-request-1".into(),
        };
        let raw = r#"{"error":{"type":"invalid_request_error","code":"bad_request","message":"sk_live_never_log"}}"#;
        let provider_error = StripeProviderError::from_remote_response(
            400,
            Some("sk_live_provider_secret"),
            Some("req_redacted"),
            raw,
        );
        let (mut client, _calls) = recording_catalog_client(config.clone());
        client.failure = Some(StripeHttpError::Provider(provider_error));
        let boundary =
            StripeCatalogProviderBoundary::new(config, StripeHttpCatalogProvider::new(client))
                .expect("boundary");

        let error = boundary
            .publish_catalog(&request)
            .await
            .expect_err("provider error");
        let rendered = error.to_string();
        let debug = format!("{error:?}");
        assert!(!rendered.contains("sk_live"));
        assert!(!debug.contains("sk_live"));
        assert!(rendered.contains("bad_request"));
    }

    fn semantic_claim(event_type: &str, object: Value) -> StripeInboxClaim {
        StripeInboxClaim {
            event_id: format!("evt_{}", Uuid::new_v4()),
            event_type: event_type.into(),
            stripe_created_at: Some(Utc::now()),
            payload: serde_json::json!({"data": {"object": object}}),
            attempts: 1,
            claimed_by: Uuid::new_v4(),
            claim_expires_at: Utc::now() + Duration::minutes(5),
        }
    }

    fn billing_metadata() -> Value {
        serde_json::json!({
            "erpx_customer_id": Uuid::new_v4().to_string(),
            "erpx_subscription_id": Uuid::new_v4().to_string()
        })
    }

    #[test]
    fn semantic_mapping_is_explicit_and_does_not_copy_payload_secrets() {
        let metadata = billing_metadata();
        let claim = semantic_claim(
            "invoice.payment_failed",
            serde_json::json!({
                "metadata": metadata,
                "payment_intent": "pi_secret_must_not_cross_boundary"
            }),
        );
        let event = StripeSemanticDispatcher::decode(&claim).expect("semantic event");
        assert!(matches!(
            event,
            StripeSemanticEvent::Billing(StripeBillingTransition {
                transition: BillingEvent::PaymentFailed,
                ..
            })
        ));
        assert!(!format!("{event:?}").contains("pi_secret_must_not_cross_boundary"));

        let active = semantic_claim(
            "customer.subscription.updated",
            serde_json::json!({
                "metadata": billing_metadata(),
                "status": "active"
            }),
        );
        assert!(matches!(
            StripeSemanticDispatcher::decode(&active).expect("active event"),
            StripeSemanticEvent::Billing(StripeBillingTransition {
                transition: BillingEvent::TrialEnded,
                ..
            })
        ));

        let entitlement = semantic_claim(
            "entitlements.active_entitlement_summary.updated",
            serde_json::json!({
                "metadata": {
                    "erpx_customer_id": Uuid::new_v4().to_string(),
                    "erpx_module_key": "finance"
                },
                "seats": 20,
                "companies": 3,
                "branches": 12,
                "active": true
            }),
        );
        assert!(matches!(
            StripeSemanticDispatcher::decode(&entitlement).expect("entitlement event"),
            StripeSemanticEvent::Entitlement(StripeEntitlementUpdate {
                capacity: EntitlementCapacity {
                    seats: 20,
                    companies: 3,
                    branches: 12
                },
                active: true,
                ..
            })
        ));

        let checkout = semantic_claim(
            "checkout.session.completed",
            serde_json::json!({
                "customer": "cus_verified",
                "subscription": "sub_verified",
                "metadata": {
                    "erpx_holding_id": "holding-a",
                    "erpx_customer_id": Uuid::new_v4().to_string(),
                    "erpx_subscription_id": Uuid::new_v4().to_string(),
                    "erpx_currency": "USD",
                    "erpx_items": "{\"holding-base-monthly\":1}"
                }
            }),
        );
        assert!(matches!(
            StripeSemanticDispatcher::decode(&checkout).expect("Checkout binding"),
            StripeSemanticEvent::Checkout(StripeCheckoutBinding {
                holding_id,
                stripe_customer_ref,
                stripe_subscription_ref,
                currency,
                ..
            }) if holding_id == "holding-a"
                && stripe_customer_ref == "cus_verified"
                && stripe_subscription_ref == "sub_verified"
                && currency == "USD"
        ));
    }

    #[test]
    fn accepted_non_mutating_events_are_noops_and_unknown_events_are_rejected() {
        let trial_reminder = semantic_claim(
            "customer.subscription.trial_will_end",
            serde_json::json!({"secret": "not inspected"}),
        );
        assert!(matches!(
            StripeSemanticDispatcher::decode(&trial_reminder).expect("trial reminder"),
            StripeSemanticEvent::NoOp(StripeSemanticNoOp {
                reason,
                ..
            }) if reason == "trial reminder"
        ));

        let unknown = semantic_claim("charge.refunded", serde_json::json!({}));
        let error = StripeSemanticDispatcher::decode(&unknown).expect_err("unknown event");
        assert_eq!(
            error,
            StripeSemanticParseError::UnsupportedEventType("charge.refunded".into())
        );
    }

    #[derive(Debug, Default)]
    struct RecordingSemanticApplier {
        events: std::sync::Mutex<Vec<StripeSemanticEvent>>,
    }

    #[async_trait]
    impl StripeSemanticApplier for RecordingSemanticApplier {
        async fn apply_stripe_semantic_event(
            &self,
            event: &StripeSemanticEvent,
        ) -> Result<StripeSemanticApplyResult, StripeSemanticApplyError> {
            self.events
                .lock()
                .expect("recording lock")
                .push(event.clone());
            Ok(StripeSemanticApplyResult::Applied)
        }
    }

    #[tokio::test]
    async fn dispatcher_uses_injected_applier_and_acknowledges_noops_without_applying() {
        let applier = RecordingSemanticApplier::default();
        let claim = semantic_claim(
            "invoice.paid",
            serde_json::json!({"metadata": billing_metadata()}),
        );
        assert_eq!(
            StripeSemanticDispatcher::dispatch(&applier, &claim)
                .await
                .expect("dispatch"),
            StripeSemanticApplyResult::Applied
        );
        assert_eq!(applier.events.lock().expect("recording lock").len(), 1);

        let no_op = semantic_claim(
            "customer.subscription.trial_will_end",
            serde_json::json!({}),
        );
        assert_eq!(
            StripeSemanticDispatcher::dispatch(&applier, &no_op)
                .await
                .expect("dispatch no-op"),
            StripeSemanticApplyResult::NoOp
        );
        assert_eq!(applier.events.lock().expect("recording lock").len(), 1);
    }

    #[test]
    fn malformed_semantic_event_is_safe_for_dead_letter_and_contains_no_payload() {
        let claim = semantic_claim(
            "invoice.paid",
            serde_json::json!({"metadata": {"erpx_customer_id": "not-a-uuid"}, "token": "secret"}),
        );
        let error = StripeSemanticDispatcher::decode(&claim).expect_err("malformed event");
        assert_eq!(
            error,
            StripeSemanticParseError::InvalidUuid("erpx_customer_id")
        );
        assert!(!error.to_string().contains("secret"));
    }

    #[tokio::test]
    #[ignore = "requires one disposable PostgreSQL control write database"]
    async fn semantic_application_is_idempotent_and_ignores_older_provider_events() {
        let config = crate::ControlPlaneDatabaseConfig {
            write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
            read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
        };
        let pools = crate::ControlPlanePools::connect(&config)
            .await
            .expect("control pools");
        erpx_migration::migrate_control_plane(&pools.write)
            .await
            .expect("write migration");
        let control = crate::SqlxControlPlane::new(pools);
        let store = SqlxCommercialStore::new(&control);
        let customer_id = Uuid::new_v4();
        let subscription_id = Uuid::new_v4();
        let base = Utc::now();

        let failed_claim = semantic_claim(
            "invoice.payment_failed",
            serde_json::json!({
                "metadata": {
                    "erpx_customer_id": customer_id.to_string(),
                    "erpx_subscription_id": subscription_id.to_string()
                }
            }),
        );
        let failed = StripeBillingTransition {
            event_id: failed_claim.event_id.clone(),
            event_type: failed_claim.event_type.clone(),
            created_at: base + Duration::seconds(2),
            customer_id,
            subscription_id,
            transition: BillingEvent::PaymentFailed,
            invoice: None,
        };
        assert_eq!(
            store
                .apply_stripe_semantic_event(&StripeSemanticEvent::Billing(failed.clone()))
                .await
                .expect("failed payment apply"),
            StripeSemanticApplyResult::Applied
        );
        let lifecycle_row = sqlx::query_as::<_, (i64, String)>(
            "SELECT version, state
             FROM erpx_control.billing_lifecycles
             WHERE subscription_id = $1",
        )
        .bind(subscription_id)
        .fetch_one(&control.pools().write)
        .await
        .expect("billing lifecycle authority");
        assert_eq!(lifecycle_row.0, 1);
        assert_eq!(lifecycle_row.1, "past_due");
        let evidence = sqlx::query_as::<_, (String, i64, Value)>(
            "SELECT event_type, aggregate_version, payload
             FROM erpx_control.outbox_events
             WHERE aggregate_type = $1 AND aggregate_id = $2
             ORDER BY aggregate_version DESC",
        )
        .bind(BILLING_LIFECYCLE_AGGREGATE_TYPE)
        .bind(subscription_id)
        .fetch_all(&control.pools().write)
        .await
        .expect("billing evidence event");
        assert_eq!(evidence.len(), 1);
        assert_eq!(evidence[0].0, BILLING_PROVIDER_EVENT_APPLIED_TYPE);
        assert_eq!(evidence[0].1, 1);
        assert_eq!(
            evidence[0].2["provider_event_id"],
            failed_claim.event_id.as_str()
        );
        assert_eq!(
            store
                .apply_stripe_semantic_event(&StripeSemanticEvent::Billing(failed))
                .await
                .expect("duplicate apply"),
            StripeSemanticApplyResult::DuplicateOrOutOfOrder
        );

        let older_recovery = StripeBillingTransition {
            event_id: "evt_older_recovery".into(),
            event_type: "invoice.paid".into(),
            created_at: base + Duration::seconds(1),
            customer_id,
            subscription_id,
            transition: BillingEvent::PaymentRecovered,
            invoice: None,
        };
        assert_eq!(
            store
                .apply_stripe_semantic_event(&StripeSemanticEvent::Billing(older_recovery))
                .await
                .expect("out-of-order apply"),
            StripeSemanticApplyResult::DuplicateOrOutOfOrder
        );
        assert_eq!(
            store
                .get_billing_lifecycle(customer_id, subscription_id)
                .await
                .expect("lifecycle")
                .expect("lifecycle row")
                .0
                .state,
            BillingState::PastDue
        );

        let entitlement = StripeEntitlementUpdate {
            event_id: "evt_entitlement_1".into(),
            event_type: "entitlements.active_entitlement_summary.updated".into(),
            created_at: base + Duration::seconds(3),
            customer_id,
            module_key: "finance".into(),
            capacity: EntitlementCapacity {
                seats: 10,
                companies: 2,
                branches: 4,
            },
            active: true,
            active_until: None,
        };
        assert_eq!(
            store
                .apply_stripe_semantic_event(&StripeSemanticEvent::Entitlement(entitlement.clone()))
                .await
                .expect("entitlement apply"),
            StripeSemanticApplyResult::Applied
        );
        let entitlement_aggregate = entitlement_aggregate_type("finance");
        let entitlement_row = sqlx::query_as::<_, (i64, Value)>(
            "SELECT version, payload
             FROM erpx_control.entitlement_allocation_books
             WHERE customer_id = $1 AND module_key = 'finance'",
        )
        .bind(customer_id)
        .fetch_one(&control.pools().write)
        .await
        .expect("entitlement allocation authority");
        assert_eq!(entitlement_row.0, 1);
        assert_eq!(entitlement_row.1["module_key"], "finance");
        assert_eq!(entitlement_row.1["active"], true);
        assert_eq!(entitlement_row.1["capacity"]["seats"], 10);
        let entitlement_evidence = sqlx::query_as::<_, (String, i64, Value)>(
            "SELECT event_type, aggregate_version, payload
             FROM erpx_control.outbox_events
             WHERE aggregate_type = $1 AND aggregate_id = $2",
        )
        .bind(&entitlement_aggregate)
        .bind(customer_id)
        .fetch_all(&control.pools().write)
        .await
        .expect("entitlement evidence event");
        assert_eq!(entitlement_evidence.len(), 1);
        assert_eq!(
            entitlement_evidence[0].0,
            ENTITLEMENT_PROVIDER_EVENT_APPLIED_TYPE
        );
        assert_eq!(entitlement_evidence[0].1, 1);
        assert_eq!(
            entitlement_evidence[0].2["provider_event_id"],
            "evt_entitlement_1"
        );
        assert_eq!(entitlement_evidence[0].2["capacity"]["branches"], 4);
        assert_eq!(
            store
                .apply_stripe_semantic_event(&StripeSemanticEvent::Entitlement(entitlement))
                .await
                .expect("duplicate entitlement apply"),
            StripeSemanticApplyResult::DuplicateOrOutOfOrder
        );
        assert_eq!(
            store
                .get_entitlement_book(customer_id, "finance")
                .await
                .expect("entitlement")
                .expect("entitlement row")
                .0
                .capacity,
            EntitlementCapacity {
                seats: 10,
                companies: 2,
                branches: 4,
            }
        );
    }

    #[tokio::test]
    #[ignore = "requires one disposable PostgreSQL control write database"]
    async fn provider_entitlement_evidence_rolls_back_book_and_outbox_together() {
        let config = crate::ControlPlaneDatabaseConfig {
            write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
            read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
        };
        let pools = crate::ControlPlanePools::connect(&config)
            .await
            .expect("control pools");
        erpx_migration::migrate_control_plane(&pools.write)
            .await
            .expect("write migration");
        let control = crate::SqlxControlPlane::new(pools);
        let store = SqlxCommercialStore::new(&control);
        let customer_id = Uuid::new_v4();
        let event_id = format!("evt_entitlement_rollback_{}", Uuid::new_v4());
        let at = Utc::now();
        let entitlement_aggregate = entitlement_aggregate_type("finance");
        let context =
            CommercialCommandContext::for_provider("stripe", &event_id).expect("provider context");
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, correlation_id, idempotency_key, payload,
                 occurred_at, available_at)
             VALUES ($1, $2, 1, $3, $4, 99, $5, $6, '{}'::jsonb, $7, $7)",
        )
        .bind(Uuid::new_v4())
        .bind("test.entitlement.conflict")
        .bind(&entitlement_aggregate)
        .bind(customer_id)
        .bind(context.correlation_id)
        .bind(&context.idempotency_key)
        .bind(at)
        .execute(&control.pools().write)
        .await
        .expect("seed outbox conflict");

        let result = store
            .apply_provider_entitlement_update(
                customer_id,
                context.clone(),
                ProviderEntitlementUpdate {
                    provider_event_id: event_id,
                    created_at: at,
                    module_key: "finance".into(),
                    capacity: EntitlementCapacity {
                        seats: 10,
                        companies: 2,
                        branches: 4,
                    },
                    active: true,
                    active_until: None,
                },
            )
            .await;
        assert!(result.is_err(), "outbox conflict must abort the command");
        assert!(
            store
                .get_entitlement_book(customer_id, "finance")
                .await
                .expect("entitlement lookup")
                .is_none()
        );
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*)
             FROM erpx_control.audit_events
             WHERE customer_account_id = $1
               AND action = 'commercial.entitlement.provider_snapshot_applied'",
        )
        .bind(customer_id)
        .fetch_one(&control.pools().write)
        .await
        .expect("audit count");
        assert_eq!(audit_count, 0);
        let idempotency_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_control.idempotency_keys WHERE idempotency_key = $1",
        )
        .bind(&context.idempotency_key)
        .fetch_one(&control.pools().write)
        .await
        .expect("idempotency count");
        assert_eq!(idempotency_count, 0);
    }

    #[tokio::test]
    #[ignore = "requires one disposable PostgreSQL control write database"]
    async fn entitlement_allocation_and_activation_commands_are_atomic_versioned_and_replay_safe() {
        let config = crate::ControlPlaneDatabaseConfig {
            write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
            read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
        };
        let pools = crate::ControlPlanePools::connect(&config)
            .await
            .expect("control pools");
        erpx_migration::migrate_control_plane(&pools.write)
            .await
            .expect("write migration");
        let control = crate::SqlxControlPlane::new(pools);
        let store = SqlxCommercialStore::new(&control);
        let customer_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let branch_id = Uuid::new_v4();
        let capacity = EntitlementCapacity {
            seats: 10,
            companies: 2,
            branches: 4,
        };
        let mut book = EntitlementAllocationBook::new("finance", capacity, None);
        book.set_active(true, None);
        assert_eq!(
            store
                .upsert_entitlement_book(customer_id, &book, None)
                .await
                .expect("seed entitlement book"),
            1
        );

        let allocation_context = CommercialCommandContext::new(
            Uuid::new_v4(),
            format!("commercial-test-allocate-{}", Uuid::new_v4()),
            Uuid::new_v4(),
            "test-commercial-admin",
            "allocate entitlement company",
        )
        .expect("allocation context");
        let allocation = store
            .allocate_entitlement_company_with_expected_version(
                AllocateCompanyCommand {
                    customer_id,
                    module_key: "finance".into(),
                    company_id,
                    seats: 3,
                    branch_limit: 2,
                    context: allocation_context.clone(),
                },
                Some(1),
            )
            .await
            .expect("allocate company");
        assert_eq!(allocation.aggregate_version, 2);
        assert!(!allocation.replayed);

        let replay = store
            .allocate_entitlement_company_with_expected_version(
                AllocateCompanyCommand {
                    customer_id,
                    module_key: "finance".into(),
                    company_id,
                    seats: 3,
                    branch_limit: 2,
                    context: allocation_context,
                },
                Some(1),
            )
            .await
            .expect("replay allocation");
        assert!(replay.replayed);
        assert_eq!(replay.aggregate_version, 2);

        let company_context = CommercialCommandContext::new(
            Uuid::new_v4(),
            format!("commercial-test-company-activate-{}", Uuid::new_v4()),
            Uuid::new_v4(),
            "test-commercial-admin",
            "activate entitlement company",
        )
        .expect("company activation context");
        let company_activation = store
            .activate_entitlement_company_with_expected_version(
                ActivateCompanyModuleCommand {
                    customer_id,
                    module_key: "finance".into(),
                    company_id,
                    at: Utc::now(),
                    context: company_context,
                },
                Some(2),
            )
            .await
            .expect("activate company");
        assert_eq!(company_activation.aggregate_version, 3);

        let branch_context = CommercialCommandContext::new(
            Uuid::new_v4(),
            format!("commercial-test-branch-activate-{}", Uuid::new_v4()),
            Uuid::new_v4(),
            "test-commercial-admin",
            "activate entitlement branch",
        )
        .expect("branch activation context");
        let branch_activation = store
            .activate_entitlement_branch_with_expected_version(
                ActivateBranchModuleCommand {
                    customer_id,
                    module_key: "finance".into(),
                    company_id,
                    branch_id,
                    at: Utc::now(),
                    context: branch_context,
                },
                Some(3),
            )
            .await
            .expect("activate branch");
        assert_eq!(branch_activation.aggregate_version, 4);

        let persisted = store
            .get_entitlement_book(customer_id, "finance")
            .await
            .expect("read entitlement book")
            .expect("persisted entitlement book");
        assert_eq!(persisted.1, 4);
        assert!(
            persisted
                .0
                .allocation(company_id)
                .is_some_and(|allocation| {
                    allocation.active && allocation.active_branches.contains(&branch_id)
                })
        );

        let events = sqlx::query_as::<_, (i64, String)>(
            "SELECT aggregate_version, event_type
             FROM erpx_control.outbox_events
             WHERE aggregate_type = $1 AND aggregate_id = $2
             ORDER BY aggregate_version",
        )
        .bind(entitlement_aggregate_type("finance"))
        .bind(customer_id)
        .fetch_all(&control.pools().write)
        .await
        .expect("entitlement command events");
        assert_eq!(events.len(), 4);
        assert_eq!(events[1], (2, ENTITLEMENT_COMPANY_ALLOCATED_TYPE.into()));
        assert_eq!(events[2], (3, ENTITLEMENT_COMPANY_ACTIVATED_TYPE.into()));
        assert_eq!(events[3], (4, ENTITLEMENT_BRANCH_ACTIVATED_TYPE.into()));

        let stale_context = CommercialCommandContext::new(
            Uuid::new_v4(),
            format!("commercial-test-stale-{}", Uuid::new_v4()),
            Uuid::new_v4(),
            "test-commercial-admin",
            "stale entitlement command",
        )
        .expect("stale context");
        let stale = store
            .activate_entitlement_branch_with_expected_version(
                ActivateBranchModuleCommand {
                    customer_id,
                    module_key: "finance".into(),
                    company_id,
                    branch_id: Uuid::new_v4(),
                    at: Utc::now(),
                    context: stale_context,
                },
                Some(3),
            )
            .await;
        assert!(matches!(
            stale,
            Err(PersistenceError::OptimisticConcurrency {
                actual: Some(4),
                ..
            })
        ));
        assert_eq!(
            store
                .get_entitlement_book(customer_id, "finance")
                .await
                .expect("re-read entitlement book")
                .expect("entitlement book")
                .1,
            4
        );
    }

    #[tokio::test]
    #[ignore = "requires one disposable PostgreSQL control write database"]
    async fn provider_billing_evidence_rolls_back_lifecycle_and_outbox_together() {
        let config = crate::ControlPlaneDatabaseConfig {
            write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
            read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
        };
        let pools = crate::ControlPlanePools::connect(&config)
            .await
            .expect("control pools");
        erpx_migration::migrate_control_plane(&pools.write)
            .await
            .expect("write migration");
        let control = crate::SqlxControlPlane::new(pools);
        let store = SqlxCommercialStore::new(&control);
        let customer_id = Uuid::new_v4();
        let subscription_id = Uuid::new_v4();
        let provider_event_id = format!("evt_rollback_{}", Uuid::new_v4());
        let created_at = Utc::now();
        let context = CommercialCommandContext::for_provider("stripe", &provider_event_id)
            .expect("provider context");
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, correlation_id, idempotency_key, payload,
                 occurred_at, available_at)
             VALUES ($1, $2, 1, $3, $4, 99, $5, $6, '{}'::jsonb, $7, $7)",
        )
        .bind(Uuid::new_v4())
        .bind("test.billing.conflict")
        .bind(BILLING_LIFECYCLE_AGGREGATE_TYPE)
        .bind(subscription_id)
        .bind(context.correlation_id)
        .bind(&context.idempotency_key)
        .bind(created_at)
        .execute(&control.pools().write)
        .await
        .expect("seed outbox conflict");

        let result = store
            .apply_provider_billing_event(
                BillingPolicyCommand {
                    customer_id,
                    subscription_id,
                    at: created_at,
                    context: context.clone(),
                },
                ProviderBillingEvent {
                    provider_event_id,
                    created_at,
                    event: BillingEvent::PaymentFailed,
                },
            )
            .await;
        assert!(result.is_err(), "outbox conflict must abort the command");
        assert!(
            store
                .get_billing_lifecycle(customer_id, subscription_id)
                .await
                .expect("lifecycle lookup")
                .is_none()
        );
        let audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*)
             FROM erpx_control.audit_events
             WHERE customer_account_id = $1
               AND action = 'commercial.billing.provider_event_applied'",
        )
        .bind(customer_id)
        .fetch_one(&control.pools().write)
        .await
        .expect("audit count");
        assert_eq!(audit_count, 0);
        let idempotency_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_control.idempotency_keys WHERE idempotency_key = $1",
        )
        .bind(&context.idempotency_key)
        .fetch_one(&control.pools().write)
        .await
        .expect("idempotency count");
        assert_eq!(idempotency_count, 0);
    }

    #[tokio::test]
    #[ignore = "requires one disposable PostgreSQL control write database"]
    async fn billing_policy_transition_persists_worker_boundary_and_rolls_back_atomically() {
        let config = crate::ControlPlaneDatabaseConfig {
            write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
            read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
        };
        let pools = crate::ControlPlanePools::connect(&config)
            .await
            .expect("control pools");
        erpx_migration::migrate_control_plane(&pools.write)
            .await
            .expect("write migration");
        let control = crate::SqlxControlPlane::new(pools);
        let store = SqlxCommercialStore::new(&control);

        let customer_id = Uuid::new_v4();
        let subscription_id = Uuid::new_v4();
        let base = Utc::now() - Duration::hours(1);
        store
            .upsert_billing_lifecycle(
                customer_id,
                subscription_id,
                &BillingLifecycle::new(BillingState::PastDue, base),
                None,
            )
            .await
            .expect("seed past-due lifecycle");
        let context = CommercialCommandContext::new(
            Uuid::new_v4(),
            format!("billing-policy-{}", Uuid::new_v4()),
            Uuid::new_v4(),
            "billing-worker",
            "advance payment policy",
        )
        .expect("policy context");
        let at = base + Duration::minutes(1);
        let receipt = store
            .advance_billing_policy(BillingPolicyCommand {
                customer_id,
                subscription_id,
                at,
                context: context.clone(),
            })
            .await
            .expect("advance policy");
        assert_eq!(receipt.aggregate_version, 2);

        let lifecycle = store
            .get_billing_lifecycle(customer_id, subscription_id)
            .await
            .expect("read lifecycle")
            .expect("lifecycle exists");
        assert_eq!(lifecycle.0.state, BillingState::Grace);
        assert_eq!(lifecycle.1, 2);

        let batch_customer_id = Uuid::new_v4();
        let batch_subscription_id = Uuid::new_v4();
        store
            .upsert_billing_lifecycle(
                batch_customer_id,
                batch_subscription_id,
                &BillingLifecycle::new(BillingState::PastDue, base),
                None,
            )
            .await
            .expect("seed batch policy lifecycle");
        let batch_report = store
            .advance_due_billing_policies("billing-policy-worker", at, 10)
            .await
            .expect("advance due policy batch");
        assert_eq!(batch_report.considered, 1);
        assert_eq!(batch_report.advanced, 1);
        assert_eq!(
            store
                .get_billing_lifecycle(batch_customer_id, batch_subscription_id)
                .await
                .expect("batch lifecycle lookup")
                .expect("batch lifecycle")
                .0
                .state,
            BillingState::Grace
        );
        let empty_batch = store
            .advance_due_billing_policies("billing-policy-worker", at, 10)
            .await
            .expect("empty policy batch");
        assert_eq!(empty_batch.considered, 0);
        assert_eq!(empty_batch.advanced, 0);
        let lifecycle_row = sqlx::query_as::<_, (i64, String)>(
            "SELECT version, state
             FROM erpx_control.billing_lifecycles
             WHERE subscription_id = $1",
        )
        .bind(subscription_id)
        .fetch_one(&control.pools().write)
        .await
        .expect("policy lifecycle authority");
        assert_eq!(lifecycle_row.0, 2);
        assert_eq!(lifecycle_row.1, "grace");
        let evidence = sqlx::query_as::<_, (String, i64, Value, Option<String>)>(
            "SELECT event_type, aggregate_version, payload, published_at::text
             FROM erpx_control.outbox_events
             WHERE aggregate_type = $1 AND aggregate_id = $2
             ORDER BY aggregate_version DESC",
        )
        .bind(BILLING_LIFECYCLE_AGGREGATE_TYPE)
        .bind(subscription_id)
        .fetch_one(&control.pools().write)
        .await
        .expect("policy outbox evidence");
        assert_eq!(evidence.0, BILLING_POLICY_ADVANCED_TYPE);
        assert_eq!(evidence.1, 2);
        assert_eq!(evidence.2["state"], "grace");
        assert_eq!(evidence.2["change"], "Applied");
        assert!(evidence.3.is_none(), "worker evidence starts unpublished");

        let worker_id = Uuid::new_v4();
        let bootstrap_claim = control
            .claim_outbox(
                worker_id,
                10,
                at + Duration::seconds(1),
                Duration::minutes(5),
            )
            .await
            .expect("claim bootstrap evidence");
        let bootstrap = bootstrap_claim
            .into_iter()
            .find(|event| event.aggregate_id == subscription_id)
            .expect("bootstrap event is worker-visible");
        control
            .mark_outbox_published(bootstrap.id, worker_id, at + Duration::seconds(2))
            .await
            .expect("publish bootstrap evidence");
        let claimed = control
            .claim_outbox(
                worker_id,
                10,
                at + Duration::seconds(3),
                Duration::minutes(5),
            )
            .await
            .expect("claim policy evidence");
        let claimed = claimed
            .into_iter()
            .find(|event| {
                event.aggregate_id == subscription_id
                    && event.event_type == BILLING_POLICY_ADVANCED_TYPE
            })
            .expect("policy event is worker-visible");
        assert_eq!(claimed.event_type, BILLING_POLICY_ADVANCED_TYPE);
        control
            .mark_outbox_published(claimed.id, worker_id, at + Duration::seconds(4))
            .await
            .expect("publish policy evidence");

        let rollback_customer_id = Uuid::new_v4();
        let rollback_subscription_id = Uuid::new_v4();
        let rollback_base = Utc::now() - Duration::hours(1);
        store
            .upsert_billing_lifecycle(
                rollback_customer_id,
                rollback_subscription_id,
                &BillingLifecycle::new(BillingState::PastDue, rollback_base),
                None,
            )
            .await
            .expect("seed rollback lifecycle");
        let rollback_context = CommercialCommandContext::new(
            Uuid::new_v4(),
            format!("billing-policy-conflict-{}", Uuid::new_v4()),
            Uuid::new_v4(),
            "billing-worker",
            "advance payment policy",
        )
        .expect("rollback context");
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, correlation_id, idempotency_key, payload,
                 occurred_at, available_at)
             VALUES ($1, $2, 1, $3, $4, 99, $5, $6, '{}'::jsonb, $7, $7)",
        )
        .bind(Uuid::new_v4())
        .bind("test.billing.policy.conflict")
        .bind(BILLING_LIFECYCLE_AGGREGATE_TYPE)
        .bind(rollback_subscription_id)
        .bind(rollback_context.correlation_id)
        .bind(&rollback_context.idempotency_key)
        .bind(rollback_base)
        .execute(&control.pools().write)
        .await
        .expect("seed policy outbox conflict");

        assert!(
            store
                .advance_billing_policy(BillingPolicyCommand {
                    customer_id: rollback_customer_id,
                    subscription_id: rollback_subscription_id,
                    at: rollback_base + Duration::minutes(1),
                    context: rollback_context.clone(),
                })
                .await
                .is_err()
        );
        let rolled_back = store
            .get_billing_lifecycle(rollback_customer_id, rollback_subscription_id)
            .await
            .expect("read rolled-back lifecycle")
            .expect("rollback lifecycle exists");
        assert_eq!(rolled_back.0.state, BillingState::PastDue);
        assert_eq!(rolled_back.1, 1);
        let idempotency_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_control.idempotency_keys WHERE idempotency_key = $1",
        )
        .bind(&rollback_context.idempotency_key)
        .fetch_one(&control.pools().write)
        .await
        .expect("rollback idempotency count");
        assert_eq!(idempotency_count, 0);
    }

    #[tokio::test]
    #[ignore = "requires one disposable PostgreSQL control write database"]
    async fn persisted_catalog_reconciliation_is_deterministic_without_provider_calls() {
        let config = crate::ControlPlaneDatabaseConfig {
            write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
            read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
        };
        let pools = crate::ControlPlanePools::connect(&config)
            .await
            .expect("control pools");
        erpx_migration::migrate_control_plane(&pools.write)
            .await
            .expect("write migration");
        let control = crate::SqlxControlPlane::new(pools);
        let store = SqlxCommercialStore::new(&control);
        let catalog = CommercialCatalogRevisionDraft {
            id: Uuid::new_v4(),
            revision_number: 41,
            prices: vec![erpx_domain::commercial::CatalogPrice {
                item_key: "base".into(),
                display_name: "Base".into(),
                kind: erpx_domain::commercial::CatalogPriceKind::BaseFee,
                pricing_unit: erpx_domain::commercial::PricingUnit::Flat,
                charge: erpx_domain::commercial::ChargeKind::Recurring(
                    erpx_domain::commercial::BillingInterval::Monthly,
                ),
                amount: erpx_domain::commercial::Money {
                    amount_minor: 1_000,
                    currency: "THB".into(),
                },
                trial: None,
                coupon: None,
                setup_fee: None,
                included_usage: None,
                overage: None,
            }],
        };
        let published = store
            .publish_catalog_revision(catalog, Utc::now(), "catalog-admin")
            .await
            .expect("publish catalog");
        let provider = ProviderCatalogRevision {
            provider_revision_ref: "provider-revision-41".into(),
            revision_number: 41,
            prices: vec![ProviderCatalogPrice {
                product_ref: Some("prod_base".into()),
                price_ref: Some("price_base".into()),
                price: published.price("base").expect("base price").clone(),
            }],
        };
        let in_sync = store
            .reconcile_catalog_revision(41, &provider)
            .await
            .expect("reconcile catalog");
        assert!(in_sync.is_in_sync());

        let mut changed = provider.clone();
        changed.prices[0].price.amount.amount_minor = 1_100;
        let mismatch = store
            .reconcile_catalog_revision(41, &changed)
            .await
            .expect("reconcile changed catalog");
        assert!(!mismatch.is_in_sync());
        assert!(mismatch.mismatches.iter().any(|entry| {
            entry.kind == erpx_domain::commercial::CatalogMismatchKind::PriceFieldMismatch
                && entry.item_key.as_deref() == Some("base")
                && entry.field.as_deref() == Some("amount")
        }));

        let missing = store
            .reconcile_catalog_revision(999, &provider)
            .await
            .expect("reconcile missing catalog");
        assert_eq!(
            missing.mismatches[0].kind,
            erpx_domain::commercial::CatalogMismatchKind::MissingLocalRevision
        );
    }

    #[tokio::test]
    #[ignore = "requires two disposable PostgreSQL control databases"]
    async fn authenticated_commercial_read_uses_authoritative_scope_and_read_pool() {
        let config = crate::ControlPlaneDatabaseConfig {
            write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
            read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
        };
        let pools = crate::ControlPlanePools::connect(&config)
            .await
            .expect("control pools");
        erpx_migration::migrate_control_plane(&pools.write)
            .await
            .expect("write migration");
        erpx_migration::migrate_control_read(&pools.read)
            .await
            .expect("read migration");
        let control = crate::SqlxControlPlane::new(pools);
        let customer_id = Uuid::new_v4();
        let holding_id = Uuid::new_v4();
        let mut projection = Customer360Projection::default();
        projection.account.id = customer_id;
        projection.account.account_code = "COMMERCIAL-READ-1".into();
        projection.account.country = "TH".into();
        projection.account.identity.trading_name = "Commercial Read Customer".into();
        projection
            .organization
            .holdings
            .push(erpx_domain::customer::OrganizationUnit {
                id: holding_id,
                name: "Holding 1".into(),
                kind: "holding".into(),
                parent_id: None,
                active: true,
            });
        projection.commercial.payment_state = BillingState::Active;
        projection.commercial.stripe_customer_ref = Some("cus_authenticated_read".into());
        control
            .upsert_customer_projection_read_model(&projection)
            .await
            .expect("project customer commercial read");

        let store = SqlxCommercialStore::new(&control);
        let context = Customer360DetailReadContext {
            customer_id,
            scope: erpx_domain::contracts::AuthorityScope::Holding { holding_id },
            global_read: erpx_domain::customer::CustomerGlobalReadContext {
                actor: erpx_domain::customer::GlobalReadActor {
                    principal_ref: "billing-reader".into(),
                    session_ref: Some("session-read-1".into()),
                },
                module: "commercial".into(),
                purpose: "review billing entitlement state".into(),
                correlation_id: Uuid::new_v4(),
            },
        };
        let view = store
            .read_authenticated_commercial(&context)
            .await
            .expect("authenticated commercial read");
        let CustomerReadView::Present { data, metadata } = view else {
            panic!("commercial read projection should be present");
        };
        assert_eq!(metadata.customer_id, customer_id);
        assert!(metadata.read_only);
        assert_eq!(
            data.commercial.stripe_customer_ref.as_deref(),
            Some("cus_authenticated_read")
        );

        let unauthorized = Customer360DetailReadContext {
            scope: erpx_domain::contracts::AuthorityScope::Holding {
                holding_id: Uuid::new_v4(),
            },
            ..context
        };
        assert!(matches!(
            store.read_authenticated_commercial(&unauthorized).await,
            Err(PersistenceError::InvalidData(message))
                if message == "commercial read scope is not authorized"
        ));
    }

    #[tokio::test]
    #[ignore = "requires two disposable PostgreSQL databases"]
    async fn stripe_inbox_claims_and_transitions_are_durable_and_owned() {
        let config = crate::ControlPlaneDatabaseConfig {
            write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
            read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
        };
        let pools = crate::ControlPlanePools::connect(&config)
            .await
            .expect("control pools");
        erpx_migration::migrate_control_plane(&pools.write)
            .await
            .expect("write migration");
        let control = crate::SqlxControlPlane::new(pools);
        let store = SqlxCommercialStore::new(&control);
        let policy = StripeInboxRetryPolicy::new(2, Duration::seconds(1), Duration::seconds(2))
            .expect("retry policy");
        let event_id = format!("evt_{}", Uuid::new_v4());
        let event = StripeInboxEvent {
            event_id: event_id.clone(),
            event_type: "invoice.paid".into(),
            stripe_created_at: Some(Utc::now()),
            payload: serde_json::json!({
                "id": event_id,
                "type": "invoice.paid",
                "payment_intent": "pi_sensitive_fixture"
            }),
            raw_body: b"secret raw Stripe body".to_vec(),
        };
        assert!(!format!("{event:?}").contains("secret raw Stripe body"));
        let webhook_correlation = Uuid::new_v4();
        let webhook_context =
            StripeWebhookPersistenceContext::from_raw_body(&event.raw_body, webhook_correlation)
                .expect("webhook persistence context");
        assert_eq!(
            store
                .insert_stripe_webhook(&event, &webhook_context)
                .await
                .expect("insert"),
            StripeInboxInsert::Inserted
        );
        let audit_row = sqlx::query_as::<_, (Vec<u8>, Uuid)>(
            "SELECT raw_body_sha256, correlation_id
             FROM erpx_control.stripe_event_inbox
             WHERE event_id = $1",
        )
        .bind(&event.event_id)
        .fetch_one(&control.pools().write)
        .await
        .expect("webhook audit metadata");
        let expected_digest: [u8; 32] = Sha256::digest(&event.raw_body).into();
        assert_eq!(audit_row.0.as_slice(), expected_digest.as_slice());
        assert_eq!(audit_row.1, webhook_correlation);
        assert_eq!(
            store.insert_stripe_event(&event).await.expect("duplicate"),
            StripeInboxInsert::Duplicate
        );
        let mut tampered = event.clone();
        tampered.raw_body = b"different signed body".to_vec();
        assert!(matches!(
            store.insert_stripe_event(&tampered).await,
            Err(PersistenceError::IdempotencyConflict { .. })
        ));

        let worker_a = Uuid::new_v4();
        let worker_b = Uuid::new_v4();
        let now = Utc::now();
        let claim = store
            .claim_stripe_events(worker_a, 10, now, Duration::minutes(5), policy)
            .await
            .expect("claim");
        assert_eq!(claim.len(), 1);
        assert_eq!(claim[0].attempts, 1);
        assert_eq!(claim[0].claimed_by, worker_a);
        assert!(!format!("{:?}", claim[0]).contains("secret raw Stripe body"));
        assert!(!format!("{:?}", claim[0]).contains("pi_sensitive_fixture"));
        assert!(matches!(
            store
                .mark_stripe_event_processed(&claim[0].event_id, worker_b, now)
                .await,
            Err(PersistenceError::InvalidData(_))
        ));

        let retry = store
            .retry_stripe_event(
                &claim[0].event_id,
                worker_a,
                "temporary failure",
                now,
                policy,
            )
            .await
            .expect("retry");
        let retry_at = match retry {
            StripeInboxTransition::RetryScheduled {
                attempts,
                available_at,
            } => {
                assert_eq!(attempts, 1);
                assert_eq!(available_at, now + Duration::seconds(1));
                available_at
            }
            other => panic!("unexpected transition: {other:?}"),
        };
        assert!(
            store
                .claim_stripe_events(worker_b, 10, now, Duration::minutes(5), policy)
                .await
                .expect("early claim")
                .is_empty()
        );
        let second_claim = store
            .claim_stripe_events(worker_b, 10, retry_at, Duration::minutes(5), policy)
            .await
            .expect("second claim");
        assert_eq!(second_claim[0].attempts, 2);
        let dead = store
            .retry_stripe_event(
                &second_claim[0].event_id,
                worker_b,
                "permanent failure",
                retry_at,
                policy,
            )
            .await
            .expect("dead letter");
        assert!(matches!(
            dead,
            StripeInboxTransition::DeadLettered { attempts: 2, .. }
        ));
        let state = store
            .get_stripe_event_state(&second_claim[0].event_id)
            .await
            .expect("state")
            .expect("state exists");
        assert_eq!(state.status, StripeInboxStatus::DeadLetter);
        assert_eq!(state.attempts, 2);
        assert!(
            store
                .claim_stripe_events(
                    worker_a,
                    10,
                    retry_at + Duration::hours(1),
                    Duration::minutes(5),
                    policy,
                )
                .await
                .expect("dead-letter is not claimable")
                .is_empty()
        );

        let replay_authorization = StripeReplayAuthorizationContext::new(
            "billing-admin",
            "replay Stripe commercial dead letter",
            Uuid::new_v4(),
        )
        .expect("replay authorization");
        store
            .replay_stripe_dead_letter_authenticated(
                &second_claim[0].event_id,
                retry_at,
                StripeInboxReplayPolicy::default(),
                &replay_authorization,
            )
            .await
            .expect("authenticated replay");
        let replay_audit_count: i64 = sqlx::query_scalar(
            "SELECT count(*)
             FROM erpx_control.audit_events
             WHERE action = 'commercial.stripe.dead_letter_replayed'
               AND correlation_id = $1",
        )
        .bind(replay_authorization.correlation_id())
        .fetch_one(&control.pools().write)
        .await
        .expect("replay audit");
        assert_eq!(replay_audit_count, 1);
        let replayed = store
            .claim_stripe_events(worker_a, 10, retry_at, Duration::minutes(5), policy)
            .await
            .expect("replayed claim");
        assert_eq!(replayed[0].attempts, 1);
        let processed = store
            .mark_stripe_event_processed(&replayed[0].event_id, worker_a, retry_at)
            .await
            .expect("processed");
        assert!(matches!(
            processed,
            StripeInboxTransition::Processed { attempts: 1, .. }
        ));
        assert_eq!(
            store
                .insert_stripe_event(&event)
                .await
                .expect("processed duplicate"),
            StripeInboxInsert::Duplicate
        );
        assert!(
            store
                .claim_stripe_events(
                    worker_b,
                    10,
                    retry_at + Duration::hours(1),
                    Duration::minutes(5),
                    policy,
                )
                .await
                .expect("processed is not claimable")
                .is_empty()
        );
    }

    #[tokio::test]
    #[ignore = "requires one disposable PostgreSQL control write database"]
    async fn stripe_dead_letter_replay_budget_is_durable_and_bounded() {
        let config = crate::ControlPlaneDatabaseConfig {
            write_url: std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required"),
            read_url: std::env::var("ERPX_TEST_CONTROL_READ_DATABASE_URL")
                .expect("ERPX_TEST_CONTROL_READ_DATABASE_URL is required"),
        };
        let pools = crate::ControlPlanePools::connect(&config)
            .await
            .expect("control pools");
        erpx_migration::migrate_control_plane(&pools.write)
            .await
            .expect("write migration");
        let control = crate::SqlxControlPlane::new(pools);
        let store = SqlxCommercialStore::new(&control);
        let worker = Uuid::new_v4();
        let at = Utc::now();
        let event = StripeInboxEvent {
            event_id: format!("evt_replay_{}", Uuid::new_v4()),
            event_type: "invoice.paid".into(),
            stripe_created_at: Some(at),
            payload: serde_json::json!({"id": "evt_replay"}),
            raw_body: b"verified-replay-body".to_vec(),
        };
        store
            .insert_stripe_event(&event)
            .await
            .expect("insert replay fixture");
        let claim_at = Utc::now();
        let policy = StripeInboxRetryPolicy::new(1, Duration::seconds(1), Duration::seconds(1))
            .expect("retry policy");
        let claim = store
            .claim_stripe_events(worker, 1, claim_at, Duration::minutes(5), policy)
            .await
            .expect("claim")[0]
            .clone();
        store
            .retry_stripe_event(&claim.event_id, worker, "bounded failure", claim_at, policy)
            .await
            .expect("dead letter");
        let replay_policy = StripeInboxReplayPolicy::new(1).expect("replay policy");
        let replay_authorization = StripeReplayAuthorizationContext::new(
            "test-operator",
            "retry dead-letter event",
            Uuid::new_v4(),
        )
        .expect("replay authorization");
        store
            .replay_stripe_dead_letter_authenticated(
                &claim.event_id,
                claim_at,
                replay_policy,
                &replay_authorization,
            )
            .await
            .expect("first replay");
        let replayed_state = store
            .get_stripe_event_state(&claim.event_id)
            .await
            .expect("replayed state")
            .expect("replayed row");
        assert_eq!(replayed_state.replay_count, 1);
        let replayed_claim = store
            .claim_stripe_events(worker, 1, claim_at, Duration::minutes(5), policy)
            .await
            .expect("replayed claim")[0]
            .clone();
        store
            .retry_stripe_event(
                &replayed_claim.event_id,
                worker,
                "second bounded failure",
                claim_at,
                policy,
            )
            .await
            .expect("second dead letter");
        assert_eq!(
            store
                .replay_stripe_dead_letter_authenticated(
                    &claim.event_id,
                    claim_at,
                    replay_policy,
                    &replay_authorization,
                )
                .await
                .expect_err("replay budget")
                .to_string(),
            "invalid persisted value: Stripe event replay limit has been reached"
        );
    }
}
