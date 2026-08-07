use chrono::{DateTime, Duration, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    DomainEvent, IdentityAccessDomainError, RoleId, next_version, validate_scope, validate_text,
};

macro_rules! identity_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, IdentityAccessDomainError> {
                if value.is_nil() {
                    return Err(IdentityAccessDomainError::NilIdentifier { field: $field });
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }

        impl TryFrom<Uuid> for $name {
            type Error = IdentityAccessDomainError;

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

identity_id!(PrincipalId, "principal_id");
identity_id!(RoleAssignmentId, "role_assignment_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PrincipalStatus {
    Invited,
    Active,
    Suspended,
    Disabled,
}

impl PrincipalStatus {
    const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Invited, Self::Active | Self::Disabled)
                | (Self::Active, Self::Suspended | Self::Disabled)
                | (Self::Suspended, Self::Active | Self::Disabled)
        )
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RiskTier {
    PlatformRoot,
    Administrator,
    High,
    Standard,
    Contractor,
    SupportDiagnostic,
    SupportMutation,
    Emergency,
}

impl RiskTier {
    pub fn maximum_duration(self) -> Duration {
        match self {
            Self::PlatformRoot | Self::Administrator | Self::High => Duration::days(90),
            Self::Standard => Duration::days(365),
            Self::Contractor => Duration::days(30),
            Self::SupportDiagnostic => Duration::hours(8),
            Self::SupportMutation => Duration::hours(2),
            Self::Emergency => Duration::hours(1),
        }
    }

    pub const fn requires_independent_approval(self) -> bool {
        matches!(self, Self::PlatformRoot | Self::Administrator | Self::High)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoleAssignment {
    id: RoleAssignmentId,
    role_id: RoleId,
    scope: TenantScope,
    risk_tier: RiskTier,
    valid_from: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    grant_reason: String,
    ticket_reference: Option<String>,
    grantor_ref: String,
    approver_ref: Option<String>,
    revoked_at: Option<DateTime<Utc>>,
}

impl RoleAssignment {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: RoleAssignmentId,
        role_id: RoleId,
        principal_scope: &TenantScope,
        scope: TenantScope,
        risk_tier: RiskTier,
        valid_from: DateTime<Utc>,
        valid_until: DateTime<Utc>,
        grant_reason: String,
        ticket_reference: Option<String>,
        grantor_ref: String,
        approver_ref: Option<String>,
    ) -> Result<Self, IdentityAccessDomainError> {
        validate_scope(&scope)?;
        if !principal_scope.contains(&scope) {
            return Err(IdentityAccessDomainError::AssignmentScopeOutsidePrincipal);
        }
        validate_text(&grant_reason, "grant_reason")?;
        validate_text(&grantor_ref, "grantor_ref")?;
        if let Some(value) = ticket_reference.as_deref() {
            validate_text(value, "ticket_reference")?;
        }
        if let Some(value) = approver_ref.as_deref() {
            validate_text(value, "approver_ref")?;
        }
        if valid_until == DateTime::<Utc>::MAX_UTC || valid_until <= valid_from {
            return Err(IdentityAccessDomainError::InvalidAssignmentWindow);
        }
        if valid_until - valid_from > risk_tier.maximum_duration() {
            return Err(IdentityAccessDomainError::AssignmentDurationExceeded { risk_tier });
        }
        if risk_tier.requires_independent_approval()
            && approver_ref
                .as_deref()
                .is_none_or(|approver| approver == grantor_ref)
        {
            return Err(IdentityAccessDomainError::IndependentApproverRequired);
        }
        Ok(Self {
            id,
            role_id,
            scope,
            risk_tier,
            valid_from,
            valid_until,
            grant_reason,
            ticket_reference,
            grantor_ref,
            approver_ref,
            revoked_at: None,
        })
    }

    pub fn is_active_at(&self, at: DateTime<Utc>) -> bool {
        self.revoked_at.is_none() && self.valid_from <= at && at < self.valid_until
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.valid_from < other.valid_until && other.valid_from < self.valid_until
    }

    pub const fn id(&self) -> RoleAssignmentId {
        self.id
    }

    pub const fn role_id(&self) -> RoleId {
        self.role_id
    }

    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }

    pub const fn risk_tier(&self) -> RiskTier {
        self.risk_tier
    }

    pub const fn valid_from(&self) -> DateTime<Utc> {
        self.valid_from
    }

    pub const fn valid_until(&self) -> DateTime<Utc> {
        self.valid_until
    }

    pub fn grant_reason(&self) -> &str {
        &self.grant_reason
    }

    pub fn ticket_reference(&self) -> Option<&str> {
        self.ticket_reference.as_deref()
    }

    pub fn grantor_ref(&self) -> &str {
        &self.grantor_ref
    }

    pub fn approver_ref(&self) -> Option<&str> {
        self.approver_ref.as_deref()
    }

    pub const fn revoked_at(&self) -> Option<DateTime<Utc>> {
        self.revoked_at
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AccessPrincipal {
    id: PrincipalId,
    version: u64,
    scope: TenantScope,
    display_name: String,
    status: PrincipalStatus,
    assignments: Vec<RoleAssignment>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl AccessPrincipal {
    pub fn create(
        id: PrincipalId,
        scope: TenantScope,
        display_name: String,
        at: DateTime<Utc>,
    ) -> Result<Self, IdentityAccessDomainError> {
        validate_scope(&scope)?;
        validate_text(&display_name, "display_name")?;
        let mut value = Self {
            id,
            version: 1,
            scope,
            display_name,
            status: PrincipalStatus::Invited,
            assignments: Vec::new(),
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.push_access_changed(at);
        Ok(value)
    }

    pub fn activate(&mut self, at: DateTime<Utc>) -> Result<(), IdentityAccessDomainError> {
        self.transition(PrincipalStatus::Active, at)
    }

    pub fn suspend(&mut self, at: DateTime<Utc>) -> Result<(), IdentityAccessDomainError> {
        self.transition(PrincipalStatus::Suspended, at)
    }

    pub fn disable(&mut self, at: DateTime<Utc>) -> Result<(), IdentityAccessDomainError> {
        self.transition(PrincipalStatus::Disabled, at)?;
        self.uncommitted_events
            .push(DomainEvent::PrincipalDisabled {
                principal_id: self.id,
                at,
            });
        Ok(())
    }

    pub fn assign_role(
        &mut self,
        assignment: RoleAssignment,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityAccessDomainError> {
        if self.status != PrincipalStatus::Active {
            return Err(IdentityAccessDomainError::PrincipalDisabled);
        }
        if at < self.updated_at {
            return Err(IdentityAccessDomainError::InvalidTimestamp);
        }
        if !self.scope.contains(assignment.scope()) {
            return Err(IdentityAccessDomainError::AssignmentScopeOutsidePrincipal);
        }
        if self.assignments.iter().any(|existing| {
            existing.revoked_at().is_none()
                && existing.role_id() == assignment.role_id()
                && existing.scope() == assignment.scope()
                && existing.overlaps(&assignment)
        }) {
            return Err(IdentityAccessDomainError::DuplicateActiveAssignment);
        }
        let mut candidate = self.clone();
        candidate.version = next_version(candidate.version)?;
        candidate.assignments.push(assignment);
        candidate.updated_at = at;
        *self = candidate;
        self.push_access_changed(at);
        Ok(())
    }

    pub fn revoke_role(
        &mut self,
        assignment_id: RoleAssignmentId,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityAccessDomainError> {
        if at < self.updated_at {
            return Err(IdentityAccessDomainError::InvalidTimestamp);
        }
        let mut candidate = self.clone();
        let assignment = candidate
            .assignments
            .iter_mut()
            .find(|value| value.id() == assignment_id)
            .ok_or(IdentityAccessDomainError::AssignmentNotFound)?;
        if assignment.revoked_at.is_some() {
            return Err(IdentityAccessDomainError::AssignmentAlreadyRevoked);
        }
        if at < assignment.valid_from {
            return Err(IdentityAccessDomainError::InvalidTimestamp);
        }
        assignment.revoked_at = Some(at);
        candidate.version = next_version(candidate.version)?;
        candidate.updated_at = at;
        *self = candidate;
        self.push_access_changed(at);
        Ok(())
    }

    fn transition(
        &mut self,
        next: PrincipalStatus,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityAccessDomainError> {
        if !self.status.can_transition_to(next) {
            return Err(IdentityAccessDomainError::InvalidPrincipalTransition {
                from: self.status,
                to: next,
            });
        }
        if at < self.updated_at {
            return Err(IdentityAccessDomainError::InvalidTimestamp);
        }
        let mut candidate = self.clone();
        candidate.version = next_version(candidate.version)?;
        candidate.status = next;
        candidate.updated_at = at;
        *self = candidate;
        self.push_access_changed(at);
        Ok(())
    }

    fn push_access_changed(&mut self, at: DateTime<Utc>) {
        let mut assigned_roles = self.active_role_ids(at);
        assigned_roles.sort_unstable();
        assigned_roles.dedup();
        self.uncommitted_events
            .push(DomainEvent::PrincipalAccessChanged {
                principal_id: self.id,
                display_name: self.display_name.clone(),
                status: self.status,
                assigned_roles,
                at,
            });
    }

    pub fn active_role_ids(&self, at: DateTime<Utc>) -> Vec<RoleId> {
        if self.status != PrincipalStatus::Active {
            return Vec::new();
        }
        self.assignments
            .iter()
            .filter(|assignment| assignment.is_active_at(at))
            .map(RoleAssignment::role_id)
            .collect()
    }

    pub const fn id(&self) -> PrincipalId {
        self.id
    }

    pub const fn version(&self) -> u64 {
        self.version
    }

    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub const fn status(&self) -> PrincipalStatus {
        self.status
    }

    pub fn assignments(&self) -> &[RoleAssignment] {
        &self.assignments
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

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn at(hour: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, hour, 0, 0)
            .single()
            .unwrap()
    }

    fn scope() -> TenantScope {
        TenantScope::Holding { holding_id: id(1) }
    }

    fn principal() -> AccessPrincipal {
        let mut principal = AccessPrincipal::create(
            PrincipalId::try_from_uuid(id(2)).unwrap(),
            scope(),
            "Holding administrator".into(),
            at(8),
        )
        .unwrap();
        principal.activate(at(9)).unwrap();
        principal.take_uncommitted_events();
        principal
    }

    fn assignment(
        assignment_id: u128,
        role_id: u128,
        risk_tier: RiskTier,
        approver: Option<&str>,
    ) -> Result<RoleAssignment, IdentityAccessDomainError> {
        RoleAssignment::new(
            RoleAssignmentId::try_from_uuid(id(assignment_id)).unwrap(),
            RoleId::try_from_uuid(id(role_id)).unwrap(),
            &scope(),
            TenantScope::Company {
                holding_id: id(1),
                company_id: id(3),
            },
            risk_tier,
            at(9),
            at(10),
            "temporary coverage".into(),
            Some("SEC-123".into()),
            "grantor".into(),
            approver.map(str::to_owned),
        )
    }

    #[test]
    fn principal_lifecycle_is_explicit_and_disabled_is_terminal() {
        let mut principal = principal();
        principal.suspend(at(10)).unwrap();
        principal.activate(at(11)).unwrap();
        principal.disable(at(12)).unwrap();
        assert_eq!(principal.status(), PrincipalStatus::Disabled);
        assert_eq!(principal.version(), 5);
        assert!(matches!(
            principal.activate(at(13)),
            Err(IdentityAccessDomainError::InvalidPrincipalTransition { .. })
        ));
    }

    #[test]
    fn high_risk_assignment_requires_independent_approval() {
        assert_eq!(
            assignment(10, 20, RiskTier::High, None),
            Err(IdentityAccessDomainError::IndependentApproverRequired)
        );
        assert_eq!(
            assignment(10, 20, RiskTier::High, Some("grantor")),
            Err(IdentityAccessDomainError::IndependentApproverRequired)
        );
        assert!(assignment(10, 20, RiskTier::High, Some("reviewer")).is_ok());
    }

    #[test]
    fn assignment_scope_and_duplicates_are_rejected_without_mutation() {
        let outside = RoleAssignment::new(
            RoleAssignmentId::try_from_uuid(id(10)).unwrap(),
            RoleId::try_from_uuid(id(20)).unwrap(),
            &scope(),
            TenantScope::Holding { holding_id: id(99) },
            RiskTier::Standard,
            at(9),
            at(10),
            "invalid scope".into(),
            None,
            "grantor".into(),
            None,
        );
        assert_eq!(
            outside,
            Err(IdentityAccessDomainError::AssignmentScopeOutsidePrincipal)
        );

        let mut principal = principal();
        principal
            .assign_role(assignment(10, 20, RiskTier::Standard, None).unwrap(), at(9))
            .unwrap();
        let before = principal.clone();
        assert_eq!(
            principal.assign_role(assignment(11, 20, RiskTier::Standard, None).unwrap(), at(9),),
            Err(IdentityAccessDomainError::DuplicateActiveAssignment)
        );
        assert_eq!(principal, before);
    }

    #[test]
    fn revoked_assignment_is_excluded_from_effective_access() {
        let mut principal = principal();
        let assignment = assignment(10, 20, RiskTier::Standard, None).unwrap();
        principal.assign_role(assignment, at(9)).unwrap();
        assert_eq!(principal.active_role_ids(at(9)).len(), 1);
        principal
            .revoke_role(RoleAssignmentId::try_from_uuid(id(10)).unwrap(), at(10))
            .unwrap();
        assert!(principal.active_role_ids(at(10)).is_empty());
    }
}
