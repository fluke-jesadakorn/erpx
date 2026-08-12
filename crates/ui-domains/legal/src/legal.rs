//! Target-neutral Legal presentation contracts.
//!
//! This module contains only bounded opaque identifiers, scalar display models, route metadata,
//! and explicit presentation states. It deliberately has no transport, storage, document bytes,
//! authorization, retention worker, or mutation implementation.

use std::{error::Error, fmt};

use erpx_ui_core::{Locale, NavItem, ProjectionFreshness, UiStateKind, redact_deep_link};

/// Maximum bytes retained by any opaque Legal identifier.
pub const MAX_LEGAL_IDENTIFIER_BYTES: usize = 128;
/// Maximum bytes retained by one Legal display scalar.
pub const MAX_LEGAL_DISPLAY_TEXT_BYTES: usize = 256;

/// A bilingual Legal label for the shared shell.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegalLabel {
    english: &'static str,
    thai: &'static str,
}

impl LegalLabel {
    /// Create a static bilingual label.
    pub const fn new(english: &'static str, thai: &'static str) -> Self {
        Self { english, thai }
    }

    /// Return the label for a supported locale.
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

/// Stable identifiers for the Legal route set.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalRouteId {
    /// Legal landing page.
    Legal,
    /// Contract list and summaries.
    Contracts,
    /// Document list and versions.
    Documents,
    /// Document access-history entries.
    AccessHistory,
    /// Retention policy and review view.
    Retention,
}

/// Target-neutral Legal route descriptor. The descriptor is display metadata only.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegalRouteDescriptor {
    /// Stable route identifier.
    pub id: LegalRouteId,
    /// Canonical application path.
    pub template: &'static str,
    /// Bilingual route label.
    pub label: LegalLabel,
    /// Static icon token interpreted by the consuming target.
    pub icon: &'static str,
}

impl LegalRouteDescriptor {
    /// Return the localized route label.
    pub const fn label_for(self, locale: Locale) -> &'static str {
        self.label.text(locale)
    }

    /// Build a target-neutral navigation item.
    pub fn nav_item(self, locale: Locale) -> NavItem {
        NavItem::new(self.label_for(locale), self.template, self.icon)
    }
}

/// Canonical Legal routes in display order.
pub const LEGAL_ROUTES: &[LegalRouteDescriptor] = &[
    LegalRouteDescriptor {
        id: LegalRouteId::Legal,
        template: "/legal",
        label: LegalLabel::new("Legal", "กฎหมาย"),
        icon: "§",
    },
    LegalRouteDescriptor {
        id: LegalRouteId::Contracts,
        template: "/legal/contracts",
        label: LegalLabel::new("Contracts", "สัญญา"),
        icon: "▤",
    },
    LegalRouteDescriptor {
        id: LegalRouteId::Documents,
        template: "/legal/documents",
        label: LegalLabel::new("Documents", "เอกสาร"),
        icon: "▱",
    },
    LegalRouteDescriptor {
        id: LegalRouteId::AccessHistory,
        template: "/legal/access-history",
        label: LegalLabel::new("Access history", "ประวัติการเข้าถึง"),
        icon: "◷",
    },
    LegalRouteDescriptor {
        id: LegalRouteId::Retention,
        template: "/legal/retention",
        label: LegalLabel::new("Retention", "การเก็บรักษา"),
        icon: "⌛",
    },
];

/// Backwards-readable alias for callers that name the registry directly.
pub const LEGAL_ROUTE_REGISTRY: &[LegalRouteDescriptor] = LEGAL_ROUTES;

/// Cheap handle to the canonical Legal route registry.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LegalRouteRegistry;

impl LegalRouteRegistry {
    /// Construct the canonical registry.
    pub const fn new() -> Self {
        Self
    }

    /// Return all Legal route descriptors in canonical order.
    pub const fn definitions(self) -> &'static [LegalRouteDescriptor] {
        LEGAL_ROUTES
    }

    /// Resolve a browser path after query and fragment redaction.
    pub fn resolve(self, path: &str) -> Result<LegalRoute, LegalRouteParseError> {
        LegalRoute::parse(path)
    }

    /// Resolve a path to its stable route identifier.
    pub fn resolve_id(self, path: &str) -> Result<LegalRouteId, LegalRouteParseError> {
        self.resolve(path).map(LegalRoute::id)
    }
}

/// A parsed route from the Legal allow-list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalRoute {
    /// Legal landing page.
    Legal,
    /// Contract list and summaries.
    Contracts,
    /// Document list and versions.
    Documents,
    /// Document access-history entries.
    AccessHistory,
    /// Retention policy and review view.
    Retention,
}

impl LegalRoute {
    /// Parse an allow-listed route without retaining user-supplied query or fragment data.
    pub fn parse(path: &str) -> Result<Self, LegalRouteParseError> {
        match redact_deep_link(path).as_str() {
            "/legal" => Ok(Self::Legal),
            "/legal/contracts" => Ok(Self::Contracts),
            "/legal/documents" => Ok(Self::Documents),
            "/legal/access-history" => Ok(Self::AccessHistory),
            "/legal/retention" => Ok(Self::Retention),
            _ => Err(LegalRouteParseError),
        }
    }

    /// Return the stable route identifier.
    pub const fn id(self) -> LegalRouteId {
        match self {
            Self::Legal => LegalRouteId::Legal,
            Self::Contracts => LegalRouteId::Contracts,
            Self::Documents => LegalRouteId::Documents,
            Self::AccessHistory => LegalRouteId::AccessHistory,
            Self::Retention => LegalRouteId::Retention,
        }
    }

    /// Return the canonical route path.
    pub const fn path(self) -> &'static str {
        match self {
            Self::Legal => "/legal",
            Self::Contracts => "/legal/contracts",
            Self::Documents => "/legal/documents",
            Self::AccessHistory => "/legal/access-history",
            Self::Retention => "/legal/retention",
        }
    }
}

impl fmt::Display for LegalRoute {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.path())
    }
}

/// Error returned for a path outside the Legal allow-list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegalRouteParseError;

impl fmt::Display for LegalRouteParseError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str("no ERPX Legal route matches the supplied path")
    }
}

impl Error for LegalRouteParseError {}

/// Error returned for an unsafe opaque identifier.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalIdentifierError {
    /// The identifier is empty.
    Empty,
    /// The identifier exceeds the bounded size.
    TooLong,
    /// The identifier contains whitespace, control, path, or query characters.
    UnsafeCharacters,
    /// The identifier resembles sensitive transport material.
    SensitiveMarker,
}

impl fmt::Display for LegalIdentifierError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Empty => "Legal identifier is empty",
            Self::TooLong => "Legal identifier is too long",
            Self::UnsafeCharacters => "Legal identifier contains unsafe characters",
            Self::SensitiveMarker => "Legal identifier contains a sensitive marker",
        })
    }
}

impl Error for LegalIdentifierError {}

macro_rules! define_legal_identifier {
    ($name:ident) => {
        #[derive(Debug, Clone, PartialEq, Eq, Hash)]
        pub struct $name(String);

        impl $name {
            /// Validate and construct an opaque identifier.
            pub fn new(value: impl Into<String>) -> Result<Self, LegalIdentifierError> {
                let value = value.into();
                validate_identifier(&value)?;
                Ok(Self(value))
            }

            /// Borrow the validated identifier.
            pub fn as_str(&self) -> &str {
                &self.0
            }
        }

        impl TryFrom<&str> for $name {
            type Error = LegalIdentifierError;

            fn try_from(value: &str) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }

        impl TryFrom<String> for $name {
            type Error = LegalIdentifierError;

            fn try_from(value: String) -> Result<Self, Self::Error> {
                Self::new(value)
            }
        }
    };
}

define_legal_identifier!(LegalContractId);
define_legal_identifier!(LegalPartyId);
define_legal_identifier!(LegalApprovalId);
define_legal_identifier!(LegalRetentionId);
define_legal_identifier!(LegalDocumentId);
define_legal_identifier!(LegalDocumentVersionId);
define_legal_identifier!(LegalAccessHistoryEntryId);
define_legal_identifier!(LegalPrincipalId);

/// Error returned when untrusted text cannot cross the scalar display boundary.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalDisplayTextError {
    /// The text is required but empty.
    Required,
    /// The text exceeds the bounded display size.
    TooLong,
    /// The text contains a control character.
    ControlCharacter,
    /// The text appears to be a structured payload.
    StructuredValue,
    /// The text appears to be a URL.
    Url,
    /// The text contains a credential, token, storage, or payment marker.
    SensitiveMarker,
}

impl fmt::Display for LegalDisplayTextError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(match self {
            Self::Required => "Legal display text is required",
            Self::TooLong => "Legal display text is too long",
            Self::ControlCharacter => "Legal display text contains a control character",
            Self::StructuredValue => "Legal display text must be scalar",
            Self::Url => "Legal display text must not contain a URL",
            Self::SensitiveMarker => "Legal display text contains a sensitive marker",
        })
    }
}

impl Error for LegalDisplayTextError {}

/// A bounded scalar accepted by Legal display models and projection metadata.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalScalar(String);

impl LegalScalar {
    /// Validate scalar text before retaining it in a display contract.
    pub fn new(value: impl Into<String>) -> Result<Self, LegalDisplayTextError> {
        let value = value.into();
        validate_scalar(&value)?;
        Ok(Self(value))
    }

    /// Borrow the validated scalar.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

/// Safe projection freshness and mutation metadata for Legal reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalProjectionMetadata {
    projection_updated_at: Option<LegalScalar>,
    freshness: ProjectionFreshness,
    read_only: bool,
}

impl LegalProjectionMetadata {
    /// Build metadata for a fresh projection.
    pub fn fresh(projection_updated_at: impl Into<String>) -> Result<Self, LegalDisplayTextError> {
        Self::present(projection_updated_at, ProjectionFreshness::Fresh)
    }

    /// Build metadata for a stale but displayable projection.
    pub fn stale(projection_updated_at: impl Into<String>) -> Result<Self, LegalDisplayTextError> {
        Self::present(projection_updated_at, ProjectionFreshness::Stale)
    }

    /// Build metadata when no projection timestamp is available.
    pub const fn unknown() -> Self {
        Self {
            projection_updated_at: None,
            freshness: ProjectionFreshness::Unknown,
            read_only: true,
        }
    }

    /// Build metadata with an explicit freshness classification.
    pub fn present(
        projection_updated_at: impl Into<String>,
        freshness: ProjectionFreshness,
    ) -> Result<Self, LegalDisplayTextError> {
        let timestamp = LegalScalar::new(projection_updated_at)?;
        Ok(Self {
            projection_updated_at: Some(timestamp),
            freshness,
            read_only: true,
        })
    }

    /// Return the projection timestamp, when supplied.
    pub fn projection_updated_at(&self) -> Option<&str> {
        self.projection_updated_at.as_ref().map(LegalScalar::as_str)
    }

    /// Return the supplied freshness classification.
    pub const fn freshness(&self) -> ProjectionFreshness {
        self.freshness
    }

    /// Set the explicit read-only flag without changing freshness.
    pub const fn with_read_only(mut self, read_only: bool) -> Self {
        self.read_only = read_only;
        self
    }

    /// Whether the projection is explicitly read-only.
    pub const fn is_read_only(&self) -> bool {
        self.read_only
    }

    /// Whether the projection is outside its freshness budget.
    pub const fn is_stale(&self) -> bool {
        matches!(self.freshness, ProjectionFreshness::Stale)
    }
}

/// Contract lifecycle status shown in Legal lists.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum LegalContractStatus {
    /// Contract is being prepared.
    Draft,
    /// Contract is currently effective.
    Active,
    /// Contract reached its end date.
    Expired,
    /// Contract was ended before its scheduled end date.
    Terminated,
}

/// Party role shown in a contract summary.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum LegalPartyRole {
    /// Owning legal entity.
    Owner,
    /// Other named contract party.
    Counterparty,
    /// Party expected to sign.
    Signatory,
    /// Legal adviser or counsel.
    Counsel,
    /// Witness to a signing event.
    Witness,
    /// A bounded role supplied for display.
    Other,
}

/// Approval lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum LegalApprovalStatus {
    /// Approval is waiting for review.
    Pending,
    /// Approval was accepted.
    Approved,
    /// Approval was declined.
    Rejected,
    /// Approval window ended without acceptance.
    Expired,
}

/// Retention lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum LegalRetentionStatus {
    /// Retention policy is in force.
    Active,
    /// A retention review is due.
    DueForReview,
    /// A hold prevents normal disposition.
    OnHold,
    /// Disposition is recorded by a future boundary.
    Disposed,
}

/// Document lifecycle status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum LegalDocumentStatus {
    /// Document is being prepared.
    Draft,
    /// Document is the current display version.
    Active,
    /// Document is no longer current but remains retained.
    Archived,
    /// Document was replaced by a later version.
    Superseded,
}

/// A safe scalar label for an access-history action.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Ord, PartialOrd)]
pub enum LegalAccessAction {
    /// A document was viewed.
    Viewed,
    /// A version list was displayed.
    Listed,
    /// A document metadata read was attempted.
    MetadataRead,
    /// An access attempt was denied.
    Denied,
}

macro_rules! impl_legal_lifecycle_labels {
    ($type:ty, { $( $variant:path => ($token:literal, $english:literal, $thai:literal) ),+ $(,)? }) => {
        impl $type {
            /// Return a stable scalar token for adapter mapping.
            pub const fn as_str(self) -> &'static str {
                match self {
                    $( $variant => $token, )+
                }
            }

            /// Return a bilingual user-facing label.
            pub const fn label(self, locale: Locale) -> &'static str {
                let label = match self {
                    $( $variant => LegalLabel::new($english, $thai), )+
                };
                label.text(locale)
            }
        }
    };
}

impl_legal_lifecycle_labels!(LegalContractStatus, {
    Self::Draft => ("draft", "Draft", "ร่าง"),
    Self::Active => ("active", "Active", "มีผลบังคับใช้"),
    Self::Expired => ("expired", "Expired", "หมดอายุ"),
    Self::Terminated => ("terminated", "Terminated", "สิ้นสุดก่อนกำหนด"),
});

impl_legal_lifecycle_labels!(LegalPartyRole, {
    Self::Owner => ("owner", "Owner", "เจ้าของ"),
    Self::Counterparty => ("counterparty", "Counterparty", "คู่สัญญา"),
    Self::Signatory => ("signatory", "Signatory", "ผู้ลงนาม"),
    Self::Counsel => ("counsel", "Counsel", "ที่ปรึกษากฎหมาย"),
    Self::Witness => ("witness", "Witness", "พยาน"),
    Self::Other => ("other", "Other", "อื่น ๆ"),
});

impl_legal_lifecycle_labels!(LegalApprovalStatus, {
    Self::Pending => ("pending", "Pending", "รอดำเนินการ"),
    Self::Approved => ("approved", "Approved", "อนุมัติแล้ว"),
    Self::Rejected => ("rejected", "Rejected", "ปฏิเสธ"),
    Self::Expired => ("expired", "Expired", "หมดอายุ"),
});

impl_legal_lifecycle_labels!(LegalRetentionStatus, {
    Self::Active => ("active", "Active", "ใช้งานอยู่"),
    Self::DueForReview => ("due_for_review", "Due for review", "ถึงกำหนดทบทวน"),
    Self::OnHold => ("on_hold", "On hold", "พักไว้"),
    Self::Disposed => ("disposed", "Disposed", "ดำเนินการแล้ว"),
});

impl_legal_lifecycle_labels!(LegalDocumentStatus, {
    Self::Draft => ("draft", "Draft", "ร่าง"),
    Self::Active => ("active", "Active", "ใช้งานอยู่"),
    Self::Archived => ("archived", "Archived", "เก็บถาวร"),
    Self::Superseded => ("superseded", "Superseded", "ถูกแทนที่"),
});

impl_legal_lifecycle_labels!(LegalAccessAction, {
    Self::Viewed => ("viewed", "Viewed", "เปิดดู"),
    Self::Listed => ("listed", "Listed", "แสดงรายการ"),
    Self::MetadataRead => ("metadata_read", "Metadata read", "อ่านข้อมูลกำกับ"),
    Self::Denied => ("denied", "Denied", "ปฏิเสธ"),
});

/// A redacted contract summary containing scalar facts only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalContractDisplayModel {
    contract_id: LegalContractId,
    contract_number: LegalScalar,
    title: LegalScalar,
    status: LegalContractStatus,
    effective_on: LegalScalar,
    expires_on: Option<LegalScalar>,
    party_count: u32,
}

impl LegalContractDisplayModel {
    /// Construct a contract summary from already-authorized scalar facts.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        contract_id: LegalContractId,
        contract_number: impl Into<String>,
        title: impl Into<String>,
        status: LegalContractStatus,
        effective_on: impl Into<String>,
        expires_on: Option<String>,
        party_count: u32,
    ) -> Result<Self, LegalDisplayTextError> {
        Ok(Self {
            contract_id,
            contract_number: LegalScalar::new(contract_number)?,
            title: LegalScalar::new(title)?,
            status,
            effective_on: LegalScalar::new(effective_on)?,
            expires_on: optional_scalar(expires_on)?,
            party_count,
        })
    }

    /// Borrow the opaque contract identifier.
    pub const fn contract_id(&self) -> &LegalContractId {
        &self.contract_id
    }

    /// Borrow the contract reference.
    pub fn contract_number(&self) -> &str {
        self.contract_number.as_str()
    }

    /// Borrow the safe contract title.
    pub fn title(&self) -> &str {
        self.title.as_str()
    }

    /// Return the contract status.
    pub const fn status(&self) -> LegalContractStatus {
        self.status
    }

    /// Borrow the effective date text.
    pub fn effective_on(&self) -> &str {
        self.effective_on.as_str()
    }

    /// Borrow the optional expiry date text.
    pub fn expires_on(&self) -> Option<&str> {
        self.expires_on.as_ref().map(LegalScalar::as_str)
    }

    /// Return the bounded party count.
    pub const fn party_count(&self) -> u32 {
        self.party_count
    }
}

/// A redacted contract-party summary containing scalar facts only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalPartyDisplayModel {
    party_id: LegalPartyId,
    contract_id: LegalContractId,
    display_name: LegalScalar,
    role: LegalPartyRole,
    reference: Option<LegalScalar>,
}

impl LegalPartyDisplayModel {
    /// Construct a party summary from safe display facts.
    pub fn new(
        party_id: LegalPartyId,
        contract_id: LegalContractId,
        display_name: impl Into<String>,
        role: LegalPartyRole,
        reference: Option<String>,
    ) -> Result<Self, LegalDisplayTextError> {
        Ok(Self {
            party_id,
            contract_id,
            display_name: LegalScalar::new(display_name)?,
            role,
            reference: optional_scalar(reference)?,
        })
    }

    /// Borrow the opaque party identifier.
    pub const fn party_id(&self) -> &LegalPartyId {
        &self.party_id
    }

    /// Borrow the related contract identifier.
    pub const fn contract_id(&self) -> &LegalContractId {
        &self.contract_id
    }

    /// Borrow the party display name.
    pub fn display_name(&self) -> &str {
        self.display_name.as_str()
    }

    /// Return the party role.
    pub const fn role(&self) -> LegalPartyRole {
        self.role
    }

    /// Borrow the optional safe party reference.
    pub fn reference(&self) -> Option<&str> {
        self.reference.as_ref().map(LegalScalar::as_str)
    }
}

/// A redacted approval summary containing scalar facts only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalApprovalDisplayModel {
    approval_id: LegalApprovalId,
    contract_id: LegalContractId,
    approver: LegalPrincipalId,
    approver_label: LegalScalar,
    status: LegalApprovalStatus,
    decided_at: Option<LegalScalar>,
    note: Option<LegalScalar>,
}

impl LegalApprovalDisplayModel {
    /// Construct an approval summary from safe display facts.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        approval_id: LegalApprovalId,
        contract_id: LegalContractId,
        approver: LegalPrincipalId,
        approver_label: impl Into<String>,
        status: LegalApprovalStatus,
        decided_at: Option<String>,
        note: Option<String>,
    ) -> Result<Self, LegalDisplayTextError> {
        Ok(Self {
            approval_id,
            contract_id,
            approver,
            approver_label: LegalScalar::new(approver_label)?,
            status,
            decided_at: optional_scalar(decided_at)?,
            note: optional_scalar(note)?,
        })
    }

    /// Borrow the opaque approval identifier.
    pub const fn approval_id(&self) -> &LegalApprovalId {
        &self.approval_id
    }

    /// Borrow the related contract identifier.
    pub const fn contract_id(&self) -> &LegalContractId {
        &self.contract_id
    }

    /// Borrow the opaque approver identifier.
    pub const fn approver(&self) -> &LegalPrincipalId {
        &self.approver
    }

    /// Borrow the safe approver label.
    pub fn approver_label(&self) -> &str {
        self.approver_label.as_str()
    }

    /// Return the approval status.
    pub const fn status(&self) -> LegalApprovalStatus {
        self.status
    }

    /// Borrow the optional decision timestamp.
    pub fn decided_at(&self) -> Option<&str> {
        self.decided_at.as_ref().map(LegalScalar::as_str)
    }

    /// Borrow the optional safe note.
    pub fn note(&self) -> Option<&str> {
        self.note.as_ref().map(LegalScalar::as_str)
    }
}

/// A redacted retention summary containing scalar policy facts only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalRetentionDisplayModel {
    retention_id: LegalRetentionId,
    policy_label: LegalScalar,
    status: LegalRetentionStatus,
    review_on: Option<LegalScalar>,
    retention_days: Option<u32>,
    disposition_label: Option<LegalScalar>,
}

impl LegalRetentionDisplayModel {
    /// Construct a retention summary from safe display facts.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        retention_id: LegalRetentionId,
        policy_label: impl Into<String>,
        status: LegalRetentionStatus,
        review_on: Option<String>,
        retention_days: Option<u32>,
        disposition_label: Option<String>,
    ) -> Result<Self, LegalDisplayTextError> {
        Ok(Self {
            retention_id,
            policy_label: LegalScalar::new(policy_label)?,
            status,
            review_on: optional_scalar(review_on)?,
            retention_days,
            disposition_label: optional_scalar(disposition_label)?,
        })
    }

    /// Borrow the opaque retention identifier.
    pub const fn retention_id(&self) -> &LegalRetentionId {
        &self.retention_id
    }

    /// Borrow the policy label.
    pub fn policy_label(&self) -> &str {
        self.policy_label.as_str()
    }

    /// Return the retention status.
    pub const fn status(&self) -> LegalRetentionStatus {
        self.status
    }

    /// Borrow the optional review date.
    pub fn review_on(&self) -> Option<&str> {
        self.review_on.as_ref().map(LegalScalar::as_str)
    }

    /// Return the optional retention duration in days.
    pub const fn retention_days(&self) -> Option<u32> {
        self.retention_days
    }

    /// Borrow the optional disposition label.
    pub fn disposition_label(&self) -> Option<&str> {
        self.disposition_label.as_ref().map(LegalScalar::as_str)
    }
}

/// A redacted document summary. It contains no document bytes or retrieval target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalDocumentDisplayModel {
    document_id: LegalDocumentId,
    contract_id: LegalContractId,
    title: LegalScalar,
    document_type: LegalScalar,
    status: LegalDocumentStatus,
    current_version_id: Option<LegalDocumentVersionId>,
    retention_id: Option<LegalRetentionId>,
}

impl LegalDocumentDisplayModel {
    /// Construct a document summary from safe scalar facts and opaque references.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        document_id: LegalDocumentId,
        contract_id: LegalContractId,
        title: impl Into<String>,
        document_type: impl Into<String>,
        status: LegalDocumentStatus,
        current_version_id: Option<LegalDocumentVersionId>,
        retention_id: Option<LegalRetentionId>,
    ) -> Result<Self, LegalDisplayTextError> {
        Ok(Self {
            document_id,
            contract_id,
            title: LegalScalar::new(title)?,
            document_type: LegalScalar::new(document_type)?,
            status,
            current_version_id,
            retention_id,
        })
    }

    /// Borrow the opaque document identifier.
    pub const fn document_id(&self) -> &LegalDocumentId {
        &self.document_id
    }

    /// Borrow the related contract identifier.
    pub const fn contract_id(&self) -> &LegalContractId {
        &self.contract_id
    }

    /// Borrow the document title.
    pub fn title(&self) -> &str {
        self.title.as_str()
    }

    /// Borrow the safe document type label.
    pub fn document_type(&self) -> &str {
        self.document_type.as_str()
    }

    /// Return the document status.
    pub const fn status(&self) -> LegalDocumentStatus {
        self.status
    }

    /// Borrow the optional current version identifier.
    pub const fn current_version_id(&self) -> Option<&LegalDocumentVersionId> {
        self.current_version_id.as_ref()
    }

    /// Borrow the optional retention identifier.
    pub const fn retention_id(&self) -> Option<&LegalRetentionId> {
        self.retention_id.as_ref()
    }
}

/// A redacted document-version summary. It contains metadata only, never bytes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalDocumentVersionDisplayModel {
    version_id: LegalDocumentVersionId,
    document_id: LegalDocumentId,
    version_label: LegalScalar,
    created_at: LegalScalar,
    created_by: LegalPrincipalId,
    created_by_label: LegalScalar,
    change_summary: Option<LegalScalar>,
    page_count: Option<u32>,
}

impl LegalDocumentVersionDisplayModel {
    /// Construct a document-version metadata summary from safe scalar facts.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        version_id: LegalDocumentVersionId,
        document_id: LegalDocumentId,
        version_label: impl Into<String>,
        created_at: impl Into<String>,
        created_by: LegalPrincipalId,
        created_by_label: impl Into<String>,
        change_summary: Option<String>,
        page_count: Option<u32>,
    ) -> Result<Self, LegalDisplayTextError> {
        Ok(Self {
            version_id,
            document_id,
            version_label: LegalScalar::new(version_label)?,
            created_at: LegalScalar::new(created_at)?,
            created_by,
            created_by_label: LegalScalar::new(created_by_label)?,
            change_summary: optional_scalar(change_summary)?,
            page_count,
        })
    }

    /// Borrow the opaque version identifier.
    pub const fn version_id(&self) -> &LegalDocumentVersionId {
        &self.version_id
    }

    /// Borrow the related document identifier.
    pub const fn document_id(&self) -> &LegalDocumentId {
        &self.document_id
    }

    /// Borrow the version label.
    pub fn version_label(&self) -> &str {
        self.version_label.as_str()
    }

    /// Borrow the creation timestamp text.
    pub fn created_at(&self) -> &str {
        self.created_at.as_str()
    }

    /// Borrow the opaque creator identifier.
    pub const fn created_by(&self) -> &LegalPrincipalId {
        &self.created_by
    }

    /// Borrow the safe creator label.
    pub fn created_by_label(&self) -> &str {
        self.created_by_label.as_str()
    }

    /// Borrow the optional change summary.
    pub fn change_summary(&self) -> Option<&str> {
        self.change_summary.as_ref().map(LegalScalar::as_str)
    }

    /// Return the optional page count.
    pub const fn page_count(&self) -> Option<u32> {
        self.page_count
    }
}

/// A redacted document-access history entry containing scalar audit facts only.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalAccessHistoryEntryDisplayModel {
    entry_id: LegalAccessHistoryEntryId,
    document_id: LegalDocumentId,
    actor: LegalPrincipalId,
    actor_label: LegalScalar,
    action: LegalAccessAction,
    occurred_at: LegalScalar,
    result_label: LegalScalar,
}

impl LegalAccessHistoryEntryDisplayModel {
    /// Construct an access-history entry from safe scalar facts.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        entry_id: LegalAccessHistoryEntryId,
        document_id: LegalDocumentId,
        actor: LegalPrincipalId,
        actor_label: impl Into<String>,
        action: LegalAccessAction,
        occurred_at: impl Into<String>,
        result_label: impl Into<String>,
    ) -> Result<Self, LegalDisplayTextError> {
        Ok(Self {
            entry_id,
            document_id,
            actor,
            actor_label: LegalScalar::new(actor_label)?,
            action,
            occurred_at: LegalScalar::new(occurred_at)?,
            result_label: LegalScalar::new(result_label)?,
        })
    }

    /// Borrow the opaque history-entry identifier.
    pub const fn entry_id(&self) -> &LegalAccessHistoryEntryId {
        &self.entry_id
    }

    /// Borrow the opaque document identifier.
    pub const fn document_id(&self) -> &LegalDocumentId {
        &self.document_id
    }

    /// Borrow the opaque actor identifier.
    pub const fn actor(&self) -> &LegalPrincipalId {
        &self.actor
    }

    /// Borrow the safe actor label.
    pub fn actor_label(&self) -> &str {
        self.actor_label.as_str()
    }

    /// Return the access action.
    pub const fn action(&self) -> LegalAccessAction {
        self.action
    }

    /// Borrow the event timestamp text.
    pub fn occurred_at(&self) -> &str {
        self.occurred_at.as_str()
    }

    /// Borrow the safe result label.
    pub fn result_label(&self) -> &str {
        self.result_label.as_str()
    }
}

/// Explicit Legal page-state kinds, including the successful ready state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LegalPageStateKind {
    /// A read is in progress.
    Loading,
    /// The read completed without display rows/content.
    Empty,
    /// Safe display data is available.
    Ready,
    /// The presentation boundary cannot show the content.
    Denied,
    /// Cached data is outside its freshness budget.
    Stale,
    /// The presentation surface is explicitly non-mutating.
    ReadOnly,
    /// A revision boundary requires review.
    Conflict,
    /// The backend is unavailable.
    Offline,
    /// A safe generic failure state.
    Error,
}

impl LegalPageStateKind {
    /// Map to the existing shared state kind for shell rendering.
    pub const fn shared_kind(self) -> Option<UiStateKind> {
        match self {
            Self::Ready => None,
            Self::Loading => Some(UiStateKind::Loading),
            Self::Empty => Some(UiStateKind::Empty),
            Self::Denied => Some(UiStateKind::Denied),
            Self::Stale => Some(UiStateKind::Stale),
            Self::ReadOnly => Some(UiStateKind::ReadOnly),
            Self::Conflict => Some(UiStateKind::Conflict),
            Self::Offline => Some(UiStateKind::Offline),
            Self::Error => Some(UiStateKind::Error),
        }
    }

    /// Return a bilingual state label.
    pub const fn label(self, locale: Locale) -> Option<&'static str> {
        let label = match self {
            Self::Loading => LegalLabel::new("Loading", "กำลังโหลด"),
            Self::Empty => LegalLabel::new("Nothing to show", "ไม่มีข้อมูลที่แสดง"),
            Self::Ready => return None,
            Self::Denied => LegalLabel::new("Access unavailable", "ไม่สามารถเข้าถึงได้"),
            Self::Stale => LegalLabel::new("Data may be stale", "ข้อมูลอาจล้าสมัย"),
            Self::ReadOnly => LegalLabel::new("Read-only mode", "โหมดอ่านอย่างเดียว"),
            Self::Conflict => LegalLabel::new("Review required", "ต้องตรวจสอบ"),
            Self::Offline => LegalLabel::new("Offline", "ออฟไลน์"),
            Self::Error => LegalLabel::new("Unable to show Legal data", "ไม่สามารถแสดงข้อมูลกฎหมายได้"),
        };
        Some(label.text(locale))
    }

    /// Return safe user-facing explanatory copy for the state.
    pub const fn message(self, locale: Locale) -> Option<&'static str> {
        let message = match self {
            Self::Loading => LegalLabel::new("Legal data is loading.", "กำลังโหลดข้อมูลกฎหมาย"),
            Self::Empty => {
                LegalLabel::new("There is no Legal data to display.", "ไม่มีข้อมูลกฎหมายที่จะแสดง")
            }
            Self::Ready => return None,
            Self::Denied => LegalLabel::new(
                "This Legal content is not available in the current context.",
                "เนื้อหากฎหมายนี้ไม่พร้อมใช้งานในบริบทปัจจุบัน",
            ),
            Self::Stale => LegalLabel::new(
                "Showing saved Legal data; it may need refresh.",
                "กำลังแสดงข้อมูลกฎหมายที่บันทึกไว้ ซึ่งอาจต้องรีเฟรช",
            ),
            Self::ReadOnly => LegalLabel::new(
                "This Legal view is read-only.",
                "มุมมองกฎหมายนี้เป็นแบบอ่านอย่างเดียว",
            ),
            Self::Conflict => LegalLabel::new(
                "The Legal data changed elsewhere; review is required.",
                "ข้อมูลกฎหมายเปลี่ยนแปลงที่อื่น จึงต้องตรวจสอบ",
            ),
            Self::Offline => LegalLabel::new(
                "The Legal service is unavailable; try again later.",
                "บริการกฎหมายไม่พร้อมใช้งาน โปรดลองอีกครั้งภายหลัง",
            ),
            Self::Error => LegalLabel::new(
                "Legal data could not be shown. Try again later.",
                "ไม่สามารถแสดงข้อมูลกฎหมายได้ โปรดลองอีกครั้งภายหลัง",
            ),
        };
        Some(message.text(locale))
    }
}

/// Explicit Legal state for future target adapters.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LegalPageState<T> {
    /// A read is in progress.
    Loading {
        /// Previously known metadata, if any.
        metadata: Option<LegalProjectionMetadata>,
    },
    /// The read completed with no display rows/content.
    Empty {
        /// Metadata accompanying the empty result.
        metadata: LegalProjectionMetadata,
    },
    /// Current safe display data is available.
    Ready {
        /// Redacted display payload.
        data: T,
        /// Projection freshness and read-only metadata.
        metadata: LegalProjectionMetadata,
    },
    /// The current presentation context cannot show this content.
    Denied {
        /// Metadata may be absent when denial precedes a read.
        metadata: Option<LegalProjectionMetadata>,
    },
    /// Cached safe display data is outside the freshness budget.
    Stale {
        /// Last safe display payload.
        data: T,
        /// Stale projection metadata.
        metadata: LegalProjectionMetadata,
    },
    /// Safe data is available but the view is explicitly non-mutating.
    ReadOnly {
        /// Last safe display payload.
        data: T,
        /// Projection freshness and read-only metadata.
        metadata: LegalProjectionMetadata,
    },
    /// A revision boundary needs review.
    Conflict {
        /// Last safe display payload, when available.
        data: Option<T>,
        /// Metadata, when available.
        metadata: Option<LegalProjectionMetadata>,
    },
    /// The backend is unavailable; cached safe data may remain visible.
    Offline {
        /// Cached safe display payload, when available.
        data: Option<T>,
        /// Cached metadata, when available.
        metadata: Option<LegalProjectionMetadata>,
    },
    /// A safe generic failure state.
    Error,
}

impl<T> LegalPageState<T> {
    /// Return the explicit Legal state kind.
    pub const fn state_kind(&self) -> LegalPageStateKind {
        match self {
            Self::Loading { .. } => LegalPageStateKind::Loading,
            Self::Empty { .. } => LegalPageStateKind::Empty,
            Self::Ready { .. } => LegalPageStateKind::Ready,
            Self::Denied { .. } => LegalPageStateKind::Denied,
            Self::Stale { .. } => LegalPageStateKind::Stale,
            Self::ReadOnly { .. } => LegalPageStateKind::ReadOnly,
            Self::Conflict { .. } => LegalPageStateKind::Conflict,
            Self::Offline { .. } => LegalPageStateKind::Offline,
            Self::Error => LegalPageStateKind::Error,
        }
    }

    /// Return the shared state kind for a non-ready state.
    pub const fn kind(&self) -> Option<UiStateKind> {
        self.state_kind().shared_kind()
    }

    /// Return localized safe state-label copy.
    pub const fn state_label(&self, locale: Locale) -> Option<&'static str> {
        self.state_kind().label(locale)
    }

    /// Return localized safe explanatory copy.
    pub const fn state_message(&self, locale: Locale) -> Option<&'static str> {
        self.state_kind().message(locale)
    }

    /// Return current or cached safe display data, when available.
    pub fn data(&self) -> Option<&T> {
        match self {
            Self::Ready { data, .. } | Self::Stale { data, .. } | Self::ReadOnly { data, .. } => {
                Some(data)
            }
            Self::Conflict { data, .. } | Self::Offline { data, .. } => data.as_ref(),
            Self::Loading { .. } | Self::Empty { .. } | Self::Denied { .. } | Self::Error => None,
        }
    }

    /// Return projection metadata, when available.
    pub fn metadata(&self) -> Option<&LegalProjectionMetadata> {
        match self {
            Self::Loading { metadata }
            | Self::Denied { metadata }
            | Self::Conflict { metadata, .. }
            | Self::Offline { metadata, .. } => metadata.as_ref(),
            Self::Empty { metadata }
            | Self::Ready { metadata, .. }
            | Self::Stale { metadata, .. }
            | Self::ReadOnly { metadata, .. } => Some(metadata),
            Self::Error => None,
        }
    }

    /// Whether this contract explicitly disables mutations.
    pub fn is_read_only(&self) -> bool {
        matches!(self.state_kind(), LegalPageStateKind::ReadOnly)
            || self
                .metadata()
                .is_some_and(LegalProjectionMetadata::is_read_only)
    }

    /// This presentation contract never exposes a mutation surface.
    pub const fn can_mutate(&self) -> bool {
        false
    }
}

/// Concrete state for contract summaries.
pub type LegalContractState = LegalPageState<LegalContractDisplayModel>;
/// Concrete state for party summaries.
pub type LegalPartyState = LegalPageState<LegalPartyDisplayModel>;
/// Concrete state for approval summaries.
pub type LegalApprovalState = LegalPageState<LegalApprovalDisplayModel>;
/// Concrete state for retention summaries.
pub type LegalRetentionState = LegalPageState<LegalRetentionDisplayModel>;
/// Concrete state for document summaries.
pub type LegalDocumentState = LegalPageState<LegalDocumentDisplayModel>;
/// Concrete state for document-version summaries.
pub type LegalDocumentVersionState = LegalPageState<LegalDocumentVersionDisplayModel>;
/// Concrete state for access-history entries.
pub type LegalAccessHistoryEntryState = LegalPageState<LegalAccessHistoryEntryDisplayModel>;

fn validate_identifier(value: &str) -> Result<(), LegalIdentifierError> {
    if value.is_empty() {
        return Err(LegalIdentifierError::Empty);
    }
    if value.len() > MAX_LEGAL_IDENTIFIER_BYTES {
        return Err(LegalIdentifierError::TooLong);
    }
    if value == "."
        || value == ".."
        || value.chars().any(|character| {
            character.is_control()
                || character.is_whitespace()
                || matches!(character, '/' | '\\' | '?' | '#')
        })
    {
        return Err(LegalIdentifierError::UnsafeCharacters);
    }
    if has_sensitive_marker(value) {
        return Err(LegalIdentifierError::SensitiveMarker);
    }
    Ok(())
}

fn validate_scalar(value: &str) -> Result<(), LegalDisplayTextError> {
    if value.trim().is_empty() {
        return Err(LegalDisplayTextError::Required);
    }
    if value.len() > MAX_LEGAL_DISPLAY_TEXT_BYTES {
        return Err(LegalDisplayTextError::TooLong);
    }
    if value.chars().any(char::is_control) {
        return Err(LegalDisplayTextError::ControlCharacter);
    }
    if value.contains(['{', '}', '[', ']']) {
        return Err(LegalDisplayTextError::StructuredValue);
    }
    if value.contains("://") || value.to_ascii_lowercase().contains("www.") {
        return Err(LegalDisplayTextError::Url);
    }
    if has_sensitive_marker(value) {
        return Err(LegalDisplayTextError::SensitiveMarker);
    }
    Ok(())
}

fn has_sensitive_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "access_token",
        "refresh_token",
        "id_token",
        "authorization",
        "bearer ",
        "client_secret",
        "password",
        "credential",
        "signed_url",
        "presigned_url",
        "api_key",
        "private_key",
        "storage_key",
        "payment",
        "card_number",
        "cvv",
        "bank_account",
        "account_number",
        "raw_bytes",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn optional_scalar(value: Option<String>) -> Result<Option<LegalScalar>, LegalDisplayTextError> {
    match value {
        None => Ok(None),
        Some(value) if value.trim().is_empty() => Ok(None),
        Some(value) => LegalScalar::new(value).map(Some),
    }
}
