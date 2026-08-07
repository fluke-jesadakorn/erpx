use chrono::{DateTime, Utc};
use erpx_kernel::TenantScope;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

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

    pub fn tenant_scope(
        &self,
        node_id: OrganizationNodeId,
    ) -> Result<TenantScope, OrganizationDomainError> {
        self.validate_for(node_id)?;
        let holding_id = self.holding_id(node_id);
        match self {
            Self::Holding => Ok(TenantScope::Holding { holding_id }),
            Self::Company { .. } | Self::Headquarters { .. } => Ok(TenantScope::Company {
                holding_id,
                company_id: self
                    .company_id(node_id)
                    .expect("company placement always has a company"),
            }),
            Self::Branch { .. } => Ok(TenantScope::Branch {
                holding_id,
                company_id: self
                    .company_id(node_id)
                    .expect("branch placement always has a company"),
                branch_id: node_id.as_uuid(),
            }),
            Self::Department {
                company_id,
                branch_id,
                ..
            }
            | Self::Team {
                company_id,
                branch_id,
                ..
            } => branch_id.map_or_else(
                || {
                    Ok(TenantScope::Company {
                        holding_id,
                        company_id: *company_id,
                    })
                },
                |branch_id| {
                    Ok(TenantScope::Branch {
                        holding_id,
                        company_id: *company_id,
                        branch_id,
                    })
                },
            ),
        }
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
    scope: TenantScope,
    status: OrganizationNodeStatus,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

impl OrganizationNode {
    pub fn create(
        id: OrganizationNodeId,
        placement: OrganizationPlacement,
        code: String,
        name: String,
        at: DateTime<Utc>,
    ) -> Result<Self, OrganizationDomainError> {
        let scope = placement.tenant_scope(id)?;
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
        let expected_scope = self.placement.tenant_scope(self.id)?;
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
        let scope = placement.tenant_scope(self.id)?;
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

    pub const fn scope(&self) -> &TenantScope {
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
            &TenantScope::Branch {
                holding_id: id(1),
                company_id: id(2),
                branch_id: id(3),
            }
        );
        assert_eq!(node.placement().parent_id(node.id()), Some(node_id(3)));
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
            &TenantScope::Branch {
                holding_id: id(1),
                company_id: id(2),
                branch_id: id(5),
            }
        );
        assert_eq!(node.version(), 2);
    }
}
