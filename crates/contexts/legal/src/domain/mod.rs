use chrono::{DateTime, Duration, Utc};
use erpx_integration_contracts::RecordRef;
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_REFERENCE_BYTES: usize = 255;
pub const MAX_TITLE_BYTES: usize = 255;
pub const MAX_PURPOSE_BYTES: usize = 512;
pub const MAX_PARTIES: usize = 128;
pub const MAX_DOCUMENT_VERSIONS: usize = 1_024;
pub const MAX_RETENTION_DAYS: i64 = 36_525;
pub const MAX_ACCESS_CLOCK_SKEW_MINUTES: i64 = 5;
pub const MAX_MATTER_LINKS: usize = 1_024;

macro_rules! context_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, LegalDomainError> {
                if value.is_nil() {
                    return Err(LegalDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = LegalDomainError;

            fn try_from(value: Uuid) -> Result<Self, Self::Error> {
                Self::try_from_uuid(value)
            }
        }

        impl From<$name> for Uuid {
            fn from(value: $name) -> Self {
                value.0
            }
        }
    };
}

context_id!(LegalContractId, "contract_id");
context_id!(LegalDocumentId, "document_id");
context_id!(AccessEntryId, "access_entry_id");
context_id!(LegalMatterId, "matter_id");

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum LegalDomainError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {maximum} bytes")]
    TooLong { field: &'static str, maximum: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} contains forbidden secret material")]
    SensitiveMaterial { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("Legal records require an exact company or branch scope")]
    InvalidScope,
    #[error("the Legal lifecycle transition is not allowed")]
    InvalidTransition,
    #[error("the approval actor must be distinct from the contract creator")]
    ApprovalSeparation,
    #[error("the retention deadline is invalid or exceeds the bounded horizon")]
    InvalidRetention,
    #[error("the retention deadline has not elapsed")]
    RetentionActive,
    #[error("the Legal record is terminal")]
    Terminal,
    #[error("the contract party is duplicated or exceeds the bounded limit")]
    InvalidParty,
    #[error("the document version is invalid or exceeds the bounded limit")]
    InvalidDocumentVersion,
    #[error("the access timestamp exceeds the permitted clock skew")]
    AccessClockSkew,
    #[error("the aggregate version overflowed")]
    VersionOverflow,
    #[error("the aggregate timestamp moved backwards")]
    TimestampRegression,
    #[error("the Legal matter record link is invalid, duplicated, or exceeds the bounded limit")]
    InvalidRecordLink,
    #[error("the linked record belongs to another tenant holding")]
    CrossTenantLink,
    #[error("the Legal hold transition is not allowed")]
    InvalidLegalHold,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractStatus {
    Draft,
    Review,
    Approved,
    Active,
    Expired,
    Terminated,
    Archived,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContractTransition {
    SubmitForReview,
    Approve,
    Activate,
    Expire,
    Terminate,
    Archive,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractParty {
    pub party_id: Uuid,
    pub role: String,
}

impl ContractParty {
    pub fn try_new(party_id: Uuid, role: String) -> Result<Self, LegalDomainError> {
        if party_id.is_nil() {
            return Err(LegalDomainError::NilIdentifier { field: "party_id" });
        }
        validate_text(&role, "party_role", MAX_REFERENCE_BYTES)?;
        Ok(Self { party_id, role })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    ContractChanged,
    DocumentChanged,
    AccessRecorded,
    MatterChanged,
    RecordLinked(RecordLink),
    LegalHoldChanged(LegalHold),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalContract {
    id: LegalContractId,
    version: u64,
    scope: TenantScope,
    reference: String,
    title: String,
    object_ref: String,
    content_sha256: String,
    retention_until: DateTime<Utc>,
    status: ContractStatus,
    parties: Vec<ContractParty>,
    created_by: String,
    approved_by: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl LegalContract {
    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: LegalContractId,
        scope: TenantScope,
        reference: String,
        title: String,
        object_ref: String,
        content_sha256: String,
        retention_until: DateTime<Utc>,
        created_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, LegalDomainError> {
        validate_scope(&scope)?;
        validate_text(&reference, "contract_reference", MAX_REFERENCE_BYTES)?;
        validate_text(&title, "contract_title", MAX_TITLE_BYTES)?;
        validate_object_ref(&object_ref)?;
        validate_digest(&content_sha256)?;
        validate_text(&created_by, "created_by", MAX_REFERENCE_BYTES)?;
        validate_retention(retention_until, at)?;
        let value = Self {
            id,
            version: 1,
            scope,
            reference,
            title,
            object_ref,
            content_sha256,
            retention_until,
            status: ContractStatus::Draft,
            parties: Vec::new(),
            created_by,
            approved_by: None,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::ContractChanged],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn add_party(
        &mut self,
        party: ContractParty,
        at: DateTime<Utc>,
    ) -> Result<(), LegalDomainError> {
        if self.status != ContractStatus::Draft {
            return Err(LegalDomainError::Terminal);
        }
        if self.parties.len() >= MAX_PARTIES
            || self
                .parties
                .iter()
                .any(|existing| existing.party_id == party.party_id)
        {
            return Err(LegalDomainError::InvalidParty);
        }
        let mut next = self.clone();
        next.advance(at)?;
        next.parties.push(party);
        next.uncommitted_events.push(DomainEvent::ContractChanged);
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn transition(
        &mut self,
        transition: ContractTransition,
        actor: &str,
        at: DateTime<Utc>,
    ) -> Result<(), LegalDomainError> {
        validate_text(actor, "actor", MAX_REFERENCE_BYTES)?;
        let (next_status, approver) = match transition {
            ContractTransition::SubmitForReview if self.status == ContractStatus::Draft => {
                (ContractStatus::Review, None)
            }
            ContractTransition::Approve
                if self.status == ContractStatus::Review && actor != self.created_by =>
            {
                (ContractStatus::Approved, Some(actor.to_owned()))
            }
            ContractTransition::Activate if self.status == ContractStatus::Approved => {
                (ContractStatus::Active, self.approved_by.clone())
            }
            ContractTransition::Expire
                if self.status == ContractStatus::Active && at >= self.retention_until =>
            {
                (ContractStatus::Expired, self.approved_by.clone())
            }
            ContractTransition::Terminate if self.status == ContractStatus::Active => {
                (ContractStatus::Terminated, self.approved_by.clone())
            }
            ContractTransition::Archive
                if matches!(
                    self.status,
                    ContractStatus::Expired | ContractStatus::Terminated
                ) && at >= self.retention_until =>
            {
                (ContractStatus::Archived, self.approved_by.clone())
            }
            ContractTransition::Approve if self.status == ContractStatus::Review => {
                return Err(LegalDomainError::ApprovalSeparation);
            }
            ContractTransition::Expire
                if self.status == ContractStatus::Active && at < self.retention_until =>
            {
                return Err(LegalDomainError::RetentionActive);
            }
            ContractTransition::Archive
                if matches!(
                    self.status,
                    ContractStatus::Expired | ContractStatus::Terminated
                ) && at < self.retention_until =>
            {
                return Err(LegalDomainError::RetentionActive);
            }
            _ => return Err(LegalDomainError::InvalidTransition),
        };
        let mut next = self.clone();
        next.advance(at)?;
        next.status = next_status;
        if approver.is_some() {
            next.approved_by = approver;
        }
        next.uncommitted_events.push(DomainEvent::ContractChanged);
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), LegalDomainError> {
        validate_scope(&self.scope)?;
        validate_text(&self.reference, "contract_reference", MAX_REFERENCE_BYTES)?;
        validate_text(&self.title, "contract_title", MAX_TITLE_BYTES)?;
        validate_object_ref(&self.object_ref)?;
        validate_digest(&self.content_sha256)?;
        validate_text(&self.created_by, "created_by", MAX_REFERENCE_BYTES)?;
        validate_common(self.version, self.created_at, self.updated_at)?;
        validate_retention(self.retention_until, self.created_at)?;
        if self.parties.len() > MAX_PARTIES {
            return Err(LegalDomainError::InvalidParty);
        }
        let mut party_ids = std::collections::HashSet::with_capacity(self.parties.len());
        for party in &self.parties {
            if !party_ids.insert(party.party_id) {
                return Err(LegalDomainError::InvalidParty);
            }
            ContractParty::try_new(party.party_id, party.role.clone())?;
        }
        if let Some(approved_by) = &self.approved_by {
            validate_text(approved_by, "approved_by", MAX_REFERENCE_BYTES)?;
            if approved_by == &self.created_by {
                return Err(LegalDomainError::ApprovalSeparation);
            }
        }
        if matches!(
            self.status,
            ContractStatus::Approved
                | ContractStatus::Active
                | ContractStatus::Expired
                | ContractStatus::Terminated
                | ContractStatus::Archived
        ) && self.approved_by.is_none()
        {
            return Err(LegalDomainError::InvalidTransition);
        }
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), LegalDomainError> {
        advance(&mut self.version, &mut self.updated_at, at)
    }

    pub const fn id(&self) -> LegalContractId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub fn reference(&self) -> &str {
        &self.reference
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn object_ref(&self) -> &str {
        &self.object_ref
    }
    pub fn content_sha256(&self) -> &str {
        &self.content_sha256
    }
    pub const fn retention_until(&self) -> DateTime<Utc> {
        self.retention_until
    }
    pub const fn status(&self) -> ContractStatus {
        self.status
    }
    pub fn parties(&self) -> &[ContractParty] {
        &self.parties
    }
    pub fn created_by(&self) -> &str {
        &self.created_by
    }
    pub fn approved_by(&self) -> Option<&str> {
        self.approved_by.as_deref()
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentVersion {
    pub version: u64,
    pub object_ref: String,
    pub content_sha256: String,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalDocument {
    id: LegalDocumentId,
    version: u64,
    scope: TenantScope,
    title: String,
    versions: Vec<DocumentVersion>,
    archived: bool,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl LegalDocument {
    pub fn create(
        id: LegalDocumentId,
        scope: TenantScope,
        title: String,
        at: DateTime<Utc>,
    ) -> Result<Self, LegalDomainError> {
        validate_scope(&scope)?;
        validate_text(&title, "document_title", MAX_TITLE_BYTES)?;
        let value = Self {
            id,
            version: 1,
            scope,
            title,
            versions: Vec::new(),
            archived: false,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::DocumentChanged],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn add_version(
        &mut self,
        object_ref: String,
        content_sha256: String,
        actor: String,
        at: DateTime<Utc>,
    ) -> Result<(), LegalDomainError> {
        if self.archived {
            return Err(LegalDomainError::Terminal);
        }
        if self.versions.len() >= MAX_DOCUMENT_VERSIONS {
            return Err(LegalDomainError::InvalidDocumentVersion);
        }
        validate_object_ref(&object_ref)?;
        validate_digest(&content_sha256)?;
        validate_text(&actor, "created_by", MAX_REFERENCE_BYTES)?;
        let document_version = u64::try_from(self.versions.len())
            .ok()
            .and_then(|value| value.checked_add(1))
            .ok_or(LegalDomainError::InvalidDocumentVersion)?;
        let mut next = self.clone();
        next.advance(at)?;
        next.versions.push(DocumentVersion {
            version: document_version,
            object_ref,
            content_sha256,
            created_at: at,
            created_by: actor,
        });
        next.uncommitted_events.push(DomainEvent::DocumentChanged);
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn archive(&mut self, at: DateTime<Utc>) -> Result<(), LegalDomainError> {
        if self.archived {
            return Err(LegalDomainError::Terminal);
        }
        if self.versions.is_empty() {
            return Err(LegalDomainError::InvalidDocumentVersion);
        }
        let mut next = self.clone();
        next.advance(at)?;
        next.archived = true;
        next.uncommitted_events.push(DomainEvent::DocumentChanged);
        *self = next;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), LegalDomainError> {
        validate_scope(&self.scope)?;
        validate_text(&self.title, "document_title", MAX_TITLE_BYTES)?;
        validate_common(self.version, self.created_at, self.updated_at)?;
        if self.versions.len() > MAX_DOCUMENT_VERSIONS
            || (self.archived && self.versions.is_empty())
        {
            return Err(LegalDomainError::InvalidDocumentVersion);
        }
        for (index, version) in self.versions.iter().enumerate() {
            let expected_version = u64::try_from(index)
                .map_err(|_| LegalDomainError::InvalidDocumentVersion)?
                .checked_add(1)
                .ok_or(LegalDomainError::InvalidDocumentVersion)?;
            if version.version != expected_version
                || version.created_at < self.created_at
                || version.created_at > self.updated_at
            {
                return Err(LegalDomainError::InvalidDocumentVersion);
            }
            validate_object_ref(&version.object_ref)?;
            validate_digest(&version.content_sha256)?;
            validate_text(&version.created_by, "created_by", MAX_REFERENCE_BYTES)?;
        }
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), LegalDomainError> {
        advance(&mut self.version, &mut self.updated_at, at)
    }

    pub const fn id(&self) -> LegalDocumentId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub fn versions(&self) -> &[DocumentVersion] {
        &self.versions
    }
    pub fn current_version(&self) -> u64 {
        u64::try_from(self.versions.len()).expect("bounded Legal document version count")
    }
    pub const fn archived(&self) -> bool {
        self.archived
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessEntry {
    id: AccessEntryId,
    version: u64,
    scope: TenantScope,
    document_id: LegalDocumentId,
    document_version: u64,
    actor_ref: String,
    action: String,
    purpose: String,
    observed_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl AccessEntry {
    #[allow(clippy::too_many_arguments)]
    pub fn record(
        id: AccessEntryId,
        scope: TenantScope,
        document_id: LegalDocumentId,
        document_version: u64,
        actor_ref: String,
        action: String,
        purpose: String,
        observed_at: DateTime<Utc>,
        now: DateTime<Utc>,
    ) -> Result<Self, LegalDomainError> {
        validate_scope(&scope)?;
        if document_version == 0 {
            return Err(LegalDomainError::InvalidDocumentVersion);
        }
        validate_text(&actor_ref, "actor_ref", MAX_REFERENCE_BYTES)?;
        validate_text(&action, "action", MAX_REFERENCE_BYTES)?;
        validate_text(&purpose, "purpose", MAX_PURPOSE_BYTES)?;
        if observed_at > now + Duration::minutes(MAX_ACCESS_CLOCK_SKEW_MINUTES) {
            return Err(LegalDomainError::AccessClockSkew);
        }
        let value = Self {
            id,
            version: 1,
            scope,
            document_id,
            document_version,
            actor_ref,
            action,
            purpose,
            observed_at,
            uncommitted_events: vec![DomainEvent::AccessRecorded],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), LegalDomainError> {
        validate_scope(&self.scope)?;
        if self.version != 1 || self.document_version == 0 {
            return Err(LegalDomainError::InvalidDocumentVersion);
        }
        validate_text(&self.actor_ref, "actor_ref", MAX_REFERENCE_BYTES)?;
        validate_text(&self.action, "action", MAX_REFERENCE_BYTES)?;
        validate_text(&self.purpose, "purpose", MAX_PURPOSE_BYTES)
    }

    pub const fn id(&self) -> AccessEntryId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn document_id(&self) -> LegalDocumentId {
        self.document_id
    }
    pub const fn document_version(&self) -> u64 {
        self.document_version
    }
    pub fn actor_ref(&self) -> &str {
        &self.actor_ref
    }
    pub fn action(&self) -> &str {
        &self.action
    }
    pub fn purpose(&self) -> &str {
        &self.purpose
    }
    pub const fn observed_at(&self) -> DateTime<Utc> {
        self.observed_at
    }
    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MatterStatus {
    Open,
    Closed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecordRelation {
    GovernedBy,
    Supports,
    BilledUnder,
    AttributedTo,
    Disputes,
    SettlementFor,
    Supersedes,
    AccountingFor,
}

/// Immutable evidence of the exact external record revision Legal reviewed.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecordLink {
    pub relation: RecordRelation,
    pub target: RecordRef,
    pub target_scope: TenantScope,
    pub target_digest: String,
    pub linked_by: String,
    pub linked_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalHold {
    pub active: bool,
    pub reason: String,
    pub placed_by: String,
    pub placed_at: DateTime<Utc>,
    pub released_by: Option<String>,
    pub released_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalMatter {
    id: LegalMatterId,
    version: u64,
    scope: TenantScope,
    reference: String,
    title: String,
    status: MatterStatus,
    links: Vec<RecordLink>,
    legal_hold: Option<LegalHold>,
    created_by: String,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl LegalMatter {
    pub fn create(
        id: LegalMatterId,
        scope: TenantScope,
        reference: String,
        title: String,
        created_by: String,
        at: DateTime<Utc>,
    ) -> Result<Self, LegalDomainError> {
        validate_scope(&scope)?;
        validate_text(&reference, "matter_reference", MAX_REFERENCE_BYTES)?;
        validate_text(&title, "matter_title", MAX_TITLE_BYTES)?;
        validate_text(&created_by, "created_by", MAX_REFERENCE_BYTES)?;
        let value = Self {
            id,
            version: 1,
            scope,
            reference,
            title,
            status: MatterStatus::Open,
            links: Vec::new(),
            legal_hold: None,
            created_by,
            created_at: at,
            updated_at: at,
            uncommitted_events: vec![DomainEvent::MatterChanged],
        };
        value.validate()?;
        Ok(value)
    }

    pub fn link_record(
        &mut self,
        relation: RecordRelation,
        target: RecordRef,
        target_scope: TenantScope,
        target_digest: String,
        actor: String,
        at: DateTime<Utc>,
    ) -> Result<(), LegalDomainError> {
        if self.status != MatterStatus::Open || self.links.len() >= MAX_MATTER_LINKS {
            return Err(LegalDomainError::InvalidRecordLink);
        }
        validate_scope(&target_scope)?;
        validate_same_holding(&self.scope, &target_scope)?;
        validate_digest(&target_digest)?;
        validate_text(&actor, "linked_by", MAX_REFERENCE_BYTES)?;
        if self.links.iter().any(|link| {
            link.relation == relation
                && link.target.context == target.context
                && link.target.aggregate_kind == target.aggregate_kind
                && link.target.aggregate_id == target.aggregate_id
        }) {
            return Err(LegalDomainError::InvalidRecordLink);
        }
        let link = RecordLink {
            relation,
            target,
            target_scope,
            target_digest,
            linked_by: actor,
            linked_at: at,
        };
        validate_record_link(&self.scope, &link, self.created_at, at)?;
        let mut next = self.clone();
        next.advance(at)?;
        next.links.push(link.clone());
        next.uncommitted_events
            .push(DomainEvent::RecordLinked(link));
        next.uncommitted_events.push(DomainEvent::MatterChanged);
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn place_hold(
        &mut self,
        reason: String,
        actor: String,
        at: DateTime<Utc>,
    ) -> Result<(), LegalDomainError> {
        if self.status != MatterStatus::Open
            || self.legal_hold.as_ref().is_some_and(|hold| hold.active)
        {
            return Err(LegalDomainError::InvalidLegalHold);
        }
        validate_text(&reason, "legal_hold_reason", MAX_TITLE_BYTES)?;
        validate_text(&actor, "placed_by", MAX_REFERENCE_BYTES)?;
        let hold = LegalHold {
            active: true,
            reason,
            placed_by: actor,
            placed_at: at,
            released_by: None,
            released_at: None,
        };
        let mut next = self.clone();
        next.advance(at)?;
        next.legal_hold = Some(hold.clone());
        next.uncommitted_events
            .push(DomainEvent::LegalHoldChanged(hold));
        next.uncommitted_events.push(DomainEvent::MatterChanged);
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn release_hold(
        &mut self,
        actor: String,
        at: DateTime<Utc>,
    ) -> Result<(), LegalDomainError> {
        validate_text(&actor, "released_by", MAX_REFERENCE_BYTES)?;
        let Some(current) = self.legal_hold.as_ref() else {
            return Err(LegalDomainError::InvalidLegalHold);
        };
        if !current.active || current.placed_by == actor {
            return Err(LegalDomainError::InvalidLegalHold);
        }
        let mut hold = current.clone();
        hold.active = false;
        hold.released_by = Some(actor);
        hold.released_at = Some(at);
        let mut next = self.clone();
        next.advance(at)?;
        next.legal_hold = Some(hold.clone());
        next.uncommitted_events
            .push(DomainEvent::LegalHoldChanged(hold));
        next.uncommitted_events.push(DomainEvent::MatterChanged);
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn close(&mut self, at: DateTime<Utc>) -> Result<(), LegalDomainError> {
        if self.status != MatterStatus::Open
            || self.legal_hold.as_ref().is_some_and(|hold| hold.active)
        {
            return Err(LegalDomainError::InvalidTransition);
        }
        let mut next = self.clone();
        next.advance(at)?;
        next.status = MatterStatus::Closed;
        next.uncommitted_events.push(DomainEvent::MatterChanged);
        next.validate()?;
        *self = next;
        Ok(())
    }

    pub fn validate(&self) -> Result<(), LegalDomainError> {
        validate_scope(&self.scope)?;
        validate_text(&self.reference, "matter_reference", MAX_REFERENCE_BYTES)?;
        validate_text(&self.title, "matter_title", MAX_TITLE_BYTES)?;
        validate_text(&self.created_by, "created_by", MAX_REFERENCE_BYTES)?;
        validate_common(self.version, self.created_at, self.updated_at)?;
        if self.links.len() > MAX_MATTER_LINKS {
            return Err(LegalDomainError::InvalidRecordLink);
        }
        for (index, link) in self.links.iter().enumerate() {
            validate_record_link(&self.scope, link, self.created_at, self.updated_at)?;
            if self.links[..index].iter().any(|existing| {
                existing.relation == link.relation
                    && existing.target.context == link.target.context
                    && existing.target.aggregate_kind == link.target.aggregate_kind
                    && existing.target.aggregate_id == link.target.aggregate_id
            }) {
                return Err(LegalDomainError::InvalidRecordLink);
            }
        }
        if let Some(hold) = &self.legal_hold {
            validate_text(&hold.reason, "legal_hold_reason", MAX_TITLE_BYTES)?;
            validate_text(&hold.placed_by, "placed_by", MAX_REFERENCE_BYTES)?;
            if hold.placed_at < self.created_at || hold.placed_at > self.updated_at {
                return Err(LegalDomainError::InvalidLegalHold);
            }
            match (hold.active, hold.released_by.as_deref(), hold.released_at) {
                (true, None, None) => {}
                (false, Some(released_by), Some(released_at))
                    if released_by != hold.placed_by
                        && released_at >= hold.placed_at
                        && released_at <= self.updated_at =>
                {
                    validate_text(released_by, "released_by", MAX_REFERENCE_BYTES)?;
                }
                _ => return Err(LegalDomainError::InvalidLegalHold),
            }
        }
        if self.status == MatterStatus::Closed
            && self.legal_hold.as_ref().is_some_and(|hold| hold.active)
        {
            return Err(LegalDomainError::InvalidLegalHold);
        }
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), LegalDomainError> {
        advance(&mut self.version, &mut self.updated_at, at)
    }

    pub const fn id(&self) -> LegalMatterId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub fn reference(&self) -> &str {
        &self.reference
    }
    pub fn title(&self) -> &str {
        &self.title
    }
    pub const fn status(&self) -> MatterStatus {
        self.status
    }
    pub fn links(&self) -> &[RecordLink] {
        &self.links
    }
    pub fn legal_hold(&self) -> Option<&LegalHold> {
        self.legal_hold.as_ref()
    }
    pub fn created_by(&self) -> &str {
        &self.created_by
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "aggregate", content = "state", rename_all = "snake_case")]
pub enum LegalAggregate {
    Contract(LegalContract),
    Document(LegalDocument),
    AccessEntry(AccessEntry),
    Matter(LegalMatter),
}

impl LegalAggregate {
    pub fn validate(&self) -> Result<(), LegalDomainError> {
        match self {
            Self::Contract(value) => value.validate(),
            Self::Document(value) => value.validate(),
            Self::AccessEntry(value) => value.validate(),
            Self::Matter(value) => value.validate(),
        }
    }

    pub const fn occurred_at(&self) -> DateTime<Utc> {
        match self {
            Self::Contract(value) => value.updated_at(),
            Self::Document(value) => value.updated_at(),
            Self::AccessEntry(value) => value.observed_at(),
            Self::Matter(value) => value.updated_at(),
        }
    }
    pub fn scope(&self) -> &TenantScope {
        match self {
            Self::Contract(value) => value.scope(),
            Self::Document(value) => value.scope(),
            Self::AccessEntry(value) => value.scope(),
            Self::Matter(value) => value.scope(),
        }
    }

    pub const fn version(&self) -> u64 {
        match self {
            Self::Contract(value) => value.version(),
            Self::Document(value) => value.version(),
            Self::AccessEntry(value) => value.version(),
            Self::Matter(value) => value.version(),
        }
    }

    pub const fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Contract(_) => "legal_contract",
            Self::Document(_) => "legal_document",
            Self::AccessEntry(_) => "legal_access_entry",
            Self::Matter(_) => "legal_matter",
        }
    }

    pub const fn id_uuid(&self) -> Uuid {
        match self {
            Self::Contract(value) => value.id().as_uuid(),
            Self::Document(value) => value.id().as_uuid(),
            Self::AccessEntry(value) => value.id().as_uuid(),
            Self::Matter(value) => value.id().as_uuid(),
        }
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        match self {
            Self::Contract(value) => value.take_uncommitted_events(),
            Self::Document(value) => value.take_uncommitted_events(),
            Self::AccessEntry(value) => value.take_uncommitted_events(),
            Self::Matter(value) => value.take_uncommitted_events(),
        }
    }
}

fn validate_record_link(
    matter_scope: &TenantScope,
    link: &RecordLink,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
) -> Result<(), LegalDomainError> {
    validate_scope(&link.target_scope)?;
    validate_same_holding(matter_scope, &link.target_scope)?;
    if link.target.aggregate_id.is_nil() || link.target.aggregate_version == 0 {
        return Err(LegalDomainError::InvalidRecordLink);
    }
    validate_text(
        link.target.aggregate_kind.as_str(),
        "target_aggregate_kind",
        MAX_REFERENCE_BYTES,
    )?;
    validate_digest(&link.target_digest)?;
    validate_text(&link.linked_by, "linked_by", MAX_REFERENCE_BYTES)?;
    if link.linked_at < created_at || link.linked_at > updated_at {
        return Err(LegalDomainError::InvalidRecordLink);
    }
    Ok(())
}

fn validate_same_holding(left: &TenantScope, right: &TenantScope) -> Result<(), LegalDomainError> {
    fn holding(scope: &TenantScope) -> Option<Uuid> {
        match scope {
            TenantScope::Company { holding_id, .. } | TenantScope::Branch { holding_id, .. } => {
                Some(*holding_id)
            }
            _ => None,
        }
    }
    if holding(left).is_some() && holding(left) == holding(right) {
        Ok(())
    } else {
        Err(LegalDomainError::CrossTenantLink)
    }
}

fn validate_scope(scope: &TenantScope) -> Result<(), LegalDomainError> {
    scope
        .validate()
        .map_err(|_| LegalDomainError::InvalidScope)?;
    if matches!(
        scope,
        TenantScope::Company { .. } | TenantScope::Branch { .. }
    ) {
        Ok(())
    } else {
        Err(LegalDomainError::InvalidScope)
    }
}

fn validate_text(value: &str, field: &'static str, maximum: usize) -> Result<(), LegalDomainError> {
    if value.trim().is_empty() {
        return Err(LegalDomainError::Required { field });
    }
    if value.len() > maximum {
        return Err(LegalDomainError::TooLong { field, maximum });
    }
    if value.chars().any(char::is_control) {
        return Err(LegalDomainError::ControlCharacter { field });
    }
    let lower = value.to_ascii_lowercase();
    if [
        "password",
        "secret",
        "token",
        "private_key",
        "payment_instrument",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
    {
        return Err(LegalDomainError::SensitiveMaterial { field });
    }
    Ok(())
}

fn validate_object_ref(value: &str) -> Result<(), LegalDomainError> {
    validate_text(value, "object_ref", MAX_REFERENCE_BYTES)
}

fn validate_digest(value: &str) -> Result<(), LegalDomainError> {
    validate_text(value, "content_sha256", MAX_REFERENCE_BYTES)?;
    let digest = value.strip_prefix("sha256:").unwrap_or(value);
    if digest.len() != 64 || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(LegalDomainError::Required {
            field: "content_sha256",
        });
    }
    Ok(())
}

fn validate_retention(
    retention_until: DateTime<Utc>,
    begins_at: DateTime<Utc>,
) -> Result<(), LegalDomainError> {
    if retention_until < begins_at
        || retention_until > begins_at + Duration::days(MAX_RETENTION_DAYS)
    {
        return Err(LegalDomainError::InvalidRetention);
    }
    Ok(())
}

fn validate_common(
    version: u64,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
) -> Result<(), LegalDomainError> {
    if version == 0 {
        return Err(LegalDomainError::VersionOverflow);
    }
    if updated_at < created_at {
        return Err(LegalDomainError::TimestampRegression);
    }
    Ok(())
}

fn advance(
    version: &mut u64,
    updated_at: &mut DateTime<Utc>,
    at: DateTime<Utc>,
) -> Result<(), LegalDomainError> {
    if at < *updated_at {
        return Err(LegalDomainError::TimestampRegression);
    }
    *version = version
        .checked_add(1)
        .ok_or(LegalDomainError::VersionOverflow)?;
    *updated_at = at;
    Ok(())
}

#[cfg(test)]
mod tests;
