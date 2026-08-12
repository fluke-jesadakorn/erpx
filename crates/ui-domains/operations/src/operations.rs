//! Client-local Operations navigation, projection, and display models.
//!
//! These types describe the presentation contract for Operations without importing a transport,
//! repository, or server authorization implementation. Operations coordinates source contexts;
//! it never becomes the owner of Sales, Procurement, Inventory, Finance, Maintenance, Support,
//! or Integration transactions. Applications convert an authorized read response into these
//! display-safe values at their boundary.

use std::fmt;

use erpx_ui_core::{Locale, NavItem, ProjectionFreshness, TranslationKey, UiStateKind};

/// The Operations pages in canonical navigation order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationsPage {
    /// Cross-domain operational summary.
    Overview,
    /// Sales-to-cash orchestration view.
    LeadToCash,
    /// Purchase-to-payment orchestration view.
    ProcureToPay,
    /// Inventory movement and availability flow.
    InventoryFlow,
    /// Detected, assigned, escalated, and resolved exceptions.
    Exceptions,
    /// Operational commitments and due work.
    Commitments,
    /// Fulfillment scenarios and plans.
    Fulfillment,
    /// Dispatch batches and execution.
    Dispatch,
    /// Capacity versions and utilization.
    Capacity,
    /// Operating calendars.
    Calendar,
    /// Branch shifts.
    Shifts,
    /// Shift handovers.
    Handover,
    /// Standard operating procedures and checklists.
    Sops,
    /// Cross-domain operational analytics.
    Analytics,
}

/// Stable page order used by web, desktop, and mobile navigation shells.
pub const OPERATIONS_PAGES: &[OperationsPage] = &[
    OperationsPage::Overview,
    OperationsPage::LeadToCash,
    OperationsPage::ProcureToPay,
    OperationsPage::InventoryFlow,
    OperationsPage::Exceptions,
    OperationsPage::Commitments,
    OperationsPage::Fulfillment,
    OperationsPage::Dispatch,
    OperationsPage::Capacity,
    OperationsPage::Calendar,
    OperationsPage::Shifts,
    OperationsPage::Handover,
    OperationsPage::Sops,
    OperationsPage::Analytics,
];

impl OperationsPage {
    /// Return every page in stable navigation order.
    pub const fn all() -> &'static [Self] {
        OPERATIONS_PAGES
    }

    /// Return the canonical application path for this page.
    pub const fn path(self) -> &'static str {
        match self {
            Self::Overview => "/operations",
            Self::LeadToCash => "/operations/lead-to-cash",
            Self::ProcureToPay => "/operations/procure-to-pay",
            Self::InventoryFlow => "/operations/inventory-flow",
            Self::Exceptions => "/operations/exceptions",
            Self::Commitments => "/operations/commitments",
            Self::Fulfillment => "/operations/fulfillment",
            Self::Dispatch => "/operations/dispatch",
            Self::Capacity => "/operations/capacity",
            Self::Calendar => "/operations/calendar",
            Self::Shifts => "/operations/shifts",
            Self::Handover => "/operations/handover",
            Self::Sops => "/operations/sops",
            Self::Analytics => "/operations/analytics",
        }
    }

    /// Return a small stable icon token interpreted by the consuming theme.
    pub const fn icon(self) -> &'static str {
        match self {
            Self::Overview => "⌁",
            Self::LeadToCash => "↗",
            Self::ProcureToPay => "⇄",
            Self::InventoryFlow => "▤",
            Self::Exceptions => "!",
            Self::Commitments => "◷",
            Self::Fulfillment => "□",
            Self::Dispatch => "➜",
            Self::Capacity => "▥",
            Self::Calendar => "□",
            Self::Shifts => "◴",
            Self::Handover => "⇢",
            Self::Sops => "☷",
            Self::Analytics => "⌁",
        }
    }

    /// Return the translation key for this page label.
    pub(crate) const fn translation_key(self) -> TranslationKey {
        match self {
            Self::Overview => TranslationKey::OperationsOverview,
            Self::LeadToCash => TranslationKey::OperationsLeadToCash,
            Self::ProcureToPay => TranslationKey::OperationsProcureToPay,
            Self::InventoryFlow => TranslationKey::OperationsInventoryFlow,
            Self::Exceptions => TranslationKey::OperationsExceptions,
            Self::Commitments => TranslationKey::OperationsCommitments,
            Self::Fulfillment => TranslationKey::OperationsFulfillment,
            Self::Dispatch => TranslationKey::OperationsDispatch,
            Self::Capacity => TranslationKey::OperationsCapacity,
            Self::Calendar => TranslationKey::OperationsCalendar,
            Self::Shifts => TranslationKey::OperationsShifts,
            Self::Handover => TranslationKey::OperationsHandover,
            Self::Sops => TranslationKey::OperationsSops,
            Self::Analytics => TranslationKey::OperationsAnalytics,
        }
    }

    /// Return the localized page label.
    pub const fn label(self, locale: Locale) -> &'static str {
        self.translation_key().text(locale)
    }

    /// Build an existing route-agnostic navigation item for a shell.
    pub fn nav_item(self, locale: Locale) -> NavItem {
        NavItem::new(self.label(locale), self.path(), self.icon())
    }

    /// Return source domains whose events may be represented on this page.
    ///
    /// The list intentionally has no `Operations` source domain: Operations owns coordination
    /// and projections, while source transactions remain in their bounded contexts.
    pub const fn source_domains(self) -> &'static [OperationsSourceDomain] {
        match self {
            Self::Overview | Self::Exceptions | Self::Commitments | Self::Analytics => {
                ALL_SOURCE_DOMAINS
            }
            Self::LeadToCash => &[
                OperationsSourceDomain::Sales,
                OperationsSourceDomain::Finance,
                OperationsSourceDomain::Support,
                OperationsSourceDomain::Integrations,
            ],
            Self::ProcureToPay => &[
                OperationsSourceDomain::Procurement,
                OperationsSourceDomain::Finance,
                OperationsSourceDomain::Inventory,
                OperationsSourceDomain::Integrations,
            ],
            Self::InventoryFlow => &[
                OperationsSourceDomain::Inventory,
                OperationsSourceDomain::Sales,
                OperationsSourceDomain::Procurement,
                OperationsSourceDomain::Maintenance,
                OperationsSourceDomain::Integrations,
            ],
            Self::Fulfillment => &[
                OperationsSourceDomain::Sales,
                OperationsSourceDomain::Procurement,
                OperationsSourceDomain::Inventory,
                OperationsSourceDomain::Maintenance,
                OperationsSourceDomain::Integrations,
            ],
            Self::Dispatch => &[
                OperationsSourceDomain::Inventory,
                OperationsSourceDomain::Sales,
                OperationsSourceDomain::Procurement,
                OperationsSourceDomain::Maintenance,
            ],
            Self::Capacity => &[
                OperationsSourceDomain::Inventory,
                OperationsSourceDomain::Maintenance,
                OperationsSourceDomain::Sales,
                OperationsSourceDomain::Procurement,
            ],
            Self::Calendar | Self::Shifts | Self::Handover | Self::Sops => &[],
        }
    }

    /// Whether this page is a coordination/projection surface rather than a source ledger.
    pub const fn owns_source_transactions(self) -> bool {
        false
    }
}

/// Source contexts that can feed Operations projections.
///
/// `Operations` is deliberately absent. This closed set makes it impossible for a page's
/// declared source ownership to label an Operations-owned source transaction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationsSourceDomain {
    /// Sales leads, orders, deliveries, and collections.
    Sales,
    /// Purchase requests, orders, receipts, and supplier bills.
    Procurement,
    /// Products, stock movements, transfers, and counts.
    Inventory,
    /// Invoices, journals, payments, and accounting periods.
    Finance,
    /// Equipment work and maintenance capacity signals.
    Maintenance,
    /// Support tickets and customer-approved support activity.
    Support,
    /// External integration events and adapter failures.
    Integrations,
}

const ALL_SOURCE_DOMAINS: &[OperationsSourceDomain] = &[
    OperationsSourceDomain::Sales,
    OperationsSourceDomain::Procurement,
    OperationsSourceDomain::Inventory,
    OperationsSourceDomain::Finance,
    OperationsSourceDomain::Maintenance,
    OperationsSourceDomain::Support,
    OperationsSourceDomain::Integrations,
];

impl OperationsSourceDomain {
    /// Return the localized source-domain ownership label.
    pub const fn label(self, locale: Locale) -> &'static str {
        let key = match self {
            Self::Sales => TranslationKey::OperationsSourceSales,
            Self::Procurement => TranslationKey::OperationsSourceProcurement,
            Self::Inventory => TranslationKey::OperationsSourceInventory,
            Self::Finance => TranslationKey::OperationsSourceFinance,
            Self::Maintenance => TranslationKey::OperationsSourceMaintenance,
            Self::Support => TranslationKey::OperationsSourceSupport,
            Self::Integrations => TranslationKey::OperationsSourceIntegrations,
        };
        key.text(locale)
    }

    /// Stable, non-localized ownership token for filtering and telemetry labels.
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Sales => "sales",
            Self::Procurement => "procurement",
            Self::Inventory => "inventory",
            Self::Finance => "finance",
            Self::Maintenance => "maintenance",
            Self::Support => "support",
            Self::Integrations => "integrations",
        }
    }
}

/// Scope levels that can be safely displayed by an Operations client.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationsScopeKind {
    /// Holding-wide scope.
    Holding,
    /// Legal-company scope.
    Company,
    /// Branch scope.
    Branch,
    /// Explicit holding subtree scope.
    Subtree,
}

/// Safe scope metadata supplied by an authorized read boundary.
#[derive(Clone, PartialEq, Eq)]
pub struct OperationsScope {
    id: String,
    label: String,
    /// Scope level used by the presentation shell.
    pub kind: OperationsScopeKind,
}

impl fmt::Debug for OperationsScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OperationsScope")
            .field("id", &self.id)
            .field("label", &self.label)
            .field("kind", &self.kind)
            .finish()
    }
}

impl OperationsScope {
    /// Construct scope metadata, replacing suspicious display values with a safe marker.
    pub fn new(id: impl Into<String>, label: impl Into<String>, kind: OperationsScopeKind) -> Self {
        Self {
            id: redact_value(&id.into()),
            label: redact_value(&label.into()),
            kind,
        }
    }

    /// Return the opaque scope identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Return the safe scope display label.
    pub fn label(&self) -> &str {
        &self.label
    }
}

/// Safe projection metadata shared by every Operations page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsProjectionMetadata {
    /// Holding/company/branch scope of the projection.
    pub scope: OperationsScope,
    /// Server-provided display timestamp, kept opaque to the client model.
    pub projection_updated_at: Option<String>,
    /// Freshness classification supplied by the read boundary.
    pub freshness: ProjectionFreshness,
    /// Whether the current context is explicitly non-mutating.
    pub read_only: bool,
}

impl OperationsProjectionMetadata {
    /// Build metadata for a fresh projection.
    pub fn fresh(scope: OperationsScope, projection_updated_at: impl Into<String>) -> Self {
        Self::present(scope, projection_updated_at, ProjectionFreshness::Fresh)
    }

    /// Build metadata for a stale but displayable projection.
    pub fn stale(scope: OperationsScope, projection_updated_at: impl Into<String>) -> Self {
        Self::present(scope, projection_updated_at, ProjectionFreshness::Stale)
    }

    /// Build metadata when no projection timestamp is available.
    pub fn unknown(scope: OperationsScope) -> Self {
        Self {
            scope,
            projection_updated_at: None,
            freshness: ProjectionFreshness::Unknown,
            read_only: true,
        }
    }

    /// Build present metadata with an explicit freshness classification.
    pub fn present(
        scope: OperationsScope,
        projection_updated_at: impl Into<String>,
        freshness: ProjectionFreshness,
    ) -> Self {
        let projection_updated_at = safe_optional_value(&projection_updated_at.into());
        Self {
            scope,
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

    /// Whether this projection is explicitly read-only.
    pub const fn is_read_only(&self) -> bool {
        self.read_only || !self.is_fresh()
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

/// One untrusted field handed to the Operations display boundary.
pub struct OperationsFieldInput<'a> {
    key: &'a str,
    value: &'a str,
}

impl<'a> OperationsFieldInput<'a> {
    /// Create a field candidate from an authorized projection DTO.
    pub const fn new(key: &'a str, value: &'a str) -> Self {
        Self { key, value }
    }
}

impl fmt::Debug for OperationsFieldInput<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("OperationsFieldInput")
            .field("key", &self.key)
            .field("value", &"REDACTED")
            .finish()
    }
}

/// A field safe for Operations display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsDisplayField {
    key: String,
    value: String,
}

impl OperationsDisplayField {
    /// Copy a field only when its key is allow-listed and its value is bounded and safe.
    pub fn from_untrusted(input: OperationsFieldInput<'_>) -> Option<Self> {
        if !is_allowed_display_key(input.key) {
            return None;
        }
        let value = safe_optional_value(input.value)?;
        Some(Self {
            key: input.key.trim().to_ascii_lowercase().replace('-', "_"),
            value,
        })
    }

    /// Stable field key.
    pub fn key(&self) -> &str {
        &self.key
    }

    /// Safe display value.
    pub fn value(&self) -> &str {
        &self.value
    }
}

/// Safe, page-oriented Operations display data.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsDisplayModel {
    /// The Operations page this model renders.
    pub page: OperationsPage,
    /// Scope and freshness metadata for the page.
    pub metadata: OperationsProjectionMetadata,
    fields: Vec<OperationsDisplayField>,
    redacted_field_count: usize,
}

impl OperationsDisplayModel {
    /// Create an empty display model for a page.
    pub fn new(page: OperationsPage, metadata: OperationsProjectionMetadata) -> Self {
        Self {
            page,
            metadata,
            fields: Vec::new(),
            redacted_field_count: 0,
        }
    }

    /// Create a model from projection field candidates.
    pub fn from_untrusted_fields<'a>(
        page: OperationsPage,
        metadata: OperationsProjectionMetadata,
        inputs: impl IntoIterator<Item = OperationsFieldInput<'a>>,
    ) -> Self {
        let mut model = Self::new(page, metadata);
        for input in inputs {
            model.push_untrusted(input);
        }
        model
    }

    /// Try to append one field. Returns `true` only when it is retained.
    pub fn push_untrusted(&mut self, input: OperationsFieldInput<'_>) -> bool {
        if self.fields.len() >= MAX_OPERATIONS_DISPLAY_FIELDS {
            self.redacted_field_count = self.redacted_field_count.saturating_add(1);
            return false;
        }
        match OperationsDisplayField::from_untrusted(input) {
            Some(field) => {
                self.fields.push(field);
                true
            }
            None => {
                self.redacted_field_count += 1;
                false
            }
        }
    }

    /// Return safe display fields only.
    pub fn fields(&self) -> &[OperationsDisplayField] {
        &self.fields
    }

    /// Number of input fields omitted at the redaction boundary.
    pub const fn redacted_field_count(&self) -> usize {
        self.redacted_field_count
    }

    /// Return the localized label for this page.
    pub const fn page_label(&self, locale: Locale) -> &'static str {
        self.page.label(locale)
    }
}

/// Explicit Operations projection state machine.
///
/// Cached safe data is retained for stale, read-only, conflict, and offline states. Denied and
/// error states carry no display payload by construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OperationsViewState<T> {
    /// A read is in progress.
    Loading {
        /// Metadata from the previously displayed projection, if available.
        metadata: Option<OperationsProjectionMetadata>,
    },
    /// The read completed with no rows/content.
    Empty {
        /// Metadata accompanying the empty result.
        metadata: OperationsProjectionMetadata,
    },
    /// Current display data is available.
    Ready {
        /// Safe display data.
        data: T,
        /// Projection metadata.
        metadata: OperationsProjectionMetadata,
    },
    /// The principal cannot view this page.
    Denied {
        /// Metadata may be absent when denial happens before a read.
        metadata: Option<OperationsProjectionMetadata>,
        /// Safe policy/message text only.
        reason: String,
    },
    /// Cached data is outside the freshness budget.
    Stale {
        /// Last safe display data.
        data: T,
        /// Stale projection metadata.
        metadata: OperationsProjectionMetadata,
        /// Age supplied by the read boundary.
        age_seconds: u64,
        /// Safe freshness explanation.
        reason: String,
    },
    /// Data is available but mutations are disabled.
    ReadOnly {
        /// Last safe display data.
        data: T,
        /// Projection metadata.
        metadata: OperationsProjectionMetadata,
        /// Safe read-only explanation.
        reason: String,
    },
    /// A concurrent write or revision race needs review.
    Conflict {
        /// Last safe display data, when available.
        data: Option<T>,
        /// Projection metadata, when available.
        metadata: Option<OperationsProjectionMetadata>,
        /// Safe conflict explanation.
        reason: String,
    },
    /// The backend is unavailable; cached display data may remain visible.
    Offline {
        /// Cached safe display data, when available.
        data: Option<T>,
        /// Cached projection metadata, when available.
        metadata: Option<OperationsProjectionMetadata>,
        /// Safe connectivity explanation.
        reason: String,
    },
    /// An unexpected read failure occurred.
    Error {
        /// Safe error explanation.
        reason: String,
    },
}

impl<T> OperationsViewState<T> {
    /// Return the shared UI state kind used by existing Dioxus state components.
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

    /// Return the explicit state label for the requested locale.
    pub const fn state_label(&self, locale: Locale) -> Option<&'static str> {
        let key = match self.kind() {
            Some(UiStateKind::Loading) => TranslationKey::Loading,
            Some(UiStateKind::Submitting) => TranslationKey::Loading,
            Some(UiStateKind::Empty) => TranslationKey::Empty,
            Some(UiStateKind::Success) => TranslationKey::Empty,
            Some(UiStateKind::Denied) => TranslationKey::AccessDenied,
            Some(UiStateKind::Stale) => TranslationKey::Stale,
            Some(UiStateKind::ReadOnly) => TranslationKey::ReadOnly,
            Some(UiStateKind::Conflict) => TranslationKey::Conflict,
            Some(UiStateKind::Offline) => TranslationKey::Offline,
            Some(UiStateKind::RetryableError | UiStateKind::Unavailable) => TranslationKey::Error,
            Some(UiStateKind::Error) => TranslationKey::Error,
            None => return None,
        };
        Some(key.text(locale))
    }

    /// Return safe cached/current display data, if this state has one.
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

    /// Return projection metadata, if available.
    pub fn metadata(&self) -> Option<&OperationsProjectionMetadata> {
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
            Self::Ready { metadata, .. } => metadata.is_read_only(),
            // A mutation must never be offered while the displayed projection
            // is loading, empty, stale, denied, conflicted, offline, or in an
            // error state. Only an explicitly ready projection may opt into a
            // write-capable presentation boundary.
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
}

/// Concrete state type for Operations display models.
pub type OperationsState = OperationsViewState<OperationsDisplayModel>;

fn is_allowed_display_key(key: &str) -> bool {
    if key.trim().len() > MAX_OPERATIONS_DISPLAY_KEY_BYTES {
        return false;
    }
    let key = key.trim().to_ascii_lowercase().replace('-', "_");
    matches!(
        key.as_str(),
        "plan_name"
            | "commitment_subject"
            | "commitment_quantity"
            | "commitment_unit"
            | "commitment_due_at"
            | "fulfillment_status"
            | "capacity_label"
            | "capacity_quantity"
            | "exception_category"
            | "exception_severity"
            | "exception_status"
            | "exception_assignee_label"
            | "exception_due_at"
            | "queue_name"
            | "queue_status"
            | "calendar_name"
            | "calendar_timezone"
            | "shift_branch_label"
            | "shift_starts_at"
            | "shift_ends_at"
            | "shift_status"
            | "handover_outgoing_label"
            | "handover_incoming_label"
            | "handover_summary"
            | "sop_name"
            | "sop_revision"
            | "sop_status"
            | "dispatch_status"
            | "source_domain"
            | "source_type"
            | "source_reference"
            | "event_count"
            | "projection_lag_seconds"
            | "last_activity_at"
            | "reconciliation_status"
            | "reconciliation_matched_count"
            | "reconciliation_unmatched_count"
            | "reconciliation_difference"
            | "reconciliation_last_checked_at"
    )
}

fn safe_optional_value(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_OPERATIONS_DISPLAY_VALUE_BYTES
        || contains_secret_marker(value)
        || contains_control_character(value)
    {
        None
    } else {
        Some(value.to_string())
    }
}

fn redact_value(value: &str) -> String {
    safe_optional_value(value).unwrap_or_else(|| "[redacted]".to_string())
}

fn contains_secret_marker(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "access_token=",
        "token=",
        "refresh_token=",
        "client_secret=",
        "client-secret=",
        "password=",
        "password:",
        "authorization: bearer ",
        "authorization:bearer ",
        "bearer ",
        "-----begin ",
        "_secret",
        "secret=",
        "secret:",
        "api_key=",
        "api-key=",
        "apikey=",
        "private_key=",
        "signed_url=",
        "presigned_url=",
        "sk_live_",
        "rk_live_",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

fn contains_control_character(value: &str) -> bool {
    value
        .chars()
        .any(|character| character.is_control() && !matches!(character, '\n' | '\r' | '\t'))
}

const MAX_OPERATIONS_DISPLAY_FIELDS: usize = 128;
const MAX_OPERATIONS_DISPLAY_KEY_BYTES: usize = 128;
const MAX_OPERATIONS_DISPLAY_VALUE_BYTES: usize = 512;

#[cfg(test)]
mod tests {
    use super::*;

    fn scope() -> OperationsScope {
        OperationsScope::new("holding_123", "Demo holding", OperationsScopeKind::Holding)
    }

    fn metadata() -> OperationsProjectionMetadata {
        OperationsProjectionMetadata::fresh(scope(), "2026-07-24T00:00:00Z")
    }

    #[test]
    fn required_operations_pages_have_stable_paths_and_bilingual_labels() {
        let expected_paths = [
            "/operations",
            "/operations/lead-to-cash",
            "/operations/procure-to-pay",
            "/operations/inventory-flow",
            "/operations/exceptions",
            "/operations/commitments",
            "/operations/fulfillment",
            "/operations/dispatch",
            "/operations/capacity",
            "/operations/calendar",
            "/operations/shifts",
            "/operations/handover",
            "/operations/sops",
            "/operations/analytics",
        ];

        assert_eq!(OperationsPage::all().len(), expected_paths.len());
        for (page, expected_path) in OperationsPage::all().iter().zip(expected_paths) {
            assert_eq!(page.path(), expected_path);
            assert!(!page.label(Locale::En).is_empty());
            assert!(!page.label(Locale::Th).is_empty());
            assert_ne!(page.label(Locale::En), page.label(Locale::Th));
            assert_eq!(page.nav_item(Locale::En).path, expected_path);
        }
    }

    #[test]
    fn source_domain_ownership_never_labels_operations_as_a_source_owner() {
        let all_domains = OperationsPage::all()
            .iter()
            .flat_map(|page| page.source_domains())
            .collect::<Vec<_>>();

        assert!(
            all_domains
                .iter()
                .all(|domain| domain.as_str() != "operations")
        );
        assert!(
            OperationsPage::all()
                .iter()
                .all(|page| !page.owns_source_transactions())
        );
        assert_eq!(OperationsPage::Calendar.source_domains(), &[]);
        assert_eq!(OperationsSourceDomain::Sales.label(Locale::En), "Sales");
        assert_eq!(
            OperationsSourceDomain::Procurement.label(Locale::Th),
            "จัดซื้อ"
        );
    }

    #[test]
    fn scope_freshness_and_read_only_metadata_are_explicit() {
        let fresh = metadata();
        assert!(fresh.is_fresh());
        assert!(!fresh.is_stale());
        assert!(fresh.is_read_only());
        assert_eq!(fresh.scope.label(), "Demo holding");

        let writable = fresh.clone().with_read_only(false);
        assert!(!writable.is_read_only());

        let stale = OperationsProjectionMetadata::stale(scope(), "2026-07-23T00:00:00Z");
        assert!(stale.is_stale());
        assert_eq!(stale.freshness, ProjectionFreshness::Stale);

        let unknown = OperationsProjectionMetadata::unknown(scope());
        assert_eq!(unknown.freshness, ProjectionFreshness::Unknown);
        assert!(unknown.is_read_only());
    }

    #[test]
    fn stale_and_unknown_metadata_cannot_become_write_capable() {
        let stale = OperationsProjectionMetadata::stale(scope(), "2026-07-23T00:00:00Z")
            .with_read_only(false);
        assert!(stale.is_read_only());

        let unknown = OperationsProjectionMetadata::unknown(scope()).with_read_only(false);
        assert!(unknown.is_read_only());

        let model = OperationsDisplayModel::new(OperationsPage::Overview, metadata());
        assert!(
            OperationsState::Ready {
                data: model,
                metadata: stale,
            }
            .is_read_only()
        );
    }

    #[test]
    fn all_non_ready_state_variants_remain_distinguishable() {
        let model = OperationsDisplayModel::new(OperationsPage::Overview, metadata());
        let states = [
            OperationsState::Loading {
                metadata: Some(metadata()),
            }
            .kind(),
            OperationsState::Empty {
                metadata: metadata(),
            }
            .kind(),
            OperationsState::Denied {
                metadata: None,
                reason: "policy".into(),
            }
            .kind(),
            OperationsState::Stale {
                data: model.clone(),
                metadata: OperationsProjectionMetadata::stale(scope(), "2026-07-23T00:00:00Z"),
                age_seconds: 90,
                reason: "lag".into(),
            }
            .kind(),
            OperationsState::ReadOnly {
                data: model.clone(),
                metadata: metadata(),
                reason: "billing".into(),
            }
            .kind(),
            OperationsState::Conflict {
                data: Some(model.clone()),
                metadata: Some(metadata()),
                reason: "version".into(),
            }
            .kind(),
            OperationsState::Offline {
                data: Some(model),
                metadata: None,
                reason: "network".into(),
            }
            .kind(),
            OperationsViewState::<OperationsDisplayModel>::Error {
                reason: "backend".into(),
            }
            .kind(),
        ];

        assert_eq!(
            states,
            [
                Some(UiStateKind::Loading),
                Some(UiStateKind::Empty),
                Some(UiStateKind::Denied),
                Some(UiStateKind::Stale),
                Some(UiStateKind::ReadOnly),
                Some(UiStateKind::Conflict),
                Some(UiStateKind::Offline),
                Some(UiStateKind::Error),
            ]
        );
    }

    #[test]
    fn only_ready_projection_state_can_be_write_capable() {
        let model = OperationsDisplayModel::new(OperationsPage::Exceptions, metadata());
        let writable_metadata = metadata().with_read_only(false);
        assert!(
            !OperationsState::Ready {
                data: model.clone(),
                metadata: writable_metadata.clone(),
            }
            .is_read_only()
        );
        assert!(
            OperationsState::Stale {
                data: model.clone(),
                metadata: writable_metadata.clone(),
                age_seconds: 301,
                reason: "projection lag".into(),
            }
            .is_read_only()
        );
        assert!(
            OperationsState::Conflict {
                data: Some(model.clone()),
                metadata: Some(writable_metadata.clone()),
                reason: "concurrent write".into(),
            }
            .is_read_only()
        );
        assert!(
            OperationsState::Offline {
                data: Some(model),
                metadata: Some(writable_metadata),
                reason: "backend unavailable".into(),
            }
            .is_read_only()
        );
    }

    #[test]
    fn untrusted_display_values_are_allow_listed_and_redacted() {
        let safe = OperationsDisplayField::from_untrusted(OperationsFieldInput::new(
            "exception-status",
            "escalated",
        ))
        .expect("allow-listed value should display");
        assert_eq!(safe.key(), "exception_status");
        assert_eq!(safe.value(), "escalated");

        for secret in [
            "access_token=abc",
            "Bearer abc",
            "client_secret=abc",
            "https://example.test/file?presigned_url=secret",
            "-----BEGIN PRIVATE KEY-----",
        ] {
            assert!(
                OperationsDisplayField::from_untrusted(OperationsFieldInput::new(
                    "source_reference",
                    secret,
                ))
                .is_none(),
                "secret marker must not reach display: {secret}"
            );
        }

        let mut model = OperationsDisplayModel::new(OperationsPage::Exceptions, metadata());
        assert!(!model.push_untrusted(OperationsFieldInput::new(
            "source_reference",
            "refresh_token=abc",
        )));
        assert!(
            !model.push_untrusted(OperationsFieldInput::new("unknown_field", "business value",))
        );
        assert_eq!(model.fields().len(), 0);
        assert_eq!(model.redacted_field_count(), 2);
        assert!(!format!("{model:?}").contains("refresh_token=abc"));
    }

    #[test]
    fn reconciliation_fields_are_allow_listed_and_display_bounds_are_enforced() {
        let model = OperationsDisplayModel::from_untrusted_fields(
            OperationsPage::Overview,
            metadata(),
            [
                OperationsFieldInput::new("reconciliation_status", "matched"),
                OperationsFieldInput::new("reconciliation_matched_count", "12"),
                OperationsFieldInput::new("reconciliation_unmatched_count", "0"),
                OperationsFieldInput::new("reconciliation_difference", "0"),
                OperationsFieldInput::new("reconciliation_last_checked_at", "2026-07-24T00:00:00Z"),
            ],
        );
        assert_eq!(model.fields().len(), 5);

        let oversized = "x".repeat(MAX_OPERATIONS_DISPLAY_VALUE_BYTES + 1);
        assert!(
            OperationsDisplayField::from_untrusted(OperationsFieldInput::new(
                "reconciliation_status",
                &oversized,
            ))
            .is_none()
        );
        assert!(
            OperationsDisplayField::from_untrusted(OperationsFieldInput::new(
                "reconciliation_status",
                "matched\u{0000}",
            ))
            .is_none()
        );

        let mut bounded = OperationsDisplayModel::new(OperationsPage::Overview, metadata());
        for _ in 0..MAX_OPERATIONS_DISPLAY_FIELDS {
            assert!(bounded.push_untrusted(OperationsFieldInput::new(
                "reconciliation_status",
                "matched",
            )));
        }
        assert!(!bounded.push_untrusted(OperationsFieldInput::new(
            "reconciliation_status",
            "matched",
        )));
        assert_eq!(bounded.fields().len(), MAX_OPERATIONS_DISPLAY_FIELDS);
        assert_eq!(bounded.redacted_field_count(), 1);
    }

    #[test]
    fn suspicious_scope_and_timestamp_values_are_redacted() {
        let scope = OperationsScope::new(
            "access_token=secret",
            "Bearer secret",
            OperationsScopeKind::Branch,
        );
        assert_eq!(scope.id(), "[redacted]");
        assert_eq!(scope.label(), "[redacted]");

        let metadata = OperationsProjectionMetadata::fresh(scope, "password=secret");
        assert_eq!(metadata.projection_updated_at, None);
        assert_eq!(metadata.freshness, ProjectionFreshness::Unknown);
        assert!(!format!("{metadata:?}").contains("password=secret"));
    }
}
