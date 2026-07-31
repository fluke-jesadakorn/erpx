//! Target-neutral POS route, read-state, and display contracts.
//!
//! This module is a presentation boundary for an already-authorized POS read. It contains no
//! transport, database, authentication material, or write command surface. The display models
//! retain only normalized sale facts and payment aggregates; instrument-specific values are not
//! part of their shape.

use std::{error::Error, fmt};

use crate::{
    Locale, NavItem, ProjectionFreshness, RouteAuthRequirement, TranslationKey, UiStateKind,
    redact_deep_link,
};

/// Maximum bytes retained for an opaque POS sale route identifier.
pub const MAX_POS_SALE_ID_BYTES: usize = 128;
/// Maximum bytes retained for an opaque POS line identifier.
pub const MAX_POS_LINE_ID_BYTES: usize = 128;
/// Maximum bytes retained for a POS display value.
pub const MAX_POS_DISPLAY_VALUE_BYTES: usize = 255;
/// Maximum number of lines retained on one sale display model.
pub const MAX_POS_DISPLAY_LINES: usize = 1_000;
/// Maximum number of payment method aggregates retained on one sale display model.
pub const MAX_POS_PAYMENT_SUMMARIES: usize = 16;

/// Stable identifiers for the allow-listed POS route set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosRouteId {
    /// POS landing page.
    Pos,
    /// Sale list.
    Sales,
    /// Sale detail page.
    Sale,
    /// Register list.
    Registers,
    /// Receipt list.
    Receipts,
    /// Refund list.
    Refunds,
    /// Shift-close page.
    ShiftClose,
}

/// A target-neutral POS route definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PosRouteDefinition {
    /// Stable route identifier.
    pub id: PosRouteId,
    /// Canonical route template.
    pub template: &'static str,
    /// Existing localization key for the route label.
    pub label: TranslationKey,
    /// Authentication metadata mirrored for navigation UX.
    pub auth: RouteAuthRequirement,
    /// Static icon token interpreted by the consuming target theme.
    pub icon: &'static str,
}

impl PosRouteDefinition {
    /// Return the route label in the requested locale.
    pub const fn label_for(self, locale: Locale) -> &'static str {
        self.label.text(locale)
    }

    /// Build a target-neutral navigation item.
    pub fn nav_item(self, locale: Locale) -> NavItem {
        NavItem::new(self.label_for(locale), self.template, self.icon)
    }
}

/// Canonical POS route allow-list in stable display order.
pub const POS_ROUTES: &[PosRouteDefinition] = &[
    PosRouteDefinition {
        id: PosRouteId::Pos,
        template: "/pos",
        label: TranslationKey::Pos,
        auth: RouteAuthRequirement::Authenticated,
        icon: "⌂",
    },
    PosRouteDefinition {
        id: PosRouteId::Sales,
        template: "/pos/sales",
        label: TranslationKey::PosSales,
        auth: RouteAuthRequirement::Authenticated,
        icon: "▦",
    },
    PosRouteDefinition {
        id: PosRouteId::Sale,
        template: "/pos/sales/:id",
        label: TranslationKey::PosSale,
        auth: RouteAuthRequirement::Authenticated,
        icon: "▤",
    },
    PosRouteDefinition {
        id: PosRouteId::Registers,
        template: "/pos/registers",
        label: TranslationKey::PosRegisters,
        auth: RouteAuthRequirement::Authenticated,
        icon: "▥",
    },
    PosRouteDefinition {
        id: PosRouteId::Receipts,
        template: "/pos/receipts",
        label: TranslationKey::PosReceipts,
        auth: RouteAuthRequirement::Authenticated,
        icon: "□",
    },
    PosRouteDefinition {
        id: PosRouteId::Refunds,
        template: "/pos/refunds",
        label: TranslationKey::PosRefunds,
        auth: RouteAuthRequirement::Authenticated,
        icon: "↶",
    },
    PosRouteDefinition {
        id: PosRouteId::ShiftClose,
        template: "/pos/shift-close",
        label: TranslationKey::PosShiftClose,
        auth: RouteAuthRequirement::Authenticated,
        icon: "✓",
    },
];

/// Backwards-readable alias for callers that name the exported registry directly.
pub const POS_ROUTE_REGISTRY: &[PosRouteDefinition] = POS_ROUTES;

/// Cheap, copyable handle to the POS route registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct PosRouteRegistry;

impl PosRouteRegistry {
    /// Construct the canonical POS registry.
    pub const fn new() -> Self {
        Self
    }

    /// Return every allow-listed POS route definition in canonical order.
    pub const fn definitions(self) -> &'static [PosRouteDefinition] {
        POS_ROUTES
    }

    /// Resolve a normalized or browser deep link into a typed POS route.
    pub fn resolve(self, path: &str) -> Result<PosRoute, PosRouteParseError> {
        PosRoute::parse(path)
    }

    /// Resolve a path to its stable route identifier.
    pub fn resolve_id(self, path: &str) -> Result<PosRouteId, PosRouteParseError> {
        self.resolve(path).map(|route| route.id())
    }
}

/// A parsed route from the POS allow-list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PosRoute {
    /// POS landing page.
    Pos,
    /// Sale list.
    Sales,
    /// Sale detail with an opaque sale identifier.
    Sale { sale_id: PosSaleId },
    /// Register list.
    Registers,
    /// Receipt list.
    Receipts,
    /// Refund list.
    Refunds,
    /// Shift-close page.
    ShiftClose,
}

impl PosRoute {
    /// Parse a route after removing browser-only fragments and sensitive query material.
    pub fn parse(path: &str) -> Result<Self, PosRouteParseError> {
        let normalized = redact_deep_link(path);
        match normalized.as_str() {
            "/pos" => Ok(Self::Pos),
            "/pos/sales" => Ok(Self::Sales),
            "/pos/registers" => Ok(Self::Registers),
            "/pos/receipts" => Ok(Self::Receipts),
            "/pos/refunds" => Ok(Self::Refunds),
            "/pos/shift-close" => Ok(Self::ShiftClose),
            _ => {
                if let Some(value) = normalized.strip_prefix("/pos/sales/")
                    && !value.contains('/')
                {
                    let sale_id = PosSaleId::new(value)
                        .map_err(|_| PosRouteParseError::new(normalized.clone()))?;
                    return Ok(Self::Sale { sale_id });
                }
                Err(PosRouteParseError::new(normalized))
            }
        }
    }

    /// Return the stable route identifier.
    pub const fn id(&self) -> PosRouteId {
        match self {
            Self::Pos => PosRouteId::Pos,
            Self::Sales => PosRouteId::Sales,
            Self::Sale { .. } => PosRouteId::Sale,
            Self::Registers => PosRouteId::Registers,
            Self::Receipts => PosRouteId::Receipts,
            Self::Refunds => PosRouteId::Refunds,
            Self::ShiftClose => PosRouteId::ShiftClose,
        }
    }

    /// Return the canonical path, including the opaque sale identifier for detail routes.
    pub fn path(&self) -> String {
        match self {
            Self::Pos => "/pos".into(),
            Self::Sales => "/pos/sales".into(),
            Self::Sale { sale_id } => format!("/pos/sales/{}", sale_id.as_str()),
            Self::Registers => "/pos/registers".into(),
            Self::Receipts => "/pos/receipts".into(),
            Self::Refunds => "/pos/refunds".into(),
            Self::ShiftClose => "/pos/shift-close".into(),
        }
    }

    /// Return the detail identifier when this is a sale route.
    pub const fn sale_id(&self) -> Option<&PosSaleId> {
        match self {
            Self::Sale { sale_id } => Some(sale_id),
            _ => None,
        }
    }
}

impl fmt::Display for PosRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.path())
    }
}

/// Error returned when a path is not in the POS allow-list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosRouteParseError {
    /// Normalized, non-query path that failed to resolve.
    pub normalized_path: String,
}

impl PosRouteParseError {
    fn new(normalized_path: String) -> Self {
        Self { normalized_path }
    }
}

impl fmt::Display for PosRouteParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "no ERPX POS route matches {}",
            self.normalized_path
        )
    }
}

impl Error for PosRouteParseError {}

/// Error returned for an unsafe opaque POS identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosIdentifierError {
    /// The identifier is empty.
    Empty,
    /// The identifier exceeds its bounded route/display size.
    TooLong,
    /// The identifier contains path, control, or whitespace characters.
    UnsafeCharacters,
    /// The identifier resembles sensitive transport material.
    SensitiveMarker,
}

impl fmt::Display for PosIdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "POS identifier is empty",
            Self::TooLong => "POS identifier is too long",
            Self::UnsafeCharacters => "POS identifier contains unsafe characters",
            Self::SensitiveMarker => "POS identifier contains a sensitive marker",
        })
    }
}

impl Error for PosIdentifierError {}

/// Validated opaque sale identifier used by the POS route surface.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PosSaleId(String);

impl PosSaleId {
    /// Validate an opaque route segment without interpreting its identifier scheme.
    pub fn new(value: impl Into<String>) -> Result<Self, PosIdentifierError> {
        let value = value.into();
        validate_opaque_identifier(&value, MAX_POS_SALE_ID_BYTES)?;
        Ok(Self(value))
    }

    /// Borrow the validated identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for PosSaleId {
    type Error = PosIdentifierError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for PosSaleId {
    type Error = PosIdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

/// Validated opaque POS line identifier retained by a sale display model.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct PosLineId(String);

impl PosLineId {
    /// Validate an opaque line identifier.
    pub fn new(value: impl Into<String>) -> Result<Self, PosIdentifierError> {
        let value = value.into();
        validate_opaque_identifier(&value, MAX_POS_LINE_ID_BYTES)?;
        Ok(Self(value))
    }

    /// Borrow the validated line identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// POS sale lifecycle state exposed to a display client.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum PosSaleStatus {
    /// Sale is being assembled.
    Draft,
    /// Sale is open for display/operation at the source boundary.
    Open,
    /// Sale is held.
    Held,
    /// Sale is complete.
    Completed,
    /// Sale is voided.
    Voided,
    /// Sale is refunded.
    Refunded,
}

impl PosSaleStatus {
    /// Parse the bounded status token emitted by a POS read adapter.
    pub fn from_transport(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "draft" => Some(Self::Draft),
            "open" => Some(Self::Open),
            "held" => Some(Self::Held),
            "completed" => Some(Self::Completed),
            "voided" => Some(Self::Voided),
            "refunded" => Some(Self::Refunded),
            _ => None,
        }
    }

    /// Return the stable transport token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Open => "open",
            Self::Held => "held",
            Self::Completed => "completed",
            Self::Voided => "voided",
            Self::Refunded => "refunded",
        }
    }

    /// Return the localized lifecycle label.
    pub const fn label(self, locale: Locale) -> &'static str {
        let key = match self {
            Self::Draft => TranslationKey::PosStatusDraft,
            Self::Open => TranslationKey::PosStatusOpen,
            Self::Held => TranslationKey::PosStatusHeld,
            Self::Completed => TranslationKey::PosStatusCompleted,
            Self::Voided => TranslationKey::PosStatusVoided,
            Self::Refunded => TranslationKey::PosStatusRefunded,
        };
        key.text(locale)
    }
}

/// Coarse payment method category used only for safe payment aggregates.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum PosPaymentMethod {
    /// Cash category.
    Cash,
    /// Card category without instrument details.
    Card,
    /// Bank-transfer category without account details.
    BankTransfer,
    /// Mobile-wallet category without provider or instrument details.
    MobileWallet,
    /// Voucher category.
    Voucher,
    /// Other category supplied by the source boundary.
    Other,
}

impl PosPaymentMethod {
    /// Parse the bounded method token emitted by a POS read adapter.
    pub fn from_transport(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "cash" => Some(Self::Cash),
            "card" => Some(Self::Card),
            "bank_transfer" => Some(Self::BankTransfer),
            "mobile_wallet" => Some(Self::MobileWallet),
            "voucher" => Some(Self::Voucher),
            "other" => Some(Self::Other),
            _ => None,
        }
    }

    /// Return the stable transport token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Cash => "cash",
            Self::Card => "card",
            Self::BankTransfer => "bank_transfer",
            Self::MobileWallet => "mobile_wallet",
            Self::Voucher => "voucher",
            Self::Other => "other",
        }
    }

    /// Return the localized aggregate label.
    pub const fn label(self, locale: Locale) -> &'static str {
        let key = match self {
            Self::Cash => TranslationKey::PosPaymentCash,
            Self::Card => TranslationKey::PosPaymentCard,
            Self::BankTransfer => TranslationKey::PosPaymentBankTransfer,
            Self::MobileWallet => TranslationKey::PosPaymentMobileWallet,
            Self::Voucher => TranslationKey::PosPaymentVoucher,
            Self::Other => TranslationKey::PosPaymentOther,
        };
        key.text(locale)
    }
}

/// Alias that emphasizes the redacted nature of the payment category.
pub type PosPaymentCategory = PosPaymentMethod;

/// Error returned for display values that cannot cross the POS display boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosDisplayValueError {
    /// The value is empty.
    Required,
    /// The value exceeds its bounded display size.
    TooLong,
    /// The value contains a control character.
    ControlCharacter,
    /// The value looks like an unstructured serialized payload.
    StructuredValue,
    /// The value contains a sensitive transport marker.
    SensitiveMarker,
}

impl fmt::Display for PosDisplayValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Required => "POS display value is required",
            Self::TooLong => "POS display value is too long",
            Self::ControlCharacter => "POS display value contains a control character",
            Self::StructuredValue => "POS display value must be a scalar value",
            Self::SensitiveMarker => "POS display value contains a sensitive marker",
        })
    }
}

impl Error for PosDisplayValueError {}

/// Error returned when a currency code is not a bounded uppercase ISO-like token.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PosCurrencyError;

impl fmt::Display for PosCurrencyError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("POS currency must be a three-letter uppercase code")
    }
}

impl Error for PosCurrencyError {}

/// Error returned when a safe POS display model cannot be constructed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosViewModelError {
    /// A typed identifier was invalid.
    InvalidIdentifier(PosIdentifierError),
    /// A scalar display value was invalid.
    InvalidDisplayValue(PosDisplayValueError),
    /// A currency code was invalid.
    InvalidCurrency,
    /// A monetary value or quantity was invalid.
    InvalidAmount,
    /// A sale must retain at least one line.
    EmptyLines,
    /// A sale exceeded the bounded line count.
    TooManyLines,
    /// A sale exceeded the bounded payment aggregate count.
    TooManyPaymentSummaries,
    /// Sale totals did not reconcile.
    InconsistentTotal,
}

impl fmt::Display for PosViewModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::InvalidIdentifier(error) => return error.fmt(formatter),
            Self::InvalidDisplayValue(error) => return error.fmt(formatter),
            Self::InvalidCurrency => "POS currency is invalid",
            Self::InvalidAmount => "POS amount or quantity is invalid",
            Self::EmptyLines => "POS sale has no lines",
            Self::TooManyLines => "POS sale has too many lines",
            Self::TooManyPaymentSummaries => "POS sale has too many payment summaries",
            Self::InconsistentTotal => "POS sale totals do not reconcile",
        })
    }
}

impl Error for PosViewModelError {}

impl From<PosIdentifierError> for PosViewModelError {
    fn from(error: PosIdentifierError) -> Self {
        Self::InvalidIdentifier(error)
    }
}

impl From<PosDisplayValueError> for PosViewModelError {
    fn from(error: PosDisplayValueError) -> Self {
        Self::InvalidDisplayValue(error)
    }
}

/// A validated three-letter currency code.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosCurrency(String);

impl PosCurrency {
    /// Validate and construct a currency code.
    pub fn new(value: impl Into<String>) -> Result<Self, PosCurrencyError> {
        let value = value.into();
        if value.len() != 3 || !value.bytes().all(|byte| byte.is_ascii_uppercase()) {
            return Err(PosCurrencyError);
        }
        Ok(Self(value))
    }

    /// Borrow the validated currency code.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// A redacted sale line display model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosSaleLine {
    line_id: PosLineId,
    item_reference: String,
    description: String,
    quantity: u64,
    unit_price_minor: i64,
    tax_minor: i64,
    total_minor: i64,
}

impl PosSaleLine {
    /// Construct a line from scalar projection facts only.
    pub fn new(
        line_id: PosLineId,
        item_reference: impl Into<String>,
        description: impl Into<String>,
        quantity: u64,
        unit_price_minor: i64,
        tax_minor: i64,
    ) -> Result<Self, PosViewModelError> {
        if quantity == 0 || unit_price_minor < 0 || tax_minor < 0 {
            return Err(PosViewModelError::InvalidAmount);
        }
        let item_reference = safe_display_value(item_reference.into())?;
        let description = safe_display_value(description.into())?;
        let subtotal_minor = i64::try_from(quantity)
            .ok()
            .and_then(|quantity| quantity.checked_mul(unit_price_minor))
            .ok_or(PosViewModelError::InvalidAmount)?;
        let total_minor = subtotal_minor
            .checked_add(tax_minor)
            .ok_or(PosViewModelError::InvalidAmount)?;
        Ok(Self {
            line_id,
            item_reference,
            description,
            quantity,
            unit_price_minor,
            tax_minor,
            total_minor,
        })
    }

    /// Borrow the opaque line identifier.
    pub const fn line_id(&self) -> &PosLineId {
        &self.line_id
    }

    /// Borrow the safe item reference.
    pub fn item_reference(&self) -> &str {
        &self.item_reference
    }

    /// Borrow the safe display description.
    pub fn description(&self) -> &str {
        &self.description
    }

    /// Return the line quantity.
    pub const fn quantity(&self) -> u64 {
        self.quantity
    }

    /// Return the unit price in minor currency units.
    pub const fn unit_price_minor(&self) -> i64 {
        self.unit_price_minor
    }

    /// Return line tax in minor currency units.
    pub const fn tax_minor(&self) -> i64 {
        self.tax_minor
    }

    /// Return the reconciled line total in minor currency units.
    pub const fn total_minor(&self) -> i64 {
        self.total_minor
    }
}

/// A redacted aggregate of payments grouped by coarse method category.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PosPaymentSummary {
    method: PosPaymentMethod,
    payment_count: u32,
    amount_minor: i64,
}

impl PosPaymentSummary {
    /// Construct a payment aggregate without retaining individual payment records.
    pub fn new(
        method: PosPaymentMethod,
        payment_count: u32,
        amount_minor: i64,
    ) -> Result<Self, PosViewModelError> {
        if payment_count == 0 || amount_minor <= 0 {
            return Err(PosViewModelError::InvalidAmount);
        }
        Ok(Self {
            method,
            payment_count,
            amount_minor,
        })
    }

    /// Return the coarse payment category.
    pub const fn method(self) -> PosPaymentMethod {
        self.method
    }

    /// Return the number of payments in this aggregate.
    pub const fn payment_count(self) -> u32 {
        self.payment_count
    }

    /// Return the aggregate amount in minor currency units.
    pub const fn amount_minor(self) -> i64 {
        self.amount_minor
    }
}

/// A redacted POS sale summary suitable for a future authenticated read adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosSaleSummary {
    id: PosSaleId,
    reference: String,
    currency: PosCurrency,
    status: PosSaleStatus,
    lines: Vec<PosSaleLine>,
    payment_summaries: Vec<PosPaymentSummary>,
    subtotal_minor: i64,
    tax_minor: i64,
    total_minor: i64,
    paid_minor: i64,
}

impl PosSaleSummary {
    /// Construct a sale summary from normalized scalar facts and redacted child models.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: PosSaleId,
        reference: impl Into<String>,
        currency: impl Into<String>,
        status: PosSaleStatus,
        lines: Vec<PosSaleLine>,
        payment_summaries: Vec<PosPaymentSummary>,
        subtotal_minor: i64,
        tax_minor: i64,
        total_minor: i64,
        paid_minor: i64,
    ) -> Result<Self, PosViewModelError> {
        if lines.is_empty() {
            return Err(PosViewModelError::EmptyLines);
        }
        if lines.len() > MAX_POS_DISPLAY_LINES {
            return Err(PosViewModelError::TooManyLines);
        }
        if payment_summaries.len() > MAX_POS_PAYMENT_SUMMARIES {
            return Err(PosViewModelError::TooManyPaymentSummaries);
        }
        let reference = safe_display_value(reference.into())?;
        let currency =
            PosCurrency::new(currency.into()).map_err(|_| PosViewModelError::InvalidCurrency)?;
        if subtotal_minor < 0
            || tax_minor < 0
            || total_minor < 0
            || paid_minor < 0
            || paid_minor > total_minor
            || subtotal_minor.checked_add(tax_minor) != Some(total_minor)
        {
            return Err(PosViewModelError::InconsistentTotal);
        }
        let line_total = lines.iter().try_fold(0_i64, |total, line| {
            total
                .checked_add(line.total_minor)
                .ok_or(PosViewModelError::InconsistentTotal)
        })?;
        if line_total != total_minor {
            return Err(PosViewModelError::InconsistentTotal);
        }
        let payment_total = payment_summaries.iter().try_fold(0_i64, |total, payment| {
            total
                .checked_add(payment.amount_minor)
                .ok_or(PosViewModelError::InconsistentTotal)
        })?;
        if payment_total != paid_minor {
            return Err(PosViewModelError::InconsistentTotal);
        }
        Ok(Self {
            id,
            reference,
            currency,
            status,
            lines,
            payment_summaries,
            subtotal_minor,
            tax_minor,
            total_minor,
            paid_minor,
        })
    }

    /// Borrow the opaque sale identifier.
    pub const fn id(&self) -> &PosSaleId {
        &self.id
    }

    /// Borrow the safe sale reference.
    pub fn reference(&self) -> &str {
        &self.reference
    }

    /// Borrow the validated currency.
    pub const fn currency(&self) -> &PosCurrency {
        &self.currency
    }

    /// Return the sale lifecycle status.
    pub const fn status(&self) -> PosSaleStatus {
        self.status
    }

    /// Borrow redacted sale lines.
    pub fn lines(&self) -> &[PosSaleLine] {
        &self.lines
    }

    /// Borrow redacted payment aggregates.
    pub fn payment_summaries(&self) -> &[PosPaymentSummary] {
        &self.payment_summaries
    }

    /// Return the subtotal in minor currency units.
    pub const fn subtotal_minor(&self) -> i64 {
        self.subtotal_minor
    }

    /// Return tax in minor currency units.
    pub const fn tax_minor(&self) -> i64 {
        self.tax_minor
    }

    /// Return the total in minor currency units.
    pub const fn total_minor(&self) -> i64 {
        self.total_minor
    }

    /// Return the paid amount in minor currency units.
    pub const fn paid_minor(&self) -> i64 {
        self.paid_minor
    }
}

/// Readable aliases for consumers that prefer view-model terminology.
pub type PosSaleViewModel = PosSaleSummary;
/// Readable alias for a sale summary view model.
pub type PosSaleSummaryViewModel = PosSaleSummary;
/// Readable alias for a sale line view model.
pub type PosSaleLineViewModel = PosSaleLine;
/// Readable alias for a payment summary view model.
pub type PosPaymentSummaryViewModel = PosPaymentSummary;

/// Safe projection freshness and mutation metadata for POS reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosProjectionMetadata {
    /// Opaque display timestamp supplied by the read boundary.
    pub projection_updated_at: Option<String>,
    /// Freshness supplied by the read boundary.
    pub freshness: ProjectionFreshness,
    /// Whether the read surface is explicitly non-mutating.
    pub read_only: bool,
}

impl PosProjectionMetadata {
    /// Build metadata for a fresh read projection.
    pub fn fresh(projection_updated_at: impl Into<String>) -> Self {
        Self::present(projection_updated_at, ProjectionFreshness::Fresh)
    }

    /// Build metadata for a stale but displayable read projection.
    pub fn stale(projection_updated_at: impl Into<String>) -> Self {
        Self::present(projection_updated_at, ProjectionFreshness::Stale)
    }

    /// Build metadata when the read boundary has no projection timestamp.
    pub fn unknown() -> Self {
        Self {
            projection_updated_at: None,
            freshness: ProjectionFreshness::Unknown,
            read_only: true,
        }
    }

    /// Build bounded metadata with an explicit freshness classification.
    pub fn present(
        projection_updated_at: impl Into<String>,
        freshness: ProjectionFreshness,
    ) -> Self {
        let projection_updated_at = safe_optional_display_value(projection_updated_at.into());
        Self {
            freshness: if projection_updated_at.is_some() {
                freshness
            } else {
                ProjectionFreshness::Unknown
            },
            projection_updated_at,
            read_only: true,
        }
    }

    /// Set the explicit read-only flag.
    pub const fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Whether the projection declares a read-only context.
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Whether the projection is stale.
    pub const fn is_stale(&self) -> bool {
        matches!(self.freshness, ProjectionFreshness::Stale)
    }

    /// Whether the projection is fresh.
    pub const fn is_fresh(&self) -> bool {
        matches!(self.freshness, ProjectionFreshness::Fresh)
    }
}

/// All explicit POS page state kinds, including the successful ready state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PosPageStateKind {
    /// A read is in progress.
    Loading,
    /// The read completed with no rows/content.
    Empty,
    /// Safe display data is available.
    Ready,
    /// The principal cannot view the page.
    Denied,
    /// Cached data is outside the freshness budget.
    Stale,
    /// The page is explicitly non-mutating.
    ReadOnly,
    /// A revision or write boundary conflict needs review.
    Conflict,
    /// The backend is unavailable.
    Offline,
    /// An unexpected read failure occurred.
    Error,
}

impl PosPageStateKind {
    /// Map a non-ready POS state to the shared UI state kind.
    pub const fn shared_kind(self) -> Option<UiStateKind> {
        match self {
            Self::Loading => Some(UiStateKind::Loading),
            Self::Empty => Some(UiStateKind::Empty),
            Self::Ready => None,
            Self::Denied => Some(UiStateKind::Denied),
            Self::Stale => Some(UiStateKind::Stale),
            Self::ReadOnly => Some(UiStateKind::ReadOnly),
            Self::Conflict => Some(UiStateKind::Conflict),
            Self::Offline => Some(UiStateKind::Offline),
            Self::Error => Some(UiStateKind::Error),
        }
    }

    /// Return the shared bilingual label for this state kind.
    pub const fn label(self, locale: Locale) -> Option<&'static str> {
        let key = match self {
            Self::Loading => TranslationKey::Loading,
            Self::Empty => TranslationKey::Empty,
            Self::Ready => return None,
            Self::Denied => TranslationKey::AccessDenied,
            Self::Stale => TranslationKey::Stale,
            Self::ReadOnly => TranslationKey::ReadOnly,
            Self::Conflict => TranslationKey::Conflict,
            Self::Offline => TranslationKey::Offline,
            Self::Error => TranslationKey::Error,
        };
        Some(key.text(locale))
    }
}

/// Explicit POS page state for a future authenticated read transport.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PosPageState<T> {
    /// A read is in progress.
    Loading {
        /// Previously known metadata, if any.
        metadata: Option<PosProjectionMetadata>,
    },
    /// The read completed with no rows/content.
    Empty {
        /// Metadata accompanying the empty result.
        metadata: PosProjectionMetadata,
    },
    /// Current safe display data is available.
    Ready {
        /// Redacted display payload.
        data: T,
        /// Projection metadata.
        metadata: PosProjectionMetadata,
    },
    /// The principal cannot view this page.
    Denied {
        /// Metadata may be absent when denial precedes the read.
        metadata: Option<PosProjectionMetadata>,
        /// Safe policy/message text.
        reason: String,
    },
    /// Cached safe display data is outside the freshness budget.
    Stale {
        /// Last safe display payload.
        data: T,
        /// Stale projection metadata.
        metadata: PosProjectionMetadata,
        /// Age supplied by the read boundary.
        age_seconds: u64,
        /// Safe freshness explanation.
        reason: String,
    },
    /// Data is available but this surface cannot mutate it.
    ReadOnly {
        /// Last safe display payload.
        data: T,
        /// Projection metadata.
        metadata: PosProjectionMetadata,
        /// Safe read-only explanation.
        reason: String,
    },
    /// A concurrent revision/write boundary conflict needs review.
    Conflict {
        /// Last safe display payload, when available.
        data: Option<T>,
        /// Projection metadata, when available.
        metadata: Option<PosProjectionMetadata>,
        /// Safe conflict explanation.
        reason: String,
    },
    /// The backend is unavailable; cached safe data may remain visible.
    Offline {
        /// Cached safe display payload, when available.
        data: Option<T>,
        /// Cached projection metadata, when available.
        metadata: Option<PosProjectionMetadata>,
        /// Safe connectivity explanation.
        reason: String,
    },
    /// An unexpected read failure occurred.
    Error {
        /// Safe error explanation.
        reason: String,
    },
}

impl<T> PosPageState<T> {
    /// Return the explicit POS state kind.
    pub const fn state_kind(&self) -> PosPageStateKind {
        match self {
            Self::Loading { .. } => PosPageStateKind::Loading,
            Self::Empty { .. } => PosPageStateKind::Empty,
            Self::Ready { .. } => PosPageStateKind::Ready,
            Self::Denied { .. } => PosPageStateKind::Denied,
            Self::Stale { .. } => PosPageStateKind::Stale,
            Self::ReadOnly { .. } => PosPageStateKind::ReadOnly,
            Self::Conflict { .. } => PosPageStateKind::Conflict,
            Self::Offline { .. } => PosPageStateKind::Offline,
            Self::Error { .. } => PosPageStateKind::Error,
        }
    }

    /// Return the shared UI state kind for non-ready states.
    pub const fn kind(&self) -> Option<UiStateKind> {
        self.state_kind().shared_kind()
    }

    /// Return the shared bilingual state label for non-ready states.
    pub const fn state_label(&self, locale: Locale) -> Option<&'static str> {
        self.state_kind().label(locale)
    }

    /// Return safe current or cached data when the state has one.
    pub fn data(&self) -> Option<&T> {
        match self {
            Self::Ready { data, .. } | Self::Stale { data, .. } | Self::ReadOnly { data, .. } => {
                Some(data)
            }
            Self::Conflict { data, .. } | Self::Offline { data, .. } => data.as_ref(),
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Denied { .. }
            | Self::Error { .. } => None,
        }
    }

    /// Return projection metadata when available.
    pub fn metadata(&self) -> Option<&PosProjectionMetadata> {
        match self {
            Self::Loading { metadata }
            | Self::Denied { metadata, .. }
            | Self::Conflict { metadata, .. }
            | Self::Offline { metadata, .. } => metadata.as_ref(),
            Self::Empty { metadata }
            | Self::Ready { metadata, .. }
            | Self::Stale { metadata, .. }
            | Self::ReadOnly { metadata, .. } => Some(metadata),
            Self::Error { .. } => None,
        }
    }

    /// Whether the state or its projection metadata is read-only.
    pub fn is_read_only(&self) -> bool {
        matches!(self.state_kind(), PosPageStateKind::ReadOnly)
            || self
                .metadata()
                .is_some_and(PosProjectionMetadata::is_read_only)
    }

    /// POS-004 exposes only a read surface, so no state can authorize a mutation.
    pub const fn can_mutate(&self) -> bool {
        false
    }

    /// Whether a safe display payload is available for rendering.
    pub fn can_render_data(&self) -> bool {
        self.data().is_some()
    }
}

/// Concrete state type for redacted POS sale summaries.
pub type PosSaleState = PosPageState<PosSaleSummary>;
/// Generic alias for callers that use view-state terminology.
pub type PosViewState<T = PosSaleSummary> = PosPageState<T>;
/// Generic alias for callers that use state terminology.
pub type PosState<T = PosSaleSummary> = PosPageState<T>;

fn validate_opaque_identifier(value: &str, max_bytes: usize) -> Result<(), PosIdentifierError> {
    if value.is_empty() {
        return Err(PosIdentifierError::Empty);
    }
    if value.len() > max_bytes {
        return Err(PosIdentifierError::TooLong);
    }
    if value == "."
        || value == ".."
        || value.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '/' | '\\' | '?' | '#')
        })
    {
        return Err(PosIdentifierError::UnsafeCharacters);
    }
    if has_sensitive_marker(value) {
        return Err(PosIdentifierError::SensitiveMarker);
    }
    Ok(())
}

fn safe_display_value(value: String) -> Result<String, PosDisplayValueError> {
    if value.trim().is_empty() {
        return Err(PosDisplayValueError::Required);
    }
    if value.len() > MAX_POS_DISPLAY_VALUE_BYTES {
        return Err(PosDisplayValueError::TooLong);
    }
    if value.chars().any(char::is_control) {
        return Err(PosDisplayValueError::ControlCharacter);
    }
    if value.contains(['{', '}', '[', ']']) {
        return Err(PosDisplayValueError::StructuredValue);
    }
    if has_sensitive_marker(&value) {
        return Err(PosDisplayValueError::SensitiveMarker);
    }
    Ok(value)
}

fn safe_optional_display_value(value: String) -> Option<String> {
    safe_display_value(value).ok()
}

fn has_sensitive_marker(value: &str) -> bool {
    const MARKERS: &[&str] = &[
        "access_token",
        "refresh_token",
        "id_token",
        "authorization",
        "client_secret",
        "password",
        "provider_token",
        "secret",
    ];
    let lower = value.to_ascii_lowercase();
    MARKERS.iter().any(|marker| lower.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn metadata() -> PosProjectionMetadata {
        PosProjectionMetadata::fresh("2026-07-25T00:00:00Z")
    }

    fn sale() -> PosSaleSummary {
        let line = PosSaleLine::new(
            PosLineId::new("line-1").expect("valid line id"),
            "sku-1",
            "Coffee",
            2,
            100,
            20,
        )
        .expect("valid line");
        let payment =
            PosPaymentSummary::new(PosPaymentMethod::Cash, 1, 220).expect("valid payment summary");
        PosSaleSummary::new(
            PosSaleId::new("opaque-sale-1").expect("valid sale id"),
            "sale-ref-1",
            "THB",
            PosSaleStatus::Completed,
            vec![line],
            vec![payment],
            200,
            20,
            220,
            220,
        )
        .expect("valid sale summary")
    }

    #[test]
    fn route_registry_normalizes_and_enforces_the_allow_list() {
        let expected = [
            (PosRouteId::Pos, "/pos"),
            (PosRouteId::Sales, "/pos/sales"),
            (PosRouteId::Sale, "/pos/sales/sale-1"),
            (PosRouteId::Registers, "/pos/registers"),
            (PosRouteId::Receipts, "/pos/receipts"),
            (PosRouteId::Refunds, "/pos/refunds"),
            (PosRouteId::ShiftClose, "/pos/shift-close"),
        ];
        let registry = PosRouteRegistry::new();
        assert_eq!(registry.definitions().len(), expected.len());
        for (definition, (id, path)) in registry.definitions().iter().zip(expected) {
            assert_eq!(definition.id, id);
            assert_eq!(definition.auth, RouteAuthRequirement::Authenticated);
            assert_eq!(registry.resolve_id(&format!("{path}/?tab=summary")), Ok(id));
        }
        assert_eq!(
            registry.resolve("/#/pos/sales/sale-1"),
            Ok(PosRoute::Sale {
                sale_id: PosSaleId::new("sale-1").expect("valid sale id"),
            })
        );
        assert!(registry.resolve("/pos/sales/sale-1/extra").is_err());
        assert!(registry.resolve("/pos/unknown").is_err());
        assert!(registry.resolve("/pos/sales?access_token=hidden").is_err());
        assert_eq!(crate::normalize_deep_link("/pos//sales///"), "/pos/sales");
    }

    #[test]
    fn opaque_sale_ids_are_bounded_and_not_interpreted() {
        let id = PosSaleId::new("not-a-uuid-but-opaque").expect("opaque IDs are supported");
        assert_eq!(id.as_str(), "not-a-uuid-but-opaque");
        assert!(PosSaleId::new("").is_err());
        assert!(PosSaleId::new("sale/id").is_err());
        assert!(PosSaleId::new("sale id").is_err());
        assert!(PosSaleId::new("..").is_err());
        assert!(PosSaleId::new("refresh_token-value").is_err());
        assert!(PosSaleId::new("x".repeat(MAX_POS_SALE_ID_BYTES + 1)).is_err());
    }

    #[test]
    fn page_states_keep_shared_kinds_data_and_mutation_guards_distinct() {
        let display = sale();
        let states = [
            PosSaleState::Loading {
                metadata: Some(metadata()),
            },
            PosSaleState::Empty {
                metadata: metadata(),
            },
            PosSaleState::Denied {
                metadata: None,
                reason: "policy".into(),
            },
            PosSaleState::Ready {
                data: display.clone(),
                metadata: metadata(),
            },
            PosSaleState::Stale {
                data: display.clone(),
                metadata: PosProjectionMetadata::stale("2026-07-24T00:00:00Z"),
                age_seconds: 301,
                reason: "projection lag".into(),
            },
            PosSaleState::ReadOnly {
                data: display.clone(),
                metadata: metadata(),
                reason: "POS read surface".into(),
            },
            PosSaleState::Conflict {
                data: Some(display.clone()),
                metadata: Some(metadata()),
                reason: "revision changed".into(),
            },
            PosSaleState::Offline {
                data: Some(display),
                metadata: Some(metadata()),
                reason: "backend unavailable".into(),
            },
        ];
        let shared_kinds: Vec<_> = states.iter().filter_map(PosPageState::kind).collect();
        assert_eq!(
            shared_kinds,
            vec![
                UiStateKind::Loading,
                UiStateKind::Empty,
                UiStateKind::Denied,
                UiStateKind::Stale,
                UiStateKind::ReadOnly,
                UiStateKind::Conflict,
                UiStateKind::Offline,
            ]
        );
        assert!(states[0].data().is_none());
        assert!(states[3].can_render_data());
        assert!(states[4].is_read_only());
        assert!(states[5].is_read_only());
        assert!(states[6].can_render_data());
        assert!(states[7].can_render_data());
        assert!(states.iter().all(|state| !state.can_mutate()));
    }

    #[test]
    fn route_status_payment_labels_are_bilingual() {
        for definition in POS_ROUTES {
            assert!(!definition.label_for(Locale::En).is_empty());
            assert!(!definition.label_for(Locale::Th).is_empty());
            assert_ne!(
                definition.label_for(Locale::En),
                definition.label_for(Locale::Th)
            );
            assert_eq!(definition.nav_item(Locale::Th).path, definition.template);
        }
        for status in [
            PosSaleStatus::Draft,
            PosSaleStatus::Open,
            PosSaleStatus::Held,
            PosSaleStatus::Completed,
            PosSaleStatus::Voided,
            PosSaleStatus::Refunded,
        ] {
            assert_ne!(status.label(Locale::En), status.label(Locale::Th));
        }
        for method in [
            PosPaymentMethod::Cash,
            PosPaymentMethod::Card,
            PosPaymentMethod::BankTransfer,
            PosPaymentMethod::MobileWallet,
            PosPaymentMethod::Voucher,
            PosPaymentMethod::Other,
        ] {
            assert_ne!(method.label(Locale::En), method.label(Locale::Th));
        }
        assert_eq!(PosPageStateKind::Loading.label(Locale::En), Some("Loading"));
        assert_eq!(
            PosPageStateKind::Loading.label(Locale::Th),
            Some("กำลังโหลด")
        );
    }

    #[test]
    fn display_models_are_scalar_and_reject_structured_or_sensitive_values() {
        let invalid_line = PosSaleLine::new(
            PosLineId::new("line-1").expect("valid line id"),
            "sku-1",
            "{structured}",
            1,
            100,
            0,
        );
        assert!(invalid_line.is_err());
        assert!(
            PosSaleLine::new(
                PosLineId::new("line-1").expect("valid line id"),
                "authorization-value",
                "Item",
                1,
                100,
                0,
            )
            .is_err()
        );
        let model = sale();
        assert_eq!(model.lines().len(), 1);
        assert_eq!(model.payment_summaries()[0].payment_count(), 1);
        assert_eq!(model.payment_summaries()[0].amount_minor(), 220);
        assert!(!format!("{model:?}").contains("authorization-value"));
    }
}
