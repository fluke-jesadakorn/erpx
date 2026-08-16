//! Versioned, tenant-local authority evidence consumed by the Item Directory.
//!
//! These facts contain only authorization inputs. They never include record
//! titles, comment bodies, prompts, or other business payloads.

use chrono::{DateTime, Utc};
use erpx_kernel::{DataClassification, PrincipalId, TenantScope};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{RecordVersionRef, ReferenceValidationError, catalog::ContextId};

pub const AUTHORITY_EVIDENCE_EVENT_TYPE: &str = "erpx.authority.evidence";
pub const AUTHORITY_EVIDENCE_VERSION: u32 = 2;
pub const MAX_AUTHORITY_PERMISSIONS: usize = 256;
pub const MAX_AUTHORITY_ASSIGNMENTS: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityPrincipalKind {
    Human,
    Service,
    Agent,
    HumanGroup,
    AgentPool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityRealm {
    InternalWorkforce,
    PlatformWorkforce,
    CustomerPortal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AuthorityAssignmentEvidence {
    pub assignment_id: Uuid,
    pub role_id: Uuid,
    pub assignment_version: u64,
    pub scope: TenantScope,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PrincipalAuthorityEvidence {
    pub principal_id: PrincipalId,
    pub principal_kind: AuthorityPrincipalKind,
    pub realm: AuthorityRealm,
    pub active: bool,
    pub principal_version: u64,
    pub scope: TenantScope,
    pub assignments: Vec<AuthorityAssignmentEvidence>,
    #[serde(default)]
    pub group_memberships: Vec<GroupMembershipAuthorityEvidence>,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleAuthorityEvidence {
    pub role_id: Uuid,
    pub role_version: u64,
    pub scope: TenantScope,
    pub permissions: Vec<String>,
    pub classification_ceiling: DataClassification,
    pub active: bool,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMembershipAuthorityEvidence {
    pub group_id: PrincipalId,
    pub group_kind: AuthorityPrincipalKind,
    pub group_version: u64,
    pub member_id: PrincipalId,
    pub membership_version: u64,
    pub scope: TenantScope,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoritySharingDirection {
    SourceToTarget,
    Mutual,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthoritySharingStatus {
    Proposed,
    Active,
    Rejected,
    Revoked,
    Expired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityPortalUseCase {
    FieldServiceRecordAccess,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthorityPortalBindingStatus {
    Proposed,
    Active,
    Rejected,
    Revoked,
    Expired,
}

/// Customer Authority's opaque, tenant-local authorization evidence. It never
/// carries customer names, support content, or an internal directory entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustomerPortalBindingAuthorityEvidence {
    pub binding_id: Uuid,
    pub binding_version: u64,
    pub customer: RecordVersionRef,
    pub portal_principal_id: PrincipalId,
    pub principal_kind: AuthorityPrincipalKind,
    pub realm: AuthorityRealm,
    pub scope: TenantScope,
    pub verified_identity_digest: String,
    pub verified_identity_version: u64,
    pub permitted_use_cases: Vec<AuthorityPortalUseCase>,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub proposed_by: PrincipalId,
    pub approved_by: Option<PrincipalId>,
    pub ended_at: Option<DateTime<Utc>>,
    pub status: AuthorityPortalBindingStatus,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationSharingAuthorityEvidence {
    pub sharing_agreement_id: Uuid,
    pub agreement_version: u64,
    pub source_scope: TenantScope,
    pub target_scope: TenantScope,
    pub direction: AuthoritySharingDirection,
    pub permits_open_record: bool,
    pub source_approved: bool,
    pub target_approved: bool,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
    pub status: AuthoritySharingStatus,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "authority_fact", content = "fact", rename_all = "snake_case")]
pub enum AuthorityEvidenceFact {
    Principal(PrincipalAuthorityEvidence),
    Role(RoleAuthorityEvidence),
    GroupMembership(GroupMembershipAuthorityEvidence),
    OrganizationSharing(OrganizationSharingAuthorityEvidence),
    CustomerPortalBinding(CustomerPortalBindingAuthorityEvidence),
}

impl AuthorityEvidenceFact {
    pub fn validate(&self) -> Result<(), ReferenceValidationError> {
        match self {
            Self::Principal(value) => {
                value.scope.validate()?;
                if value.principal_version == 0
                    || value.assignments.len() > MAX_AUTHORITY_ASSIGNMENTS
                    || value.group_memberships.len() > MAX_AUTHORITY_ASSIGNMENTS
                {
                    return Err(ReferenceValidationError::MalformedAuthorityEvidence);
                }
                for assignment in &value.assignments {
                    assignment.scope.validate()?;
                    if assignment.assignment_id.is_nil()
                        || assignment.role_id.is_nil()
                        || assignment.assignment_version == 0
                        || assignment.valid_until <= assignment.valid_from
                    {
                        return Err(ReferenceValidationError::MalformedAuthorityEvidence);
                    }
                }
                let expected_group_kind = match value.principal_kind {
                    AuthorityPrincipalKind::HumanGroup => Some(AuthorityPrincipalKind::HumanGroup),
                    AuthorityPrincipalKind::AgentPool => Some(AuthorityPrincipalKind::AgentPool),
                    _ => None,
                };
                if value.group_memberships.iter().any(|membership| {
                    Some(membership.group_kind) != expected_group_kind
                        || membership.group_id != value.principal_id
                        || membership.group_version != value.principal_version
                        || validate_group_membership(membership).is_err()
                }) {
                    return Err(ReferenceValidationError::MalformedAuthorityEvidence);
                }
            }
            Self::Role(value) => {
                value.scope.validate()?;
                if value.role_id.is_nil()
                    || value.role_version == 0
                    || value.permissions.is_empty()
                    || value.permissions.len() > MAX_AUTHORITY_PERMISSIONS
                    || value.permissions.iter().any(|permission| {
                        permission.trim().is_empty()
                            || permission.len() > 128
                            || permission.contains('*')
                    })
                {
                    return Err(ReferenceValidationError::MalformedAuthorityEvidence);
                }
                let mut permissions = value.permissions.clone();
                permissions.sort_unstable();
                permissions.dedup();
                if permissions.len() != value.permissions.len() {
                    return Err(ReferenceValidationError::MalformedAuthorityEvidence);
                }
            }
            Self::GroupMembership(value) => {
                validate_group_membership(value)?;
            }
            Self::OrganizationSharing(value) => {
                value.source_scope.validate()?;
                value.target_scope.validate()?;
                if value.sharing_agreement_id.is_nil()
                    || value.agreement_version == 0
                    || value.valid_until <= value.valid_from
                    || !same_holding_distinct_branches(&value.source_scope, &value.target_scope)
                    || (value.status == AuthoritySharingStatus::Active
                        && (!value.permits_open_record
                            || !value.source_approved
                            || !value.target_approved))
                {
                    return Err(ReferenceValidationError::MalformedAuthorityEvidence);
                }
            }
            Self::CustomerPortalBinding(value) => {
                value.customer.validate()?;
                value.scope.validate()?;
                if value.binding_id.is_nil()
                    || value.binding_id.get_version_num() != 4
                    || value.binding_version == 0
                    || value.customer.record().owner_context() != ContextId::CustomerAuthority
                    || value.customer.record().record_type().as_str() != "customer.account"
                    || !matches!(
                        value.principal_kind,
                        AuthorityPrincipalKind::Human | AuthorityPrincipalKind::Service
                    )
                    || value.realm != AuthorityRealm::CustomerPortal
                    || value.verified_identity_digest.len() != 64
                    || !value
                        .verified_identity_digest
                        .bytes()
                        .all(|byte| byte.is_ascii_hexdigit())
                    || value.verified_identity_version == 0
                    || value.permitted_use_cases.is_empty()
                    || value.valid_until <= value.valid_from
                    || (value.status == AuthorityPortalBindingStatus::Proposed
                        && (value.approved_by.is_some() || value.ended_at.is_some()))
                    || (value.status == AuthorityPortalBindingStatus::Active
                        && (value.approved_by.is_none()
                            || value.approved_by == Some(value.proposed_by)
                            || value.ended_at.is_some()))
                    || (matches!(
                        value.status,
                        AuthorityPortalBindingStatus::Rejected
                            | AuthorityPortalBindingStatus::Revoked
                            | AuthorityPortalBindingStatus::Expired
                    ) && value.ended_at.is_none())
                {
                    return Err(ReferenceValidationError::MalformedAuthorityEvidence);
                }
                let mut use_cases = value.permitted_use_cases.clone();
                use_cases.sort_unstable();
                use_cases.dedup();
                if use_cases.len() != value.permitted_use_cases.len() {
                    return Err(ReferenceValidationError::MalformedAuthorityEvidence);
                }
            }
        }
        Ok(())
    }
}

fn validate_group_membership(
    value: &GroupMembershipAuthorityEvidence,
) -> Result<(), ReferenceValidationError> {
    value.scope.validate()?;
    if !matches!(
        value.group_kind,
        AuthorityPrincipalKind::HumanGroup | AuthorityPrincipalKind::AgentPool
    ) || value.group_id == value.member_id
        || value.group_version == 0
        || value.membership_version == 0
        || value.valid_until <= value.valid_from
    {
        return Err(ReferenceValidationError::MalformedAuthorityEvidence);
    }
    Ok(())
}

fn same_holding_distinct_branches(source: &TenantScope, target: &TenantScope) -> bool {
    match (source, target) {
        (
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
        ) => source_holding == target_holding && source_branch != target_branch,
        _ => false,
    }
}

#[cfg(test)]
mod tests {
    use chrono::Duration;
    use erpx_kernel::RecordId;

    use super::*;
    use crate::{RecordTypeId, StableRecordRef};

    fn id() -> Uuid {
        Uuid::new_v4()
    }

    fn branch(holding_id: Uuid, branch_id: Uuid) -> TenantScope {
        TenantScope::Branch {
            holding_id,
            company_id: id(),
            branch_id,
        }
    }

    #[test]
    fn active_sharing_requires_two_party_same_holding_open_record_evidence() {
        let now = Utc::now();
        let holding = id();
        let mut fact =
            AuthorityEvidenceFact::OrganizationSharing(OrganizationSharingAuthorityEvidence {
                sharing_agreement_id: id(),
                agreement_version: 3,
                source_scope: branch(holding, id()),
                target_scope: branch(holding, id()),
                direction: AuthoritySharingDirection::Mutual,
                permits_open_record: true,
                source_approved: true,
                target_approved: true,
                valid_from: now - Duration::minutes(1),
                valid_until: now + Duration::minutes(10),
                revoked_at: None,
                status: AuthoritySharingStatus::Active,
                changed_at: now,
            });
        assert!(fact.validate().is_ok());
        let AuthorityEvidenceFact::OrganizationSharing(value) = &mut fact else {
            unreachable!()
        };
        value.target_approved = false;
        assert_eq!(
            fact.validate(),
            Err(ReferenceValidationError::MalformedAuthorityEvidence)
        );
    }

    #[test]
    fn portal_binding_is_exact_versioned_customer_authority_without_business_payload() {
        let now = Utc::now();
        let maker = PrincipalId::try_from_uuid(id()).unwrap();
        let checker = PrincipalId::try_from_uuid(id()).unwrap();
        let customer = RecordVersionRef::try_new(
            StableRecordRef::try_new(
                RecordId::try_from_uuid(id()).unwrap(),
                ContextId::CustomerAuthority,
                RecordTypeId::try_new("customer.account").unwrap(),
            )
            .unwrap(),
            9,
        )
        .unwrap();
        let mut fact =
            AuthorityEvidenceFact::CustomerPortalBinding(CustomerPortalBindingAuthorityEvidence {
                binding_id: id(),
                binding_version: 2,
                customer,
                portal_principal_id: PrincipalId::try_from_uuid(id()).unwrap(),
                principal_kind: AuthorityPrincipalKind::Service,
                realm: AuthorityRealm::CustomerPortal,
                scope: branch(id(), id()),
                verified_identity_digest: "a".repeat(64),
                verified_identity_version: 3,
                permitted_use_cases: vec![AuthorityPortalUseCase::FieldServiceRecordAccess],
                valid_from: now - Duration::minutes(1),
                valid_until: now + Duration::minutes(10),
                proposed_by: maker,
                approved_by: Some(checker),
                ended_at: None,
                status: AuthorityPortalBindingStatus::Active,
                changed_at: now,
            });
        assert!(fact.validate().is_ok());
        let AuthorityEvidenceFact::CustomerPortalBinding(value) = &mut fact else {
            unreachable!()
        };
        value.realm = AuthorityRealm::InternalWorkforce;
        assert_eq!(
            fact.validate(),
            Err(ReferenceValidationError::MalformedAuthorityEvidence)
        );
        let wire = serde_json::to_string(&fact).unwrap();
        assert!(!wire.contains("customer_name"));
        assert!(!wire.contains("support"));
    }
}
