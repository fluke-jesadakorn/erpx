//! Organization-owned facts and a transport-neutral hierarchy witness.
//!
//! [`AuthorityScope::contains`](erpx_kernel::AuthorityScope::contains) can
//! decide containment when all required ancestry is embedded in the two
//! scopes. An exact `Subtree` scope deliberately carries only a node ID, so
//! consumers must use an Organization-authored [`OrganizationLineageWitness`]
//! instead of inferring ancestry from UUID equality.

use std::{error::Error, fmt};

use erpx_kernel::{AuthorityScope, HoldingId};
use serde::{Deserialize, Deserializer, Serialize, de};
use uuid::Uuid;

use crate::{
    BoundedContext, ContractValidationError, DisplayName, IntegrationEvent, ReferenceCode,
};

/// Maximum supported root-to-node path, including both the Holding and target.
pub const MAX_ORGANIZATION_LINEAGE_DEPTH: usize = 5;
pub const ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE: &str = "organization.lineage_changed";
pub const ORGANIZATION_LINEAGE_CHANGED_VERSION: u16 = 1;

owned_uuid_reference!(CompanyRef);
owned_uuid_reference!(BranchRef);
owned_uuid_reference!(BusinessUnitRef);

/// A stable lookup key for request-local deduplication and batched hierarchy
/// resolution. Node UUIDs are only unique within their declared holding at
/// this boundary; consumers must never drop the holding component.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct OrganizationLineageKey {
    holding_id: HoldingId,
    node_id: BusinessUnitRef,
}

impl OrganizationLineageKey {
    pub fn try_new(
        holding_id: Uuid,
        node_id: Uuid,
    ) -> Result<Self, OrganizationLineageValidationError> {
        Ok(Self {
            holding_id: HoldingId::try_from_uuid(holding_id)
                .map_err(|_| OrganizationLineageValidationError::NilIdentifier("holding_id"))?,
            node_id: BusinessUnitRef::try_from_uuid(node_id)
                .map_err(|_| OrganizationLineageValidationError::NilIdentifier("node_id"))?,
        })
    }

    /// Returns the organization node directly named by a non-platform scope.
    /// A separate normalized ownership key is still required when a record is
    /// attached to a node (such as Headquarters) whose active scope names an
    /// ancestor rather than that node itself.
    pub fn try_from_scope(
        scope: &AuthorityScope,
    ) -> Result<Option<Self>, OrganizationLineageValidationError> {
        scope
            .validate()
            .map_err(|_| OrganizationLineageValidationError::InvalidScope)?;
        let value = match scope {
            AuthorityScope::Platform => return Ok(None),
            AuthorityScope::Holding { holding_id } => Self::try_new(*holding_id, *holding_id)?,
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Self::try_new(*holding_id, *company_id)?,
            AuthorityScope::Branch {
                holding_id,
                branch_id,
                ..
            } => Self::try_new(*holding_id, *branch_id)?,
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Self::try_new(*holding_id, *organization_node_id)?,
        };
        Ok(Some(value))
    }

    pub const fn holding_id(self) -> HoldingId {
        self.holding_id
    }

    pub const fn node_id(self) -> BusinessUnitRef {
        self.node_id
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationNodeKind {
    Holding,
    Company,
    Headquarters,
    Branch,
    Department,
    Team,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OrganizationNodeStatus {
    Active,
    Suspended,
    Archived,
}

/// Organization-authored, version-pinned proof of one node's current path.
///
/// `lineage_node_ids` is ordered from Holding through the direct parent and
/// excludes `key.node_id`. Its shape is closed and bounded by ERPX's current
/// hierarchy grammar, so a malformed or future deeper hierarchy fails closed
/// until this contract is deliberately versioned.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct OrganizationLineageWitness {
    key: OrganizationLineageKey,
    kind: OrganizationNodeKind,
    status: OrganizationNodeStatus,
    parent_node_id: Option<BusinessUnitRef>,
    lineage_node_ids: Vec<BusinessUnitRef>,
    aggregate_version: u64,
}

impl OrganizationLineageWitness {
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        key: OrganizationLineageKey,
        kind: OrganizationNodeKind,
        status: OrganizationNodeStatus,
        parent_node_id: Option<Uuid>,
        lineage_node_ids: Vec<Uuid>,
        aggregate_version: u64,
    ) -> Result<Self, OrganizationLineageValidationError> {
        let parent_node_id = parent_node_id
            .map(|value| {
                BusinessUnitRef::try_from_uuid(value).map_err(|_| {
                    OrganizationLineageValidationError::NilIdentifier("parent_node_id")
                })
            })
            .transpose()?;
        let lineage_node_ids = lineage_node_ids
            .into_iter()
            .map(|value| {
                BusinessUnitRef::try_from_uuid(value).map_err(|_| {
                    OrganizationLineageValidationError::NilIdentifier("lineage_node_ids")
                })
            })
            .collect::<Result<Vec<_>, _>>()?;
        Self::try_from_refs(
            key,
            kind,
            status,
            parent_node_id,
            lineage_node_ids,
            aggregate_version,
        )
    }

    fn try_from_refs(
        key: OrganizationLineageKey,
        kind: OrganizationNodeKind,
        status: OrganizationNodeStatus,
        parent_node_id: Option<BusinessUnitRef>,
        lineage_node_ids: Vec<BusinessUnitRef>,
        aggregate_version: u64,
    ) -> Result<Self, OrganizationLineageValidationError> {
        let value = Self {
            key,
            kind,
            status,
            parent_node_id,
            lineage_node_ids,
            aggregate_version,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), OrganizationLineageValidationError> {
        if self.aggregate_version == 0 {
            return Err(OrganizationLineageValidationError::ZeroVersion);
        }
        let depth = self.lineage_node_ids.len() + 1;
        if depth > MAX_ORGANIZATION_LINEAGE_DEPTH {
            return Err(OrganizationLineageValidationError::LineageTooDeep {
                maximum: MAX_ORGANIZATION_LINEAGE_DEPTH,
                actual: depth,
            });
        }
        if self
            .lineage_node_ids
            .iter()
            .enumerate()
            .any(|(index, node)| {
                *node == self.key.node_id || self.lineage_node_ids[..index].contains(node)
            })
        {
            return Err(OrganizationLineageValidationError::DuplicateNode);
        }

        let holding_id = self.key.holding_id.as_uuid();
        let node_id = self.key.node_id.as_uuid();
        let lineage = self
            .lineage_node_ids
            .iter()
            .map(|node| node.as_uuid())
            .collect::<Vec<_>>();
        let valid_shape = match self.kind {
            OrganizationNodeKind::Holding => {
                node_id == holding_id && lineage.is_empty() && self.parent_node_id.is_none()
            }
            OrganizationNodeKind::Company => {
                lineage.as_slice() == [holding_id]
                    && self.parent_node_id == self.lineage_node_ids.last().copied()
            }
            OrganizationNodeKind::Headquarters | OrganizationNodeKind::Branch => {
                lineage.len() == 2
                    && lineage[0] == holding_id
                    && self.parent_node_id == self.lineage_node_ids.last().copied()
            }
            OrganizationNodeKind::Department => {
                matches!(lineage.len(), 2 | 3)
                    && lineage[0] == holding_id
                    && self.parent_node_id == self.lineage_node_ids.last().copied()
            }
            OrganizationNodeKind::Team => {
                matches!(lineage.len(), 3 | 4)
                    && lineage[0] == holding_id
                    && self.parent_node_id == self.lineage_node_ids.last().copied()
            }
        };
        if !valid_shape {
            return Err(OrganizationLineageValidationError::InvalidLineageShape);
        }
        Ok(())
    }

    /// Resolves this node's current structural containment. Invalid or
    /// inactive witnesses, invalid scopes, and cross-holding comparisons all
    /// fail closed. Exact target-scope decisions should use [`contains_scope`]
    /// so the witness is also bound to the requested Department or Team.
    pub fn is_within(&self, authority_scope: &AuthorityScope) -> bool {
        if self.validate().is_err()
            || self.status != OrganizationNodeStatus::Active
            || authority_scope.validate().is_err()
        {
            return false;
        }
        let holding_id = self.key.holding_id.as_uuid();
        match authority_scope {
            AuthorityScope::Platform => true,
            AuthorityScope::Holding {
                holding_id: authority_holding,
            } => *authority_holding == holding_id,
            AuthorityScope::Company {
                holding_id: authority_holding,
                company_id: authority_company,
            } => {
                *authority_holding == holding_id
                    && self
                        .company_id()
                        .is_some_and(|company| company.as_uuid() == *authority_company)
            }
            AuthorityScope::Branch {
                holding_id: authority_holding,
                company_id: authority_company,
                branch_id: authority_branch,
            } => {
                *authority_holding == holding_id
                    && self
                        .company_id()
                        .is_some_and(|company| company.as_uuid() == *authority_company)
                    && self
                        .branch_id()
                        .is_some_and(|branch| branch.as_uuid() == *authority_branch)
            }
            AuthorityScope::Subtree {
                holding_id: authority_holding,
                organization_node_id,
            } => {
                *authority_holding == holding_id
                    && matches!(
                        self.kind,
                        OrganizationNodeKind::Department | OrganizationNodeKind::Team
                    )
                    && (self.key.node_id.as_uuid() == *organization_node_id
                        || (self.kind == OrganizationNodeKind::Team
                            && self.department_id().is_some_and(|department| {
                                department.as_uuid() == *organization_node_id
                            })))
            }
        }
    }

    pub const fn key(&self) -> OrganizationLineageKey {
        self.key
    }

    pub const fn kind(&self) -> OrganizationNodeKind {
        self.kind
    }

    pub const fn status(&self) -> OrganizationNodeStatus {
        self.status
    }

    pub const fn parent_node_id(&self) -> Option<BusinessUnitRef> {
        self.parent_node_id
    }

    pub fn lineage_node_ids(&self) -> &[BusinessUnitRef] {
        &self.lineage_node_ids
    }

    pub const fn aggregate_version(&self) -> u64 {
        self.aggregate_version
    }

    pub fn company_id(&self) -> Option<BusinessUnitRef> {
        match self.kind {
            OrganizationNodeKind::Holding => None,
            OrganizationNodeKind::Company => Some(self.key.node_id),
            OrganizationNodeKind::Headquarters
            | OrganizationNodeKind::Branch
            | OrganizationNodeKind::Department
            | OrganizationNodeKind::Team => self.lineage_node_ids.get(1).copied(),
        }
    }

    pub fn branch_id(&self) -> Option<BusinessUnitRef> {
        match self.kind {
            OrganizationNodeKind::Branch => Some(self.key.node_id),
            OrganizationNodeKind::Department if self.lineage_node_ids.len() == 3 => {
                self.lineage_node_ids.get(2).copied()
            }
            OrganizationNodeKind::Team if self.lineage_node_ids.len() == 4 => {
                self.lineage_node_ids.get(2).copied()
            }
            OrganizationNodeKind::Holding
            | OrganizationNodeKind::Company
            | OrganizationNodeKind::Headquarters
            | OrganizationNodeKind::Department
            | OrganizationNodeKind::Team => None,
        }
    }

    pub fn department_id(&self) -> Option<BusinessUnitRef> {
        match self.kind {
            OrganizationNodeKind::Department => Some(self.key.node_id),
            OrganizationNodeKind::Team => self.lineage_node_ids.last().copied(),
            OrganizationNodeKind::Holding
            | OrganizationNodeKind::Company
            | OrganizationNodeKind::Headquarters
            | OrganizationNodeKind::Branch => None,
        }
    }
}

impl<'de> Deserialize<'de> for OrganizationLineageWitness {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        #[derive(Deserialize)]
        struct WireWitness {
            key: OrganizationLineageKey,
            kind: OrganizationNodeKind,
            status: OrganizationNodeStatus,
            parent_node_id: Option<BusinessUnitRef>,
            lineage_node_ids: Vec<BusinessUnitRef>,
            aggregate_version: u64,
        }

        let value = WireWitness::deserialize(deserializer)?;
        Self::try_from_refs(
            value.key,
            value.kind,
            value.status,
            value.parent_node_id,
            value.lineage_node_ids,
            value.aggregate_version,
        )
        .map_err(de::Error::custom)
    }
}

/// Complete, fail-closed scope containment. Non-Subtree targets use only the
/// structured scope relationship; every exact Subtree target additionally
/// requires a current active Department/Team witness bound to that exact key.
pub fn contains_scope(
    authority_scope: &AuthorityScope,
    target_scope: &AuthorityScope,
    target_lineage: Option<&OrganizationLineageWitness>,
) -> bool {
    if authority_scope.validate().is_err() || target_scope.validate().is_err() {
        return false;
    }
    let AuthorityScope::Subtree {
        holding_id,
        organization_node_id,
    } = target_scope
    else {
        // A Subtree ID alone cannot prove that it owns a Company/Branch target;
        // this removes the UUID-equality heuristic from the kernel fallback.
        return !matches!(authority_scope, AuthorityScope::Subtree { .. })
            && authority_scope.contains(target_scope);
    };
    let Some(witness) = target_lineage else {
        return false;
    };
    matches!(
        witness.kind,
        OrganizationNodeKind::Department | OrganizationNodeKind::Team
    ) && witness.key.holding_id.as_uuid() == *holding_id
        && witness.key.node_id.as_uuid() == *organization_node_id
        && witness.is_within(authority_scope)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OrganizationLineageValidationError {
    NilIdentifier(&'static str),
    InvalidScope,
    ZeroVersion,
    LineageTooDeep { maximum: usize, actual: usize },
    DuplicateNode,
    InvalidLineageShape,
}

impl fmt::Display for OrganizationLineageValidationError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NilIdentifier(field) => write!(formatter, "{field} must not be nil"),
            Self::InvalidScope => formatter.write_str("authority scope is invalid"),
            Self::ZeroVersion => formatter.write_str("aggregate_version must be nonzero"),
            Self::LineageTooDeep { maximum, actual } => write!(
                formatter,
                "organization lineage depth {actual} exceeds maximum {maximum}"
            ),
            Self::DuplicateNode => {
                formatter.write_str("organization lineage contains a duplicate node")
            }
            Self::InvalidLineageShape => {
                formatter.write_str("organization lineage does not match its node kind")
            }
        }
    }
}

impl Error for OrganizationLineageValidationError {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OrganizationUnitFact {
    pub business_unit: BusinessUnitRef,
    pub code: ReferenceCode,
    pub name: DisplayName,
    pub holding: HoldingId,
    pub company: Option<CompanyRef>,
    pub branch: Option<BranchRef>,
    pub active: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", content = "fact", rename_all = "snake_case")]
pub enum Event {
    OrganizationUnitChanged(OrganizationUnitFact),
    OrganizationUnitRetired { business_unit: BusinessUnitRef },
    OrganizationLineageChanged(OrganizationLineageWitness),
}

impl IntegrationEvent for Event {
    fn owner(&self) -> BoundedContext {
        BoundedContext::Organization
    }

    fn event_type(&self) -> &'static str {
        match self {
            Self::OrganizationUnitChanged(_) => "organization.unit_changed",
            Self::OrganizationUnitRetired { .. } => "organization.unit_retired",
            Self::OrganizationLineageChanged(_) => ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE,
        }
    }

    fn schema_version(&self) -> u16 {
        match self {
            Self::OrganizationLineageChanged(_) => ORGANIZATION_LINEAGE_CHANGED_VERSION,
            Self::OrganizationUnitChanged(_) | Self::OrganizationUnitRetired { .. } => 1,
        }
    }

    fn validate(&self) -> Result<(), ContractValidationError> {
        match self {
            Self::OrganizationLineageChanged(witness) => witness
                .validate()
                .map_err(|_| ContractValidationError::OutOfRange("organization.lineage")),
            Self::OrganizationUnitChanged(_) | Self::OrganizationUnitRetired { .. } => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::json;

    use super::*;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn team_witness(status: OrganizationNodeStatus) -> OrganizationLineageWitness {
        OrganizationLineageWitness::try_new(
            OrganizationLineageKey::try_new(id(1), id(5)).unwrap(),
            OrganizationNodeKind::Team,
            status,
            Some(id(4)),
            vec![id(1), id(2), id(3), id(4)],
            7,
        )
        .unwrap()
    }

    fn department_witness() -> OrganizationLineageWitness {
        OrganizationLineageWitness::try_new(
            OrganizationLineageKey::try_new(id(1), id(4)).unwrap(),
            OrganizationNodeKind::Department,
            OrganizationNodeStatus::Active,
            Some(id(3)),
            vec![id(1), id(2), id(3)],
            3,
        )
        .unwrap()
    }

    #[test]
    fn batch_key_keeps_holding_and_node_and_is_scope_derived() {
        let first = OrganizationLineageKey::try_new(id(1), id(5)).unwrap();
        let second = OrganizationLineageKey::try_new(id(2), id(5)).unwrap();
        let keys = BTreeSet::from([second, first, first]);
        assert_eq!(keys.len(), 2);
        assert_eq!(keys.into_iter().next(), Some(first));
        assert_eq!(
            OrganizationLineageKey::try_from_scope(&AuthorityScope::Subtree {
                holding_id: id(1),
                organization_node_id: id(5),
            }),
            Ok(Some(first))
        );
        assert_eq!(
            OrganizationLineageKey::try_from_scope(&AuthorityScope::Platform),
            Ok(None)
        );
    }

    #[test]
    fn bounded_team_witness_resolves_normalized_ancestors() {
        let witness = team_witness(OrganizationNodeStatus::Active);
        assert_eq!(
            witness.company_id().map(BusinessUnitRef::as_uuid),
            Some(id(2))
        );
        assert_eq!(
            witness.branch_id().map(BusinessUnitRef::as_uuid),
            Some(id(3))
        );
        assert_eq!(
            witness.department_id().map(BusinessUnitRef::as_uuid),
            Some(id(4))
        );
        assert!(witness.is_within(&AuthorityScope::Platform));
        assert!(witness.is_within(&AuthorityScope::Holding { holding_id: id(1) }));
        assert!(!witness.is_within(&AuthorityScope::Company {
            holding_id: id(2),
            company_id: id(2),
        }));
        assert!(!witness.is_within(&AuthorityScope::Branch {
            holding_id: id(1),
            company_id: id(9),
            branch_id: id(3),
        }));
    }

    #[test]
    fn subtree_containment_requires_an_exact_active_department_or_team_witness() {
        let target = AuthorityScope::Subtree {
            holding_id: id(1),
            organization_node_id: id(5),
        };
        let authority = AuthorityScope::Company {
            holding_id: id(1),
            company_id: id(2),
        };
        let active = team_witness(OrganizationNodeStatus::Active);
        assert!(contains_scope(&authority, &target, Some(&active)));
        assert!(!contains_scope(&authority, &target, None));

        let archived = team_witness(OrganizationNodeStatus::Archived);
        assert!(!contains_scope(&authority, &target, Some(&archived)));
        let suspended = team_witness(OrganizationNodeStatus::Suspended);
        assert!(!contains_scope(&authority, &target, Some(&suspended)));

        let mismatched = department_witness();
        assert!(!contains_scope(&authority, &target, Some(&mismatched)));

        for (kind, parent, lineage) in [
            (OrganizationNodeKind::Company, id(1), vec![id(1)]),
            (
                OrganizationNodeKind::Headquarters,
                id(2),
                vec![id(1), id(2)],
            ),
            (OrganizationNodeKind::Branch, id(2), vec![id(1), id(2)]),
        ] {
            let non_subtree = OrganizationLineageWitness::try_new(
                OrganizationLineageKey::try_new(id(1), id(5)).unwrap(),
                kind,
                OrganizationNodeStatus::Active,
                Some(parent),
                lineage,
                2,
            )
            .unwrap();
            assert!(!contains_scope(&authority, &target, Some(&non_subtree)));
            assert!(!non_subtree.is_within(&AuthorityScope::Subtree {
                holding_id: id(1),
                organization_node_id: id(5),
            }));
        }

        let foreign = AuthorityScope::Company {
            holding_id: id(2),
            company_id: id(2),
        };
        assert!(!contains_scope(&foreign, &target, Some(&active)));
    }

    #[test]
    fn subtree_ancestors_contain_department_and_team_but_siblings_do_not() {
        let department_target = AuthorityScope::Subtree {
            holding_id: id(1),
            organization_node_id: id(4),
        };
        let team_target = AuthorityScope::Subtree {
            holding_id: id(1),
            organization_node_id: id(5),
        };
        let department = department_witness();
        let team = team_witness(OrganizationNodeStatus::Active);

        assert!(contains_scope(
            &AuthorityScope::Company {
                holding_id: id(1),
                company_id: id(2),
            },
            &department_target,
            Some(&department),
        ));
        assert!(contains_scope(
            &AuthorityScope::Branch {
                holding_id: id(1),
                company_id: id(2),
                branch_id: id(3),
            },
            &team_target,
            Some(&team),
        ));
        assert!(contains_scope(
            &department_target,
            &team_target,
            Some(&team),
        ));
        assert!(!contains_scope(
            &AuthorityScope::Subtree {
                holding_id: id(1),
                organization_node_id: id(2),
            },
            &team_target,
            Some(&team),
        ));
        assert!(!contains_scope(
            &AuthorityScope::Subtree {
                holding_id: id(1),
                organization_node_id: id(3),
            },
            &team_target,
            Some(&team),
        ));
        assert!(!contains_scope(
            &AuthorityScope::Subtree {
                holding_id: id(1),
                organization_node_id: id(6),
            },
            &team_target,
            Some(&team),
        ));
    }

    #[test]
    fn invalid_shapes_duplicates_and_over_depth_fail_closed() {
        let key = OrganizationLineageKey::try_new(id(1), id(6)).unwrap();
        assert_eq!(
            OrganizationLineageWitness::try_new(
                key,
                OrganizationNodeKind::Team,
                OrganizationNodeStatus::Active,
                Some(id(5)),
                vec![id(1), id(2), id(3), id(4), id(5)],
                1,
            ),
            Err(OrganizationLineageValidationError::LineageTooDeep {
                maximum: MAX_ORGANIZATION_LINEAGE_DEPTH,
                actual: 6,
            })
        );
        assert_eq!(
            OrganizationLineageWitness::try_new(
                key,
                OrganizationNodeKind::Team,
                OrganizationNodeStatus::Active,
                Some(id(4)),
                vec![id(1), id(2), id(2), id(4)],
                1,
            ),
            Err(OrganizationLineageValidationError::DuplicateNode)
        );
        assert_eq!(
            OrganizationLineageWitness::try_new(
                key,
                OrganizationNodeKind::Department,
                OrganizationNodeStatus::Active,
                Some(id(4)),
                vec![id(1), id(2)],
                1,
            ),
            Err(OrganizationLineageValidationError::InvalidLineageShape)
        );
    }

    #[test]
    fn deserialization_cannot_bypass_lineage_validation() {
        let witness = team_witness(OrganizationNodeStatus::Active);
        let mut wire = serde_json::to_value(&witness).unwrap();
        wire["aggregate_version"] = json!(0);
        assert!(serde_json::from_value::<OrganizationLineageWitness>(wire).is_err());

        let mut wire = serde_json::to_value(&witness).unwrap();
        wire["key"]["holding_id"] = json!(id(9));
        assert!(serde_json::from_value::<OrganizationLineageWitness>(wire).is_err());
    }

    #[test]
    fn lineage_event_has_a_distinct_validated_contract() {
        let event = Event::OrganizationLineageChanged(team_witness(OrganizationNodeStatus::Active));
        assert_eq!(event.event_type(), ORGANIZATION_LINEAGE_CHANGED_EVENT_TYPE);
        assert_eq!(event.schema_version(), ORGANIZATION_LINEAGE_CHANGED_VERSION);
        assert!(event.validate().is_ok());
        let wire = serde_json::to_value(event).unwrap();
        assert_eq!(wire["type"], json!("organization_lineage_changed"));
    }
}
