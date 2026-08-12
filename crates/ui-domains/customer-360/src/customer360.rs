//! Client-local Customer 360 view and display models.
//!
//! These types mirror the shape of the existing customer projection read contracts without
//! depending on the domain or transport crates. They contain presentation data only: a
//! consuming application is responsible for converting an authorized read response into these
//! models. No endpoint, credential, provider reference, or secret payload belongs here.

use std::fmt;

use erpx_ui_core::{
    CustomerId, CustomerView, Locale, ProjectionFreshness, RouteId, TranslationKey, UiStateKind,
};

/// The complete platform Customer 360 sub-page set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Customer360Section {
    /// Customer summary and account health at a glance.
    Overview,
    /// Organization hierarchy and counts.
    Organization,
    /// Responsibility contacts and customer-success owner.
    Contacts,
    /// Billing and commercial read model.
    Commercial,
    /// Subscription and organization module allocations.
    Entitlements,
    /// Onboarding progress and ownership.
    Onboarding,
    /// Operational health and projection signals.
    Health,
    /// Support workload summary.
    Support,
    /// Customer-visible global-read access audit.
    AccessAudit,
    /// Customer notes read model.
    Notes,
}

/// Canonical section order used by Customer 360 navigation.
pub const CUSTOMER_360_SECTIONS: &[Customer360Section] = &[
    Customer360Section::Overview,
    Customer360Section::Organization,
    Customer360Section::Contacts,
    Customer360Section::Commercial,
    Customer360Section::Entitlements,
    Customer360Section::Onboarding,
    Customer360Section::Health,
    Customer360Section::Support,
    Customer360Section::AccessAudit,
    Customer360Section::Notes,
];

impl Customer360Section {
    /// Return every section in stable navigation order.
    pub const fn all() -> &'static [Self] {
        CUSTOMER_360_SECTIONS
    }

    /// Map a display section to the existing typed route view.
    pub const fn route_view(self) -> CustomerView {
        match self {
            Self::Overview => CustomerView::Overview,
            Self::Organization => CustomerView::Organization,
            Self::Contacts => CustomerView::Contacts,
            Self::Commercial => CustomerView::Commercial,
            Self::Entitlements => CustomerView::Entitlements,
            Self::Onboarding => CustomerView::Onboarding,
            Self::Health => CustomerView::Health,
            Self::Support => CustomerView::Support,
            Self::AccessAudit => CustomerView::AccessAudit,
            Self::Notes => CustomerView::Notes,
        }
    }

    /// Map a display section to the existing stable route identifier.
    pub const fn route_id(self) -> RouteId {
        match self {
            Self::Overview => RouteId::CustomerOverview,
            Self::Organization => RouteId::CustomerOrganization,
            Self::Contacts => RouteId::CustomerContacts,
            Self::Commercial => RouteId::CustomerCommercial,
            Self::Entitlements => RouteId::CustomerEntitlements,
            Self::Onboarding => RouteId::CustomerOnboarding,
            Self::Health => RouteId::CustomerHealth,
            Self::Support => RouteId::CustomerSupport,
            Self::AccessAudit => RouteId::CustomerAccessAudit,
            Self::Notes => RouteId::CustomerNotes,
        }
    }

    /// Return the localized section label.
    pub(crate) const fn translation_key(self) -> TranslationKey {
        match self {
            Self::Overview => TranslationKey::Customer360Overview,
            Self::Organization => TranslationKey::Customer360Organization,
            Self::Contacts => TranslationKey::Customer360Contacts,
            Self::Commercial => TranslationKey::Customer360Commercial,
            Self::Entitlements => TranslationKey::Customer360Entitlements,
            Self::Onboarding => TranslationKey::Customer360Onboarding,
            Self::Health => TranslationKey::Customer360Health,
            Self::Support => TranslationKey::Customer360Support,
            Self::AccessAudit => TranslationKey::Customer360AccessAudit,
            Self::Notes => TranslationKey::Customer360Notes,
        }
    }

    /// Return the localized section label.
    pub const fn label(self, locale: Locale) -> &'static str {
        self.translation_key().text(locale)
    }
}

impl From<CustomerView> for Customer360Section {
    fn from(view: CustomerView) -> Self {
        match view {
            CustomerView::Overview => Self::Overview,
            CustomerView::Organization => Self::Organization,
            CustomerView::Contacts => Self::Contacts,
            CustomerView::Commercial => Self::Commercial,
            CustomerView::Entitlements => Self::Entitlements,
            CustomerView::Onboarding => Self::Onboarding,
            CustomerView::Health => Self::Health,
            CustomerView::Support => Self::Support,
            CustomerView::AccessAudit => Self::AccessAudit,
            CustomerView::Notes => Self::Notes,
        }
    }
}

impl From<Customer360Section> for CustomerView {
    fn from(section: Customer360Section) -> Self {
        section.route_view()
    }
}

/// Backwards-readable name for callers that refer to Customer 360 freshness directly.
pub type Customer360Freshness = ProjectionFreshness;

/// Safe projection metadata shared by every Customer 360 section.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer360ProjectionMetadata {
    /// Validated route customer identifier.
    pub customer_id: CustomerId,
    /// Server-provided display timestamp, kept opaque to this client model.
    pub projection_updated_at: Option<String>,
    /// Freshness classification supplied by the read boundary.
    pub freshness: ProjectionFreshness,
    /// Whether the source projection is explicitly read-only.
    read_only: bool,
}

impl Customer360ProjectionMetadata {
    const MAX_TIMESTAMP_BYTES: usize = 64;

    /// Build metadata for a fresh projection.
    pub fn fresh(customer_id: CustomerId, projection_updated_at: impl Into<String>) -> Self {
        Self::present(
            customer_id,
            projection_updated_at,
            ProjectionFreshness::Fresh,
        )
    }

    /// Build metadata for a stale but displayable projection.
    pub fn stale(customer_id: CustomerId, projection_updated_at: impl Into<String>) -> Self {
        Self::present(
            customer_id,
            projection_updated_at,
            ProjectionFreshness::Stale,
        )
    }

    /// Build metadata when the source did not provide a projection timestamp.
    pub fn unknown(customer_id: CustomerId) -> Self {
        Self {
            customer_id,
            projection_updated_at: None,
            freshness: ProjectionFreshness::Unknown,
            read_only: true,
        }
    }

    /// Build present metadata with an explicit freshness classification.
    pub fn present(
        customer_id: CustomerId,
        projection_updated_at: impl Into<String>,
        freshness: ProjectionFreshness,
    ) -> Self {
        let projection_updated_at = safe_projection_timestamp(&projection_updated_at.into());
        let has_timestamp = projection_updated_at.is_some();
        Self {
            customer_id,
            projection_updated_at,
            freshness: if has_timestamp {
                freshness
            } else {
                ProjectionFreshness::Unknown
            },
            read_only: true,
        }
    }

    /// Preserve the read-only Customer 360 contract without changing freshness metadata.
    ///
    /// The argument remains for source compatibility with shared projection adapters, but
    /// Customer 360 has no client-side mutation surface and cannot become writable.
    pub const fn with_read_only(mut self, _read_only: bool) -> Self {
        self.read_only = true;
        self
    }

    /// Whether the projection is explicitly read-only.
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Whether the projection is outside the freshness budget.
    pub const fn is_stale(&self) -> bool {
        matches!(self.freshness, ProjectionFreshness::Stale)
    }

    /// Whether the projection is within the freshness budget.
    pub const fn is_fresh(&self) -> bool {
        matches!(self.freshness, ProjectionFreshness::Fresh)
    }

    /// Whether this metadata belongs to the requested customer route scope.
    pub fn is_bound_to_customer(&self, customer_id: &CustomerId) -> bool {
        self.customer_id.as_str() == customer_id.as_str()
    }
}

/// One untrusted field handed to the display boundary by a consuming application.
///
/// This type deliberately does not expose its value. The value is inspected only long enough to
/// either copy an allow-listed, non-secret value into `Customer360DisplayModel` or discard it.
pub struct Customer360FieldInput<'a> {
    key: &'a str,
    value: &'a str,
}

impl<'a> Customer360FieldInput<'a> {
    /// Create a field candidate from a projection DTO value.
    pub const fn new(key: &'a str, value: &'a str) -> Self {
        Self { key, value }
    }
}

impl fmt::Debug for Customer360FieldInput<'_> {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("Customer360FieldInput")
            .field("key", &self.key)
            .field("value", &"REDACTED")
            .finish()
    }
}

/// A field safe for Customer 360 display.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer360DisplayField {
    key: String,
    value: String,
}

impl Customer360DisplayField {
    const MAX_KEY_BYTES: usize = 64;
    const MAX_VALUE_BYTES: usize = 512;

    /// Copy an input field only when its name and value are safe for display.
    pub fn from_untrusted(input: Customer360FieldInput<'_>) -> Option<Self> {
        let key = input.key.trim();
        let value = input.value.trim();
        if input.key.chars().any(char::is_control)
            || input.value.chars().any(char::is_control)
            || key.len() > Self::MAX_KEY_BYTES
            || key.chars().any(char::is_control)
            || !is_allowed_display_key(key)
            || value.is_empty()
            || value.len() > Self::MAX_VALUE_BYTES
            || value.chars().any(char::is_control)
            || contains_secret_marker(value)
        {
            return None;
        }
        Some(Self {
            key: key.to_string(),
            value: value.to_string(),
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

/// Safe, section-oriented display data for a Customer 360 page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Customer360DisplayModel {
    /// The Customer 360 sub-page this model renders.
    pub section: Customer360Section,
    /// Shared projection metadata for the page.
    pub metadata: Customer360ProjectionMetadata,
    fields: Vec<Customer360DisplayField>,
    redacted_field_count: usize,
}

/// One note candidate received from an authorized Customer 360 detail read.
///
/// The consuming application must validate the transport cursor before constructing the page;
/// this model keeps the cursor opaque and never owns continuation behavior.
pub struct CustomerNoteDtoInput {
    id: String,
    author_ref: String,
    created_at: String,
    body: String,
}

impl CustomerNoteDtoInput {
    /// Create a note candidate from a transport response.
    pub fn new(
        id: impl Into<String>,
        author_ref: impl Into<String>,
        created_at: impl Into<String>,
        body: impl Into<String>,
    ) -> Self {
        Self {
            id: id.into(),
            author_ref: author_ref.into(),
            created_at: created_at.into(),
            body: body.into(),
        }
    }
}

impl fmt::Debug for CustomerNoteDtoInput {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("CustomerNoteDtoInput")
            .field("id", &self.id)
            .field("author_ref", &self.author_ref)
            .field("created_at", &self.created_at)
            .field("body", &"REDACTED")
            .finish()
    }
}

/// A note safe for display. Bodies remain bounded and secret-bearing values are discarded at the
/// client boundary even though the server already redacts the authoritative read model.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerNoteDisplayItem {
    id: String,
    author_ref: String,
    created_at: String,
    body: String,
}

impl CustomerNoteDisplayItem {
    const MAX_ID_BYTES: usize = 128;
    const MAX_AUTHOR_BYTES: usize = 256;
    const MAX_TIMESTAMP_BYTES: usize = 64;
    const MAX_BODY_BYTES: usize = 16 * 1024;

    fn from_untrusted(input: CustomerNoteDtoInput) -> Option<Self> {
        let id = safe_note_scalar(&input.id, Self::MAX_ID_BYTES)?;
        let author_ref = safe_note_scalar(&input.author_ref, Self::MAX_AUTHOR_BYTES)?;
        let created_at = safe_note_scalar(&input.created_at, Self::MAX_TIMESTAMP_BYTES)?;
        let body = input.body.trim();
        if body.is_empty()
            || body.len() > Self::MAX_BODY_BYTES
            || body.chars().any(char::is_control)
            || contains_secret_marker(body)
        {
            return None;
        }
        Some(Self {
            id,
            author_ref,
            created_at,
            body: body.to_owned(),
        })
    }

    /// Opaque stable note identifier.
    pub fn id(&self) -> &str {
        &self.id
    }

    /// Redacted author reference.
    pub fn author_ref(&self) -> &str {
        &self.author_ref
    }

    /// Server-provided creation timestamp.
    pub fn created_at(&self) -> &str {
        &self.created_at
    }

    /// Safe note body.
    pub fn body(&self) -> &str {
        &self.body
    }
}

/// Bounded, display-safe Customer 360 notes page.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerNotesDisplayModel {
    notes: Vec<CustomerNoteDisplayItem>,
    next_before_occurred_at: Option<String>,
    next_before_id: Option<String>,
    dropped_record_count: usize,
}

impl CustomerNotesDisplayModel {
    const MAX_CURSOR_BYTES: usize = 128;
    const MAX_PAGE_SIZE: usize = 100;

    /// Build a page from untrusted note DTOs and an already validated continuation cursor.
    ///
    /// Cursor values are revalidated at this display boundary as defense in depth. A cursor is
    /// retained only when both parts of the keyset pair are present, bounded, control-free, and
    /// free of secret markers; invalid or partial continuation data fails closed by ending the
    /// page rather than exposing an unsafe continuation to the consuming client.
    pub fn from_safe_dtos(
        inputs: impl IntoIterator<Item = CustomerNoteDtoInput>,
        next_before_occurred_at: Option<String>,
        next_before_id: Option<String>,
    ) -> Self {
        let mut notes = Vec::new();
        let mut dropped_record_count = 0;
        for input in inputs {
            if notes.len() >= Self::MAX_PAGE_SIZE {
                dropped_record_count += 1;
                continue;
            }
            match CustomerNoteDisplayItem::from_untrusted(input) {
                Some(note) => notes.push(note),
                None => dropped_record_count += 1,
            }
        }
        let (next_before_occurred_at, next_before_id) =
            safe_cursor_pair(next_before_occurred_at, next_before_id);
        Self {
            notes,
            next_before_occurred_at,
            next_before_id,
            dropped_record_count,
        }
    }

    /// Safe notes in server order.
    pub fn notes(&self) -> &[CustomerNoteDisplayItem] {
        &self.notes
    }

    /// Continuation timestamp, if another page exists.
    pub fn next_before_occurred_at(&self) -> Option<&str> {
        self.next_before_occurred_at.as_deref()
    }

    /// Continuation note identifier, if another page exists.
    pub fn next_before_id(&self) -> Option<&str> {
        self.next_before_id.as_deref()
    }

    /// Number of invalid or secret-bearing notes omitted at the display boundary.
    pub const fn dropped_record_count(&self) -> usize {
        self.dropped_record_count
    }
}

impl Customer360DisplayModel {
    const MAX_FIELDS: usize = 64;

    /// Create an empty display model for a section.
    pub fn new(section: Customer360Section, metadata: Customer360ProjectionMetadata) -> Self {
        Self {
            section,
            metadata,
            fields: Vec::new(),
            redacted_field_count: 0,
        }
    }

    /// Create a model from projection DTO field candidates.
    pub fn from_untrusted_fields<'a>(
        section: Customer360Section,
        metadata: Customer360ProjectionMetadata,
        inputs: impl IntoIterator<Item = Customer360FieldInput<'a>>,
    ) -> Self {
        let mut model = Self::new(section, metadata);
        for (index, input) in inputs.into_iter().enumerate() {
            if index >= Self::MAX_FIELDS {
                model.redacted_field_count += 1;
                continue;
            }
            model.push_untrusted(input);
        }
        model
    }

    /// Try to append one field. Returns `true` only when it is retained for display.
    pub fn push_untrusted(&mut self, input: Customer360FieldInput) -> bool {
        match Customer360DisplayField::from_untrusted(input) {
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
    pub fn fields(&self) -> &[Customer360DisplayField] {
        &self.fields
    }

    /// Number of input fields omitted at the redaction boundary.
    pub const fn redacted_field_count(&self) -> usize {
        self.redacted_field_count
    }

    /// Return the label for this section in the requested locale.
    pub const fn section_label(&self, locale: Locale) -> &'static str {
        self.section.label(locale)
    }

    /// Whether this display model remains bound to the requested customer route scope.
    pub fn is_scope_bound_to(&self, customer_id: &CustomerId) -> bool {
        self.metadata.is_bound_to_customer(customer_id)
    }
}

/// Explicit Customer 360 state machine for read-model display.
///
/// Cached data is retained for stale, read-only, conflict, and offline states where it is safe
/// to do so. Denied and error states carry no customer display payload by construction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Customer360ViewState<T> {
    /// A read is in progress.
    Loading {
        /// Metadata from a previously displayed projection, if available.
        metadata: Option<Customer360ProjectionMetadata>,
    },
    /// The read completed successfully with no display rows/content.
    Empty {
        /// Metadata accompanying the empty result.
        metadata: Customer360ProjectionMetadata,
    },
    /// A current display model is available.
    Ready {
        /// Safe display data.
        data: T,
        /// Projection metadata.
        metadata: Customer360ProjectionMetadata,
    },
    /// The principal cannot view this section.
    Denied {
        /// Metadata is optional because policy denial may happen before a read.
        metadata: Option<Customer360ProjectionMetadata>,
        /// Safe policy/message text only.
        reason: String,
    },
    /// Cached data is available but outside the freshness budget.
    Stale {
        /// Last safe display model.
        data: T,
        /// Stale projection metadata.
        metadata: Customer360ProjectionMetadata,
        /// Age supplied by the read boundary.
        age_seconds: u64,
        /// Safe freshness explanation.
        reason: String,
    },
    /// Data is available but writes are disabled by the current context.
    ReadOnly {
        /// Last safe display model.
        data: T,
        /// Projection metadata.
        metadata: Customer360ProjectionMetadata,
        /// Safe read-only explanation.
        reason: String,
    },
    /// A concurrent write or revision race needs review.
    Conflict {
        /// Last safe display model, when one exists.
        data: Option<T>,
        /// Projection metadata, when one exists.
        metadata: Option<Customer360ProjectionMetadata>,
        /// Safe conflict explanation.
        reason: String,
    },
    /// The backend is unavailable; cached display data may still be shown.
    Offline {
        /// Cached safe display model, when one exists.
        data: Option<T>,
        /// Cached projection metadata, when one exists.
        metadata: Option<Customer360ProjectionMetadata>,
        /// Safe connectivity explanation.
        reason: String,
    },
    /// An unexpected read failure occurred.
    Error {
        /// Safe error explanation.
        reason: String,
    },
}

impl<T> Customer360ViewState<T> {
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

    /// Return the safe cached/current display data, if the state has one.
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

    /// Return projection metadata, if this state has any.
    pub fn metadata(&self) -> Option<&Customer360ProjectionMetadata> {
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
        matches!(self.kind(), Some(UiStateKind::ReadOnly))
            || self
                .metadata()
                .is_some_and(Customer360ProjectionMetadata::is_read_only)
    }
}

impl Customer360ViewState<Customer360DisplayModel> {
    /// Fail closed when cached/current payload metadata is absent or belongs to another customer.
    ///
    /// States without display data are safe when no metadata is available because they expose no
    /// customer payload. Offline and conflict states with cached data require both state metadata
    /// and model metadata to bind to the requested customer.
    pub fn is_scope_bound_to(&self, customer_id: &CustomerId) -> bool {
        match self {
            Self::Loading { metadata } | Self::Denied { metadata, .. } => metadata
                .as_ref()
                .is_none_or(|metadata| metadata.is_bound_to_customer(customer_id)),
            Self::Empty { metadata } => metadata.is_bound_to_customer(customer_id),
            Self::Ready { data, metadata }
            | Self::Stale { data, metadata, .. }
            | Self::ReadOnly { data, metadata, .. } => {
                metadata.is_bound_to_customer(customer_id) && data.is_scope_bound_to(customer_id)
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
            } => metadata.is_bound_to_customer(customer_id) && data.is_scope_bound_to(customer_id),
            Self::Conflict {
                data: None,
                metadata: None,
                ..
            }
            | Self::Offline {
                data: None,
                metadata: None,
                ..
            }
            | Self::Error { .. } => true,
            Self::Conflict { .. } | Self::Offline { .. } => false,
        }
    }
}

/// The concrete state type for safe Customer 360 display models.
pub type Customer360State = Customer360ViewState<Customer360DisplayModel>;

fn is_allowed_display_key(key: &str) -> bool {
    let key = key.trim().to_ascii_lowercase().replace('-', "_");
    matches!(
        key.as_str(),
        "account_code"
            | "trading_name"
            | "legal_name"
            | "industry"
            | "country"
            | "locale"
            | "account_state"
            | "subscription_state"
            | "renewal_at"
            | "trial_ends_at"
            | "lifetime_value_minor"
            | "holding_count"
            | "company_count"
            | "branch_count"
            | "active_user_count"
            | "expiring_role_count"
            | "open_ticket_count"
            | "module_count"
            | "health_state"
            | "unit_name"
            | "unit_kind"
            | "unit_active"
            | "contact_display_name"
            | "contact_email"
            | "contact_phone"
            | "contact_responsibilities"
            | "customer_success_owner_name"
            | "payment_state"
            | "invoice_ref"
            | "invoice_state"
            | "invoice_amount_minor"
            | "invoice_currency"
            | "quote_ref"
            | "quote_state"
            | "discount"
            | "module_key"
            | "purchased_quantity"
            | "allocated_quantity"
            | "allocation_active"
            | "onboarding_state"
            | "imports_completed"
            | "training_completed"
            | "go_live_at"
            | "onboarding_owner_name"
            | "database_schema_version"
            | "projection_lag_seconds"
            | "worker_failures"
            | "storage_status"
            | "backup_status"
            | "point_in_time_recovery_status"
            | "integration_failure_count"
            | "api_usage_count"
            | "last_activity_at"
            | "read_count"
            | "last_access_at"
            | "customer_visible"
            | "audit_actor_ref"
            | "audit_module"
            | "scope_label"
            | "audit_purpose"
            | "occurred_at"
            | "note_author_ref"
            | "note_created_at"
            | "note_body"
    )
}

fn safe_projection_timestamp(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > Customer360ProjectionMetadata::MAX_TIMESTAMP_BYTES
        || value.chars().any(char::is_control)
    {
        None
    } else {
        Some(value.to_owned())
    }
}

fn safe_note_scalar(value: &str, max_bytes: usize) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > max_bytes
        || value.chars().any(char::is_control)
        || contains_secret_marker(value)
    {
        None
    } else {
        Some(value.to_owned())
    }
}

fn contains_secret_marker(value: &str) -> bool {
    let value = value.to_ascii_lowercase();
    [
        "access_token=",
        "refresh_token=",
        "client_secret=",
        "password=",
        "authorization: bearer ",
        "bearer ",
        "-----begin ",
        "_secret",
        "api_key=",
        "private_key=",
        "signed_url=",
        "presigned_url=",
        "sk_live_",
        "rk_live_",
    ]
    .iter()
    .any(|marker| value.contains(marker))
}

fn safe_cursor_pair(
    occurred_at: Option<String>,
    id: Option<String>,
) -> (Option<String>, Option<String>) {
    match (occurred_at.as_deref(), id.as_deref()) {
        (Some(occurred_at), Some(id)) => {
            match (safe_cursor_value(occurred_at), safe_cursor_value(id)) {
                (Some(occurred_at), Some(id)) => (Some(occurred_at), Some(id)),
                _ => (None, None),
            }
        }
        _ => (None, None),
    }
}

fn safe_cursor_value(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > CustomerNotesDisplayModel::MAX_CURSOR_BYTES
        || value.chars().any(char::is_control)
        || contains_secret_marker(value)
    {
        None
    } else {
        Some(value.to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn customer_id() -> CustomerId {
        CustomerId::new("cus_123").expect("valid customer ID")
    }

    fn metadata() -> Customer360ProjectionMetadata {
        Customer360ProjectionMetadata::fresh(customer_id(), "2026-07-24T00:00:00Z")
    }

    #[test]
    fn every_customer_360_route_has_a_typed_section_and_bilingual_label() {
        assert_eq!(Customer360Section::all().len(), 10);
        for section in Customer360Section::all() {
            assert!(
                Customer360Section::all()
                    .iter()
                    .any(|candidate| candidate.route_id() == section.route_id())
            );
            assert!(!section.label(Locale::En).is_empty());
            assert!(!section.label(Locale::Th).is_empty());
            assert_ne!(section.label(Locale::En), section.label(Locale::Th));
        }
        assert_eq!(
            Customer360Section::Health.route_view(),
            CustomerView::Health
        );
        assert_eq!(Customer360Section::Notes.route_id(), RouteId::CustomerNotes);
    }

    #[test]
    fn secret_bearing_fields_are_omitted_and_input_debug_is_redacted() {
        let input = Customer360FieldInput::new("access_token", "access-token-value");
        assert!(!format!("{input:?}").contains("access-token-value"));

        let model = Customer360DisplayModel::from_untrusted_fields(
            Customer360Section::Commercial,
            metadata(),
            [
                Customer360FieldInput::new("trading_name", "Acme Co."),
                Customer360FieldInput::new("stripe_customer_ref", "cus_provider_secret"),
                Customer360FieldInput::new("access_token", "access-token-value"),
                Customer360FieldInput::new("note_body", "Authorization: Bearer hidden-token"),
            ],
        );

        assert_eq!(model.fields().len(), 1);
        assert_eq!(model.fields()[0].key(), "trading_name");
        assert_eq!(model.redacted_field_count(), 3);
        assert!(!format!("{model:?}").contains("access-token-value"));
        assert!(!format!("{model:?}").contains("cus_provider_secret"));
    }

    #[test]
    fn display_boundary_bounds_fields_and_projection_metadata() {
        let oversized = "x".repeat(Customer360DisplayField::MAX_VALUE_BYTES + 1);
        let model = Customer360DisplayModel::from_untrusted_fields(
            Customer360Section::Overview,
            Customer360ProjectionMetadata::fresh(customer_id(), oversized.clone()),
            [
                Customer360FieldInput::new("legal_name\n", "safe"),
                Customer360FieldInput::new("legal_name", &oversized),
                Customer360FieldInput::new("legal_name", "safe\nvalue"),
            ],
        );
        assert!(model.fields().is_empty());
        assert_eq!(model.redacted_field_count(), 3);
        assert_eq!(model.metadata.freshness, ProjectionFreshness::Unknown);
        assert_eq!(model.metadata.projection_updated_at, None);
    }

    #[test]
    fn notes_boundary_rejects_control_metadata_and_caps_page_size() {
        let mut inputs = vec![CustomerNoteDtoInput::new(
            "note\n1",
            "agent-1",
            "2026-07-28T00:00:00Z",
            "safe",
        )];
        inputs.extend((0..=CustomerNotesDisplayModel::MAX_PAGE_SIZE).map(|index| {
            CustomerNoteDtoInput::new(
                format!("note-{index}"),
                "agent-1",
                "2026-07-28T00:00:00Z",
                "safe",
            )
        }));
        let page = CustomerNotesDisplayModel::from_safe_dtos(inputs, None, None);
        assert_eq!(page.notes().len(), CustomerNotesDisplayModel::MAX_PAGE_SIZE);
        assert_eq!(page.dropped_record_count(), 2);
    }

    #[test]
    fn notes_page_keeps_safe_rows_cursor_and_redacts_invalid_rows() {
        let input = CustomerNoteDtoInput::new(
            "note-1",
            "agent-1",
            "2026-07-28T00:00:00Z",
            "Approved renewal path",
        );
        assert!(!format!("{input:?}").contains("Approved renewal path"));
        let page = CustomerNotesDisplayModel::from_safe_dtos(
            [
                input,
                CustomerNoteDtoInput::new(
                    "note-2",
                    "agent-2",
                    "2026-07-27T00:00:00Z",
                    "Authorization: Bearer secret",
                ),
            ],
            Some("2026-07-27T00:00:00Z".into()),
            Some("note-1".into()),
        );
        assert_eq!(page.notes().len(), 1);
        assert_eq!(page.notes()[0].body(), "Approved renewal path");
        assert_eq!(page.next_before_id(), Some("note-1"));
        assert_eq!(page.dropped_record_count(), 1);
        assert!(!format!("{page:?}").contains("secret"));
    }

    #[test]
    fn notes_page_fails_closed_for_invalid_or_partial_cursor_pairs() {
        let note = || {
            CustomerNoteDtoInput::new(
                "note-1",
                "agent-1",
                "2026-07-28T00:00:00Z",
                "Approved renewal path",
            )
        };

        let partial = CustomerNotesDisplayModel::from_safe_dtos(
            [note()],
            Some("2026-07-27T00:00:00Z".into()),
            None,
        );
        assert_eq!(partial.next_before_occurred_at(), None);
        assert_eq!(partial.next_before_id(), None);

        let oversized = CustomerNotesDisplayModel::from_safe_dtos(
            [note()],
            Some("2026-07-27T00:00:00Z".into()),
            Some("x".repeat(CustomerNotesDisplayModel::MAX_CURSOR_BYTES + 1)),
        );
        assert_eq!(oversized.next_before_occurred_at(), None);
        assert_eq!(oversized.next_before_id(), None);

        let unsafe_cursor = CustomerNotesDisplayModel::from_safe_dtos(
            [note()],
            Some("2026-07-27T00:00:00Z".into()),
            Some("Authorization: Bearer hidden-token".into()),
        );
        assert_eq!(unsafe_cursor.next_before_occurred_at(), None);
        assert_eq!(unsafe_cursor.next_before_id(), None);
    }

    #[test]
    fn state_machine_keeps_required_states_distinct() {
        let display = Customer360DisplayModel::new(Customer360Section::Overview, metadata());
        let states = [
            Customer360State::Loading { metadata: None },
            Customer360State::Empty {
                metadata: metadata(),
            },
            Customer360State::Denied {
                metadata: None,
                reason: "policy".into(),
            },
            Customer360State::Stale {
                data: display.clone(),
                metadata: Customer360ProjectionMetadata::stale(
                    customer_id(),
                    "2026-07-23T00:00:00Z",
                ),
                age_seconds: 301,
                reason: "projection lag".into(),
            },
            Customer360State::ReadOnly {
                data: display.clone(),
                metadata: metadata().with_read_only(true),
                reason: "billing policy".into(),
            },
            Customer360State::Conflict {
                data: Some(display.clone()),
                metadata: Some(metadata()),
                reason: "revision changed".into(),
            },
            Customer360State::Offline {
                data: Some(display.clone()),
                metadata: Some(metadata()),
                reason: "network unavailable".into(),
            },
        ];
        let kinds: Vec<_> = states
            .iter()
            .filter_map(Customer360ViewState::kind)
            .collect();
        assert_eq!(
            kinds,
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
        assert!(matches!(states[2], Customer360State::Denied { .. }));
        assert!(matches!(states[3], Customer360State::Stale { .. }));
        assert!(matches!(states[4], Customer360State::ReadOnly { .. }));
        assert_ne!(states[2].kind(), states[3].kind());
        assert_ne!(states[3].kind(), states[4].kind());
        assert_eq!(
            states[3].data().expect("stale cache").section,
            Customer360Section::Overview
        );
        assert!(states[4].is_read_only());
        assert!(states[5].data().is_some(), "conflict retains cached data");
        assert!(states[6].is_scope_bound_to(&customer_id()));
        assert!(
            Customer360State::Ready {
                data: display,
                metadata: metadata().with_read_only(false),
            }
            .is_read_only()
        );
    }

    #[test]
    fn cached_payloads_fail_closed_without_matching_customer_scope() {
        let display = Customer360DisplayModel::new(Customer360Section::Overview, metadata());
        let other_customer = CustomerId::new("cus_other").expect("valid customer ID");

        let unbound_offline = Customer360State::Offline {
            data: Some(display.clone()),
            metadata: None,
            reason: "network unavailable".into(),
        };
        assert!(!unbound_offline.is_scope_bound_to(&customer_id()));

        let mismatched_offline = Customer360State::Offline {
            data: Some(display.clone()),
            metadata: Some(Customer360ProjectionMetadata::fresh(
                other_customer,
                "2026-07-24T00:00:00Z",
            )),
            reason: "network unavailable".into(),
        };
        assert!(!mismatched_offline.is_scope_bound_to(&customer_id()));

        let bound_offline = Customer360State::Offline {
            data: Some(display),
            metadata: Some(metadata()),
            reason: "network unavailable".into(),
        };
        assert!(bound_offline.is_scope_bound_to(&customer_id()));
    }

    #[test]
    fn state_labels_use_both_locales() {
        let state = Customer360State::Loading { metadata: None };
        assert_eq!(state.state_label(Locale::En), Some("Loading"));
        assert_eq!(state.state_label(Locale::Th), Some("กำลังโหลด"));

        let denied = Customer360State::Denied {
            metadata: None,
            reason: "policy".into(),
        };
        assert_eq!(denied.state_label(Locale::En), Some("Access denied"));
        assert_eq!(denied.state_label(Locale::Th), Some("ไม่มีสิทธิ์เข้าถึง"));
    }
}
