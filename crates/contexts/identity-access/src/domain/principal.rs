use chrono::{DateTime, Duration, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    DomainEvent, IdentityAccessDomainError, NonHumanPrincipalGovernance, PrincipalKind,
    PrincipalRealm, RoleId, next_version, validate_scope, validate_text,
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GroupMembership {
    member_principal_id: PrincipalId,
    version: u64,
    valid_from: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    revoked_at: Option<DateTime<Utc>>,
}

impl GroupMembership {
    pub fn new(
        group_id: PrincipalId,
        member_principal_id: PrincipalId,
        valid_from: DateTime<Utc>,
        valid_until: DateTime<Utc>,
    ) -> Result<Self, IdentityAccessDomainError> {
        if group_id == member_principal_id
            || valid_until == DateTime::<Utc>::MAX_UTC
            || valid_until <= valid_from
        {
            return Err(IdentityAccessDomainError::InvalidGroupMembership);
        }
        Ok(Self {
            member_principal_id,
            version: 1,
            valid_from,
            valid_until,
            revoked_at: None,
        })
    }

    pub fn revoke(&mut self, at: DateTime<Utc>) -> Result<(), IdentityAccessDomainError> {
        if self.revoked_at.is_some() || at < self.valid_from {
            return Err(IdentityAccessDomainError::InvalidGroupMembership);
        }
        self.version = next_version(self.version)?;
        self.revoked_at = Some(at);
        Ok(())
    }

    pub fn is_active_at(&self, at: DateTime<Utc>) -> bool {
        self.revoked_at.is_none() && self.valid_from <= at && at < self.valid_until
    }

    pub const fn member_principal_id(&self) -> PrincipalId {
        self.member_principal_id
    }
    pub const fn version(&self) -> u64 {
        self.version
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
    scope: AuthorityScope,
    risk_tier: RiskTier,
    valid_from: DateTime<Utc>,
    valid_until: DateTime<Utc>,
    grant_reason: String,
    ticket_reference: Option<String>,
    grantor_ref: String,
    approver_ref: Option<String>,
    revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RoleAssignmentState {
    pub id: RoleAssignmentId,
    pub role_id: RoleId,
    pub scope: AuthorityScope,
    pub risk_tier: RiskTier,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub grant_reason: String,
    pub ticket_reference: Option<String>,
    pub grantor_ref: String,
    pub approver_ref: Option<String>,
    pub revoked_at: Option<DateTime<Utc>>,
}

impl RoleAssignment {
    pub fn try_rehydrate(
        principal_scope: &AuthorityScope,
        state: RoleAssignmentState,
    ) -> Result<Self, IdentityAccessDomainError> {
        let mut value = Self::new(
            state.id,
            state.role_id,
            principal_scope,
            state.scope,
            state.risk_tier,
            state.valid_from,
            state.valid_until,
            state.grant_reason,
            state.ticket_reference,
            state.grantor_ref,
            state.approver_ref,
        )?;
        if let Some(revoked_at) = state.revoked_at {
            if revoked_at < value.valid_from {
                return Err(IdentityAccessDomainError::InvalidTimestamp);
            }
            value.revoked_at = Some(revoked_at);
        }
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: RoleAssignmentId,
        role_id: RoleId,
        principal_scope: &AuthorityScope,
        scope: AuthorityScope,
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

    pub const fn scope(&self) -> &AuthorityScope {
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
    scope: AuthorityScope,
    display_name: String,
    #[serde(default)]
    kind: PrincipalKind,
    #[serde(default)]
    realm: PrincipalRealm,
    #[serde(default)]
    non_human_governance: Option<NonHumanPrincipalGovernance>,
    status: PrincipalStatus,
    assignments: Vec<RoleAssignment>,
    #[serde(default)]
    group_memberships: Vec<GroupMembership>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GroupMembershipState {
    pub member_principal_id: PrincipalId,
    pub version: u64,
    pub valid_from: DateTime<Utc>,
    pub valid_until: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccessPrincipalState {
    pub id: PrincipalId,
    pub version: u64,
    pub scope: AuthorityScope,
    pub display_name: String,
    pub kind: PrincipalKind,
    pub realm: PrincipalRealm,
    pub non_human_governance: Option<NonHumanPrincipalGovernance>,
    pub status: PrincipalStatus,
    pub assignments: Vec<RoleAssignment>,
    pub group_memberships: Vec<GroupMembershipState>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl AccessPrincipal {
    pub fn try_rehydrate(state: AccessPrincipalState) -> Result<Self, IdentityAccessDomainError> {
        validate_scope(&state.scope)?;
        validate_text(&state.display_name, "display_name")?;
        if state.version == 0
            || state.updated_at < state.created_at
            || (state.kind == PrincipalKind::Human) != state.non_human_governance.is_none()
            || (!matches!(
                state.kind,
                PrincipalKind::HumanGroup | PrincipalKind::AgentPool
            ) && !state.group_memberships.is_empty())
        {
            return Err(IdentityAccessDomainError::InvalidPrincipalGovernance);
        }
        let mut group_memberships = Vec::with_capacity(state.group_memberships.len());
        for membership in state.group_memberships {
            let mut value = GroupMembership::new(
                state.id,
                membership.member_principal_id,
                membership.valid_from,
                membership.valid_until,
            )?;
            if membership.version == 0
                || membership.version > 2
                || (membership.version == 1) != membership.revoked_at.is_none()
            {
                return Err(IdentityAccessDomainError::InvalidGroupMembership);
            }
            value.version = membership.version;
            value.revoked_at = membership.revoked_at;
            if value.revoked_at.is_some_and(|at| at < value.valid_from) {
                return Err(IdentityAccessDomainError::InvalidGroupMembership);
            }
            group_memberships.push(value);
        }
        let value = Self {
            id: state.id,
            version: state.version,
            scope: state.scope,
            display_name: state.display_name,
            kind: state.kind,
            realm: state.realm,
            non_human_governance: state.non_human_governance,
            status: state.status,
            assignments: state.assignments,
            group_memberships,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        if value.assignments.iter().any(|assignment| {
            !value.scope.contains(assignment.scope())
                || assignment.valid_until() <= assignment.valid_from()
        }) {
            return Err(IdentityAccessDomainError::AssignmentScopeOutsidePrincipal);
        }
        for (index, assignment) in value.assignments.iter().enumerate() {
            if value.assignments[..index].iter().any(|existing| {
                existing.id() == assignment.id()
                    || (existing.revoked_at().is_none()
                        && assignment.revoked_at().is_none()
                        && existing.role_id() == assignment.role_id()
                        && existing.scope() == assignment.scope()
                        && existing.overlaps(assignment))
            }) {
                return Err(IdentityAccessDomainError::DuplicateActiveAssignment);
            }
        }
        Ok(value)
    }

    pub fn create(
        id: PrincipalId,
        scope: AuthorityScope,
        display_name: String,
        at: DateTime<Utc>,
    ) -> Result<Self, IdentityAccessDomainError> {
        Self::create_human_in_realm(
            id,
            scope,
            display_name,
            PrincipalRealm::InternalWorkforce,
            at,
        )
    }

    pub fn create_human_in_realm(
        id: PrincipalId,
        scope: AuthorityScope,
        display_name: String,
        realm: PrincipalRealm,
        at: DateTime<Utc>,
    ) -> Result<Self, IdentityAccessDomainError> {
        Self::create_governed(
            id,
            scope,
            display_name,
            PrincipalKind::Human,
            realm,
            None,
            at,
        )
    }

    pub fn create_non_human(
        id: PrincipalId,
        scope: AuthorityScope,
        display_name: String,
        kind: PrincipalKind,
        governance: NonHumanPrincipalGovernance,
        at: DateTime<Utc>,
    ) -> Result<Self, IdentityAccessDomainError> {
        if kind == PrincipalKind::Human {
            return Err(IdentityAccessDomainError::InvalidPrincipalGovernance);
        }
        Self::create_governed(
            id,
            scope,
            display_name,
            kind,
            PrincipalRealm::InternalWorkforce,
            Some(governance),
            at,
        )
    }

    fn create_governed(
        id: PrincipalId,
        scope: AuthorityScope,
        display_name: String,
        kind: PrincipalKind,
        realm: PrincipalRealm,
        governance: Option<NonHumanPrincipalGovernance>,
        at: DateTime<Utc>,
    ) -> Result<Self, IdentityAccessDomainError> {
        validate_scope(&scope)?;
        validate_text(&display_name, "display_name")?;
        if (kind == PrincipalKind::Human) != governance.is_none() {
            return Err(IdentityAccessDomainError::InvalidPrincipalGovernance);
        }
        let mut value = Self {
            id,
            version: 1,
            scope,
            display_name,
            kind,
            realm,
            non_human_governance: governance,
            status: PrincipalStatus::Invited,
            assignments: Vec::new(),
            group_memberships: Vec::new(),
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

    #[allow(clippy::too_many_arguments)]
    pub fn renew_role(
        &mut self,
        assignment_id: RoleAssignmentId,
        replacement_assignment_id: RoleAssignmentId,
        valid_until: DateTime<Utc>,
        grantor_ref: String,
        approver_ref: Option<String>,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityAccessDomainError> {
        if self.status != PrincipalStatus::Active {
            return Err(IdentityAccessDomainError::PrincipalDisabled);
        }
        if at < self.updated_at {
            return Err(IdentityAccessDomainError::InvalidTimestamp);
        }
        if assignment_id == replacement_assignment_id
            || self
                .assignments
                .iter()
                .any(|value| value.id() == replacement_assignment_id)
        {
            return Err(IdentityAccessDomainError::DuplicateActiveAssignment);
        }
        let mut candidate = self.clone();
        let current = candidate
            .assignments
            .iter_mut()
            .find(|value| value.id() == assignment_id)
            .ok_or(IdentityAccessDomainError::AssignmentNotFound)?;
        if current.revoked_at.is_some() {
            return Err(IdentityAccessDomainError::AssignmentAlreadyRevoked);
        }
        if at < current.valid_from {
            return Err(IdentityAccessDomainError::InvalidTimestamp);
        }
        let replacement = RoleAssignment::new(
            replacement_assignment_id,
            current.role_id,
            &candidate.scope,
            current.scope.clone(),
            current.risk_tier,
            at,
            valid_until,
            current.grant_reason.clone(),
            current.ticket_reference.clone(),
            grantor_ref,
            approver_ref,
        )?;
        current.revoked_at = Some(at);
        candidate.assignments.push(replacement);
        candidate.version = next_version(candidate.version)?;
        candidate.updated_at = at;
        *self = candidate;
        self.push_access_changed(at);
        Ok(())
    }

    pub fn add_group_member(
        &mut self,
        member_principal_id: PrincipalId,
        valid_from: DateTime<Utc>,
        valid_until: DateTime<Utc>,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityAccessDomainError> {
        if !matches!(
            self.kind,
            PrincipalKind::HumanGroup | PrincipalKind::AgentPool
        ) || self.status != PrincipalStatus::Active
            || at < self.updated_at
        {
            return Err(IdentityAccessDomainError::InvalidGroupMembership);
        }
        if self.group_memberships.iter().any(|membership| {
            membership.member_principal_id == member_principal_id && membership.is_active_at(at)
        }) {
            return Err(IdentityAccessDomainError::DuplicateGroupMembership);
        }
        let membership =
            GroupMembership::new(self.id, member_principal_id, valid_from, valid_until)?;
        let mut candidate = self.clone();
        candidate.version = next_version(candidate.version)?;
        candidate.group_memberships.push(membership);
        candidate.updated_at = at;
        *self = candidate;
        self.push_access_changed(at);
        Ok(())
    }

    pub fn revoke_group_member(
        &mut self,
        member_principal_id: PrincipalId,
        at: DateTime<Utc>,
    ) -> Result<(), IdentityAccessDomainError> {
        if !matches!(
            self.kind,
            PrincipalKind::HumanGroup | PrincipalKind::AgentPool
        ) || at < self.updated_at
        {
            return Err(IdentityAccessDomainError::InvalidGroupMembership);
        }
        let mut candidate = self.clone();
        let membership = candidate
            .group_memberships
            .iter_mut()
            .find(|membership| {
                membership.member_principal_id == member_principal_id
                    && membership.revoked_at.is_none()
            })
            .ok_or(IdentityAccessDomainError::InvalidGroupMembership)?;
        membership.revoke(at)?;
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

    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }

    pub fn display_name(&self) -> &str {
        &self.display_name
    }

    pub const fn kind(&self) -> PrincipalKind {
        self.kind
    }

    pub const fn realm(&self) -> PrincipalRealm {
        self.realm
    }

    pub const fn non_human_governance(&self) -> Option<&NonHumanPrincipalGovernance> {
        self.non_human_governance.as_ref()
    }

    pub const fn status(&self) -> PrincipalStatus {
        self.status
    }

    pub fn assignments(&self) -> &[RoleAssignment] {
        &self.assignments
    }

    pub fn group_memberships(&self) -> &[GroupMembership] {
        &self.group_memberships
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

    fn scope() -> AuthorityScope {
        AuthorityScope::Holding { holding_id: id(1) }
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
            AuthorityScope::Company {
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
            AuthorityScope::Holding { holding_id: id(99) },
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

    #[test]
    fn role_renewal_revokes_history_and_creates_one_exact_replacement() {
        let mut principal = principal();
        let original_id = RoleAssignmentId::try_from_uuid(id(10)).unwrap();
        let replacement_id = RoleAssignmentId::try_from_uuid(id(11)).unwrap();
        principal
            .assign_role(assignment(10, 20, RiskTier::Standard, None).unwrap(), at(9))
            .unwrap();
        let version_before = principal.version();

        principal
            .renew_role(
                original_id,
                replacement_id,
                at(11),
                "renewing-grantor".into(),
                None,
                at(9),
            )
            .unwrap();

        assert_eq!(principal.version(), version_before + 1);
        assert_eq!(principal.assignments().len(), 2);
        assert_eq!(principal.assignments()[0].revoked_at(), Some(at(9)));
        assert_eq!(principal.assignments()[1].id(), replacement_id);
        assert!(principal.assignments()[1].is_active_at(at(10)));
        let before_duplicate = principal.clone();
        assert_eq!(
            principal.renew_role(
                replacement_id,
                original_id,
                at(11),
                "renewing-grantor".into(),
                None,
                at(9),
            ),
            Err(IdentityAccessDomainError::DuplicateActiveAssignment)
        );
        assert_eq!(principal, before_duplicate);
    }

    #[test]
    fn service_and_agent_principals_require_accountable_governance() {
        let governance = NonHumanPrincipalGovernance::new(
            "platform-owner".into(),
            "project summaries".into(),
            "worker:agent-runner".into(),
            "secret://agent/runtime".into(),
        )
        .unwrap();
        let agent = AccessPrincipal::create_non_human(
            PrincipalId::try_from_uuid(id(30)).unwrap(),
            scope(),
            "Project assistant".into(),
            PrincipalKind::Agent,
            governance,
            at(8),
        )
        .unwrap();
        assert_eq!(agent.kind(), PrincipalKind::Agent);
        assert_eq!(
            agent.non_human_governance().unwrap().owner_ref(),
            "platform-owner"
        );

        let human_profile = NonHumanPrincipalGovernance::new(
            "owner".into(),
            "purpose".into(),
            "workload".into(),
            "secret://credential".into(),
        )
        .unwrap();
        assert_eq!(
            AccessPrincipal::create_non_human(
                PrincipalId::try_from_uuid(id(31)).unwrap(),
                scope(),
                "Invalid human".into(),
                PrincipalKind::Human,
                human_profile,
                at(8),
            ),
            Err(IdentityAccessDomainError::InvalidPrincipalGovernance)
        );
    }
}
