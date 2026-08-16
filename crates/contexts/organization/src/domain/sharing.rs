use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::OrganizationDomainError;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct SharingAgreementId(Uuid);

impl SharingAgreementId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, OrganizationDomainError> {
        if value.is_nil() {
            return Err(OrganizationDomainError::NilIdentifier {
                field: "sharing_agreement_id",
            });
        }
        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharingCapability {
    DiscoverPrincipal,
    Mention,
    Reference,
    OpenRecord,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharingDirection {
    SourceToTarget,
    Mutual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SharingStatus {
    Proposed,
    Active,
    Rejected,
    Revoked,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DirectoryVisibility {
    Private,
    PublishedToDescendants,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SharingMutation {
    Changed,
    Idempotent,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationSharingAgreement {
    id: SharingAgreementId,
    version: u64,
    source_scope: TenantScope,
    target_scope: TenantScope,
    direction: SharingDirection,
    capabilities: Vec<SharingCapability>,
    reason: String,
    proposed_by: String,
    source_approved_by: Option<String>,
    target_approved_by: Option<String>,
    #[serde(default)]
    rejected_by: Option<String>,
    #[serde(default)]
    rejected_at: Option<DateTime<Utc>>,
    valid_from: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
    status: SharingStatus,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProposeSharingAgreement {
    pub id: SharingAgreementId,
    pub source_scope: TenantScope,
    pub target_scope: TenantScope,
    pub direction: SharingDirection,
    pub capabilities: Vec<SharingCapability>,
    pub reason: String,
    pub proposed_by: String,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
}

impl OrganizationSharingAgreement {
    pub fn propose(command: ProposeSharingAgreement) -> Result<Self, OrganizationDomainError> {
        validate_branch_pair(&command.source_scope, &command.target_scope)?;
        validate_text(&command.reason, "sharing_reason")?;
        validate_text(&command.proposed_by, "proposed_by")?;
        if command.valid_until <= command.valid_from {
            return Err(OrganizationDomainError::InvalidSharingWindow);
        }
        let mut capabilities = command.capabilities;
        capabilities.sort_unstable();
        let count = capabilities.len();
        capabilities.dedup();
        if capabilities.is_empty() || capabilities.len() != count {
            return Err(OrganizationDomainError::InvalidSharingCapabilities);
        }
        Ok(Self {
            id: command.id,
            version: 1,
            source_scope: command.source_scope,
            target_scope: command.target_scope,
            direction: command.direction,
            capabilities,
            reason: command.reason,
            proposed_by: command.proposed_by,
            source_approved_by: None,
            target_approved_by: None,
            rejected_by: None,
            rejected_at: None,
            valid_from: command.valid_from,
            valid_until: command.valid_until,
            revoked_at: None,
            status: SharingStatus::Proposed,
        })
    }

    /// Validates state reconstructed from normalized persistence columns.
    /// Adapters must call this after decoding; deserialization alone is never
    /// accepted as domain authority.
    pub fn validate_rehydrated(&self) -> Result<(), OrganizationDomainError> {
        validate_branch_pair(&self.source_scope, &self.target_scope)?;
        validate_text(&self.reason, "sharing_reason")?;
        validate_text(&self.proposed_by, "proposed_by")?;
        if self.version == 0 || self.valid_until <= self.valid_from {
            return Err(OrganizationDomainError::InvalidSharingWindow);
        }
        if self.capabilities.is_empty() {
            return Err(OrganizationDomainError::InvalidSharingCapabilities);
        }
        let mut capabilities = self.capabilities.clone();
        capabilities.sort_unstable();
        capabilities.dedup();
        if capabilities.len() != self.capabilities.len()
            || (self.status == SharingStatus::Active
                && (self.source_approved_by.is_none()
                    || self.target_approved_by.is_none()
                    || !self.capabilities.contains(&SharingCapability::OpenRecord)))
            || (self.status == SharingStatus::Rejected
                && (self.rejected_by.is_none() || self.rejected_at.is_none()))
            || (self.status != SharingStatus::Rejected
                && (self.rejected_by.is_some() || self.rejected_at.is_some()))
            || (self.status == SharingStatus::Revoked && self.revoked_at.is_none())
        {
            return Err(OrganizationDomainError::InvalidSharingTransition);
        }
        Ok(())
    }

    pub fn approve_source(
        &mut self,
        expected_version: u64,
        approver_ref: &str,
    ) -> Result<SharingMutation, OrganizationDomainError> {
        self.approve(expected_version, approver_ref, true)
    }

    pub fn approve_target(
        &mut self,
        expected_version: u64,
        approver_ref: &str,
    ) -> Result<SharingMutation, OrganizationDomainError> {
        self.approve(expected_version, approver_ref, false)
    }

    fn approve(
        &mut self,
        expected_version: u64,
        approver_ref: &str,
        source: bool,
    ) -> Result<SharingMutation, OrganizationDomainError> {
        self.expect_version(expected_version)?;
        validate_text(approver_ref, "sharing_approver_ref")?;
        if self.status != SharingStatus::Proposed || approver_ref == self.proposed_by {
            return Err(OrganizationDomainError::InvalidSharingApproval);
        }
        let selected = if source {
            self.source_approved_by.as_deref()
        } else {
            self.target_approved_by.as_deref()
        };
        if selected == Some(approver_ref) {
            return Ok(SharingMutation::Idempotent);
        }
        if selected.is_some()
            || self.source_approved_by.as_deref() == Some(approver_ref)
            || self.target_approved_by.as_deref() == Some(approver_ref)
        {
            return Err(OrganizationDomainError::InvalidSharingApproval);
        }
        if source {
            self.source_approved_by = Some(approver_ref.to_owned());
        } else {
            self.target_approved_by = Some(approver_ref.to_owned());
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(OrganizationDomainError::VersionOverflow)?;
        if self.source_approved_by.is_some() && self.target_approved_by.is_some() {
            self.status = SharingStatus::Active;
        }
        Ok(SharingMutation::Changed)
    }

    pub fn revoke(
        &mut self,
        expected_version: u64,
        actor_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<SharingMutation, OrganizationDomainError> {
        self.expect_version(expected_version)?;
        validate_text(actor_ref, "sharing_revoker_ref")?;
        if self.status == SharingStatus::Revoked {
            return Ok(SharingMutation::Idempotent);
        }
        if !matches!(self.status, SharingStatus::Proposed | SharingStatus::Active) {
            return Err(OrganizationDomainError::InvalidSharingTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(OrganizationDomainError::VersionOverflow)?;
        self.revoked_at = Some(at);
        self.status = SharingStatus::Revoked;
        Ok(SharingMutation::Changed)
    }

    pub fn reject(
        &mut self,
        expected_version: u64,
        actor_ref: &str,
        at: DateTime<Utc>,
    ) -> Result<SharingMutation, OrganizationDomainError> {
        self.expect_version(expected_version)?;
        validate_text(actor_ref, "sharing_rejector_ref")?;
        if self.status == SharingStatus::Rejected {
            return Ok(SharingMutation::Idempotent);
        }
        if self.status != SharingStatus::Proposed || actor_ref == self.proposed_by {
            return Err(OrganizationDomainError::InvalidSharingTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(OrganizationDomainError::VersionOverflow)?;
        self.rejected_by = Some(actor_ref.to_owned());
        self.rejected_at = Some(at);
        self.status = SharingStatus::Rejected;
        Ok(SharingMutation::Changed)
    }

    pub fn expire(
        &mut self,
        expected_version: u64,
        at: DateTime<Utc>,
    ) -> Result<SharingMutation, OrganizationDomainError> {
        self.expect_version(expected_version)?;
        if self.status == SharingStatus::Expired {
            return Ok(SharingMutation::Idempotent);
        }
        if !matches!(self.status, SharingStatus::Proposed | SharingStatus::Active)
            || at < self.valid_until
        {
            return Err(OrganizationDomainError::InvalidSharingTransition);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(OrganizationDomainError::VersionOverflow)?;
        self.status = SharingStatus::Expired;
        Ok(SharingMutation::Changed)
    }

    pub fn permits(
        &self,
        actor_scope: &TenantScope,
        target_scope: &TenantScope,
        capability: SharingCapability,
        at: DateTime<Utc>,
    ) -> bool {
        if self.status != SharingStatus::Active
            || at < self.valid_from
            || at >= self.valid_until
            || !self.capabilities.contains(&capability)
        {
            return false;
        }
        let forward = actor_scope == &self.source_scope && target_scope == &self.target_scope;
        let reverse = self.direction == SharingDirection::Mutual
            && actor_scope == &self.target_scope
            && target_scope == &self.source_scope;
        forward || reverse
    }

    fn expect_version(&self, expected: u64) -> Result<(), OrganizationDomainError> {
        if self.version != expected {
            return Err(OrganizationDomainError::SharingVersionConflict {
                expected,
                actual: self.version,
            });
        }
        Ok(())
    }

    pub const fn id(&self) -> SharingAgreementId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn status(&self) -> SharingStatus {
        self.status
    }
    pub const fn source_scope(&self) -> &TenantScope {
        &self.source_scope
    }
    pub const fn target_scope(&self) -> &TenantScope {
        &self.target_scope
    }
    pub const fn direction(&self) -> SharingDirection {
        self.direction
    }
    pub fn capabilities(&self) -> &[SharingCapability] {
        &self.capabilities
    }
    pub fn reason(&self) -> &str {
        &self.reason
    }
    pub fn proposed_by(&self) -> &str {
        &self.proposed_by
    }
    pub fn source_approved_by(&self) -> Option<&str> {
        self.source_approved_by.as_deref()
    }
    pub fn target_approved_by(&self) -> Option<&str> {
        self.target_approved_by.as_deref()
    }
    pub fn rejected_by(&self) -> Option<&str> {
        self.rejected_by.as_deref()
    }
    pub const fn rejected_at(&self) -> Option<DateTime<Utc>> {
        self.rejected_at
    }
    pub const fn valid_from(&self) -> DateTime<Utc> {
        self.valid_from
    }
    pub const fn valid_until(&self) -> DateTime<Utc> {
        self.valid_until
    }
    pub const fn revoked_at(&self) -> Option<DateTime<Utc>> {
        self.revoked_at
    }
}

pub fn can_discover_across_hierarchy(
    actor_scope: &TenantScope,
    target_scope: &TenantScope,
    visibility: DirectoryVisibility,
    capability: SharingCapability,
    agreements: &[OrganizationSharingAgreement],
    at: DateTime<Utc>,
) -> bool {
    if actor_scope == target_scope || actor_scope.contains(target_scope) {
        return true;
    }
    if target_scope.contains(actor_scope)
        && visibility == DirectoryVisibility::PublishedToDescendants
    {
        return true;
    }
    agreements
        .iter()
        .any(|agreement| agreement.permits(actor_scope, target_scope, capability, at))
}

fn validate_branch_pair(
    source: &TenantScope,
    target: &TenantScope,
) -> Result<(), OrganizationDomainError> {
    source
        .validate()
        .map_err(|_| OrganizationDomainError::InvalidScope)?;
    target
        .validate()
        .map_err(|_| OrganizationDomainError::InvalidScope)?;
    let (
        TenantScope::Branch {
            holding_id: source_holding,
            branch_id: source_branch,
            ..
        },
        TenantScope::Branch {
            holding_id: target_holding,
            branch_id: target_branch,
            ..
        },
    ) = (source, target)
    else {
        return Err(OrganizationDomainError::SharingRequiresBranches);
    };
    if source_holding != target_holding {
        return Err(OrganizationDomainError::CrossTenantSharingDenied);
    }
    if source_branch == target_branch {
        return Err(OrganizationDomainError::SelfSharingDenied);
    }
    Ok(())
}

fn validate_text(value: &str, field: &'static str) -> Result<(), OrganizationDomainError> {
    if value.trim().is_empty() {
        return Err(OrganizationDomainError::Required { field });
    }
    if value.len() > super::MAX_ORGANIZATION_NAME_BYTES {
        return Err(OrganizationDomainError::TooLong {
            field,
            maximum: super::MAX_ORGANIZATION_NAME_BYTES,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(OrganizationDomainError::ControlCharacter { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }
    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 15, 9, minute, 0)
            .single()
            .unwrap()
    }
    fn branch(branch_id: u128) -> TenantScope {
        TenantScope::Branch {
            holding_id: id(1),
            company_id: id(2),
            branch_id: id(branch_id),
        }
    }
    fn agreement() -> OrganizationSharingAgreement {
        OrganizationSharingAgreement::propose(ProposeSharingAgreement {
            id: SharingAgreementId::try_from_uuid(id(10)).unwrap(),
            source_scope: branch(3),
            target_scope: branch(4),
            direction: SharingDirection::Mutual,
            capabilities: vec![SharingCapability::Mention, SharingCapability::Reference],
            reason: "shared regional operations".into(),
            proposed_by: "sponsor".into(),
            valid_from: at(10),
            valid_until: at(50),
        })
        .unwrap()
    }

    #[test]
    fn sibling_access_requires_two_sided_active_agreement() {
        let mut agreement = agreement();
        assert!(!can_discover_across_hierarchy(
            &branch(3),
            &branch(4),
            DirectoryVisibility::Private,
            SharingCapability::Mention,
            &[agreement.clone()],
            at(20)
        ));
        agreement.approve_source(1, "source-owner").unwrap();
        assert!(!agreement.permits(&branch(3), &branch(4), SharingCapability::Mention, at(20)));
        agreement.approve_target(2, "target-owner").unwrap();
        assert!(agreement.permits(&branch(3), &branch(4), SharingCapability::Mention, at(20)));
        assert!(agreement.permits(&branch(4), &branch(3), SharingCapability::Reference, at(20)));
        assert!(!agreement.permits(
            &branch(3),
            &branch(4),
            SharingCapability::OpenRecord,
            at(20)
        ));
    }

    #[test]
    fn holding_descendant_and_published_ancestor_paths_are_distinct() {
        let holding = TenantScope::Holding { holding_id: id(1) };
        assert!(can_discover_across_hierarchy(
            &holding,
            &branch(3),
            DirectoryVisibility::Private,
            SharingCapability::Mention,
            &[],
            at(20)
        ));
        assert!(!can_discover_across_hierarchy(
            &branch(3),
            &holding,
            DirectoryVisibility::Private,
            SharingCapability::Mention,
            &[],
            at(20)
        ));
        assert!(can_discover_across_hierarchy(
            &branch(3),
            &holding,
            DirectoryVisibility::PublishedToDescendants,
            SharingCapability::Mention,
            &[],
            at(20)
        ));
    }

    #[test]
    fn revocation_is_immediate_idempotent_and_version_checked() {
        let mut agreement = agreement();
        agreement.approve_source(1, "source-owner").unwrap();
        agreement.approve_target(2, "target-owner").unwrap();
        assert_eq!(
            agreement.revoke(2, "security", at(21)),
            Err(OrganizationDomainError::SharingVersionConflict {
                expected: 2,
                actual: 3
            })
        );
        assert_eq!(
            agreement.revoke(3, "security", at(21)).unwrap(),
            SharingMutation::Changed
        );
        assert!(!agreement.permits(&branch(3), &branch(4), SharingCapability::Mention, at(22)));
        assert_eq!(
            agreement.revoke(4, "security", at(22)).unwrap(),
            SharingMutation::Idempotent
        );
        assert_eq!(agreement.version(), 4);
    }

    #[test]
    fn cross_holding_agreement_is_denied() {
        let error = OrganizationSharingAgreement::propose(ProposeSharingAgreement {
            id: SharingAgreementId::try_from_uuid(id(10)).unwrap(),
            source_scope: branch(3),
            target_scope: TenantScope::Branch {
                holding_id: id(99),
                company_id: id(98),
                branch_id: id(97),
            },
            direction: SharingDirection::Mutual,
            capabilities: vec![SharingCapability::Mention],
            reason: "invalid".into(),
            proposed_by: "sponsor".into(),
            valid_from: at(10),
            valid_until: at(50),
        });
        assert_eq!(
            error,
            Err(OrganizationDomainError::CrossTenantSharingDenied)
        );
    }
}
