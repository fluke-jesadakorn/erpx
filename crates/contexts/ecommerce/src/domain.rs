use chrono::{DateTime, Utc};
use erpx_integration_contracts::{
    MoneyAmount, StableRecordRef,
    catalog::ContextId,
    reference::{RecordTypeId, ReferenceValidationError},
};
use erpx_kernel::{RecordId, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

macro_rules! commerce_id {
    ($name:ident, $field:literal) => {
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
        #[serde(transparent)]
        pub struct $name(Uuid);
        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, CommerceError> {
                if value.is_nil() {
                    Err(CommerceError::NilIdentifier { field: $field })
                } else {
                    Ok(Self(value))
                }
            }
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }
    };
}
commerce_id!(ChannelId, "channel_id");
commerce_id!(ListingId, "listing_id");
commerce_id!(CartId, "cart_id");
commerce_id!(CheckoutId, "checkout_id");
commerce_id!(ReturnId, "return_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommerceRealm {
    Guest,
    Customer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommerceIdentity {
    pub realm: CommerceRealm,
    pub principal_id: Uuid,
    pub verified_customer: Option<StableRecordRef>,
}
impl CommerceIdentity {
    pub fn validate(&self) -> Result<(), CommerceError> {
        require_id(self.principal_id, "commerce_principal")?;
        if self.realm == CommerceRealm::Guest && self.verified_customer.is_some() {
            return Err(CommerceError::RealmViolation);
        }
        if let Some(customer) = &self.verified_customer {
            customer
                .validate()
                .map_err(|_| CommerceError::InvalidEvidence)?;
            if customer.owner_context() != ContextId::CustomerAuthority {
                return Err(CommerceError::RealmViolation);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ChannelStatus {
    Draft,
    Approved,
    Active,
    Suspended,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommerceChannel {
    id: ChannelId,
    version: u64,
    scope: TenantScope,
    code: String,
    locales: Vec<String>,
    currencies: Vec<String>,
    status: ChannelStatus,
    proposed_by: Uuid,
    approved_by: Option<Uuid>,
    updated_at: DateTime<Utc>,
}
impl CommerceChannel {
    pub fn draft(
        id: ChannelId,
        scope: TenantScope,
        code: impl Into<String>,
        locales: Vec<String>,
        currencies: Vec<String>,
        proposed_by: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Self, CommerceError> {
        validate_scope(&scope)?;
        require_id(proposed_by, "channel_proposer")?;
        let value = Self {
            id,
            version: 1,
            scope,
            code: code.into(),
            locales,
            currencies,
            status: ChannelStatus::Draft,
            proposed_by,
            approved_by: None,
            updated_at: at,
        };
        require_text(&value.code, "channel_code", 128)?;
        if value.locales.is_empty() || value.currencies.is_empty() {
            return Err(CommerceError::InvalidChannel);
        }
        Ok(value)
    }
    pub fn approve(
        &mut self,
        approver: Uuid,
        expected: u64,
        at: DateTime<Utc>,
    ) -> Result<(), CommerceError> {
        self.ensure(expected, at)?;
        if self.status != ChannelStatus::Draft || approver == self.proposed_by {
            return Err(CommerceError::SegregationOfDuties);
        }
        require_id(approver, "channel_approver")?;
        self.approved_by = Some(approver);
        self.status = ChannelStatus::Approved;
        self.advance(at)
    }
    pub fn transition(
        &mut self,
        next_status: ChannelStatus,
        expected: u64,
        at: DateTime<Utc>,
    ) -> Result<(), CommerceError> {
        self.ensure(expected, at)?;
        let allowed = matches!(
            (self.status, next_status),
            (ChannelStatus::Approved, ChannelStatus::Active)
                | (ChannelStatus::Active, ChannelStatus::Suspended)
                | (ChannelStatus::Suspended, ChannelStatus::Active)
                | (
                    ChannelStatus::Draft | ChannelStatus::Approved | ChannelStatus::Suspended,
                    ChannelStatus::Retired
                )
        );
        if !allowed || next_status == ChannelStatus::Active && self.approved_by.is_none() {
            return Err(CommerceError::InvalidTransition);
        }
        self.status = next_status;
        self.advance(at)
    }
    fn ensure(&self, expected: u64, at: DateTime<Utc>) -> Result<(), CommerceError> {
        ensure(expected, self.version, at, self.updated_at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), CommerceError> {
        self.version = next(self.version)?;
        self.updated_at = at;
        Ok(())
    }
    pub const fn id(&self) -> ChannelId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn status(&self) -> ChannelStatus {
        self.status
    }
    pub fn code(&self) -> &str {
        &self.code
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListingStatus {
    Draft,
    Approved,
    Published,
    Withdrawn,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Listing {
    id: ListingId,
    version: u64,
    scope: TenantScope,
    channel_id: ChannelId,
    product: StableRecordRef,
    source_versions: [u64; 5],
    unit_price: MoneyAmount,
    available: bool,
    status: ListingStatus,
    proposed_by: Uuid,
    approved_by: Option<Uuid>,
    updated_at: DateTime<Utc>,
}
impl Listing {
    #[allow(clippy::too_many_arguments)]
    pub fn draft(
        id: ListingId,
        scope: TenantScope,
        channel_id: ChannelId,
        product: StableRecordRef,
        source_versions: [u64; 5],
        unit_price: MoneyAmount,
        available: bool,
        proposed_by: Uuid,
        at: DateTime<Utc>,
    ) -> Result<Self, CommerceError> {
        validate_scope(&scope)?;
        product
            .validate()
            .map_err(|_| CommerceError::InvalidEvidence)?;
        require_id(proposed_by, "listing_proposer")?;
        if source_versions.contains(&0) || unit_price.minor_units() < 0 {
            return Err(CommerceError::InvalidListing);
        }
        Ok(Self {
            id,
            version: 1,
            scope,
            channel_id,
            product,
            source_versions,
            unit_price,
            available,
            status: ListingStatus::Draft,
            proposed_by,
            approved_by: None,
            updated_at: at,
        })
    }
    pub fn approve(
        &mut self,
        approver: Uuid,
        expected: u64,
        at: DateTime<Utc>,
    ) -> Result<(), CommerceError> {
        ensure(expected, self.version, at, self.updated_at)?;
        if self.status != ListingStatus::Draft || approver == self.proposed_by {
            return Err(CommerceError::SegregationOfDuties);
        }
        require_id(approver, "listing_approver")?;
        self.approved_by = Some(approver);
        self.status = ListingStatus::Approved;
        self.advance(at)
    }
    pub fn publish(
        &mut self,
        channel: &CommerceChannel,
        expected: u64,
        at: DateTime<Utc>,
    ) -> Result<(), CommerceError> {
        ensure(expected, self.version, at, self.updated_at)?;
        if self.status != ListingStatus::Approved
            || channel.id != self.channel_id
            || channel.status != ChannelStatus::Active
            || !channel.scope.contains(&self.scope)
        {
            return Err(CommerceError::InvalidTransition);
        }
        self.status = ListingStatus::Published;
        self.advance(at)
    }
    pub fn withdraw(&mut self, expected: u64, at: DateTime<Utc>) -> Result<(), CommerceError> {
        ensure(expected, self.version, at, self.updated_at)?;
        if !matches!(
            self.status,
            ListingStatus::Approved | ListingStatus::Published
        ) {
            return Err(CommerceError::InvalidTransition);
        }
        self.status = ListingStatus::Withdrawn;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), CommerceError> {
        self.version = next(self.version)?;
        self.updated_at = at;
        Ok(())
    }
    pub const fn id(&self) -> ListingId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn channel_id(&self) -> ChannelId {
        self.channel_id
    }
    pub const fn price(&self) -> MoneyAmount {
        self.unit_price
    }
    pub const fn available(&self) -> bool {
        self.available
    }
    pub const fn status(&self) -> ListingStatus {
        self.status
    }
    pub const fn source_versions(&self) -> [u64; 5] {
        self.source_versions
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CartStatus {
    Active,
    CheckoutLocked,
    Ordered,
    Expired,
    Abandoned,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CartLine {
    pub listing_id: ListingId,
    pub listing_version: u64,
    pub quantity: u32,
    pub accepted_unit_price: MoneyAmount,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Cart {
    id: CartId,
    version: u64,
    scope: TenantScope,
    channel_id: ChannelId,
    owner: CommerceIdentity,
    marketing_consent: bool,
    lines: Vec<CartLine>,
    status: CartStatus,
    updated_at: DateTime<Utc>,
}
impl Cart {
    pub fn create(
        id: CartId,
        scope: TenantScope,
        channel: &CommerceChannel,
        owner: CommerceIdentity,
        marketing_consent: bool,
        at: DateTime<Utc>,
    ) -> Result<Self, CommerceError> {
        owner.validate()?;
        validate_scope(&scope)?;
        if channel.status != ChannelStatus::Active || !channel.scope.contains(&scope) {
            return Err(CommerceError::InvalidTransition);
        }
        Ok(Self {
            id,
            version: 1,
            scope,
            channel_id: channel.id,
            owner,
            marketing_consent,
            lines: Vec::new(),
            status: CartStatus::Active,
            updated_at: at,
        })
    }
    pub fn put_line(
        &mut self,
        listing: &Listing,
        quantity: u32,
        expected: u64,
        at: DateTime<Utc>,
    ) -> Result<(), CommerceError> {
        if self.status != CartStatus::Active {
            return Err(CommerceError::InvalidTransition);
        }
        ensure(expected, self.version, at, self.updated_at)?;
        if listing.channel_id != self.channel_id
            || listing.status != ListingStatus::Published
            || !listing.available
            || !self.scope.contains(&listing.scope)
            || quantity == 0
            || quantity > 10_000
        {
            return Err(CommerceError::ListingUnavailable);
        }
        if let Some(line) = self
            .lines
            .iter_mut()
            .find(|line| line.listing_id == listing.id)
        {
            *line = CartLine {
                listing_id: listing.id,
                listing_version: listing.version,
                quantity,
                accepted_unit_price: listing.unit_price,
            };
        } else {
            self.lines.push(CartLine {
                listing_id: listing.id,
                listing_version: listing.version,
                quantity,
                accepted_unit_price: listing.unit_price,
            });
        }
        self.advance(at)
    }
    pub fn lock(&mut self, expected: u64, at: DateTime<Utc>) -> Result<(), CommerceError> {
        if self.status != CartStatus::Active {
            return Err(CommerceError::InvalidTransition);
        }
        ensure(expected, self.version, at, self.updated_at)?;
        if self.lines.is_empty() {
            return Err(CommerceError::EmptyCart);
        }
        self.status = CartStatus::CheckoutLocked;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), CommerceError> {
        self.version = next(self.version)?;
        self.updated_at = at;
        Ok(())
    }
    pub const fn id(&self) -> CartId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn owner(&self) -> &CommerceIdentity {
        &self.owner
    }
    pub fn lines(&self) -> &[CartLine] {
        &self.lines
    }
    pub const fn status(&self) -> CartStatus {
        self.status
    }
    pub const fn marketing_consent(&self) -> bool {
        self.marketing_consent
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DecisionStatus {
    Pending,
    Accepted,
    Rejected,
    Uncertain,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityDecision {
    pub status: DecisionStatus,
    pub receipt_id: Option<String>,
    pub source_version: Option<u64>,
}
impl AuthorityDecision {
    fn pending() -> Self {
        Self {
            status: DecisionStatus::Pending,
            receipt_id: None,
            source_version: None,
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CheckoutSnapshot {
    pub cart_id: CartId,
    pub cart_version: u64,
    pub owner: CommerceIdentity,
    pub lines: Vec<CartLine>,
    pub tax: MoneyAmount,
    pub shipping: MoneyAmount,
    pub total: MoneyAmount,
    pub address_digest: String,
    pub terms_version: u64,
    pub marketing_consent: bool,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CheckoutStatus {
    Revalidating,
    Reserving,
    Paying,
    ReadyForOrder,
    Ordered,
    Rejected,
    Uncertain,
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Checkout {
    id: CheckoutId,
    version: u64,
    scope: TenantScope,
    idempotency_key: String,
    snapshot: CheckoutSnapshot,
    revalidation: AuthorityDecision,
    reservation: AuthorityDecision,
    payment: AuthorityDecision,
    order_receipt: Option<String>,
    order_ref: Option<StableRecordRef>,
    status: CheckoutStatus,
    updated_at: DateTime<Utc>,
}
impl Checkout {
    #[allow(clippy::too_many_arguments)]
    pub fn start(
        id: CheckoutId,
        scope: TenantScope,
        idempotency_key: impl Into<String>,
        cart: &Cart,
        tax: MoneyAmount,
        shipping: MoneyAmount,
        total: MoneyAmount,
        address_digest: impl Into<String>,
        terms_version: u64,
        at: DateTime<Utc>,
    ) -> Result<Self, CommerceError> {
        if cart.status != CartStatus::CheckoutLocked || cart.lines.is_empty() || terms_version == 0
        {
            return Err(CommerceError::InvalidCheckout);
        }
        let value = Self {
            id,
            version: 1,
            scope,
            idempotency_key: idempotency_key.into(),
            snapshot: CheckoutSnapshot {
                cart_id: cart.id,
                cart_version: cart.version,
                owner: cart.owner.clone(),
                lines: cart.lines.clone(),
                tax,
                shipping,
                total,
                address_digest: address_digest.into(),
                terms_version,
                marketing_consent: cart.marketing_consent,
            },
            revalidation: AuthorityDecision::pending(),
            reservation: AuthorityDecision::pending(),
            payment: AuthorityDecision::pending(),
            order_receipt: None,
            order_ref: None,
            status: CheckoutStatus::Revalidating,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn reconcile_revalidation(
        &mut self,
        accepted: bool,
        receipt: impl Into<String>,
        exact_listing_versions: &[u64],
        at: DateTime<Utc>,
    ) -> Result<(), CommerceError> {
        self.at_status(CheckoutStatus::Revalidating, at)?;
        if exact_listing_versions
            != self
                .snapshot
                .lines
                .iter()
                .map(|line| line.listing_version)
                .collect::<Vec<_>>()
        {
            return Err(CommerceError::StaleCheckout);
        }
        set_decision(
            &mut self.revalidation,
            if accepted {
                DecisionStatus::Accepted
            } else {
                DecisionStatus::Rejected
            },
            receipt.into(),
            None,
        )?;
        self.status = if accepted {
            CheckoutStatus::Reserving
        } else {
            CheckoutStatus::Rejected
        };
        self.advance(at)
    }
    pub fn reconcile_reservation(
        &mut self,
        status: DecisionStatus,
        receipt: impl Into<String>,
        source_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), CommerceError> {
        self.at_status(CheckoutStatus::Reserving, at)?;
        if source_version == 0 {
            return Err(CommerceError::InvalidEvidence);
        }
        set_decision(
            &mut self.reservation,
            status,
            receipt.into(),
            Some(source_version),
        )?;
        self.status = external_next(status, CheckoutStatus::Paying)?;
        self.advance(at)
    }
    pub fn reconcile_payment(
        &mut self,
        status: DecisionStatus,
        receipt: impl Into<String>,
        source_version: u64,
        at: DateTime<Utc>,
    ) -> Result<(), CommerceError> {
        self.at_status(CheckoutStatus::Paying, at)?;
        if source_version == 0 {
            return Err(CommerceError::InvalidEvidence);
        }
        set_decision(
            &mut self.payment,
            status,
            receipt.into(),
            Some(source_version),
        )?;
        self.status = external_next(status, CheckoutStatus::ReadyForOrder)?;
        self.advance(at)
    }
    pub fn reconcile_order(
        &mut self,
        receipt: impl Into<String>,
        order: StableRecordRef,
        at: DateTime<Utc>,
    ) -> Result<(), CommerceError> {
        self.at_status(CheckoutStatus::ReadyForOrder, at)?;
        order
            .validate()
            .map_err(|_| CommerceError::InvalidEvidence)?;
        if !matches!(
            order.owner_context(),
            ContextId::Sales | ContextId::PointOfSale
        ) {
            return Err(CommerceError::OwnerBoundaryViolation);
        }
        let receipt = receipt.into();
        require_text(&receipt, "order_receipt", 255)?;
        if self.order_receipt.is_some() {
            return Err(CommerceError::DuplicateOrder);
        }
        self.order_receipt = Some(receipt);
        self.order_ref = Some(order);
        self.status = CheckoutStatus::Ordered;
        self.advance(at)
    }
    fn at_status(&self, expected: CheckoutStatus, at: DateTime<Utc>) -> Result<(), CommerceError> {
        if self.status != expected {
            Err(CommerceError::InvalidTransition)
        } else if at < self.updated_at {
            Err(CommerceError::InvalidTimestamp)
        } else {
            Ok(())
        }
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), CommerceError> {
        self.version = next(self.version)?;
        self.updated_at = at;
        self.validate()
    }
    fn validate(&self) -> Result<(), CommerceError> {
        validate_scope(&self.scope)?;
        self.snapshot.owner.validate()?;
        require_text(&self.idempotency_key, "checkout_idempotency", 255)?;
        if !valid_digest(&self.snapshot.address_digest)
            || self.snapshot.lines.is_empty()
            || self.snapshot.terms_version == 0
            || self.snapshot.tax.currency() != self.snapshot.total.currency()
            || self.snapshot.shipping.currency() != self.snapshot.total.currency()
        {
            return Err(CommerceError::InvalidCheckout);
        }
        Ok(())
    }
    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        StableRecordRef::try_new(
            RecordId::try_from_uuid(self.id.as_uuid()).expect("checkout id"),
            ContextId::Ecommerce,
            RecordTypeId::try_new("commerce.checkout")?,
        )
    }
    pub const fn id(&self) -> CheckoutId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn status(&self) -> CheckoutStatus {
        self.status
    }
    pub const fn owner(&self) -> &CommerceIdentity {
        &self.snapshot.owner
    }
    pub const fn order_ref(&self) -> Option<&StableRecordRef> {
        self.order_ref.as_ref()
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ReturnStatus {
    Requested,
    Accepted,
    Rejected,
    RefundPending,
    Refunded,
    Uncertain,
    Cancelled,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReturnRequest {
    id: ReturnId,
    version: u64,
    scope: TenantScope,
    checkout_id: CheckoutId,
    order_ref: StableRecordRef,
    owner: CommerceIdentity,
    reason: String,
    evidence_digest: String,
    handoff_receipt: Option<String>,
    refund_receipt: Option<String>,
    status: ReturnStatus,
    updated_at: DateTime<Utc>,
}
impl ReturnRequest {
    pub fn request(
        id: ReturnId,
        checkout: &Checkout,
        owner: CommerceIdentity,
        reason: impl Into<String>,
        evidence_digest: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, CommerceError> {
        if checkout.status != CheckoutStatus::Ordered || checkout.snapshot.owner != owner {
            return Err(CommerceError::RealmViolation);
        }
        let value = Self {
            id,
            version: 1,
            scope: checkout.scope.clone(),
            checkout_id: checkout.id,
            order_ref: checkout
                .order_ref
                .clone()
                .ok_or(CommerceError::InvalidEvidence)?,
            owner,
            reason: reason.into(),
            evidence_digest: evidence_digest.into(),
            handoff_receipt: None,
            refund_receipt: None,
            status: ReturnStatus::Requested,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn reconcile_disposition(
        &mut self,
        accepted: bool,
        receipt: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<(), CommerceError> {
        if !matches!(
            self.status,
            ReturnStatus::Requested | ReturnStatus::Uncertain
        ) || at < self.updated_at
        {
            return Err(CommerceError::InvalidTransition);
        }
        let receipt = receipt.into();
        require_text(&receipt, "return_receipt", 255)?;
        self.handoff_receipt = Some(receipt);
        self.status = if accepted {
            ReturnStatus::RefundPending
        } else {
            ReturnStatus::Rejected
        };
        self.advance(at)
    }
    pub fn reconcile_refund(
        &mut self,
        status: DecisionStatus,
        receipt: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<(), CommerceError> {
        if self.status != ReturnStatus::RefundPending || at < self.updated_at {
            return Err(CommerceError::InvalidTransition);
        }
        let receipt = receipt.into();
        require_text(&receipt, "refund_receipt", 255)?;
        self.refund_receipt = Some(receipt);
        self.status = match status {
            DecisionStatus::Accepted => ReturnStatus::Refunded,
            DecisionStatus::Rejected => ReturnStatus::Rejected,
            DecisionStatus::Uncertain => ReturnStatus::Uncertain,
            DecisionStatus::Pending => return Err(CommerceError::InvalidTransition),
        };
        self.advance(at)
    }
    fn validate(&self) -> Result<(), CommerceError> {
        validate_scope(&self.scope)?;
        self.owner.validate()?;
        self.order_ref
            .validate()
            .map_err(|_| CommerceError::InvalidEvidence)?;
        require_text(&self.reason, "return_reason", 1024)?;
        if !valid_digest(&self.evidence_digest) {
            return Err(CommerceError::InvalidEvidence);
        }
        Ok(())
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), CommerceError> {
        self.version = next(self.version)?;
        self.updated_at = at;
        self.validate()
    }
    pub const fn id(&self) -> ReturnId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn status(&self) -> ReturnStatus {
        self.status
    }
    pub const fn owner(&self) -> &CommerceIdentity {
        &self.owner
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "state", rename_all = "snake_case")]
pub enum CommerceAggregate {
    Channel(CommerceChannel),
    Listing(Listing),
    Cart(Cart),
    Checkout(Box<Checkout>),
    Return(ReturnRequest),
    Fulfillment(crate::FulfillmentRequest),
}
impl CommerceAggregate {
    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::Channel(v) => v.id.as_uuid(),
            Self::Listing(v) => v.id.as_uuid(),
            Self::Cart(v) => v.id.as_uuid(),
            Self::Checkout(v) => v.id.as_uuid(),
            Self::Return(v) => v.id.as_uuid(),
            Self::Fulfillment(v) => v.id(),
        }
    }
    pub const fn version(&self) -> u64 {
        match self {
            Self::Channel(v) => v.version,
            Self::Listing(v) => v.version,
            Self::Cart(v) => v.version,
            Self::Checkout(v) => v.version,
            Self::Return(v) => v.version,
            Self::Fulfillment(v) => v.version(),
        }
    }
    pub const fn scope(&self) -> &TenantScope {
        match self {
            Self::Channel(v) => &v.scope,
            Self::Listing(v) => &v.scope,
            Self::Cart(v) => &v.scope,
            Self::Checkout(v) => &v.scope,
            Self::Return(v) => &v.scope,
            Self::Fulfillment(v) => v.scope(),
        }
    }
    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Channel(_) => "commerce_channel",
            Self::Listing(_) => "commerce_listing",
            Self::Cart(_) => "commerce_cart",
            Self::Checkout(_) => "commerce_checkout",
            Self::Return(_) => "commerce_return",
            Self::Fulfillment(_) => "commerce_fulfillment_request",
        }
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        match self {
            Self::Channel(v) => v.updated_at,
            Self::Listing(v) => v.updated_at,
            Self::Cart(v) => v.updated_at,
            Self::Checkout(v) => v.updated_at,
            Self::Return(v) => v.updated_at,
            Self::Fulfillment(v) => v.updated_at(),
        }
    }
    pub const fn owner(&self) -> Option<&CommerceIdentity> {
        match self {
            Self::Cart(value) => Some(value.owner()),
            Self::Checkout(value) => Some(value.owner()),
            Self::Return(value) => Some(value.owner()),
            Self::Fulfillment(value) => Some(value.owner()),
            Self::Channel(_) | Self::Listing(_) => None,
        }
    }
}

fn set_decision(
    value: &mut AuthorityDecision,
    status: DecisionStatus,
    receipt: String,
    source_version: Option<u64>,
) -> Result<(), CommerceError> {
    if value.status != DecisionStatus::Pending || status == DecisionStatus::Pending {
        return Err(CommerceError::DuplicateDecision);
    }
    require_text(&receipt, "authority_receipt", 255)?;
    value.status = status;
    value.receipt_id = Some(receipt);
    value.source_version = source_version;
    Ok(())
}
fn external_next(
    status: DecisionStatus,
    accepted: CheckoutStatus,
) -> Result<CheckoutStatus, CommerceError> {
    match status {
        DecisionStatus::Accepted => Ok(accepted),
        DecisionStatus::Rejected => Ok(CheckoutStatus::Rejected),
        DecisionStatus::Uncertain => Ok(CheckoutStatus::Uncertain),
        DecisionStatus::Pending => Err(CommerceError::InvalidTransition),
    }
}
fn validate_scope(scope: &TenantScope) -> Result<(), CommerceError> {
    if matches!(
        scope,
        TenantScope::Company { .. } | TenantScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(CommerceError::InvalidScope)
    }
}
fn require_id(value: Uuid, field: &'static str) -> Result<(), CommerceError> {
    if value.is_nil() {
        Err(CommerceError::NilIdentifier { field })
    } else {
        Ok(())
    }
}
fn require_text(value: &str, field: &'static str, max: usize) -> Result<(), CommerceError> {
    if value.trim().is_empty() || value.len() > max {
        Err(CommerceError::InvalidText { field })
    } else {
        Ok(())
    }
}
fn valid_digest(value: &str) -> bool {
    let raw = value.strip_prefix("sha256:").unwrap_or(value);
    raw.len() == 64 && raw.bytes().all(|byte| byte.is_ascii_hexdigit())
}
fn ensure(
    expected: u64,
    actual: u64,
    at: DateTime<Utc>,
    updated: DateTime<Utc>,
) -> Result<(), CommerceError> {
    if expected != actual {
        Err(CommerceError::VersionConflict)
    } else if at < updated {
        Err(CommerceError::InvalidTimestamp)
    } else {
        Ok(())
    }
}
fn next(value: u64) -> Result<u64, CommerceError> {
    value.checked_add(1).ok_or(CommerceError::VersionOverflow)
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum CommerceError {
    #[error("{field} is nil")]
    NilIdentifier { field: &'static str },
    #[error("{field} is invalid")]
    InvalidText { field: &'static str },
    #[error("commerce scope must be an exact company or branch")]
    InvalidScope,
    #[error("commerce realm boundary was violated")]
    RealmViolation,
    #[error("maker-checker policy rejected the action")]
    SegregationOfDuties,
    #[error("invalid channel")]
    InvalidChannel,
    #[error("invalid listing")]
    InvalidListing,
    #[error("listing is unavailable or stale")]
    ListingUnavailable,
    #[error("cart is empty")]
    EmptyCart,
    #[error("checkout evidence is invalid")]
    InvalidCheckout,
    #[error("fulfillment request evidence or owner pins are invalid")]
    InvalidFulfillment,
    #[error("checkout source versions changed")]
    StaleCheckout,
    #[error("external owner boundary was violated")]
    OwnerBoundaryViolation,
    #[error("the checkout already has an order")]
    DuplicateOrder,
    #[error("an authority already decided this handoff")]
    DuplicateDecision,
    #[error("evidence is invalid")]
    InvalidEvidence,
    #[error("invalid lifecycle transition")]
    InvalidTransition,
    #[error("aggregate version conflict")]
    VersionConflict,
    #[error("timestamp precedes current state")]
    InvalidTimestamp,
    #[error("aggregate version overflow")]
    VersionOverflow,
}

#[cfg(test)]
mod tests;
