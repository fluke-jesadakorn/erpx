use std::collections::BTreeMap;

use chrono::{DateTime, Utc};
use erpx_kernel::AuthorityScope;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::{
    HoldingLifecycleState, PlatformAdministrationDomainError, PlatformHolding, PlatformHoldingId,
    bounded,
};

pub const ROUTING_SCHEMA_REVISION: u32 = 2;
const MAX_PROFILE_REF_BYTES: usize = 255;
const MAX_EVIDENCE_REF_BYTES: usize = 180;
const MAX_FAILURE_CATEGORY_BYTES: usize = 160;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataPlaneDeploymentMode {
    ErpxManaged,
    CustomerHosted,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataPlaneTopology {
    HoldingPair,
    HoldingAndBranchPairs,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataPlaneRouteState {
    Reserved,
    Provisioning,
    Verified,
    Active,
    Suspended,
    Failed,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case")]
pub enum DataPlaneRouteTarget {
    Holding,
    Branch { branch_id: Uuid },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataPlaneRoute {
    target: DataPlaneRouteTarget,
    write_profile_ref: String,
    read_profile_ref: String,
    state: DataPlaneRouteState,
    evidence_ref: Option<String>,
    failure_category: Option<String>,
    verified_at: Option<DateTime<Utc>>,
    updated_at: DateTime<Utc>,
}

impl DataPlaneRoute {
    pub const fn target(&self) -> DataPlaneRouteTarget {
        self.target
    }
    pub fn write_profile_ref(&self) -> &str {
        &self.write_profile_ref
    }
    pub fn read_profile_ref(&self) -> &str {
        &self.read_profile_ref
    }
    pub const fn state(&self) -> DataPlaneRouteState {
        self.state
    }
    pub fn evidence_ref(&self) -> Option<&str> {
        self.evidence_ref.as_deref()
    }
    pub fn failure_category(&self) -> Option<&str> {
        self.failure_category.as_deref()
    }
    pub const fn verified_at(&self) -> Option<DateTime<Utc>> {
        self.verified_at
    }
    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }
}

/// Domain-owned catalog for opaque connection-profile references. It never
/// accepts a DSN or a credential and cannot fall back from a missing branch
/// pair to the holding pair when branch-pair topology is selected.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataPlaneRouteCatalog {
    holding_id: PlatformHoldingId,
    deployment_mode: DataPlaneDeploymentMode,
    topology: DataPlaneTopology,
    version: u64,
    routes: BTreeMap<DataPlaneRouteTarget, DataPlaneRoute>,
}

impl DataPlaneRouteCatalog {
    pub fn new(
        holding_id: PlatformHoldingId,
        deployment_mode: DataPlaneDeploymentMode,
        topology: DataPlaneTopology,
    ) -> Result<Self, PlatformAdministrationDomainError> {
        Ok(Self {
            holding_id,
            deployment_mode,
            topology,
            version: 1,
            routes: BTreeMap::new(),
        })
    }

    pub fn reserve(
        &mut self,
        target: DataPlaneRouteTarget,
        write_profile_ref: String,
        read_profile_ref: String,
        at: DateTime<Utc>,
    ) -> Result<bool, PlatformAdministrationDomainError> {
        validate_target(self.topology, target)?;
        let write_profile_ref = profile_ref(write_profile_ref, "write_profile_ref")?;
        let read_profile_ref = profile_ref(read_profile_ref, "read_profile_ref")?;
        if let Some(current) = self.routes.get(&target) {
            if current.write_profile_ref == write_profile_ref
                && current.read_profile_ref == read_profile_ref
                && current.state != DataPlaneRouteState::Retired
            {
                return Ok(false);
            }
            return Err(PlatformAdministrationDomainError::RouteAlreadyReserved);
        }
        self.routes.insert(
            target,
            DataPlaneRoute {
                target,
                write_profile_ref,
                read_profile_ref,
                state: DataPlaneRouteState::Reserved,
                evidence_ref: None,
                failure_category: None,
                verified_at: None,
                updated_at: at,
            },
        );
        self.bump()?;
        Ok(true)
    }

    pub fn begin_provisioning(
        &mut self,
        target: DataPlaneRouteTarget,
        at: DateTime<Utc>,
    ) -> Result<(), PlatformAdministrationDomainError> {
        self.transition(
            target,
            DataPlaneRouteState::Reserved,
            DataPlaneRouteState::Provisioning,
            at,
        )
    }

    pub fn verify(
        &mut self,
        target: DataPlaneRouteTarget,
        write_schema_revision: u32,
        read_schema_revision: u32,
        evidence_ref: String,
        at: DateTime<Utc>,
    ) -> Result<(), PlatformAdministrationDomainError> {
        if write_schema_revision != ROUTING_SCHEMA_REVISION
            || read_schema_revision != ROUTING_SCHEMA_REVISION
        {
            return Err(PlatformAdministrationDomainError::UnsupportedRouteSchemaRevision);
        }
        let evidence_ref = opaque_ref(evidence_ref, "route_evidence_ref", MAX_EVIDENCE_REF_BYTES)?;
        let route = self.route_for_update(target, at)?;
        if route.state != DataPlaneRouteState::Provisioning {
            return Err(PlatformAdministrationDomainError::InvalidRouteTransition);
        }
        route.state = DataPlaneRouteState::Verified;
        route.evidence_ref = Some(evidence_ref);
        route.failure_category = None;
        route.verified_at = Some(at);
        route.updated_at = at;
        self.bump()
    }

    pub fn activate(
        &mut self,
        target: DataPlaneRouteTarget,
        at: DateTime<Utc>,
    ) -> Result<(), PlatformAdministrationDomainError> {
        self.transition(
            target,
            DataPlaneRouteState::Verified,
            DataPlaneRouteState::Active,
            at,
        )
    }

    pub fn suspend(
        &mut self,
        target: DataPlaneRouteTarget,
        at: DateTime<Utc>,
    ) -> Result<(), PlatformAdministrationDomainError> {
        self.transition(
            target,
            DataPlaneRouteState::Active,
            DataPlaneRouteState::Suspended,
            at,
        )
    }

    pub fn mark_failed(
        &mut self,
        target: DataPlaneRouteTarget,
        failure_category: String,
        at: DateTime<Utc>,
    ) -> Result<(), PlatformAdministrationDomainError> {
        let failure_category = opaque_ref(
            failure_category,
            "route_failure_category",
            MAX_FAILURE_CATEGORY_BYTES,
        )?;
        let route = self.route_for_update(target, at)?;
        if !matches!(
            route.state,
            DataPlaneRouteState::Provisioning
                | DataPlaneRouteState::Verified
                | DataPlaneRouteState::Active
                | DataPlaneRouteState::Suspended
        ) {
            return Err(PlatformAdministrationDomainError::InvalidRouteTransition);
        }
        route.state = DataPlaneRouteState::Failed;
        route.failure_category = Some(failure_category);
        route.updated_at = at;
        self.bump()
    }

    /// A failed route never retries itself. This explicit operation represents
    /// a freshly authorized retry command.
    pub fn retry_provisioning(
        &mut self,
        target: DataPlaneRouteTarget,
        at: DateTime<Utc>,
    ) -> Result<(), PlatformAdministrationDomainError> {
        let route = self.route_for_update(target, at)?;
        if route.state != DataPlaneRouteState::Failed {
            return Err(PlatformAdministrationDomainError::InvalidRouteTransition);
        }
        route.state = DataPlaneRouteState::Provisioning;
        route.evidence_ref = None;
        route.failure_category = None;
        route.verified_at = None;
        route.updated_at = at;
        self.bump()
    }

    pub fn resolve<'a>(
        &'a self,
        holding: &PlatformHolding,
        scope: &AuthorityScope,
    ) -> Result<&'a DataPlaneRoute, PlatformAdministrationDomainError> {
        if holding.id() != self.holding_id || holding.status() != HoldingLifecycleState::Active {
            return Err(PlatformAdministrationDomainError::RouteUnavailable);
        }
        let target = match scope {
            AuthorityScope::Holding { holding_id } | AuthorityScope::Company { holding_id, .. }
                if *holding_id == self.holding_id.as_uuid() =>
            {
                DataPlaneRouteTarget::Holding
            }
            AuthorityScope::Branch {
                holding_id,
                branch_id,
                ..
            } if *holding_id == self.holding_id.as_uuid() => match self.topology {
                DataPlaneTopology::HoldingPair => DataPlaneRouteTarget::Holding,
                DataPlaneTopology::HoldingAndBranchPairs => DataPlaneRouteTarget::Branch {
                    branch_id: *branch_id,
                },
            },
            _ => return Err(PlatformAdministrationDomainError::RouteUnavailable),
        };
        self.routes
            .get(&target)
            .filter(|route| route.state == DataPlaneRouteState::Active)
            .ok_or(PlatformAdministrationDomainError::RouteUnavailable)
    }

    fn transition(
        &mut self,
        target: DataPlaneRouteTarget,
        from: DataPlaneRouteState,
        to: DataPlaneRouteState,
        at: DateTime<Utc>,
    ) -> Result<(), PlatformAdministrationDomainError> {
        let route = self.route_for_update(target, at)?;
        if route.state != from {
            return Err(PlatformAdministrationDomainError::InvalidRouteTransition);
        }
        route.state = to;
        route.updated_at = at;
        self.bump()
    }

    fn route_for_update(
        &mut self,
        target: DataPlaneRouteTarget,
        at: DateTime<Utc>,
    ) -> Result<&mut DataPlaneRoute, PlatformAdministrationDomainError> {
        let route = self
            .routes
            .get_mut(&target)
            .ok_or(PlatformAdministrationDomainError::RouteUnavailable)?;
        if at < route.updated_at {
            return Err(PlatformAdministrationDomainError::StaleRouteObservation);
        }
        Ok(route)
    }

    fn bump(&mut self) -> Result<(), PlatformAdministrationDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PlatformAdministrationDomainError::VersionOverflow)?;
        Ok(())
    }

    pub const fn holding_id(&self) -> PlatformHoldingId {
        self.holding_id
    }
    pub const fn deployment_mode(&self) -> DataPlaneDeploymentMode {
        self.deployment_mode
    }
    pub const fn topology(&self) -> DataPlaneTopology {
        self.topology
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn route(&self, target: DataPlaneRouteTarget) -> Option<&DataPlaneRoute> {
        self.routes.get(&target)
    }
}

fn validate_target(
    topology: DataPlaneTopology,
    target: DataPlaneRouteTarget,
) -> Result<(), PlatformAdministrationDomainError> {
    if matches!(target, DataPlaneRouteTarget::Branch { branch_id } if branch_id.is_nil())
        || (topology == DataPlaneTopology::HoldingPair
            && matches!(target, DataPlaneRouteTarget::Branch { .. }))
    {
        Err(PlatformAdministrationDomainError::InvalidRouteTarget)
    } else {
        Ok(())
    }
}

fn profile_ref(
    value: String,
    field: &'static str,
) -> Result<String, PlatformAdministrationDomainError> {
    let value = opaque_ref(value, field, MAX_PROFILE_REF_BYTES)?;
    if value.contains("password") || value.contains("secret=") || value.contains("dsn") {
        return Err(PlatformAdministrationDomainError::InvalidRoutingReference);
    }
    Ok(value)
}

fn opaque_ref(
    value: String,
    field: &'static str,
    maximum: usize,
) -> Result<String, PlatformAdministrationDomainError> {
    let value = bounded(&value, field, maximum)?;
    if value.contains("://") || value.contains('@') || value.chars().any(char::is_whitespace) {
        Err(PlatformAdministrationDomainError::InvalidRoutingReference)
    } else {
        Ok(value)
    }
}
