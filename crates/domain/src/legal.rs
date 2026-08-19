//! Legal contracts and document lifecycle rules.
//!
//! The domain stores only bounded opaque object references and hashes. Raw
//! document bytes, signing keys, bearer tokens, and storage credentials never
//! enter these aggregates.

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{AuthorityScope, CommandContext};

pub const LEGAL_CONTRACT_VERSION: u16 = 1;
pub const MAX_LEGAL_REFERENCE_BYTES: usize = 255;
pub const MAX_LEGAL_TITLE_BYTES: usize = 255;
pub const MAX_LEGAL_PURPOSE_BYTES: usize = 512;
pub const MAX_LEGAL_PARTIES: usize = 128;
pub const MAX_DOCUMENT_VERSIONS: usize = 1_024;

const MAX_LEGAL_RETENTION_HORIZON: Duration = Duration::days(36_525);
const MAX_LEGAL_CLOCK_SKEW: Duration = Duration::minutes(5);

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum LegalError {
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("{field} exceeds the maximum length of {max} bytes")]
    FieldTooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} contains forbidden secret material")]
    SensitiveMaterial { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("legal scope is invalid")]
    InvalidScope,
    #[error("command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("command scope does not contain the legal record scope")]
    ScopeNotContained,
    #[error("invalid lifecycle transition from {from} to {to}")]
    InvalidTransition {
        from: &'static str,
        to: &'static str,
    },
    #[error("approval must be performed by a different principal")]
    ApprovalSeparation,
    #[error("contract retention has not ended")]
    RetentionActive,
    #[error("contract retention exceeds the bounded horizon")]
    RetentionHorizonExceeded,
    #[error("legal timestamp exceeds the allowed clock skew")]
    ClockSkewExceeded,
    #[error("the document is terminal")]
    Terminal,
    #[error("document version limit was reached")]
    VersionLimit,
    #[error("duplicate party")]
    DuplicateParty,
    #[error("download authorization is expired or invalid")]
    InvalidDownloadAuthorization,
}

/// Holding-scoped source/read reconciliation facts for the disposable Legal
/// and document projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct LegalProjectionReconciliation {
    pub source_event_count: u64,
    pub source_aggregate_count: u64,
    pub projected_aggregate_count: u64,
    pub missing_aggregate_count: u64,
    pub stale_aggregate_count: u64,
    pub orphan_aggregate_count: u64,
    /// Number of contract retention records whose authoritative write-side
    /// deadline/state/version does not have an exact read-side counterpart.
    pub retention_mismatch_count: u64,
}

impl LegalProjectionReconciliation {
    pub const fn is_consistent(self) -> bool {
        self.source_aggregate_count == self.projected_aggregate_count
            && self.source_event_count >= self.source_aggregate_count
            && self.missing_aggregate_count == 0
            && self.stale_aggregate_count == 0
            && self.orphan_aggregate_count == 0
            && self.retention_mismatch_count == 0
    }
}

fn safe(value: String, field: &'static str, max: usize) -> Result<String, LegalError> {
    if value.trim().is_empty() {
        return Err(LegalError::EmptyField { field });
    }
    if value.len() > max {
        return Err(LegalError::FieldTooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(LegalError::ControlCharacter { field });
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
        return Err(LegalError::SensitiveMaterial { field });
    }
    Ok(value)
}

fn command_actor<'a>(
    context: &'a CommandContext,
    scope: &AuthorityScope,
) -> Result<&'a str, LegalError> {
    context
        .validate()
        .map_err(|error| LegalError::InvalidCommandContext(error.to_string()))?;
    if !context.active_scope.contains(scope) {
        return Err(LegalError::ScopeNotContained);
    }
    let actor = context.actor.principal_ref.trim();
    if actor.is_empty() {
        return Err(LegalError::EmptyField { field: "actor" });
    }
    if actor.len() > MAX_LEGAL_REFERENCE_BYTES {
        return Err(LegalError::FieldTooLong {
            field: "actor",
            max: MAX_LEGAL_REFERENCE_BYTES,
        });
    }
    if actor.chars().any(char::is_control) {
        return Err(LegalError::ControlCharacter { field: "actor" });
    }
    Ok(actor)
}

/// Company/optional-branch scope for a legal record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalScope {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
}

impl LegalScope {
    pub fn new(
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
    ) -> Result<Self, LegalError> {
        let value = Self {
            holding_id,
            company_id,
            branch_id,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), LegalError> {
        if self.holding_id.is_nil() {
            return Err(LegalError::NilIdentifier {
                field: "holding_id",
            });
        }
        if self.company_id.is_nil() {
            return Err(LegalError::NilIdentifier {
                field: "company_id",
            });
        }
        if self.branch_id.is_some_and(|value| value.is_nil()) {
            return Err(LegalError::NilIdentifier { field: "branch_id" });
        }
        Ok(())
    }

    pub fn active_scope(&self) -> AuthorityScope {
        match self.branch_id {
            Some(branch_id) => AuthorityScope::Branch {
                holding_id: self.holding_id,
                company_id: self.company_id,
                branch_id,
            },
            None => AuthorityScope::Company {
                holding_id: self.holding_id,
                company_id: self.company_id,
            },
        }
    }
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

impl ContractStatus {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::Review => "review",
            Self::Approved => "approved",
            Self::Active => "active",
            Self::Expired => "expired",
            Self::Terminated => "terminated",
            Self::Archived => "archived",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContractParty {
    pub party_id: Uuid,
    pub role: String,
}

impl ContractParty {
    pub fn new(party_id: Uuid, role: impl Into<String>) -> Result<Self, LegalError> {
        if party_id.is_nil() {
            return Err(LegalError::NilIdentifier { field: "party_id" });
        }
        Ok(Self {
            party_id,
            role: safe(role.into(), "party_role", MAX_LEGAL_REFERENCE_BYTES)?,
        })
    }
}

/// Contract lifecycle aggregate. The object reference is opaque and can only
/// be used by a later authorized storage adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalContract {
    pub id: Uuid,
    pub reference: String,
    pub title: String,
    pub scope: LegalScope,
    pub object_ref: String,
    pub content_sha256: String,
    pub retention_until: DateTime<Utc>,
    pub status: ContractStatus,
    pub parties: Vec<ContractParty>,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub version: u64,
    pub last_command_id: Uuid,
}

impl LegalContract {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        reference: impl Into<String>,
        title: impl Into<String>,
        scope: LegalScope,
        object_ref: impl Into<String>,
        content_sha256: impl Into<String>,
        retention_until: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, LegalError> {
        if id.is_nil() {
            return Err(LegalError::NilIdentifier {
                field: "contract_id",
            });
        }
        scope.validate()?;
        validate_retention_horizon(retention_until)?;
        let created_by = command_actor(context, &scope.active_scope())?.to_owned();
        Ok(Self {
            id,
            reference: safe(
                reference.into(),
                "contract_reference",
                MAX_LEGAL_REFERENCE_BYTES,
            )?,
            title: safe(title.into(), "contract_title", MAX_LEGAL_TITLE_BYTES)?,
            scope,
            object_ref: safe(object_ref.into(), "object_ref", MAX_LEGAL_REFERENCE_BYTES)?,
            content_sha256: safe(
                content_sha256.into(),
                "content_sha256",
                MAX_LEGAL_REFERENCE_BYTES,
            )?,
            retention_until,
            status: ContractStatus::Draft,
            parties: Vec::new(),
            created_by,
            approved_by: None,
            version: 1,
            last_command_id: context.command_id,
        })
    }

    pub fn add_party(
        &mut self,
        party: ContractParty,
        context: &CommandContext,
    ) -> Result<(), LegalError> {
        command_actor(context, &self.scope.active_scope())?;
        if self.status != ContractStatus::Draft {
            return Err(LegalError::Terminal);
        }
        if self.parties.len() >= MAX_LEGAL_PARTIES
            || self
                .parties
                .iter()
                .any(|item| item.party_id == party.party_id)
        {
            return Err(if self.parties.len() >= MAX_LEGAL_PARTIES {
                LegalError::FieldTooLong {
                    field: "parties",
                    max: MAX_LEGAL_PARTIES,
                }
            } else {
                LegalError::DuplicateParty
            });
        }
        let mut next = self.clone();
        next.parties.push(party);
        next.commit(context)?;
        *self = next;
        Ok(())
    }

    pub fn submit_for_review(&mut self, context: &CommandContext) -> Result<(), LegalError> {
        self.transition(ContractStatus::Review, context)
    }

    pub fn approve(&mut self, context: &CommandContext) -> Result<(), LegalError> {
        let approver = command_actor(context, &self.scope.active_scope())?.to_owned();
        if approver == self.created_by {
            return Err(LegalError::ApprovalSeparation);
        }
        let mut next = self.clone();
        next.approved_by = Some(approver);
        next.transition(ContractStatus::Approved, context)?;
        *self = next;
        Ok(())
    }

    pub fn activate(&mut self, context: &CommandContext) -> Result<(), LegalError> {
        self.transition(ContractStatus::Active, context)
    }

    pub fn expire(
        &mut self,
        now: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<(), LegalError> {
        command_actor(context, &self.scope.active_scope())?;
        if now < self.retention_until && self.status == ContractStatus::Active {
            return Err(LegalError::RetentionActive);
        }
        self.transition(ContractStatus::Expired, context)
    }

    pub fn terminate(&mut self, context: &CommandContext) -> Result<(), LegalError> {
        self.transition(ContractStatus::Terminated, context)
    }

    pub fn archive(
        &mut self,
        now: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<(), LegalError> {
        command_actor(context, &self.scope.active_scope())?;
        if now < self.retention_until {
            return Err(LegalError::RetentionActive);
        }
        self.transition(ContractStatus::Archived, context)
    }

    fn transition(
        &mut self,
        next: ContractStatus,
        context: &CommandContext,
    ) -> Result<(), LegalError> {
        let allowed = matches!(
            (self.status, next),
            (ContractStatus::Draft, ContractStatus::Review)
                | (ContractStatus::Review, ContractStatus::Approved)
                | (ContractStatus::Approved, ContractStatus::Active)
                | (ContractStatus::Active, ContractStatus::Expired)
                | (ContractStatus::Active, ContractStatus::Terminated)
                | (ContractStatus::Expired, ContractStatus::Archived)
                | (ContractStatus::Terminated, ContractStatus::Archived)
        );
        if !allowed {
            return Err(LegalError::InvalidTransition {
                from: self.status.as_str(),
                to: next.as_str(),
            });
        }
        command_actor(context, &self.scope.active_scope())?;
        let mut candidate = self.clone();
        candidate.status = next;
        candidate.commit(context)?;
        *self = candidate;
        Ok(())
    }

    fn commit(&mut self, context: &CommandContext) -> Result<(), LegalError> {
        if context.command_id.is_nil()
            || context.idempotency_key.trim().is_empty()
            || context.idempotency_key.len() > 512
            || context.idempotency_key.chars().any(char::is_control)
        {
            return Err(LegalError::InvalidCommandContext("command identity".into()));
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(LegalError::InvalidCommandContext("version overflow".into()))?;
        self.last_command_id = context.command_id;
        Ok(())
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

/// Versioned document metadata with no raw content or credential material.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalDocument {
    pub id: Uuid,
    pub scope: LegalScope,
    pub title: String,
    pub versions: Vec<DocumentVersion>,
    pub current_version: u64,
    pub archived: bool,
}

impl LegalDocument {
    pub fn new(
        id: Uuid,
        scope: LegalScope,
        title: impl Into<String>,
        context: &CommandContext,
    ) -> Result<Self, LegalError> {
        if id.is_nil() {
            return Err(LegalError::NilIdentifier {
                field: "document_id",
            });
        }
        scope.validate()?;
        command_actor(context, &scope.active_scope())?;
        Ok(Self {
            id,
            scope,
            title: safe(title.into(), "document_title", MAX_LEGAL_TITLE_BYTES)?,
            versions: Vec::new(),
            current_version: 0,
            archived: false,
        })
    }

    pub fn add_version(
        &mut self,
        object_ref: impl Into<String>,
        content_sha256: impl Into<String>,
        context: &CommandContext,
    ) -> Result<u64, LegalError> {
        if self.archived {
            return Err(LegalError::Terminal);
        }
        let created_by = command_actor(context, &self.scope.active_scope())?.to_owned();
        if self.versions.len() >= MAX_DOCUMENT_VERSIONS {
            return Err(LegalError::VersionLimit);
        }
        let version = self
            .current_version
            .checked_add(1)
            .ok_or(LegalError::VersionLimit)?;
        let mut next = self.clone();
        next.versions.push(DocumentVersion {
            version,
            object_ref: safe(object_ref.into(), "object_ref", MAX_LEGAL_REFERENCE_BYTES)?,
            content_sha256: safe(
                content_sha256.into(),
                "content_sha256",
                MAX_LEGAL_REFERENCE_BYTES,
            )?,
            created_at: Utc::now(),
            created_by,
        });
        next.current_version = version;
        *self = next;
        Ok(version)
    }

    pub fn archive(&mut self, context: &CommandContext) -> Result<(), LegalError> {
        command_actor(context, &self.scope.active_scope())?;
        if self.archived {
            return Err(LegalError::Terminal);
        }
        if self.current_version == 0 {
            return Err(LegalError::InvalidCommandContext(
                "document must have a version before archive".into(),
            ));
        }
        let mut next = self.clone();
        next.archived = true;
        *self = next;
        Ok(())
    }
}

/// Short-lived metadata that a storage adapter may exchange for a signed URL.
/// It is not itself a URL, token, or permission decision.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SignedDownloadAuthorization {
    pub document_id: Uuid,
    pub version: u64,
    pub object_ref: String,
    pub scope: LegalScope,
    pub actor_ref: String,
    pub purpose: String,
    pub expires_at: DateTime<Utc>,
    pub max_bytes: u64,
}

impl SignedDownloadAuthorization {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        document_id: Uuid,
        version: u64,
        object_ref: impl Into<String>,
        scope: LegalScope,
        purpose: impl Into<String>,
        expires_at: DateTime<Utc>,
        max_bytes: u64,
        context: &CommandContext,
    ) -> Result<Self, LegalError> {
        if document_id.is_nil() || version == 0 || max_bytes == 0 {
            return Err(LegalError::InvalidDownloadAuthorization);
        }
        if expires_at <= Utc::now() {
            return Err(LegalError::InvalidDownloadAuthorization);
        }
        scope.validate()?;
        let actor_ref = command_actor(context, &scope.active_scope())?.to_owned();
        Ok(Self {
            document_id,
            version,
            object_ref: safe(object_ref.into(), "object_ref", MAX_LEGAL_REFERENCE_BYTES)?,
            scope,
            actor_ref,
            purpose: safe(purpose.into(), "purpose", MAX_LEGAL_PURPOSE_BYTES)?,
            expires_at,
            max_bytes,
        })
    }

    pub fn is_valid_at(&self, now: DateTime<Utc>) -> bool {
        now < self.expires_at
    }

    /// Revalidate metadata at the storage boundary. The constructor is the
    /// normal creation path, but this value is serializable and can cross a
    /// process boundary; presigners must therefore not trust a deserialized
    /// instance merely because its type is correct.
    pub fn validate_at(&self, now: DateTime<Utc>) -> Result<(), LegalError> {
        if self.document_id.is_nil() || self.version == 0 || self.max_bytes == 0 {
            return Err(LegalError::InvalidDownloadAuthorization);
        }
        self.scope.validate()?;
        safe(
            self.object_ref.clone(),
            "object_ref",
            MAX_LEGAL_REFERENCE_BYTES,
        )?;
        safe(
            self.actor_ref.clone(),
            "actor_ref",
            MAX_LEGAL_REFERENCE_BYTES,
        )?;
        safe(self.purpose.clone(), "purpose", MAX_LEGAL_PURPOSE_BYTES)?;
        if !self.is_valid_at(now) {
            return Err(LegalError::InvalidDownloadAuthorization);
        }
        Ok(())
    }
}

fn validate_retention_horizon(retention_until: DateTime<Utc>) -> Result<(), LegalError> {
    let latest_allowed = Utc::now()
        .checked_add_signed(MAX_LEGAL_RETENTION_HORIZON)
        .ok_or(LegalError::RetentionHorizonExceeded)?;
    if retention_until > latest_allowed {
        return Err(LegalError::RetentionHorizonExceeded);
    }
    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalAccessHistoryEntry {
    pub entry_id: Uuid,
    pub document_id: Uuid,
    pub version: u64,
    pub actor_ref: String,
    pub scope: LegalScope,
    pub action: String,
    pub purpose: String,
    pub observed_at: DateTime<Utc>,
}

impl LegalAccessHistoryEntry {
    pub fn new(
        document_id: Uuid,
        version: u64,
        action: impl Into<String>,
        purpose: impl Into<String>,
        context: &CommandContext,
        observed_at: DateTime<Utc>,
        scope: LegalScope,
    ) -> Result<Self, LegalError> {
        if document_id.is_nil() || version == 0 {
            return Err(LegalError::NilIdentifier {
                field: "document_id",
            });
        }
        scope.validate()?;
        let actor_ref = command_actor(context, &scope.active_scope())?.to_owned();
        if observed_at > Utc::now() + MAX_LEGAL_CLOCK_SKEW {
            return Err(LegalError::ClockSkewExceeded);
        }
        Ok(Self {
            entry_id: Uuid::new_v4(),
            document_id,
            version,
            actor_ref,
            scope,
            action: safe(action.into(), "action", MAX_LEGAL_REFERENCE_BYTES)?,
            purpose: safe(purpose.into(), "purpose", MAX_LEGAL_PURPOSE_BYTES)?,
            observed_at,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind};
    use chrono::Duration;

    fn context(actor: &str, scope: AuthorityScope, key: &str) -> CommandContext {
        CommandContext::try_new(
            key,
            ActorAttribution {
                principal_ref: actor.into(),
                kind: ActorKind::HoldingUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope,
            "legal_test",
        )
        .expect("valid context")
    }

    fn scope() -> LegalScope {
        LegalScope::new(Uuid::new_v4(), Uuid::new_v4(), Some(Uuid::new_v4())).expect("valid scope")
    }

    #[test]
    fn contract_requires_separated_approval_and_retention_before_archive() {
        let legal_scope = scope();
        let active_scope = legal_scope.active_scope();
        let creator = context("creator", active_scope.clone(), "contract-create");
        let reviewer = context("reviewer", active_scope, "contract-review");
        let now = Utc::now();
        let mut contract = LegalContract::new(
            Uuid::new_v4(),
            "contract-001",
            "Supplier agreement",
            legal_scope.clone(),
            "holding/objects/contract-001-v1",
            "sha256:abc",
            now + Duration::days(30),
            &creator,
        )
        .unwrap();
        contract
            .add_party(
                ContractParty::new(Uuid::new_v4(), "supplier").unwrap(),
                &creator,
            )
            .unwrap();
        contract.submit_for_review(&creator).unwrap();
        assert_eq!(
            contract.approve(&creator),
            Err(LegalError::ApprovalSeparation)
        );
        contract.approve(&reviewer).unwrap();
        contract.activate(&reviewer).unwrap();
        assert_eq!(
            contract.expire(now, &reviewer),
            Err(LegalError::RetentionActive)
        );
        contract
            .expire(now + Duration::days(31), &reviewer)
            .unwrap();
        contract
            .archive(now + Duration::days(31), &reviewer)
            .unwrap();
        assert_eq!(contract.status, ContractStatus::Archived);
    }

    #[test]
    fn contract_retention_is_bounded_at_aggregate_construction() {
        let legal_scope = scope();
        let context = context("creator", legal_scope.active_scope(), "retention-bound");
        assert_eq!(
            LegalContract::new(
                Uuid::new_v4(),
                "contract-retention-bound",
                "Bounded retention",
                legal_scope.clone(),
                "holding/contracts/retention-bound",
                "sha256:abc",
                Utc::now() + Duration::days(36_526),
                &context,
            ),
            Err(LegalError::RetentionHorizonExceeded)
        );
    }

    #[test]
    fn contract_termination_is_explicit_revocation_before_retention_archive() {
        let legal_scope = scope();
        let active_scope = legal_scope.active_scope();
        let creator = context("creator", active_scope.clone(), "revoke-create");
        let reviewer = context("reviewer", active_scope, "revoke-review");
        let now = Utc::now();
        let mut contract = LegalContract::new(
            Uuid::new_v4(),
            "contract-revoke-001",
            "Revocable supplier agreement",
            legal_scope,
            "holding/objects/contract-revoke-001-v1",
            "sha256:abc",
            now + Duration::days(30),
            &creator,
        )
        .unwrap();
        contract.submit_for_review(&creator).unwrap();
        contract.approve(&reviewer).unwrap();
        contract.activate(&reviewer).unwrap();
        contract.terminate(&reviewer).unwrap();
        assert_eq!(contract.status, ContractStatus::Terminated);
        assert_eq!(
            contract.archive(now, &reviewer),
            Err(LegalError::RetentionActive)
        );
        contract
            .archive(now + Duration::days(31), &reviewer)
            .unwrap();
        assert_eq!(contract.status, ContractStatus::Archived);
    }

    #[test]
    fn document_versions_and_download_authorizations_are_bounded_and_expiring() {
        let legal_scope = scope();
        let context = context("legal-admin", legal_scope.active_scope(), "document");
        let mut document = LegalDocument::new(
            Uuid::new_v4(),
            legal_scope.clone(),
            "Board resolution",
            &context,
        )
        .unwrap();
        assert_eq!(
            document.add_version("holding/docs/v1", "sha256:v1", &context),
            Ok(1)
        );
        let authorization = SignedDownloadAuthorization::new(
            document.id,
            1,
            "holding/docs/v1",
            legal_scope.clone(),
            "legal_review",
            Utc::now() + Duration::minutes(10),
            1_024,
            &context,
        )
        .unwrap();
        assert!(authorization.is_valid_at(Utc::now()));
        assert!(!authorization.is_valid_at(Utc::now() + Duration::minutes(11)));
        assert!(authorization.validate_at(Utc::now()).is_ok());
        assert_eq!(
            LegalDocument::new(Uuid::new_v4(), legal_scope, "token-bearing title", &context,),
            Err(LegalError::SensitiveMaterial {
                field: "document_title"
            })
        );
    }

    #[test]
    fn deserialized_download_metadata_is_revalidated_before_storage_use() {
        let legal_scope = scope();
        let context = context("legal-admin", legal_scope.active_scope(), "document");
        let mut authorization = SignedDownloadAuthorization::new(
            Uuid::new_v4(),
            1,
            "holding/docs/v1",
            legal_scope,
            "legal_review",
            Utc::now() + Duration::minutes(10),
            1_024,
            &context,
        )
        .expect("authorization");

        authorization.actor_ref.clear();
        assert_eq!(
            authorization.validate_at(Utc::now()),
            Err(LegalError::EmptyField { field: "actor_ref" })
        );

        authorization.actor_ref = "legal-admin".into();
        authorization.scope.holding_id = Uuid::nil();
        assert_eq!(
            authorization.validate_at(Utc::now()),
            Err(LegalError::NilIdentifier {
                field: "holding_id"
            })
        );

        assert_eq!(
            SignedDownloadAuthorization::new(
                Uuid::new_v4(),
                1,
                "holding/docs/expired",
                authorization.scope.clone(),
                "legal_review",
                Utc::now() - Duration::minutes(1),
                1_024,
                &context,
            ),
            Err(LegalError::InvalidDownloadAuthorization)
        );
    }

    #[test]
    fn access_history_rejects_cross_scope_and_sensitive_values() {
        let legal_scope = scope();
        let context = context("admin", legal_scope.active_scope(), "access");
        assert!(
            LegalAccessHistoryEntry::new(
                Uuid::new_v4(),
                1,
                "download",
                "customer_review",
                &context,
                Utc::now(),
                legal_scope.clone(),
            )
            .is_ok()
        );
        assert_eq!(
            LegalAccessHistoryEntry::new(
                Uuid::new_v4(),
                1,
                "download",
                "secret review",
                &context,
                Utc::now(),
                legal_scope.clone(),
            ),
            Err(LegalError::SensitiveMaterial { field: "purpose" })
        );
        let other_scope = LegalScope::new(Uuid::new_v4(), Uuid::new_v4(), None).unwrap();
        assert_eq!(
            SignedDownloadAuthorization::new(
                Uuid::new_v4(),
                1,
                "holding/docs/v1",
                other_scope,
                "review",
                Utc::now() + Duration::minutes(1),
                100,
                &context,
            ),
            Err(LegalError::ScopeNotContained)
        );
        assert_eq!(
            LegalAccessHistoryEntry::new(
                Uuid::new_v4(),
                1,
                "download",
                "customer_review",
                &context,
                Utc::now() + Duration::minutes(6),
                legal_scope,
            ),
            Err(LegalError::ClockSkewExceeded)
        );
    }

    #[test]
    fn duplicate_contract_parties_are_rejected() {
        let legal_scope = scope();
        let context = context("creator", legal_scope.active_scope(), "parties");
        let party = ContractParty::new(Uuid::new_v4(), "customer").unwrap();
        let mut contract = LegalContract::new(
            Uuid::new_v4(),
            "contract",
            "Agreement",
            legal_scope,
            "holding/docs/contract",
            "sha256:contract",
            Utc::now(),
            &context,
        )
        .unwrap();
        contract.add_party(party.clone(), &context).unwrap();
        assert_eq!(
            contract.add_party(party, &context),
            Err(LegalError::DuplicateParty)
        );
    }

    #[test]
    fn failed_commands_do_not_partially_mutate_legal_aggregates() {
        let legal_scope = scope();
        let creator = context("creator", legal_scope.active_scope(), "atomic-create");
        let reviewer = context("reviewer", legal_scope.active_scope(), "atomic-review");
        let mut contract = LegalContract::new(
            Uuid::new_v4(),
            "atomic-contract",
            "Atomic contract",
            legal_scope.clone(),
            "holding/contracts/atomic",
            "sha256:atomic",
            Utc::now() + Duration::days(30),
            &creator,
        )
        .expect("contract");
        contract.submit_for_review(&creator).expect("review");
        let before = contract.clone();
        let mut invalid_reviewer = reviewer.clone();
        invalid_reviewer.command_id = Uuid::nil();
        assert!(contract.approve(&invalid_reviewer).is_err());
        assert_eq!(contract, before);

        let mut document =
            LegalDocument::new(Uuid::new_v4(), legal_scope, "Atomic document", &creator)
                .expect("document");
        let before = document.clone();
        assert!(
            document
                .add_version(
                    "holding/token-documents/atomic-v1",
                    "sha256:atomic",
                    &creator
                )
                .is_err()
        );
        assert_eq!(document, before);
    }

    #[test]
    fn archived_documents_are_terminal() {
        let legal_scope = scope();
        let context = context("legal-admin", legal_scope.active_scope(), "archive");
        let mut document =
            LegalDocument::new(Uuid::new_v4(), legal_scope, "Archived document", &context)
                .expect("document");
        document
            .add_version("holding/documents/archive-v1", "sha256:archive", &context)
            .expect("version");
        document.archive(&context).expect("archive");
        assert_eq!(document.archive(&context), Err(LegalError::Terminal));
    }

    #[test]
    fn projection_reconciliation_is_green_only_without_mismatches() {
        assert!(LegalProjectionReconciliation::default().is_consistent());
        assert!(
            !LegalProjectionReconciliation {
                missing_aggregate_count: 1,
                ..LegalProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !LegalProjectionReconciliation {
                retention_mismatch_count: 1,
                ..LegalProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !LegalProjectionReconciliation {
                source_aggregate_count: 1,
                projected_aggregate_count: 0,
                ..LegalProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !LegalProjectionReconciliation {
                source_event_count: 1,
                source_aggregate_count: 2,
                projected_aggregate_count: 2,
                ..LegalProjectionReconciliation::default()
            }
            .is_consistent()
        );
    }
}
