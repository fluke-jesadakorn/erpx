//! Target-neutral Finance routes, projection metadata, and safe display contracts.
//!
//! This module is presentation-only. A consuming application converts an already-authorized
//! read response into these bounded scalar models. There is deliberately no transport client,
//! database handle, credential, token, raw payload, or mutation authorization here.

use std::{error::Error, fmt};

use crate::{
    Locale, NavItem, ProjectionFreshness, RouteAuthRequirement, TranslationKey, UiStateKind,
    redact_deep_link,
};

/// Maximum bytes retained for a Finance opaque identifier or field key.
pub const MAX_FINANCE_IDENTIFIER_BYTES: usize = 128;
/// Maximum bytes retained for a Finance scalar display value or safe message.
pub const MAX_FINANCE_DISPLAY_VALUE_BYTES: usize = 255;
/// Maximum number of fields retained by one Finance display row.
pub const MAX_FINANCE_DISPLAY_FIELDS: usize = 64;
/// Maximum number of rows retained by one Finance read page.
pub const MAX_FINANCE_PAGE_ROWS: usize = 1_000;

/// A bilingual, static label owned by the presentation contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinanceLabel {
    english: &'static str,
    thai: &'static str,
}

impl FinanceLabel {
    /// Construct a bilingual label from static, reviewable copy.
    pub const fn new(english: &'static str, thai: &'static str) -> Self {
        Self { english, thai }
    }

    /// Return the label for a client locale.
    pub const fn text(self, locale: Locale) -> &'static str {
        match locale {
            Locale::En => self.english,
            Locale::Th => self.thai,
        }
    }

    /// Return the English label.
    pub const fn english(self) -> &'static str {
        self.english
    }

    /// Return the Thai label.
    pub const fn thai(self) -> &'static str {
        self.thai
    }
}

/// Stable identifiers for the Finance navigation surfaces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum FinanceRouteId {
    /// Finance landing page.
    Finance,
    /// Chart of accounts.
    Chart,
    /// Journal list and detail surface.
    Journals,
    /// Subledger summaries.
    Subledgers,
    /// Accounts payable.
    AccountsPayable,
    /// Accounts receivable.
    AccountsReceivable,
    /// Cash surface.
    Cash,
    /// Banking and reconciliation surface.
    Banking,
    /// Period-close surface.
    Close,
    /// Budget surface.
    Budgets,
}

/// A target-neutral Finance route definition.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FinanceRouteDefinition {
    /// Stable route identifier.
    pub id: FinanceRouteId,
    /// Canonical route path.
    pub template: &'static str,
    /// Bilingual display label.
    pub label: FinanceLabel,
    /// Authentication metadata mirrored for navigation UX.
    pub auth: RouteAuthRequirement,
    /// Static icon token interpreted by the consuming target theme.
    pub icon: &'static str,
}

impl FinanceRouteDefinition {
    /// Return the route label for a client locale.
    pub const fn label_for(self, locale: Locale) -> &'static str {
        self.label.text(locale)
    }

    /// Build a target-neutral navigation item.
    pub fn nav_item(self, locale: Locale) -> NavItem {
        NavItem::new(self.label_for(locale), self.template, self.icon)
    }
}

const FINANCE_LABEL: FinanceLabel = FinanceLabel::new("Finance", "การเงิน");
const CHART_LABEL: FinanceLabel = FinanceLabel::new("Chart of accounts", "ผังบัญชี");
const JOURNALS_LABEL: FinanceLabel = FinanceLabel::new("Journals", "สมุดรายวัน");
const SUBLEDGERS_LABEL: FinanceLabel = FinanceLabel::new("Subledgers", "บัญชีย่อย");
const AP_LABEL: FinanceLabel = FinanceLabel::new("Accounts payable", "เจ้าหนี้การค้า");
const AR_LABEL: FinanceLabel = FinanceLabel::new("Accounts receivable", "ลูกหนี้การค้า");
const CASH_LABEL: FinanceLabel = FinanceLabel::new("Cash", "เงินสด");
const BANKING_LABEL: FinanceLabel = FinanceLabel::new("Banking", "ธนาคาร");
const CLOSE_LABEL: FinanceLabel = FinanceLabel::new("Period close", "ปิดงวดบัญชี");
const BUDGETS_LABEL: FinanceLabel = FinanceLabel::new("Budgets", "งบประมาณ");

/// Canonical Finance route allow-list in stable display order.
pub const FINANCE_ROUTES: &[FinanceRouteDefinition] = &[
    FinanceRouteDefinition {
        id: FinanceRouteId::Finance,
        template: "/finance",
        label: FINANCE_LABEL,
        auth: RouteAuthRequirement::Authenticated,
        icon: "¤",
    },
    FinanceRouteDefinition {
        id: FinanceRouteId::Chart,
        template: "/finance/chart",
        label: CHART_LABEL,
        auth: RouteAuthRequirement::Authenticated,
        icon: "⌘",
    },
    FinanceRouteDefinition {
        id: FinanceRouteId::Journals,
        template: "/finance/journals",
        label: JOURNALS_LABEL,
        auth: RouteAuthRequirement::Authenticated,
        icon: "▤",
    },
    FinanceRouteDefinition {
        id: FinanceRouteId::Subledgers,
        template: "/finance/subledgers",
        label: SUBLEDGERS_LABEL,
        auth: RouteAuthRequirement::Authenticated,
        icon: "▥",
    },
    FinanceRouteDefinition {
        id: FinanceRouteId::AccountsPayable,
        template: "/finance/ap",
        label: AP_LABEL,
        auth: RouteAuthRequirement::Authenticated,
        icon: "↓",
    },
    FinanceRouteDefinition {
        id: FinanceRouteId::AccountsReceivable,
        template: "/finance/ar",
        label: AR_LABEL,
        auth: RouteAuthRequirement::Authenticated,
        icon: "↑",
    },
    FinanceRouteDefinition {
        id: FinanceRouteId::Cash,
        template: "/finance/cash",
        label: CASH_LABEL,
        auth: RouteAuthRequirement::Authenticated,
        icon: "$",
    },
    FinanceRouteDefinition {
        id: FinanceRouteId::Banking,
        template: "/finance/banking",
        label: BANKING_LABEL,
        auth: RouteAuthRequirement::Authenticated,
        icon: "▣",
    },
    FinanceRouteDefinition {
        id: FinanceRouteId::Close,
        template: "/finance/close",
        label: CLOSE_LABEL,
        auth: RouteAuthRequirement::Authenticated,
        icon: "✓",
    },
    FinanceRouteDefinition {
        id: FinanceRouteId::Budgets,
        template: "/finance/budgets",
        label: BUDGETS_LABEL,
        auth: RouteAuthRequirement::Authenticated,
        icon: "◫",
    },
];

/// Backwards-readable alias for callers that name the registry directly.
pub const FINANCE_ROUTE_REGISTRY: &[FinanceRouteDefinition] = FINANCE_ROUTES;

/// Cheap, copyable handle to the Finance route registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct FinanceRouteRegistry;

impl FinanceRouteRegistry {
    /// Construct the canonical Finance route registry.
    pub const fn new() -> Self {
        Self
    }

    /// Return every Finance route definition in canonical order.
    pub const fn definitions(self) -> &'static [FinanceRouteDefinition] {
        FINANCE_ROUTES
    }

    /// Resolve a browser path to a typed Finance route.
    pub fn resolve(self, path: &str) -> Result<FinanceRoute, FinanceRouteParseError> {
        FinanceRoute::parse(path)
    }

    /// Resolve a browser path to its stable route identifier.
    pub fn resolve_id(self, path: &str) -> Result<FinanceRouteId, FinanceRouteParseError> {
        self.resolve(path).map(FinanceRoute::id)
    }
}

/// A parsed route from the Finance allow-list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinanceRoute {
    /// Finance landing page.
    Finance,
    /// Chart of accounts.
    Chart,
    /// Journal list and detail surface.
    Journals,
    /// Subledger summaries.
    Subledgers,
    /// Accounts payable.
    AccountsPayable,
    /// Accounts receivable.
    AccountsReceivable,
    /// Cash surface.
    Cash,
    /// Banking and reconciliation surface.
    Banking,
    /// Period-close surface.
    Close,
    /// Budget surface.
    Budgets,
}

impl FinanceRoute {
    /// Parse a route after discarding query, fragment, and credential-bearing deep-link data.
    pub fn parse(path: &str) -> Result<Self, FinanceRouteParseError> {
        let normalized = redact_deep_link(path);
        let route = match normalized.as_str() {
            "/finance" => Self::Finance,
            "/finance/chart" => Self::Chart,
            "/finance/journals" => Self::Journals,
            "/finance/subledgers" => Self::Subledgers,
            "/finance/ap" => Self::AccountsPayable,
            "/finance/ar" => Self::AccountsReceivable,
            "/finance/cash" => Self::Cash,
            "/finance/banking" => Self::Banking,
            "/finance/close" => Self::Close,
            "/finance/budgets" => Self::Budgets,
            _ => return Err(FinanceRouteParseError::new(normalized)),
        };
        Ok(route)
    }

    /// Return the stable route identifier.
    pub const fn id(self) -> FinanceRouteId {
        match self {
            Self::Finance => FinanceRouteId::Finance,
            Self::Chart => FinanceRouteId::Chart,
            Self::Journals => FinanceRouteId::Journals,
            Self::Subledgers => FinanceRouteId::Subledgers,
            Self::AccountsPayable => FinanceRouteId::AccountsPayable,
            Self::AccountsReceivable => FinanceRouteId::AccountsReceivable,
            Self::Cash => FinanceRouteId::Cash,
            Self::Banking => FinanceRouteId::Banking,
            Self::Close => FinanceRouteId::Close,
            Self::Budgets => FinanceRouteId::Budgets,
        }
    }

    /// Return the canonical route path.
    pub const fn path(self) -> &'static str {
        match self {
            Self::Finance => "/finance",
            Self::Chart => "/finance/chart",
            Self::Journals => "/finance/journals",
            Self::Subledgers => "/finance/subledgers",
            Self::AccountsPayable => "/finance/ap",
            Self::AccountsReceivable => "/finance/ar",
            Self::Cash => "/finance/cash",
            Self::Banking => "/finance/banking",
            Self::Close => "/finance/close",
            Self::Budgets => "/finance/budgets",
        }
    }

    /// Return the presentation surface represented by this route.
    pub const fn surface(self) -> FinanceSurface {
        match self {
            Self::Finance => FinanceSurface::Overview,
            Self::Chart => FinanceSurface::Chart,
            Self::Journals => FinanceSurface::Journals,
            Self::Subledgers => FinanceSurface::Subledgers,
            Self::AccountsPayable => FinanceSurface::AccountsPayable,
            Self::AccountsReceivable => FinanceSurface::AccountsReceivable,
            Self::Cash => FinanceSurface::Cash,
            Self::Banking => FinanceSurface::Banking,
            Self::Close => FinanceSurface::Close,
            Self::Budgets => FinanceSurface::Budgets,
        }
    }
}

impl fmt::Display for FinanceRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.path())
    }
}

/// Error returned for a path outside the Finance allow-list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceRouteParseError {
    /// Normalized non-query path that failed to resolve.
    pub normalized_path: String,
}

impl FinanceRouteParseError {
    fn new(normalized_path: String) -> Self {
        let normalized_path = if has_sensitive_marker(&normalized_path) {
            "/".to_string()
        } else {
            normalized_path
        };
        Self { normalized_path }
    }
}

impl fmt::Display for FinanceRouteParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "no ERPX Finance route matches {}",
            self.normalized_path
        )
    }
}

impl Error for FinanceRouteParseError {}

/// A Finance display surface represented by a scalar read model.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum FinanceSurface {
    /// Finance landing page.
    Overview,
    /// Chart of accounts.
    Chart,
    /// Journal list and detail surface.
    Journals,
    /// Subledger summaries.
    Subledgers,
    /// Accounts payable.
    AccountsPayable,
    /// Accounts receivable.
    AccountsReceivable,
    /// Cash surface.
    Cash,
    /// Banking and reconciliation surface.
    Banking,
    /// Period-close surface.
    Close,
    /// Budget surface.
    Budgets,
}

impl FinanceSurface {
    /// Return all Finance surfaces in navigation order.
    pub const fn all() -> &'static [Self] {
        &[
            Self::Overview,
            Self::Chart,
            Self::Journals,
            Self::Subledgers,
            Self::AccountsPayable,
            Self::AccountsReceivable,
            Self::Cash,
            Self::Banking,
            Self::Close,
            Self::Budgets,
        ]
    }

    /// Return the stable route for this surface.
    pub const fn route(self) -> FinanceRoute {
        match self {
            Self::Overview => FinanceRoute::Finance,
            Self::Chart => FinanceRoute::Chart,
            Self::Journals => FinanceRoute::Journals,
            Self::Subledgers => FinanceRoute::Subledgers,
            Self::AccountsPayable => FinanceRoute::AccountsPayable,
            Self::AccountsReceivable => FinanceRoute::AccountsReceivable,
            Self::Cash => FinanceRoute::Cash,
            Self::Banking => FinanceRoute::Banking,
            Self::Close => FinanceRoute::Close,
            Self::Budgets => FinanceRoute::Budgets,
        }
    }

    /// Return the bilingual surface label.
    pub const fn label(self) -> FinanceLabel {
        match self {
            Self::Overview => FINANCE_LABEL,
            Self::Chart => CHART_LABEL,
            Self::Journals => JOURNALS_LABEL,
            Self::Subledgers => SUBLEDGERS_LABEL,
            Self::AccountsPayable => AP_LABEL,
            Self::AccountsReceivable => AR_LABEL,
            Self::Cash => CASH_LABEL,
            Self::Banking => BANKING_LABEL,
            Self::Close => CLOSE_LABEL,
            Self::Budgets => BUDGETS_LABEL,
        }
    }
}

/// Error returned when an opaque identifier is not safe for a route or display model.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinanceIdentifierError {
    /// The identifier is empty.
    Empty,
    /// The identifier exceeds its bounded size.
    TooLong,
    /// The identifier contains whitespace, control, path, or query characters.
    UnsafeCharacters,
    /// The identifier resembles sensitive transport or infrastructure material.
    SensitiveMarker,
}

impl fmt::Display for FinanceIdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "Finance identifier is empty",
            Self::TooLong => "Finance identifier is too long",
            Self::UnsafeCharacters => "Finance identifier contains unsafe characters",
            Self::SensitiveMarker => "Finance identifier contains a sensitive marker",
        })
    }
}

impl Error for FinanceIdentifierError {}

/// A validated opaque identifier retained by a Finance display model.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct FinanceOpaqueId(String);

impl FinanceOpaqueId {
    /// Validate without interpreting the identifier scheme.
    pub fn new(value: impl Into<String>) -> Result<Self, FinanceIdentifierError> {
        let value = value.into();
        validate_opaque_identifier(&value)?;
        Ok(Self(value))
    }

    /// Borrow the validated identifier.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl TryFrom<&str> for FinanceOpaqueId {
    type Error = FinanceIdentifierError;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl TryFrom<String> for FinanceOpaqueId {
    type Error = FinanceIdentifierError;

    fn try_from(value: String) -> Result<Self, Self::Error> {
        Self::new(value)
    }
}

impl fmt::Display for FinanceOpaqueId {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// Error returned when scalar display text is unsafe or exceeds its bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinanceDisplayValueError {
    /// The value is required but empty.
    Required,
    /// The value exceeds the display bound.
    TooLong,
    /// The value contains a control character.
    ControlCharacter,
    /// The value resembles structured/raw payload data.
    StructuredValue,
    /// The value resembles a secret, token, or infrastructure credential.
    SensitiveMarker,
}

impl fmt::Display for FinanceDisplayValueError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Required => "Finance display value is required",
            Self::TooLong => "Finance display value is too long",
            Self::ControlCharacter => "Finance display value contains a control character",
            Self::StructuredValue => "Finance display value must be scalar",
            Self::SensitiveMarker => "Finance display value contains a sensitive marker",
        })
    }
}

impl Error for FinanceDisplayValueError {}

/// Validated text used for scalar display values and state messages.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct FinanceSafeText(String);

impl FinanceSafeText {
    /// Validate display text and reject structured or credential-bearing values.
    pub fn new(value: impl Into<String>) -> Result<Self, FinanceDisplayValueError> {
        Ok(Self(safe_display_value(value.into())?))
    }

    /// Borrow the validated display text.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl From<&str> for FinanceSafeText {
    /// Convert an invalid adapter message to a non-sensitive fallback.
    fn from(value: &str) -> Self {
        Self::new(value).unwrap_or_else(|_| Self("Unavailable".to_string()))
    }
}

impl From<String> for FinanceSafeText {
    /// Convert an invalid adapter message to a non-sensitive fallback.
    fn from(value: String) -> Self {
        Self::new(value).unwrap_or_else(|_| Self("Unavailable".to_string()))
    }
}

/// Scalar-only value kinds accepted by a Finance display field.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinanceScalar {
    /// Bounded human-readable text.
    Text(FinanceSafeText),
    /// An integer aggregate.
    Integer(i64),
    /// A bounded decimal representation without floating-point coercion.
    Decimal(FinanceSafeText),
    /// A boolean flag.
    Boolean(bool),
}

impl FinanceScalar {
    /// Validate a text scalar.
    pub fn text(value: impl Into<String>) -> Result<Self, FinanceDisplayValueError> {
        FinanceSafeText::new(value).map(Self::Text)
    }

    /// Validate a decimal scalar without losing source precision.
    pub fn decimal(value: impl Into<String>) -> Result<Self, FinanceDisplayValueError> {
        FinanceSafeText::new(value).map(Self::Decimal)
    }

    /// Construct an integer scalar.
    pub const fn integer(value: i64) -> Self {
        Self::Integer(value)
    }

    /// Construct a boolean scalar.
    pub const fn boolean(value: bool) -> Self {
        Self::Boolean(value)
    }

    /// Return text or decimal content, when this is a string-backed scalar.
    pub fn text_value(&self) -> Option<&str> {
        match self {
            Self::Text(value) | Self::Decimal(value) => Some(value.as_str()),
            Self::Integer(_) | Self::Boolean(_) => None,
        }
    }
}

/// A safe scalar field displayed on a Finance row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceDisplayField {
    key: FinanceOpaqueId,
    label: FinanceLabel,
    value: FinanceScalar,
}

impl FinanceDisplayField {
    /// Construct a bounded scalar field.
    pub fn new(
        key: impl Into<String>,
        label: FinanceLabel,
        value: FinanceScalar,
    ) -> Result<Self, FinanceIdentifierError> {
        Ok(Self {
            key: FinanceOpaqueId::new(key)?,
            label,
            value,
        })
    }

    /// Return the stable field key.
    pub const fn key(&self) -> &FinanceOpaqueId {
        &self.key
    }

    /// Return the bilingual field label.
    pub const fn label(&self) -> FinanceLabel {
        self.label
    }

    /// Return the scalar value.
    pub const fn value(&self) -> &FinanceScalar {
        &self.value
    }
}

/// Error returned when a Finance display row or page exceeds a presentation bound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinanceModelError {
    /// Too many fields were supplied for one display row.
    TooManyFields,
    /// Too many rows were supplied for one read page.
    TooManyRows,
}

impl fmt::Display for FinanceModelError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::TooManyFields => "Finance display row has too many fields",
            Self::TooManyRows => "Finance read page has too many rows",
        })
    }
}

impl Error for FinanceModelError {}

/// A bounded Finance read row containing only scalar fields.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceDisplayModel {
    surface: FinanceSurface,
    id: FinanceOpaqueId,
    fields: Vec<FinanceDisplayField>,
}

impl FinanceDisplayModel {
    /// Construct a display row from already-normalized scalar fields.
    pub fn new(
        surface: FinanceSurface,
        id: FinanceOpaqueId,
        fields: Vec<FinanceDisplayField>,
    ) -> Result<Self, FinanceModelError> {
        if fields.len() > MAX_FINANCE_DISPLAY_FIELDS {
            return Err(FinanceModelError::TooManyFields);
        }
        Ok(Self {
            surface,
            id,
            fields,
        })
    }

    /// Return the Finance surface owning this row.
    pub const fn surface(&self) -> FinanceSurface {
        self.surface
    }

    /// Return the opaque row identifier.
    pub const fn id(&self) -> &FinanceOpaqueId {
        &self.id
    }

    /// Return scalar display fields.
    pub fn fields(&self) -> &[FinanceDisplayField] {
        &self.fields
    }
}

/// A bounded read page for an already-authorized Finance projection.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceReadPage<T> {
    rows: Vec<T>,
    next_cursor: Option<FinanceOpaqueId>,
}

impl<T> FinanceReadPage<T> {
    /// Construct a bounded page. Cursor values remain opaque and optional.
    pub fn new(
        rows: Vec<T>,
        next_cursor: Option<FinanceOpaqueId>,
    ) -> Result<Self, FinanceModelError> {
        if rows.len() > MAX_FINANCE_PAGE_ROWS {
            return Err(FinanceModelError::TooManyRows);
        }
        Ok(Self { rows, next_cursor })
    }

    /// Return rows retained for display.
    pub fn rows(&self) -> &[T] {
        &self.rows
    }

    /// Return the opaque continuation cursor, when supplied by the adapter.
    pub const fn next_cursor(&self) -> Option<&FinanceOpaqueId> {
        self.next_cursor.as_ref()
    }
}

/// Typed tenant scope supplied by an already-authorized Finance read boundary.
///
/// The UI never derives authorization from this value; it only preserves the
/// exact holding/company binding so a consuming adapter can reject mismatched
/// payloads before rendering.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceScope {
    holding_id: FinanceOpaqueId,
    company_id: Option<FinanceOpaqueId>,
}

impl FinanceScope {
    /// Construct a holding scope with an optional company refinement.
    pub fn new(
        holding_id: impl Into<String>,
        company_id: Option<impl Into<String>>,
    ) -> Result<Self, FinanceIdentifierError> {
        Ok(Self {
            holding_id: FinanceOpaqueId::new(holding_id)?,
            company_id: company_id.map(FinanceOpaqueId::new).transpose()?,
        })
    }

    /// Return the opaque holding identity.
    pub const fn holding_id(&self) -> &FinanceOpaqueId {
        &self.holding_id
    }

    /// Return the optional opaque company identity.
    pub const fn company_id(&self) -> Option<&FinanceOpaqueId> {
        self.company_id.as_ref()
    }
}

/// Safe projection freshness metadata carried with a Finance read.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceProjectionMetadata {
    /// Exact holding/company scope supplied by the authorized read boundary.
    pub scope: Option<FinanceScope>,
    /// Bounded display timestamp, when supplied by the read boundary.
    pub projection_updated_at: Option<String>,
    /// Freshness classification supplied by the read boundary.
    pub freshness: ProjectionFreshness,
    /// Whether the presentation surface is explicitly non-mutating.
    pub read_only: bool,
}

impl FinanceProjectionMetadata {
    /// Build metadata for a fresh projection.
    pub fn fresh(projection_updated_at: impl Into<String>) -> Self {
        Self::present(projection_updated_at, ProjectionFreshness::Fresh)
    }

    /// Build metadata for a stale but displayable projection.
    pub fn stale(projection_updated_at: impl Into<String>) -> Self {
        Self::present(projection_updated_at, ProjectionFreshness::Stale)
    }

    /// Build metadata when no projection timestamp is available.
    pub fn unknown() -> Self {
        Self {
            scope: None,
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
            scope: None,
            freshness: if projection_updated_at.is_some() {
                freshness
            } else {
                ProjectionFreshness::Unknown
            },
            projection_updated_at,
            read_only: true,
        }
    }

    /// Bind this metadata to the exact tenant scope used by the read.
    pub fn with_scope(mut self, scope: FinanceScope) -> Self {
        self.scope = Some(scope);
        self
    }

    /// Return the typed tenant scope, when the adapter supplied one.
    pub const fn scope(&self) -> Option<&FinanceScope> {
        self.scope.as_ref()
    }

    /// Set the explicit read-only flag.
    pub const fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Whether this metadata declares a read-only context.
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

/// All explicit Finance page states, including successful ready data.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FinancePageStateKind {
    /// A read is in progress.
    Loading,
    /// The read completed with no rows/content.
    Empty,
    /// Safe display data is available.
    Ready,
    /// The principal cannot view this page.
    Denied,
    /// Cached data is outside the freshness budget.
    Stale,
    /// The surface is explicitly non-mutating.
    ReadOnly,
    /// A concurrent revision/write boundary needs review.
    Conflict,
    /// The backend is unavailable.
    Offline,
    /// An unexpected read failure occurred.
    Error,
    /// A high-risk read is withheld until the server confirms fresh MFA.
    MfaRequired,
}

impl FinancePageStateKind {
    /// Map a Finance state to the shared UI state kind.
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
            Self::MfaRequired => Some(UiStateKind::Denied),
        }
    }

    /// Return a shared bilingual state label.
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
            Self::MfaRequired => {
                return Some(match locale {
                    Locale::En => "MFA required",
                    Locale::Th => "ต้องยืนยัน MFA",
                });
            }
        };
        Some(key.text(locale))
    }
}

/// Explicit Finance state for future authorized read adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FinancePageState<T = Vec<FinanceDisplayModel>> {
    /// A read is in progress.
    Loading {
        /// Previously known metadata, if any.
        metadata: Option<FinanceProjectionMetadata>,
    },
    /// The read completed with no rows/content.
    Empty {
        /// Metadata accompanying the empty result.
        metadata: FinanceProjectionMetadata,
    },
    /// Current safe display data is available.
    Ready {
        /// Redacted display payload.
        data: T,
        /// Projection metadata.
        metadata: FinanceProjectionMetadata,
    },
    /// The principal cannot view this page.
    Denied {
        /// Metadata may be absent when denial precedes a read.
        metadata: Option<FinanceProjectionMetadata>,
        /// Safe policy/message text.
        reason: FinanceSafeText,
    },
    /// Cached safe display data is outside the freshness budget.
    Stale {
        /// Last safe display payload.
        data: T,
        /// Stale projection metadata.
        metadata: FinanceProjectionMetadata,
        /// Age supplied by the read boundary.
        age_seconds: u64,
        /// Safe freshness explanation.
        reason: FinanceSafeText,
    },
    /// Data is available but this presentation surface cannot mutate it.
    ReadOnly {
        /// Last safe display payload.
        data: T,
        /// Projection metadata.
        metadata: FinanceProjectionMetadata,
        /// Safe read-only explanation.
        reason: FinanceSafeText,
    },
    /// A concurrent revision/write-boundary conflict needs review.
    Conflict {
        /// Last safe display payload, when available.
        data: Option<T>,
        /// Projection metadata, when available.
        metadata: Option<FinanceProjectionMetadata>,
        /// Safe conflict explanation.
        reason: FinanceSafeText,
    },
    /// The backend is unavailable; cached safe data may remain visible.
    Offline {
        /// Cached safe display payload, when available.
        data: Option<T>,
        /// Cached projection metadata, when available.
        metadata: Option<FinanceProjectionMetadata>,
        /// Safe connectivity explanation.
        reason: FinanceSafeText,
    },
    /// An unexpected read failure occurred.
    Error {
        /// Safe error explanation.
        reason: FinanceSafeText,
    },
    /// A high-risk Finance read must be re-requested with fresh server-verified MFA.
    MfaRequired {
        /// Scope/freshness metadata may be retained, but no payload is renderable.
        metadata: Option<FinanceProjectionMetadata>,
        /// Safe policy explanation.
        reason: FinanceSafeText,
    },
}

impl<T> FinancePageState<T> {
    /// Return the explicit Finance state kind.
    pub const fn state_kind(&self) -> FinancePageStateKind {
        match self {
            Self::Loading { .. } => FinancePageStateKind::Loading,
            Self::Empty { .. } => FinancePageStateKind::Empty,
            Self::Ready { .. } => FinancePageStateKind::Ready,
            Self::Denied { .. } => FinancePageStateKind::Denied,
            Self::Stale { .. } => FinancePageStateKind::Stale,
            Self::ReadOnly { .. } => FinancePageStateKind::ReadOnly,
            Self::Conflict { .. } => FinancePageStateKind::Conflict,
            Self::Offline { .. } => FinancePageStateKind::Offline,
            Self::Error { .. } => FinancePageStateKind::Error,
            Self::MfaRequired { .. } => FinancePageStateKind::MfaRequired,
        }
    }

    /// Return the shared UI state kind for a non-ready state.
    pub const fn kind(&self) -> Option<UiStateKind> {
        self.state_kind().shared_kind()
    }

    /// Return the localized label for the current state.
    pub const fn state_label(&self, locale: Locale) -> Option<&'static str> {
        self.state_kind().label(locale)
    }

    /// Return safe current or cached display data, when available.
    pub fn data(&self) -> Option<&T> {
        match self {
            // A Finance payload is renderable only when the authorized adapter
            // supplied the exact typed holding/company scope.  Keeping the
            // scope check at this accessor prevents a target from accidentally
            // rendering an otherwise safe-looking row outside its tenant
            // binding, including cached/conflict/offline data.
            Self::Ready { data, metadata }
            | Self::Stale { data, metadata, .. }
            | Self::ReadOnly { data, metadata, .. }
                if metadata.scope().is_some() =>
            {
                Some(data)
            }
            Self::Conflict {
                data: Some(data),
                metadata: Some(metadata),
                ..
            }
            | Self::Offline {
                data: Some(data),
                metadata: Some(metadata),
                ..
            } if metadata.scope().is_some() => Some(data),
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Denied { .. }
            | Self::Error { .. }
            | Self::MfaRequired { .. }
            | Self::Ready { .. }
            | Self::Stale { .. }
            | Self::ReadOnly { .. }
            | Self::Conflict { .. }
            | Self::Offline { .. } => None,
        }
    }

    /// Return projection metadata, if available.
    pub fn metadata(&self) -> Option<&FinanceProjectionMetadata> {
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
            Self::MfaRequired { metadata, .. } => metadata.as_ref(),
        }
    }

    /// Whether the state or its metadata explicitly disables writes.
    pub fn is_read_only(&self) -> bool {
        matches!(
            self.state_kind(),
            FinancePageStateKind::ReadOnly | FinancePageStateKind::MfaRequired
        ) || self
            .metadata()
            .is_some_and(FinanceProjectionMetadata::is_read_only)
    }

    /// This presentation contract never authorizes a mutation.
    pub const fn can_mutate(&self) -> bool {
        false
    }

    /// Whether a safe display payload is available for rendering.
    pub fn can_render_data(&self) -> bool {
        self.data().is_some()
    }
}

/// Generic Finance view-state alias.
pub type FinanceViewState<T = Vec<FinanceDisplayModel>> = FinancePageState<T>;
/// Generic Finance state alias.
pub type FinanceState<T = Vec<FinanceDisplayModel>> = FinancePageState<T>;
/// Chart display model alias for adapters that prefer a surface-specific name.
pub type FinanceChartDisplayModel = FinanceDisplayModel;
/// Journal display model alias for adapters that prefer a surface-specific name.
pub type FinanceJournalDisplayModel = FinanceDisplayModel;
/// Subledger display model alias for adapters that prefer a surface-specific name.
pub type FinanceSubledgerDisplayModel = FinanceDisplayModel;
/// Banking display model alias for adapters that prefer a surface-specific name.
pub type FinanceBankingDisplayModel = FinanceDisplayModel;
/// Period-close display model alias for adapters that prefer a surface-specific name.
pub type FinanceCloseDisplayModel = FinanceDisplayModel;
/// Chart state alias for adapters that prefer a surface-specific name.
pub type FinanceChartState<T = Vec<FinanceDisplayModel>> = FinancePageState<T>;
/// Journal state alias for adapters that prefer a surface-specific name.
pub type FinanceJournalState<T = Vec<FinanceDisplayModel>> = FinancePageState<T>;
/// Subledger state alias for adapters that prefer a surface-specific name.
pub type FinanceSubledgerState<T = Vec<FinanceDisplayModel>> = FinancePageState<T>;
/// Banking state alias for adapters that prefer a surface-specific name.
pub type FinanceBankingState<T = Vec<FinanceDisplayModel>> = FinancePageState<T>;
/// Period-close state alias for adapters that prefer a surface-specific name.
pub type FinanceCloseState<T = Vec<FinanceDisplayModel>> = FinancePageState<T>;

fn validate_opaque_identifier(value: &str) -> Result<(), FinanceIdentifierError> {
    if value.is_empty() {
        return Err(FinanceIdentifierError::Empty);
    }
    if value.len() > MAX_FINANCE_IDENTIFIER_BYTES {
        return Err(FinanceIdentifierError::TooLong);
    }
    if value == "."
        || value == ".."
        || value
            .chars()
            .any(|character| character.is_control() || character.is_whitespace())
        || value.contains(['/', '\\', '?', '#'])
    {
        return Err(FinanceIdentifierError::UnsafeCharacters);
    }
    if has_sensitive_marker(value) {
        return Err(FinanceIdentifierError::SensitiveMarker);
    }
    Ok(())
}

fn safe_display_value(value: String) -> Result<String, FinanceDisplayValueError> {
    if value.is_empty() {
        return Err(FinanceDisplayValueError::Required);
    }
    if value.len() > MAX_FINANCE_DISPLAY_VALUE_BYTES {
        return Err(FinanceDisplayValueError::TooLong);
    }
    if value.chars().any(char::is_control) {
        return Err(FinanceDisplayValueError::ControlCharacter);
    }
    if value.contains(['{', '}', '[', ']']) {
        return Err(FinanceDisplayValueError::StructuredValue);
    }
    if has_sensitive_marker(&value) {
        return Err(FinanceDisplayValueError::SensitiveMarker);
    }
    Ok(value)
}

fn safe_optional_display_value(value: String) -> Option<String> {
    if value.is_empty() {
        return None;
    }
    safe_display_value(value).ok()
}

fn has_sensitive_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "access_token",
        "refresh_token",
        "id_token",
        "authorization",
        "bearer",
        "api_key",
        "client_secret",
        "password",
        "secret",
        "credential",
        "private_key",
        "database_url",
        "postgres://",
        "postgresql://",
        "minio",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}
