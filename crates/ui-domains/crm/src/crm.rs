//! Client-local CRM read and display models.
//!
//! These types are presentation-only. A consuming application converts an already-authorized
//! CRM read response into them; this module contains no transport, endpoint, holding credential,
//! database, or authorization behavior. All string candidates cross a small allow-list and
//! redaction boundary before becoming display data.

use std::fmt;

use erpx_ui_core::{Locale, ProjectionFreshness, ScopeKind, TranslationKey, UiStateKind};

/// Maximum number of display fields retained on one CRM row.
pub const MAX_CRM_DISPLAY_FIELDS: usize = 64;
/// Maximum bytes retained for one display field value.
pub const MAX_CRM_DISPLAY_VALUE_BYTES: usize = 256;
/// Maximum bytes retained for an opaque row identifier or page cursor.
pub const MAX_CRM_IDENTIFIER_BYTES: usize = 128;
/// Maximum rows requested in one CRM read page.
pub const MAX_CRM_PAGE_SIZE: usize = 100;

/// CRM read surfaces in stable presentation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrmSurface {
    /// Accepted lead handoffs from Marketing into CRM.
    AcceptedLeadHandoffs,
    /// CRM opportunity lifecycle rows.
    Opportunities,
    /// Revenue and cost attribution rows.
    Attribution,
}

/// Stable surface order used by a future client adapter.
pub const CRM_SURFACES: &[CrmSurface] = &[
    CrmSurface::AcceptedLeadHandoffs,
    CrmSurface::Opportunities,
    CrmSurface::Attribution,
];

impl CrmSurface {
    /// Return all CRM read surfaces in stable order.
    pub const fn all() -> &'static [Self] {
        CRM_SURFACES
    }

    const fn translation_key(self) -> TranslationKey {
        match self {
            Self::AcceptedLeadHandoffs => TranslationKey::CrmLeads,
            Self::Opportunities => TranslationKey::CrmOpportunities,
            Self::Attribution => TranslationKey::MarketingAttribution,
        }
    }

    /// Return the bilingual shell label for this surface.
    pub const fn label(self, locale: Locale) -> &'static str {
        self.translation_key().text(locale)
    }
}

/// Typed CRM opportunity lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum CrmOpportunityStatus {
    /// Opportunity has been created but not qualified.
    Draft,
    /// Opportunity has passed qualification.
    Qualified,
    /// Opportunity was won.
    Won,
    /// Opportunity was lost.
    Lost,
}

impl CrmOpportunityStatus {
    /// Parse the bounded status token emitted by a CRM read adapter.
    pub fn from_transport(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "draft" => Some(Self::Draft),
            "qualified" => Some(Self::Qualified),
            "won" => Some(Self::Won),
            "lost" => Some(Self::Lost),
            _ => None,
        }
    }

    /// Return the stable transport token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Qualified => "qualified",
            Self::Won => "won",
            Self::Lost => "lost",
        }
    }

    /// Whether the requested lifecycle transition is allowed by the CRM domain contract.
    pub const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::Qualified) | (Self::Qualified, Self::Won | Self::Lost)
        )
    }

    /// Return a bilingual lifecycle label.
    pub const fn label(self, locale: Locale) -> &'static str {
        match locale {
            Locale::En => match self {
                Self::Draft => "Draft",
                Self::Qualified => "Qualified",
                Self::Won => "Won",
                Self::Lost => "Lost",
            },
            Locale::Th => match self {
                Self::Draft => "ร่าง",
                Self::Qualified => "ผ่านการคัดกรอง",
                Self::Won => "ชนะการขาย",
                Self::Lost => "แพ้การขาย",
            },
        }
    }
}

/// Typed attribution model shown on CRM revenue rows.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum CrmAttributionModel {
    /// All revenue is assigned to the first touch.
    FirstTouch,
    /// All revenue is assigned to the last touch.
    LastTouch,
    /// Revenue is divided evenly across touches.
    Linear,
    /// Revenue is weighted toward the first and last touches.
    UShaped,
    /// Revenue follows the stored explicit weights.
    Weighted,
}

impl CrmAttributionModel {
    /// Parse the bounded model token emitted by a CRM read adapter.
    pub fn from_transport(value: &str) -> Option<Self> {
        match value.trim().to_ascii_lowercase().as_str() {
            "first_touch" => Some(Self::FirstTouch),
            "last_touch" => Some(Self::LastTouch),
            "linear" => Some(Self::Linear),
            "u_shaped" => Some(Self::UShaped),
            "weighted" => Some(Self::Weighted),
            _ => None,
        }
    }

    /// Return the stable transport token.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::FirstTouch => "first_touch",
            Self::LastTouch => "last_touch",
            Self::Linear => "linear",
            Self::UShaped => "u_shaped",
            Self::Weighted => "weighted",
        }
    }

    /// Return a bilingual attribution label.
    pub const fn label(self, locale: Locale) -> &'static str {
        match locale {
            Locale::En => match self {
                Self::FirstTouch => "First touch",
                Self::LastTouch => "Last touch",
                Self::Linear => "Linear",
                Self::UShaped => "U-shaped",
                Self::Weighted => "Weighted",
            },
            Locale::Th => match self {
                Self::FirstTouch => "สัมผัสแรก",
                Self::LastTouch => "สัมผัสสุดท้าย",
                Self::Linear => "เชิงเส้น",
                Self::UShaped => "รูปตัว U",
                Self::Weighted => "ถ่วงน้ำหนัก",
            },
        }
    }
}

/// Private, validated CRM scope binding used by the presentation boundary.
///
/// The shape mirrors the domain's non-platform `AuthorityScope` variants without making the UI
/// crate depend on transport or domain serialization. Holding/company/branch identifiers stay
/// opaque and are only compared for containment.
#[derive(Debug, Clone, PartialEq, Eq)]
struct CrmScope {
    holding_id: String,
    company_id: Option<String>,
    branch_id: Option<String>,
}

impl CrmScope {
    fn holding(holding_id: String) -> Option<Self> {
        Some(Self {
            holding_id: safe_scope_identifier(&holding_id)?,
            company_id: None,
            branch_id: None,
        })
    }

    fn company(holding_id: String, company_id: String) -> Option<Self> {
        Some(Self {
            holding_id: safe_scope_identifier(&holding_id)?,
            company_id: Some(safe_scope_identifier(&company_id)?),
            branch_id: None,
        })
    }

    fn branch(holding_id: String, company_id: String, branch_id: String) -> Option<Self> {
        Some(Self {
            holding_id: safe_scope_identifier(&holding_id)?,
            company_id: Some(safe_scope_identifier(&company_id)?),
            branch_id: Some(safe_scope_identifier(&branch_id)?),
        })
    }

    const fn kind(&self) -> ScopeKind {
        match (&self.company_id, &self.branch_id) {
            (None, None) => ScopeKind::Holding,
            (Some(_), None) => ScopeKind::Company,
            (Some(_), Some(_)) => ScopeKind::Branch,
            (None, Some(_)) => unreachable!(),
        }
    }

    fn contains_record_scope(
        &self,
        holding_id: &str,
        company_id: &str,
        branch_id: Option<&str>,
    ) -> bool {
        let Some(holding_id) = safe_scope_identifier(holding_id) else {
            return false;
        };
        let Some(company_id) = safe_scope_identifier(company_id) else {
            return false;
        };
        let branch_id = match branch_id {
            Some(branch_id) => {
                let Some(branch_id) = safe_scope_identifier(branch_id) else {
                    return false;
                };
                Some(branch_id)
            }
            None => None,
        };

        if self.holding_id != holding_id {
            return false;
        }
        match (&self.company_id, &self.branch_id, branch_id.as_deref()) {
            (None, None, _) => true,
            (Some(scope_company), None, _) => scope_company == &company_id,
            (Some(scope_company), Some(scope_branch), Some(record_branch)) => {
                scope_company == &company_id && scope_branch == record_branch
            }
            (Some(_), Some(_), None) | (None, Some(_), _) => false,
        }
    }
}

/// Safe projection freshness and mutation metadata for CRM reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmProjectionMetadata {
    /// Exact holding/company/branch binding supplied by the authorized read boundary.
    ///
    /// This is intentionally private: callers bind it through the validated constructors
    /// below, while `CrmViewState::data` remains fail-closed when an adapter omits it.
    scope: Option<CrmScope>,
    /// Opaque display timestamp supplied by the read boundary, when available.
    pub projection_updated_at: Option<String>,
    /// Freshness classification supplied by the read boundary.
    pub freshness: ProjectionFreshness,
    /// Whether the current context is explicitly non-mutating.
    pub read_only: bool,
}

impl CrmProjectionMetadata {
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

    /// Build metadata with an explicit freshness classification.
    pub fn present(
        projection_updated_at: impl Into<String>,
        freshness: ProjectionFreshness,
    ) -> Self {
        let projection_updated_at =
            safe_optional_value(&projection_updated_at.into(), MAX_CRM_IDENTIFIER_BYTES);
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

    /// Set the read-only flag without changing freshness metadata.
    pub const fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Bind a read to an authorized holding scope.
    ///
    /// A holding scope can read company and branch records in that holding, but a CRM record
    /// still needs a company scope before the mutation preflight can pass. Authorization is
    /// never inferred here; the consuming adapter must supply this value from its authorized
    /// query context.
    pub fn with_holding_scope(mut self, holding_id: impl Into<String>) -> Option<Self> {
        self.scope = Some(CrmScope::holding(holding_id.into())?);
        Some(self)
    }

    /// Bind a read to an authorized company scope within a holding.
    pub fn with_company_scope(
        mut self,
        holding_id: impl Into<String>,
        company_id: impl Into<String>,
    ) -> Option<Self> {
        self.scope = Some(CrmScope::company(holding_id.into(), company_id.into())?);
        Some(self)
    }

    /// Bind a read to an authorized branch scope within a company.
    pub fn with_branch_scope(
        mut self,
        holding_id: impl Into<String>,
        company_id: impl Into<String>,
        branch_id: impl Into<String>,
    ) -> Option<Self> {
        self.scope = Some(CrmScope::branch(
            holding_id.into(),
            company_id.into(),
            branch_id.into(),
        )?);
        Some(self)
    }

    /// Return the typed scope level supplied by the read boundary.
    pub fn scope_kind(&self) -> Option<ScopeKind> {
        self.scope.as_ref().map(CrmScope::kind)
    }

    /// Whether the read payload has an explicit holding scope binding.
    pub const fn has_scope(&self) -> bool {
        self.scope.is_some()
    }

    /// Whether this metadata contains a target CRM record scope.
    ///
    /// This mirrors the domain `AuthorityScope::contains` rule for the CRM-supported holding,
    /// company, and branch levels. The target must be company- or branch-scoped; a holding-only
    /// target is not a valid CRM aggregate scope.
    pub fn permits_record_scope(
        &self,
        holding_id: &str,
        company_id: &str,
        branch_id: Option<&str>,
    ) -> bool {
        self.scope
            .as_ref()
            .is_some_and(|scope| scope.contains_record_scope(holding_id, company_id, branch_id))
    }

    /// Whether this metadata is safe for a fresh, explicitly scoped CRM mutation preflight.
    pub fn can_mutate_record(
        &self,
        holding_id: &str,
        company_id: &str,
        branch_id: Option<&str>,
    ) -> bool {
        !self.read_only
            && self.is_fresh()
            && self.permits_record_scope(holding_id, company_id, branch_id)
    }

    fn can_mutate_without_record_scope(&self) -> bool {
        !self.read_only && self.is_fresh() && self.has_scope()
    }

    /// Whether this projection is explicitly read-only.
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Whether this projection is outside its freshness budget.
    pub const fn is_stale(&self) -> bool {
        matches!(self.freshness, ProjectionFreshness::Stale)
    }

    /// Whether this projection is within its freshness budget.
    pub const fn is_fresh(&self) -> bool {
        matches!(self.freshness, ProjectionFreshness::Fresh)
    }
}

/// One untrusted field candidate supplied by a JSON-derived read adapter.
pub struct CrmFieldInput<'a> {
    key: &'a str,
    value: &'a str,
}

impl<'a> CrmFieldInput<'a> {
    /// Create a field candidate. It is not display-safe until a model accepts it.
    pub const fn new(key: &'a str, value: &'a str) -> Self {
        Self { key, value }
    }
}

impl fmt::Debug for CrmFieldInput<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CrmFieldInput")
            .field("key", &self.key)
            .field("value", &"REDACTED")
            .finish()
    }
}

/// A display field retained after key, length, control-character, and sensitive-marker checks.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmDisplayField {
    key: String,
    value: String,
}

impl CrmDisplayField {
    /// Copy a candidate only when its key is allow-listed and its value is safe to display.
    pub fn from_untrusted(input: CrmFieldInput<'_>) -> Option<Self> {
        let key = normalize_key(input.key)?;
        if !is_allowed_display_key(&key) {
            return None;
        }
        let value = safe_optional_value(input.value, MAX_CRM_DISPLAY_VALUE_BYTES)?;
        Some(Self { key, value })
    }

    /// Stable allow-listed field key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Safe field value.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Safe display model for an accepted Marketing-to-CRM lead handoff.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmHandoffDisplayModel {
    id: String,
    fields: Vec<CrmDisplayField>,
    redacted_field_count: usize,
}

impl CrmHandoffDisplayModel {
    /// Convert untrusted handoff candidates into a safe model.
    pub fn from_untrusted_fields<'a>(
        id: impl Into<String>,
        inputs: impl IntoIterator<Item = CrmFieldInput<'a>>,
    ) -> Option<Self> {
        let id = safe_identifier(&id.into())?;
        let (fields, redacted_field_count) = collect_fields(inputs);
        Some(Self {
            id,
            fields,
            redacted_field_count,
        })
    }

    /// Opaque handoff identifier safe for display or route construction by the consuming app.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Safe handoff display fields.
    pub fn fields(&self) -> &[CrmDisplayField] {
        &self.fields
    }

    /// Return one safe field by its normalized allow-listed key.
    pub fn field(&self, key: &str) -> Option<&str> {
        let key = normalize_key(key)?;
        self.fields
            .iter()
            .find(|field| field.key == key)
            .map(CrmDisplayField::value)
    }

    /// Number of candidates omitted at the redaction boundary.
    pub const fn redacted_field_count(&self) -> usize {
        self.redacted_field_count
    }
}

/// Safe display model for a CRM opportunity row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmOpportunityDisplayModel {
    id: String,
    status: CrmOpportunityStatus,
    fields: Vec<CrmDisplayField>,
    redacted_field_count: usize,
}

impl CrmOpportunityDisplayModel {
    /// Convert an untrusted status and display-field set into a safe opportunity model.
    pub fn from_untrusted_fields<'a>(
        id: impl Into<String>,
        status: &str,
        inputs: impl IntoIterator<Item = CrmFieldInput<'a>>,
    ) -> Option<Self> {
        let id = safe_identifier(&id.into())?;
        let status = CrmOpportunityStatus::from_transport(status)?;
        let (fields, redacted_field_count) = collect_fields(inputs);
        Some(Self {
            id,
            status,
            fields,
            redacted_field_count,
        })
    }

    /// Opaque opportunity identifier safe for display or route construction.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Typed opportunity lifecycle status.
    pub const fn status(&self) -> CrmOpportunityStatus {
        self.status
    }

    /// Whether this displayed opportunity can move to the requested lifecycle status.
    pub const fn can_transition_to(&self, next: CrmOpportunityStatus) -> bool {
        self.status.can_transition_to(next)
    }

    /// Localized opportunity lifecycle label.
    pub const fn status_label(&self, locale: Locale) -> &'static str {
        self.status.label(locale)
    }

    /// Safe opportunity display fields.
    pub fn fields(&self) -> &[CrmDisplayField] {
        &self.fields
    }

    /// Return one safe field by its normalized allow-listed key.
    pub fn field(&self, key: &str) -> Option<&str> {
        let key = normalize_key(key)?;
        self.fields
            .iter()
            .find(|field| field.key == key)
            .map(CrmDisplayField::value)
    }

    /// Number of candidates omitted at the redaction boundary.
    pub const fn redacted_field_count(&self) -> usize {
        self.redacted_field_count
    }
}

/// Safe display model for one attribution result row.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmAttributionDisplayModel {
    id: String,
    model: CrmAttributionModel,
    fields: Vec<CrmDisplayField>,
    redacted_field_count: usize,
}

impl CrmAttributionDisplayModel {
    /// Convert an untrusted attribution model and display-field set into a safe row.
    pub fn from_untrusted_fields<'a>(
        id: impl Into<String>,
        model: &str,
        inputs: impl IntoIterator<Item = CrmFieldInput<'a>>,
    ) -> Option<Self> {
        let id = safe_identifier(&id.into())?;
        let model = CrmAttributionModel::from_transport(model)?;
        let (fields, redacted_field_count) = collect_fields(inputs);
        Some(Self {
            id,
            model,
            fields,
            redacted_field_count,
        })
    }

    /// Opaque attribution-record identifier safe for display or route construction.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Typed attribution model.
    pub const fn model(&self) -> CrmAttributionModel {
        self.model
    }

    /// Localized attribution-model label.
    pub const fn model_label(&self, locale: Locale) -> &'static str {
        self.model.label(locale)
    }

    /// Safe attribution display fields.
    pub fn fields(&self) -> &[CrmDisplayField] {
        &self.fields
    }

    /// Return one safe field by its normalized allow-listed key.
    pub fn field(&self, key: &str) -> Option<&str> {
        let key = normalize_key(key)?;
        self.fields
            .iter()
            .find(|field| field.key == key)
            .map(CrmDisplayField::value)
    }

    /// Number of candidates omitted at the redaction boundary.
    pub const fn redacted_field_count(&self) -> usize {
        self.redacted_field_count
    }
}

/// Page cursor and bound metadata supplied by the CRM read adapter.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmPageMetadata {
    page_size: usize,
    after_cursor: Option<String>,
    next_cursor: Option<String>,
    has_more: bool,
}

/// A page metadata or row-count bound was violated.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CrmPageError {
    /// Page size was zero or exceeded the client bound.
    InvalidPageSize,
    /// A cursor contained unsafe or oversized data.
    InvalidCursor,
    /// `has_more` did not agree with the presence of a next cursor.
    InconsistentCursor,
    /// The next cursor repeated the cursor used for this page.
    CursorDidNotAdvance,
    /// A page contained more safe rows than its declared bound.
    TooManyRows,
}

impl CrmPageMetadata {
    /// Construct bounded cursor metadata.
    pub fn new(
        page_size: usize,
        after_cursor: Option<&str>,
        next_cursor: Option<&str>,
        has_more: bool,
    ) -> Result<Self, CrmPageError> {
        if !(1..=MAX_CRM_PAGE_SIZE).contains(&page_size) {
            return Err(CrmPageError::InvalidPageSize);
        }
        let after_cursor = safe_cursor(after_cursor)?;
        let next_cursor = safe_cursor(next_cursor)?;
        if has_more != next_cursor.is_some() {
            return Err(CrmPageError::InconsistentCursor);
        }
        if has_more && after_cursor == next_cursor {
            return Err(CrmPageError::CursorDidNotAdvance);
        }
        Ok(Self {
            page_size,
            after_cursor,
            next_cursor,
            has_more,
        })
    }

    /// Number of rows permitted in this page.
    pub const fn page_size(&self) -> usize {
        self.page_size
    }

    /// Cursor used to request this page, if any.
    pub fn after_cursor(&self) -> Option<&str> {
        self.after_cursor.as_deref()
    }

    /// Cursor for the next page, if more rows exist.
    pub fn next_cursor(&self) -> Option<&str> {
        self.next_cursor.as_deref()
    }

    /// Whether another page is available.
    pub const fn has_more(&self) -> bool {
        self.has_more
    }
}

/// A bounded page of already-safe CRM display rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CrmReadPage<T> {
    rows: Vec<T>,
    metadata: CrmPageMetadata,
}

impl<T> CrmReadPage<T> {
    /// Construct a page and enforce its declared row bound.
    pub fn new(rows: Vec<T>, metadata: CrmPageMetadata) -> Result<Self, CrmPageError> {
        if rows.len() > metadata.page_size() {
            return Err(CrmPageError::TooManyRows);
        }
        Ok(Self { rows, metadata })
    }

    /// Safe page rows.
    pub fn rows(&self) -> &[T] {
        &self.rows
    }

    /// Cursor and bound metadata.
    pub const fn metadata(&self) -> &CrmPageMetadata {
        &self.metadata
    }

    /// Whether the page contains no rows.
    pub fn is_empty(&self) -> bool {
        self.rows.is_empty()
    }
}

/// Sort handoff rows by their opaque aggregate identifier for deterministic rendering.
pub fn sort_handoff_rows(rows: &mut [CrmHandoffDisplayModel]) {
    rows.sort_by(|left, right| left.id.cmp(&right.id));
}

/// Sort opportunity rows by their opaque aggregate identifier for deterministic rendering.
pub fn sort_opportunity_rows(rows: &mut [CrmOpportunityDisplayModel]) {
    rows.sort_by(|left, right| left.id.cmp(&right.id));
}

/// Sort attribution rows by their opaque aggregate identifier for deterministic rendering.
pub fn sort_attribution_rows(rows: &mut [CrmAttributionDisplayModel]) {
    rows.sort_by(|left, right| left.id.cmp(&right.id));
}

/// Explicit CRM read-state machine. Denied and error states intentionally carry no display rows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CrmViewState<T> {
    /// A read is in progress.
    Loading {
        /// Previously visible metadata, if available.
        metadata: Option<CrmProjectionMetadata>,
    },
    /// The read completed with no rows.
    Empty {
        /// Metadata accompanying the empty result.
        metadata: CrmProjectionMetadata,
    },
    /// Current display rows are available.
    Ready {
        /// Safe display payload.
        data: T,
        /// Projection metadata.
        metadata: CrmProjectionMetadata,
    },
    /// The principal cannot view this read surface.
    Denied {
        /// Metadata may be absent when denial precedes the read.
        metadata: Option<CrmProjectionMetadata>,
        /// Safe policy/message text.
        reason: String,
    },
    /// Cached data is outside the freshness budget.
    Stale {
        /// Last safe display payload.
        data: T,
        /// Stale projection metadata.
        metadata: CrmProjectionMetadata,
        /// Age supplied by the read boundary.
        age_seconds: u64,
        /// Safe freshness explanation.
        reason: String,
    },
    /// Data is available but mutations are disabled.
    ReadOnly {
        /// Last safe display payload.
        data: T,
        /// Projection metadata.
        metadata: CrmProjectionMetadata,
        /// Safe read-only explanation.
        reason: String,
    },
    /// A concurrent write or revision race needs review.
    Conflict {
        /// Last safe display payload, when available.
        data: Option<T>,
        /// Projection metadata, when available.
        metadata: Option<CrmProjectionMetadata>,
        /// Safe conflict explanation.
        reason: String,
    },
    /// The backend is unavailable; cached data may remain visible.
    Offline {
        /// Cached safe display payload, when available.
        data: Option<T>,
        /// Cached projection metadata, when available.
        metadata: Option<CrmProjectionMetadata>,
        /// Safe connectivity explanation.
        reason: String,
    },
    /// An unexpected read failure occurred.
    Error {
        /// Safe error explanation.
        reason: String,
    },
}

impl<T> CrmViewState<T> {
    /// Return the shared explicit UI state kind.
    pub const fn kind(&self) -> Option<UiStateKind> {
        match self {
            Self::Loading { .. } => Some(UiStateKind::Loading),
            Self::Empty { .. } => Some(UiStateKind::Empty),
            Self::Ready { .. } => None,
            Self::Denied { .. } => Some(UiStateKind::Denied),
            Self::Stale { .. } => Some(UiStateKind::Stale),
            Self::ReadOnly { .. } => Some(UiStateKind::ReadOnly),
            Self::Conflict { .. } => Some(UiStateKind::Conflict),
            Self::Offline { .. } => Some(UiStateKind::Offline),
            Self::Error { .. } => Some(UiStateKind::Error),
        }
    }

    /// Return the shared bilingual state label for a non-ready state.
    pub const fn state_label(&self, locale: Locale) -> Option<&'static str> {
        let key = match self.kind() {
            Some(UiStateKind::Loading) => TranslationKey::Loading,
            Some(UiStateKind::Empty) => TranslationKey::Empty,
            Some(UiStateKind::Denied) => TranslationKey::AccessDenied,
            Some(UiStateKind::Stale) => TranslationKey::Stale,
            Some(UiStateKind::ReadOnly) => TranslationKey::ReadOnly,
            Some(UiStateKind::Conflict) => TranslationKey::Conflict,
            Some(UiStateKind::Offline) => TranslationKey::Offline,
            Some(UiStateKind::Error) => TranslationKey::Error,
            Some(_) => TranslationKey::Error,
            None => return None,
        };
        Some(key.text(locale))
    }

    /// Return a display-safe copy of a state explanation.
    ///
    /// State variants remain structurally compatible with existing adapters, but renderers
    /// should use this accessor instead of printing an untrusted transport error verbatim.
    pub fn redacted_reason(&self) -> Option<String> {
        let reason = match self {
            Self::Denied { reason, .. }
            | Self::Stale { reason, .. }
            | Self::ReadOnly { reason, .. }
            | Self::Conflict { reason, .. }
            | Self::Offline { reason, .. }
            | Self::Error { reason } => reason,
            Self::Loading { .. } | Self::Empty { .. } | Self::Ready { .. } => return None,
        };
        Some(
            safe_optional_value(reason, MAX_CRM_DISPLAY_VALUE_BYTES)
                .unwrap_or_else(|| "Unavailable".to_owned()),
        )
    }

    /// Return safe cached/current data when this state has one.
    pub fn data(&self) -> Option<&T> {
        match self {
            // A CRM payload is renderable only when the authorized adapter supplied an exact
            // holding/company/branch binding. This applies equally to current, stale, conflict,
            // and offline cache data; an unbound cache must never look like holding data.
            Self::Ready { data, metadata }
            | Self::Stale { data, metadata, .. }
            | Self::ReadOnly { data, metadata, .. }
                if metadata.has_scope() =>
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
            } if metadata.has_scope() => Some(data),
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Denied { .. }
            | Self::Error { .. }
            | Self::Ready { .. }
            | Self::Stale { .. }
            | Self::ReadOnly { .. }
            | Self::Conflict { .. }
            | Self::Offline { .. } => None,
        }
    }

    /// Return safe current or cached data only when it contains the requested record scope.
    pub fn data_for_record_scope(
        &self,
        holding_id: &str,
        company_id: &str,
        branch_id: Option<&str>,
    ) -> Option<&T> {
        self.metadata()
            .is_some_and(|metadata| {
                metadata.permits_record_scope(holding_id, company_id, branch_id)
            })
            .then(|| self.data())
            .flatten()
    }

    /// Whether the state has a scope-bound payload that a target may render.
    pub fn can_render_data(&self) -> bool {
        self.data().is_some()
    }

    /// Return projection metadata when available.
    pub fn metadata(&self) -> Option<&CrmProjectionMetadata> {
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

    /// Whether this state or its metadata disables writes.
    pub fn is_read_only(&self) -> bool {
        match self {
            Self::Ready { metadata, .. } => !metadata.can_mutate_without_record_scope(),
            Self::Loading { .. }
            | Self::Empty { .. }
            | Self::Denied { .. }
            | Self::Stale { .. }
            | Self::ReadOnly { .. }
            | Self::Conflict { .. }
            | Self::Offline { .. }
            | Self::Error { .. } => true,
        }
    }

    /// Whether this state requires server reconciliation before another write is attempted.
    pub const fn reconciliation_required(&self) -> bool {
        matches!(self, Self::Conflict { .. })
    }

    /// Whether a mutation is safe to submit for the given CRM surface and record scope.
    ///
    /// The checks mirror the existing domain/application boundary: CRM records must be inside
    /// the authorized holding scope, stale/offline/conflict/read-only states cannot write, and
    /// attribution requires a positive optimistic version. The server remains authoritative and
    /// must repeat these checks before persistence.
    pub fn mutation_allowed(
        &self,
        surface: CrmSurface,
        expected_version: Option<u64>,
        holding_id: &str,
        company_id: &str,
        branch_id: Option<&str>,
    ) -> bool {
        let Self::Ready { metadata, .. } = self else {
            return false;
        };
        if !metadata.can_mutate_record(holding_id, company_id, branch_id) {
            return false;
        }
        match surface {
            // AcceptedLeadHandoff creation has no optimistic version field in the domain API.
            CrmSurface::AcceptedLeadHandoffs => expected_version.is_none(),
            // Opportunity create may omit a version; an update must carry a positive version.
            CrmSurface::Opportunities => expected_version.is_none_or(|version| version > 0),
            // The attribution API is explicitly tied to an immutable opportunity revision.
            CrmSurface::Attribution => expected_version.is_some_and(|version| version > 0),
        }
    }

    /// Whether an attribution write passes scope, freshness, and model validation.
    pub fn attribution_mutation_allowed(
        &self,
        model: &str,
        expected_version: u64,
        holding_id: &str,
        company_id: &str,
        branch_id: Option<&str>,
    ) -> bool {
        self.mutation_allowed(
            CrmSurface::Attribution,
            Some(expected_version),
            holding_id,
            company_id,
            branch_id,
        ) && CrmAttributionModel::from_transport(model).is_some()
    }
}

/// Concrete state type for CRM display models.
pub type CrmState<T> = CrmViewState<T>;

fn collect_fields<'a>(
    inputs: impl IntoIterator<Item = CrmFieldInput<'a>>,
) -> (Vec<CrmDisplayField>, usize) {
    let mut fields: Vec<CrmDisplayField> = Vec::new();
    let mut redacted_field_count = 0;
    for (index, input) in inputs.into_iter().enumerate() {
        if index >= MAX_CRM_DISPLAY_FIELDS {
            redacted_field_count += 1;
            continue;
        }
        match CrmDisplayField::from_untrusted(input) {
            Some(field) if fields.iter().any(|existing| existing.key == field.key) => {
                redacted_field_count += 1;
            }
            Some(field) => fields.push(field),
            None => redacted_field_count += 1,
        }
    }
    fields.sort_by(|left, right| left.key.cmp(&right.key));
    (fields, redacted_field_count)
}

fn normalize_key(value: &str) -> Option<String> {
    if value.is_empty() || value.chars().any(char::is_control) {
        return None;
    }
    Some(value.trim().to_ascii_lowercase().replace('-', "_"))
}

fn safe_identifier(value: &str) -> Option<String> {
    safe_optional_value(value, MAX_CRM_IDENTIFIER_BYTES)
}

fn safe_scope_identifier(value: &str) -> Option<String> {
    let value = safe_optional_value(value, MAX_CRM_IDENTIFIER_BYTES)?;
    if value.chars().any(char::is_whitespace) || value.contains(['/', '\\', '?', '#']) {
        None
    } else {
        Some(value)
    }
}

fn safe_cursor(value: Option<&str>) -> Result<Option<String>, CrmPageError> {
    value
        .map(|value| {
            safe_optional_value(value, MAX_CRM_IDENTIFIER_BYTES).ok_or(CrmPageError::InvalidCursor)
        })
        .transpose()
}

fn safe_optional_value(value: &str, max_bytes: usize) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty()
        || trimmed.len() > max_bytes
        || trimmed.chars().any(char::is_control)
        || looks_structured_value(trimmed)
        || contains_sensitive_marker(trimmed)
    {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn looks_structured_value(value: &str) -> bool {
    matches!(value.as_bytes().first(), Some(b'{' | b'['))
        || (value.starts_with('"') && value.ends_with('"'))
}

fn is_allowed_display_key(key: &str) -> bool {
    matches!(
        key,
        "reference"
            | "lead_label"
            | "account_label"
            | "contact_label"
            | "accepted_at"
            | "source_label"
            | "scope_label"
            | "name"
            | "currency"
            | "expected_revenue_minor"
            | "created_at"
            | "updated_at"
            | "accepted_lead_reference"
            | "opportunity_reference"
            | "model"
            | "revenue_minor"
            | "cost_minor"
            | "campaign_label"
            | "attributed_at"
    )
}

fn contains_sensitive_marker(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "access_token",
        "refresh_token",
        "id_token",
        "token",
        "bearer ",
        "authorization",
        "password",
        "secret",
        "database_url",
        "client_secret",
        "session_cookie",
        "set-cookie",
        "api_key",
        "apikey",
        "private_key",
        "credential",
        "payment",
        "card_number",
        "credit_card",
        "bank_account",
        "routing_number",
        "cvv",
        "cvc",
        "stripe",
        "sk_live_",
        "sk_test_",
        "whsec_",
        "signed_url",
        "presigned_url",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn handoff(id: &str) -> CrmHandoffDisplayModel {
        CrmHandoffDisplayModel::from_untrusted_fields(
            id,
            [
                CrmFieldInput::new("reference", "handoff-1"),
                CrmFieldInput::new("account_label", "Demo account"),
            ],
        )
        .expect("valid handoff")
    }

    fn opportunity(id: &str) -> CrmOpportunityDisplayModel {
        CrmOpportunityDisplayModel::from_untrusted_fields(
            id,
            "draft",
            [CrmFieldInput::new("name", "Demo opportunity")],
        )
        .expect("valid opportunity")
    }

    fn attribution(id: &str) -> CrmAttributionDisplayModel {
        CrmAttributionDisplayModel::from_untrusted_fields(
            id,
            "linear",
            [CrmFieldInput::new("revenue_minor", "100")],
        )
        .expect("valid attribution")
    }

    #[test]
    fn sensitive_and_unsafe_fields_are_rejected_and_counted() {
        let input = CrmFieldInput::new("name", "access_token=secret");
        assert_eq!(
            format!("{input:?}"),
            "CrmFieldInput { key: \"name\", value: \"REDACTED\" }"
        );
        assert!(CrmDisplayField::from_untrusted(input).is_none());

        let model = CrmHandoffDisplayModel::from_untrusted_fields(
            "handoff-1",
            [
                CrmFieldInput::new("name", "safe"),
                CrmFieldInput::new("unsupported", "discard"),
                CrmFieldInput::new("account_label", "private_key=secret"),
                CrmFieldInput::new("contact_label", "bad\nvalue"),
            ],
        )
        .expect("valid required handoff identifier");
        assert_eq!(model.fields().len(), 1);
        assert_eq!(model.redacted_field_count(), 3);
        assert_eq!(model.field("name"), Some("safe"));
        assert_eq!(model.field("contact_label"), None);
    }

    #[test]
    fn lifecycle_statuses_have_stable_tokens_and_bilingual_labels() {
        let statuses = [
            ("draft", CrmOpportunityStatus::Draft),
            ("qualified", CrmOpportunityStatus::Qualified),
            ("won", CrmOpportunityStatus::Won),
            ("lost", CrmOpportunityStatus::Lost),
        ];
        for (token, expected) in statuses {
            let status = CrmOpportunityStatus::from_transport(token).expect("known status");
            assert_eq!(status, expected);
            assert_eq!(status.as_str(), token);
            assert_ne!(status.label(Locale::En), status.label(Locale::Th));
        }
        assert!(CrmOpportunityStatus::from_transport("secret_token").is_none());
    }

    #[test]
    fn attribution_models_have_stable_tokens_and_bilingual_labels() {
        for (token, model) in [
            ("first_touch", CrmAttributionModel::FirstTouch),
            ("last_touch", CrmAttributionModel::LastTouch),
            ("linear", CrmAttributionModel::Linear),
            ("u_shaped", CrmAttributionModel::UShaped),
            ("weighted", CrmAttributionModel::Weighted),
        ] {
            assert_eq!(CrmAttributionModel::from_transport(token), Some(model));
            assert_eq!(model.as_str(), token);
            assert_ne!(model.label(Locale::En), model.label(Locale::Th));
        }
        assert_eq!(CrmSurface::Attribution.label(Locale::En), "Attribution");
        assert_eq!(CrmSurface::Attribution.label(Locale::Th), "การระบุแหล่งที่มา");
    }

    #[test]
    fn page_metadata_and_rows_are_bounded() {
        assert_eq!(
            CrmPageMetadata::new(0, None, None, false),
            Err(CrmPageError::InvalidPageSize)
        );
        assert_eq!(
            CrmPageMetadata::new(MAX_CRM_PAGE_SIZE + 1, None, None, false),
            Err(CrmPageError::InvalidPageSize)
        );
        assert_eq!(
            CrmPageMetadata::new(10, None, None, true),
            Err(CrmPageError::InconsistentCursor)
        );
        assert_eq!(
            CrmPageMetadata::new(10, None, Some("next"), false),
            Err(CrmPageError::InconsistentCursor)
        );
        assert_eq!(
            CrmPageMetadata::new(10, Some("same"), Some("same"), true),
            Err(CrmPageError::CursorDidNotAdvance)
        );
        assert_eq!(
            CrmPageMetadata::new(10, Some("bad\nvalue"), None, false),
            Err(CrmPageError::InvalidCursor)
        );

        let metadata = CrmPageMetadata::new(1, None, None, false).expect("valid metadata");
        assert_eq!(
            CrmReadPage::new(vec![1, 2], metadata),
            Err(CrmPageError::TooManyRows)
        );
    }

    #[test]
    fn duplicate_display_keys_are_redacted_without_shadowing_the_first_value() {
        let model = CrmHandoffDisplayModel::from_untrusted_fields(
            "handoff-1",
            [
                CrmFieldInput::new("account_label", "first"),
                CrmFieldInput::new("account_label", "second"),
            ],
        )
        .expect("valid handoff identifier");
        assert_eq!(model.fields().len(), 1);
        assert_eq!(model.redacted_field_count(), 1);
        assert_eq!(model.field("account_label"), Some("first"));
    }

    #[test]
    fn freshness_and_read_only_metadata_are_explicit() {
        let fresh = CrmProjectionMetadata::fresh("2026-07-25T00:00:00Z");
        assert!(fresh.is_fresh());
        assert!(!fresh.is_stale());
        assert!(fresh.is_read_only());

        let stale = CrmProjectionMetadata::stale("2026-07-24T00:00:00Z");
        assert!(stale.is_stale());
        assert!(stale.is_read_only());

        let unknown = CrmProjectionMetadata::unknown();
        assert_eq!(unknown.freshness, ProjectionFreshness::Unknown);
        assert!(unknown.projection_updated_at.is_none());
        assert!(unknown.is_read_only());

        let writable = fresh.with_read_only(false);
        assert!(!writable.is_read_only());
    }

    #[test]
    fn explicit_state_variants_remain_distinguishable() {
        type State = CrmViewState<()>;
        let metadata = CrmProjectionMetadata::fresh("2026-07-25T00:00:00Z");
        let scoped_metadata = metadata
            .clone()
            .with_holding_scope("holding-1")
            .expect("safe scope");
        let states = [
            State::Loading {
                metadata: Some(metadata.clone()),
            },
            State::Empty {
                metadata: metadata.clone(),
            },
            State::Ready {
                data: (),
                metadata: scoped_metadata,
            },
            State::Denied {
                metadata: None,
                reason: "policy".into(),
            },
            State::Stale {
                data: (),
                metadata: CrmProjectionMetadata::stale("2026-07-24T00:00:00Z"),
                age_seconds: 60,
                reason: "lag".into(),
            },
            State::ReadOnly {
                data: (),
                metadata,
                reason: "billing".into(),
            },
            State::Conflict {
                data: None,
                metadata: None,
                reason: "version".into(),
            },
            State::Offline {
                data: None,
                metadata: None,
                reason: "network".into(),
            },
            State::Error {
                reason: "failure".into(),
            },
        ];
        assert_eq!(states[0].kind(), Some(UiStateKind::Loading));
        assert_eq!(states[1].kind(), Some(UiStateKind::Empty));
        assert_eq!(states[2].kind(), None);
        assert_eq!(states[3].kind(), Some(UiStateKind::Denied));
        assert_eq!(states[4].kind(), Some(UiStateKind::Stale));
        assert_eq!(states[5].kind(), Some(UiStateKind::ReadOnly));
        assert_eq!(states[6].kind(), Some(UiStateKind::Conflict));
        assert_eq!(states[7].kind(), Some(UiStateKind::Offline));
        assert_eq!(states[8].kind(), Some(UiStateKind::Error));
        assert_eq!(states[0].state_label(Locale::Th), Some("กำลังโหลด"));
        assert!(states[5].is_read_only());
        assert!(states[2].data().is_some());
        assert!(states[3].data().is_none());
    }

    #[test]
    fn row_sort_helpers_are_deterministic() {
        let mut handoffs = vec![handoff("b"), handoff("a")];
        sort_handoff_rows(&mut handoffs);
        assert_eq!(
            handoffs
                .iter()
                .map(CrmHandoffDisplayModel::id)
                .collect::<Vec<_>>(),
            ["a", "b"]
        );

        let mut opportunities = vec![opportunity("b"), opportunity("a")];
        sort_opportunity_rows(&mut opportunities);
        assert_eq!(
            opportunities
                .iter()
                .map(CrmOpportunityDisplayModel::id)
                .collect::<Vec<_>>(),
            ["a", "b"]
        );

        let mut attribution_rows = vec![attribution("b"), attribution("a")];
        sort_attribution_rows(&mut attribution_rows);
        assert_eq!(
            attribution_rows
                .iter()
                .map(CrmAttributionDisplayModel::id)
                .collect::<Vec<_>>(),
            ["a", "b"]
        );
    }
}
