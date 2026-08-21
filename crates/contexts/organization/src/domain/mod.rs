use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

mod sharing;

pub use sharing::{
    DirectoryVisibility, OrganizationSharingAgreement, ProposeSharingAgreement, SharingAgreementId,
    SharingApprovalSide, SharingCapability, SharingDirection, SharingMutation, SharingStatus,
    can_discover_across_hierarchy, sharing_approval_decision_ref,
};

pub const MAX_ORGANIZATION_CODE_BYTES: usize = 128;
pub const MAX_ORGANIZATION_NAME_BYTES: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct OrganizationNodeId(Uuid);

impl OrganizationNodeId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, OrganizationDomainError> {
        require_id(value, "organization_node_id")?;
        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl TryFrom<Uuid> for OrganizationNodeId {
    type Error = OrganizationDomainError;

    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        Self::try_from_uuid(value)
    }
}

impl From<OrganizationNodeId> for Uuid {
    fn from(value: OrganizationNodeId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationNodeKind {
    Holding,
    Company,
    Headquarters,
    Branch,
    Department,
    Team,
}

/// Structural lineage for an organization node. The enum makes illegal
/// combinations such as a branch without a company unrepresentable.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum OrganizationPlacement {
    Holding,
    Company {
        holding_id: Uuid,
    },
    Headquarters {
        holding_id: Uuid,
        company_id: Uuid,
    },
    Branch {
        holding_id: Uuid,
        company_id: Uuid,
    },
    Department {
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
    },
    Team {
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
        department_id: Uuid,
    },
}

impl OrganizationPlacement {
    pub const fn kind(&self) -> OrganizationNodeKind {
        match self {
            Self::Holding => OrganizationNodeKind::Holding,
            Self::Company { .. } => OrganizationNodeKind::Company,
            Self::Headquarters { .. } => OrganizationNodeKind::Headquarters,
            Self::Branch { .. } => OrganizationNodeKind::Branch,
            Self::Department { .. } => OrganizationNodeKind::Department,
            Self::Team { .. } => OrganizationNodeKind::Team,
        }
    }

    pub const fn holding_id(&self, node_id: OrganizationNodeId) -> Uuid {
        match self {
            Self::Holding => node_id.as_uuid(),
            Self::Company { holding_id }
            | Self::Headquarters { holding_id, .. }
            | Self::Branch { holding_id, .. }
            | Self::Department { holding_id, .. }
            | Self::Team { holding_id, .. } => *holding_id,
        }
    }

    pub const fn company_id(&self, node_id: OrganizationNodeId) -> Option<Uuid> {
        match self {
            Self::Holding => None,
            Self::Company { .. } => Some(node_id.as_uuid()),
            Self::Headquarters { company_id, .. }
            | Self::Branch { company_id, .. }
            | Self::Department { company_id, .. }
            | Self::Team { company_id, .. } => Some(*company_id),
        }
    }

    pub const fn branch_id(&self, node_id: OrganizationNodeId) -> Option<Uuid> {
        match self {
            Self::Branch { .. } => Some(node_id.as_uuid()),
            Self::Department { branch_id, .. } | Self::Team { branch_id, .. } => *branch_id,
            Self::Holding | Self::Company { .. } | Self::Headquarters { .. } => None,
        }
    }

    pub const fn department_id(&self, node_id: OrganizationNodeId) -> Option<Uuid> {
        match self {
            Self::Department { .. } => Some(node_id.as_uuid()),
            Self::Team { department_id, .. } => Some(*department_id),
            Self::Holding
            | Self::Company { .. }
            | Self::Headquarters { .. }
            | Self::Branch { .. } => None,
        }
    }

    pub fn parent_id(&self, node_id: OrganizationNodeId) -> Option<OrganizationNodeId> {
        let parent = match self {
            Self::Holding => return None,
            Self::Company { holding_id } => *holding_id,
            Self::Headquarters { company_id, .. } | Self::Branch { company_id, .. } => *company_id,
            Self::Department {
                company_id,
                branch_id,
                ..
            } => branch_id.unwrap_or(*company_id),
            Self::Team { department_id, .. } => *department_id,
        };
        debug_assert_ne!(parent, node_id.as_uuid());
        OrganizationNodeId::try_from_uuid(parent).ok()
    }

    pub fn active_scope(
        &self,
        node_id: OrganizationNodeId,
    ) -> Result<AuthorityScope, OrganizationDomainError> {
        self.validate_for(node_id)?;
        let holding_id = self.holding_id(node_id);
        match self {
            Self::Holding => Ok(AuthorityScope::Holding { holding_id }),
            Self::Company { .. } | Self::Headquarters { .. } => Ok(AuthorityScope::Company {
                holding_id,
                company_id: self
                    .company_id(node_id)
                    .expect("company placement always has a company"),
            }),
            Self::Branch { .. } => Ok(AuthorityScope::Branch {
                holding_id,
                company_id: self
                    .company_id(node_id)
                    .expect("branch placement always has a company"),
                branch_id: node_id.as_uuid(),
            }),
            Self::Department { .. } | Self::Team { .. } => Ok(AuthorityScope::Subtree {
                holding_id,
                organization_node_id: node_id.as_uuid(),
            }),
        }
    }

    /// Checks an authority against this node's typed placement. Unlike
    /// [`AuthorityScope::contains`], this method has the organization lineage
    /// required to decide whether company, branch, or subtree authority covers
    /// an exact department/team scope.
    pub fn is_within_scope(
        &self,
        node_id: OrganizationNodeId,
        authority_scope: &AuthorityScope,
    ) -> Result<bool, OrganizationDomainError> {
        self.validate_for(node_id)?;
        if authority_scope.validate().is_err() {
            return Err(OrganizationDomainError::InvalidScope);
        }

        let holding_id = self.holding_id(node_id);
        let company_id = self.company_id(node_id);
        let branch_id = self.branch_id(node_id);
        let department_id = self.department_id(node_id);
        Ok(match authority_scope {
            AuthorityScope::Platform => true,
            AuthorityScope::Holding {
                holding_id: authority_holding,
            } => *authority_holding == holding_id,
            AuthorityScope::Company {
                holding_id: authority_holding,
                company_id: authority_company,
            } => *authority_holding == holding_id && company_id == Some(*authority_company),
            AuthorityScope::Branch {
                holding_id: authority_holding,
                company_id: authority_company,
                branch_id: authority_branch,
            } => {
                *authority_holding == holding_id
                    && company_id == Some(*authority_company)
                    && branch_id == Some(*authority_branch)
            }
            AuthorityScope::Subtree {
                holding_id: authority_holding,
                organization_node_id,
            } => {
                *authority_holding == holding_id
                    && matches!(self, Self::Department { .. } | Self::Team { .. })
                    && (*organization_node_id == node_id.as_uuid()
                        || (matches!(self, Self::Team { .. })
                            && department_id == Some(*organization_node_id)))
            }
        })
    }

    /// Checks authority over this placement's prospective parent. The node
    /// itself is deliberately excluded, so an exact self-subtree grant cannot
    /// be used to move that node beneath an otherwise unauthorized parent.
    pub fn is_parent_within_scope(
        &self,
        node_id: OrganizationNodeId,
        authority_scope: &AuthorityScope,
    ) -> Result<bool, OrganizationDomainError> {
        self.validate_for(node_id)?;
        if authority_scope.validate().is_err() {
            return Err(OrganizationDomainError::InvalidScope);
        }

        let (holding_id, company_id, branch_id, department_id) = match self {
            Self::Holding => return Ok(false),
            Self::Company { holding_id } => (*holding_id, None, None, None),
            Self::Headquarters {
                holding_id,
                company_id,
            }
            | Self::Branch {
                holding_id,
                company_id,
            } => (*holding_id, Some(*company_id), None, None),
            Self::Department {
                holding_id,
                company_id,
                branch_id,
            } => (*holding_id, Some(*company_id), *branch_id, None),
            Self::Team {
                holding_id,
                company_id,
                branch_id,
                department_id,
            } => (
                *holding_id,
                Some(*company_id),
                *branch_id,
                Some(*department_id),
            ),
        };
        Ok(match authority_scope {
            AuthorityScope::Platform => true,
            AuthorityScope::Holding {
                holding_id: authority_holding,
            } => *authority_holding == holding_id,
            AuthorityScope::Company {
                holding_id: authority_holding,
                company_id: authority_company,
            } => *authority_holding == holding_id && company_id == Some(*authority_company),
            AuthorityScope::Branch {
                holding_id: authority_holding,
                company_id: authority_company,
                branch_id: authority_branch,
            } => {
                *authority_holding == holding_id
                    && company_id == Some(*authority_company)
                    && branch_id == Some(*authority_branch)
            }
            AuthorityScope::Subtree {
                holding_id: authority_holding,
                organization_node_id,
            } => {
                *authority_holding == holding_id
                    && matches!(self, Self::Team { .. })
                    && department_id == Some(*organization_node_id)
            }
        })
    }

    pub fn validate_for(&self, node_id: OrganizationNodeId) -> Result<(), OrganizationDomainError> {
        let id = node_id.as_uuid();
        let ids: Vec<(&'static str, Uuid)> = match self {
            Self::Holding => Vec::new(),
            Self::Company { holding_id } => vec![("holding_id", *holding_id)],
            Self::Headquarters {
                holding_id,
                company_id,
            }
            | Self::Branch {
                holding_id,
                company_id,
            } => vec![("holding_id", *holding_id), ("company_id", *company_id)],
            Self::Department {
                holding_id,
                company_id,
                branch_id,
            } => {
                let mut ids = vec![("holding_id", *holding_id), ("company_id", *company_id)];
                if let Some(branch_id) = branch_id {
                    ids.push(("branch_id", *branch_id));
                }
                ids
            }
            Self::Team {
                holding_id,
                company_id,
                branch_id,
                department_id,
            } => {
                let mut ids = vec![
                    ("holding_id", *holding_id),
                    ("company_id", *company_id),
                    ("department_id", *department_id),
                ];
                if let Some(branch_id) = branch_id {
                    ids.push(("branch_id", *branch_id));
                }
                ids
            }
        };
        for (field, value) in &ids {
            require_id(*value, field)?;
            if *value == id {
                return Err(OrganizationDomainError::SelfParenting);
            }
        }
        for (index, (_, value)) in ids.iter().enumerate() {
            if ids.iter().skip(index + 1).any(|(_, other)| value == other) {
                return Err(OrganizationDomainError::DuplicateLineageIdentifier);
            }
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationNodeStatus {
    Active,
    Suspended,
    Archived,
}

impl OrganizationNodeStatus {
    const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Active, Self::Suspended | Self::Archived)
                | (Self::Suspended, Self::Active | Self::Archived)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum OrganizationDomainError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {maximum} bytes")]
    TooLong { field: &'static str, maximum: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("an organization node cannot be its own ancestor")]
    SelfParenting,
    #[error("organization lineage identifiers must be distinct")]
    DuplicateLineageIdentifier,
    #[error("organization scope does not exactly match its hierarchy placement")]
    InvalidScope,
    #[error("organization node kind cannot change while reparenting")]
    KindMismatch,
    #[error("the root holding cannot be reparented")]
    RootCannotMove,
    #[error("archived organization nodes cannot be changed")]
    Archived,
    #[error("organization mutation would not change the aggregate")]
    NoChange,
    #[error("organization lifecycle transition is not allowed: {from:?} -> {to:?}")]
    InvalidTransition {
        from: OrganizationNodeStatus,
        to: OrganizationNodeStatus,
    },
    #[error("organization timestamp window is invalid")]
    InvalidWindow,
    #[error("organization node version overflowed")]
    VersionOverflow,
    #[error("organization sharing agreement must connect two branch scopes")]
    SharingRequiresBranches,
    #[error("cross-holding or cross-holding sharing is denied")]
    CrossHoldingSharingDenied,
    #[error("an organization scope cannot share with itself")]
    SelfSharingDenied,
    #[error("sharing agreement must contain unique capabilities")]
    InvalidSharingCapabilities,
    #[error("sharing agreement validity window is invalid")]
    InvalidSharingWindow,
    #[error("sharing agreement approval is conflicted or invalid")]
    InvalidSharingApproval,
    #[error("sharing agreement lifecycle transition is not allowed")]
    InvalidSharingTransition,
    #[error("sharing agreement version conflict (expected {expected}, actual {actual})")]
    SharingVersionConflict { expected: u64, actual: u64 },
}

/// Authoritative organization node. Scope is stored as a consistency witness
/// and must always equal the scope derived from placement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationNode {
    id: OrganizationNodeId,
    version: u64,
    code: String,
    name: String,
    placement: OrganizationPlacement,
    scope: AuthorityScope,
    status: OrganizationNodeStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

/// Typed persistence state for deterministic rehydration from the
/// Organization-owned relational write model. Event buffers and serialized
/// aggregate documents are deliberately excluded.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OrganizationNodeState {
    pub id: OrganizationNodeId,
    pub version: u64,
    pub code: String,
    pub name: String,
    pub placement: OrganizationPlacement,
    pub status: OrganizationNodeStatus,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl OrganizationNode {
    pub fn try_rehydrate(state: OrganizationNodeState) -> Result<Self, OrganizationDomainError> {
        let scope = state.placement.active_scope(state.id)?;
        let value = Self {
            id: state.id,
            version: state.version,
            code: state.code,
            name: state.name,
            placement: state.placement,
            scope,
            status: state.status,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    pub fn create(
        id: OrganizationNodeId,
        placement: OrganizationPlacement,
        code: String,
        name: String,
        at: DateTime<Utc>,
    ) -> Result<Self, OrganizationDomainError> {
        let scope = placement.active_scope(id)?;
        let mut value = Self {
            id,
            version: 1,
            code,
            name,
            placement,
            scope,
            status: OrganizationNodeStatus::Active,
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        value.push_changed_event(at);
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), OrganizationDomainError> {
        require_id(self.id.as_uuid(), "organization_node_id")?;
        if self.version == 0 {
            return Err(OrganizationDomainError::Required { field: "version" });
        }
        validate_text(&self.code, "code", MAX_ORGANIZATION_CODE_BYTES)?;
        validate_text(&self.name, "name", MAX_ORGANIZATION_NAME_BYTES)?;
        let expected_scope = self.placement.active_scope(self.id)?;
        if expected_scope != self.scope || self.scope.validate().is_err() {
            return Err(OrganizationDomainError::InvalidScope);
        }
        if self.updated_at < self.created_at {
            return Err(OrganizationDomainError::InvalidWindow);
        }
        Ok(())
    }

    pub fn rename(
        &mut self,
        name: String,
        at: DateTime<Utc>,
    ) -> Result<(), OrganizationDomainError> {
        self.ensure_mutable(at)?;
        if self.name == name {
            return Err(OrganizationDomainError::NoChange);
        }
        let mut candidate = self.clone();
        candidate.version = next_version(candidate.version)?;
        candidate.name = name;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        self.push_changed_event(at);
        Ok(())
    }

    pub fn change_code(
        &mut self,
        code: String,
        at: DateTime<Utc>,
    ) -> Result<(), OrganizationDomainError> {
        self.ensure_mutable(at)?;
        if self.code == code {
            return Err(OrganizationDomainError::NoChange);
        }
        let mut candidate = self.clone();
        candidate.version = next_version(candidate.version)?;
        candidate.code = code;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        self.push_changed_event(at);
        Ok(())
    }

    pub fn reparent(
        &mut self,
        placement: OrganizationPlacement,
        at: DateTime<Utc>,
    ) -> Result<(), OrganizationDomainError> {
        self.ensure_mutable(at)?;
        if self.kind() == OrganizationNodeKind::Holding {
            return Err(OrganizationDomainError::RootCannotMove);
        }
        if placement.kind() != self.kind() {
            return Err(OrganizationDomainError::KindMismatch);
        }
        if placement == self.placement {
            return Err(OrganizationDomainError::NoChange);
        }
        let scope = placement.active_scope(self.id)?;
        let mut candidate = self.clone();
        candidate.version = next_version(candidate.version)?;
        candidate.placement = placement;
        candidate.scope = scope;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        self.push_changed_event(at);
        Ok(())
    }

    pub fn suspend(&mut self, at: DateTime<Utc>) -> Result<(), OrganizationDomainError> {
        self.transition(OrganizationNodeStatus::Suspended, at)
    }

    pub fn activate(&mut self, at: DateTime<Utc>) -> Result<(), OrganizationDomainError> {
        self.transition(OrganizationNodeStatus::Active, at)
    }

    pub fn archive(&mut self, at: DateTime<Utc>) -> Result<(), OrganizationDomainError> {
        self.transition(OrganizationNodeStatus::Archived, at)
    }

    fn transition(
        &mut self,
        next: OrganizationNodeStatus,
        at: DateTime<Utc>,
    ) -> Result<(), OrganizationDomainError> {
        if !self.status.can_transition_to(next) {
            return Err(OrganizationDomainError::InvalidTransition {
                from: self.status,
                to: next,
            });
        }
        if at < self.updated_at {
            return Err(OrganizationDomainError::InvalidWindow);
        }
        let mut candidate = self.clone();
        candidate.version = next_version(candidate.version)?;
        candidate.status = next;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        if next == OrganizationNodeStatus::Archived {
            self.uncommitted_events.push(DomainEvent::Retired {
                node_id: self.id,
                at,
            });
        } else {
            self.push_changed_event(at);
        }
        Ok(())
    }

    fn ensure_mutable(&self, at: DateTime<Utc>) -> Result<(), OrganizationDomainError> {
        if self.status == OrganizationNodeStatus::Archived {
            return Err(OrganizationDomainError::Archived);
        }
        if at < self.updated_at {
            return Err(OrganizationDomainError::InvalidWindow);
        }
        Ok(())
    }

    fn push_changed_event(&mut self, at: DateTime<Utc>) {
        self.uncommitted_events.push(DomainEvent::Changed {
            state: PublishedNodeState {
                node_id: self.id,
                code: self.code.clone(),
                name: self.name.clone(),
                placement: self.placement.clone(),
                active: self.status == OrganizationNodeStatus::Active,
            },
            at,
        });
    }

    pub const fn id(&self) -> OrganizationNodeId {
        self.id
    }

    pub const fn version(&self) -> u64 {
        self.version
    }

    pub fn code(&self) -> &str {
        &self.code
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    pub const fn placement(&self) -> &OrganizationPlacement {
        &self.placement
    }

    pub const fn scope(&self) -> &AuthorityScope {
        &self.scope
    }

    pub const fn status(&self) -> OrganizationNodeStatus {
        self.status
    }

    pub const fn kind(&self) -> OrganizationNodeKind {
        self.placement.kind()
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

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct PublishedNodeState {
    pub node_id: OrganizationNodeId,
    pub code: String,
    pub name: String,
    pub placement: OrganizationPlacement,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    Changed {
        state: PublishedNodeState,
        at: DateTime<Utc>,
    },
    Retired {
        node_id: OrganizationNodeId,
        at: DateTime<Utc>,
    },
}

fn require_id(id: Uuid, field: &'static str) -> Result<(), OrganizationDomainError> {
    if id.is_nil() {
        Err(OrganizationDomainError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn validate_text(
    value: &str,
    field: &'static str,
    maximum: usize,
) -> Result<(), OrganizationDomainError> {
    if value.trim().is_empty() {
        return Err(OrganizationDomainError::Required { field });
    }
    if value.len() > maximum {
        return Err(OrganizationDomainError::TooLong { field, maximum });
    }
    if value.chars().any(char::is_control) {
        return Err(OrganizationDomainError::ControlCharacter { field });
    }
    Ok(())
}

fn next_version(version: u64) -> Result<u64, OrganizationDomainError> {
    version
        .checked_add(1)
        .ok_or(OrganizationDomainError::VersionOverflow)
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn node_id(value: u128) -> OrganizationNodeId {
        OrganizationNodeId::try_from_uuid(id(value)).unwrap()
    }

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, 9, minute, 0)
            .single()
            .unwrap()
    }

    fn department() -> OrganizationNode {
        OrganizationNode::create(
            node_id(4),
            OrganizationPlacement::Department {
                holding_id: id(1),
                company_id: id(2),
                branch_id: Some(id(3)),
            },
            "OPS".into(),
            "Operations".into(),
            at(0),
        )
        .unwrap()
    }

    #[test]
    fn placement_derives_exact_scope_and_parent() {
        let node = department();
        assert_eq!(node.kind(), OrganizationNodeKind::Department);
        assert_eq!(
            node.scope(),
            &AuthorityScope::Subtree {
                holding_id: id(1),
                organization_node_id: id(4),
            }
        );
        assert_eq!(node.placement().parent_id(node.id()), Some(node_id(3)));
    }

    #[test]
    fn typed_placement_resolves_hierarchical_authority_without_widening_scope() {
        let team_id = node_id(5);
        let placement = OrganizationPlacement::Team {
            holding_id: id(1),
            company_id: id(2),
            branch_id: Some(id(3)),
            department_id: id(4),
        };
        assert_eq!(
            placement.active_scope(team_id).unwrap(),
            AuthorityScope::Subtree {
                holding_id: id(1),
                organization_node_id: id(5),
            }
        );
        assert!(
            placement
                .is_within_scope(
                    team_id,
                    &AuthorityScope::Company {
                        holding_id: id(1),
                        company_id: id(2),
                    },
                )
                .unwrap()
        );
        assert!(
            placement
                .is_within_scope(
                    team_id,
                    &AuthorityScope::Branch {
                        holding_id: id(1),
                        company_id: id(2),
                        branch_id: id(3),
                    },
                )
                .unwrap()
        );
        assert!(
            placement
                .is_within_scope(
                    team_id,
                    &AuthorityScope::Subtree {
                        holding_id: id(1),
                        organization_node_id: id(4),
                    },
                )
                .unwrap()
        );
        for noncanonical_ancestor in [id(1), id(2), id(3)] {
            assert!(
                !placement
                    .is_within_scope(
                        team_id,
                        &AuthorityScope::Subtree {
                            holding_id: id(1),
                            organization_node_id: noncanonical_ancestor,
                        },
                    )
                    .unwrap()
            );
        }
        assert!(
            !placement
                .is_within_scope(
                    team_id,
                    &AuthorityScope::Branch {
                        holding_id: id(1),
                        company_id: id(2),
                        branch_id: id(9),
                    },
                )
                .unwrap()
        );
        assert!(
            !placement
                .is_within_scope(
                    team_id,
                    &AuthorityScope::Subtree {
                        holding_id: id(1),
                        organization_node_id: id(6),
                    },
                )
                .unwrap()
        );
    }

    #[test]
    fn prospective_parent_excludes_self_and_accepts_destination_authority() {
        let department_id = node_id(4);
        let destination = OrganizationPlacement::Department {
            holding_id: id(1),
            company_id: id(2),
            branch_id: Some(id(5)),
        };
        assert!(
            !destination
                .is_parent_within_scope(
                    department_id,
                    &AuthorityScope::Subtree {
                        holding_id: id(1),
                        organization_node_id: id(4),
                    },
                )
                .unwrap()
        );
        assert!(
            destination
                .is_parent_within_scope(
                    department_id,
                    &AuthorityScope::Branch {
                        holding_id: id(1),
                        company_id: id(2),
                        branch_id: id(5),
                    },
                )
                .unwrap()
        );
        assert!(
            destination
                .is_parent_within_scope(
                    department_id,
                    &AuthorityScope::Company {
                        holding_id: id(1),
                        company_id: id(2),
                    },
                )
                .unwrap()
        );
        assert!(
            !destination
                .is_parent_within_scope(
                    department_id,
                    &AuthorityScope::Branch {
                        holding_id: id(1),
                        company_id: id(2),
                        branch_id: id(3),
                    },
                )
                .unwrap()
        );
    }

    #[test]
    fn non_leaf_node_ids_cannot_be_reinterpreted_as_subtree_authority() {
        let cases = [
            (OrganizationPlacement::Holding, node_id(1)),
            (
                OrganizationPlacement::Company { holding_id: id(1) },
                node_id(2),
            ),
            (
                OrganizationPlacement::Headquarters {
                    holding_id: id(1),
                    company_id: id(2),
                },
                node_id(3),
            ),
            (
                OrganizationPlacement::Branch {
                    holding_id: id(1),
                    company_id: id(2),
                },
                node_id(4),
            ),
        ];
        for (placement, target) in cases {
            assert!(
                !placement
                    .is_within_scope(
                        target,
                        &AuthorityScope::Subtree {
                            holding_id: id(1),
                            organization_node_id: target.as_uuid(),
                        },
                    )
                    .unwrap()
            );
        }
    }

    #[test]
    fn placement_rejects_self_parenting_and_duplicate_lineage() {
        let self_parent = OrganizationPlacement::Branch {
            holding_id: id(1),
            company_id: id(3),
        };
        assert_eq!(
            self_parent.validate_for(node_id(3)),
            Err(OrganizationDomainError::SelfParenting)
        );
        let duplicate = OrganizationPlacement::Department {
            holding_id: id(1),
            company_id: id(2),
            branch_id: Some(id(2)),
        };
        assert_eq!(
            duplicate.validate_for(node_id(4)),
            Err(OrganizationDomainError::DuplicateLineageIdentifier)
        );
    }

    #[test]
    fn lifecycle_supports_suspend_reactivate_and_terminal_archive() {
        let mut node = department();
        node.suspend(at(1)).unwrap();
        node.activate(at(2)).unwrap();
        node.archive(at(3)).unwrap();
        assert_eq!(node.status(), OrganizationNodeStatus::Archived);
        assert_eq!(node.version(), 4);
        assert_eq!(
            node.rename("Other".into(), at(4)),
            Err(OrganizationDomainError::Archived)
        );
        assert!(matches!(
            node.activate(at(4)),
            Err(OrganizationDomainError::InvalidTransition { .. })
        ));
    }

    #[test]
    fn holding_root_can_reactivate_without_a_parent() {
        let mut holding = OrganizationNode::create(
            node_id(1),
            OrganizationPlacement::Holding,
            "HOLDING".into(),
            "Holding".into(),
            at(0),
        )
        .unwrap();

        holding.suspend(at(1)).unwrap();
        holding.activate(at(2)).unwrap();

        assert_eq!(holding.status(), OrganizationNodeStatus::Active);
        assert_eq!(holding.version(), 3);
        assert_eq!(holding.placement().parent_id(holding.id()), None);
    }

    #[test]
    fn rejected_reparent_does_not_mutate_state_or_events() {
        let mut node = department();
        node.take_uncommitted_events();
        let before = node.clone();
        let error = node.reparent(
            OrganizationPlacement::Team {
                holding_id: id(1),
                company_id: id(2),
                branch_id: Some(id(3)),
                department_id: id(5),
            },
            at(1),
        );
        assert_eq!(error, Err(OrganizationDomainError::KindMismatch));
        assert_eq!(node, before);
    }

    #[test]
    fn reparent_updates_scope_atomically() {
        let mut node = department();
        node.reparent(
            OrganizationPlacement::Department {
                holding_id: id(1),
                company_id: id(2),
                branch_id: Some(id(5)),
            },
            at(1),
        )
        .unwrap();
        assert_eq!(
            node.scope(),
            &AuthorityScope::Subtree {
                holding_id: id(1),
                organization_node_id: id(4),
            }
        );
        assert_eq!(node.version(), 2);
    }

    #[test]
    fn typed_rehydration_derives_scope_and_rejects_corrupt_state() {
        let mut source = department();
        source.take_uncommitted_events();
        let state = OrganizationNodeState {
            id: source.id(),
            version: source.version(),
            code: source.code().into(),
            name: source.name().into(),
            placement: source.placement().clone(),
            status: source.status(),
            created_at: source.created_at(),
            updated_at: source.updated_at(),
        };
        assert_eq!(
            OrganizationNode::try_rehydrate(state.clone()).unwrap(),
            source
        );

        let mut corrupt = state;
        corrupt.version = 0;
        assert!(OrganizationNode::try_rehydrate(corrupt).is_err());
    }
}
