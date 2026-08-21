use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{Quantity, RecordVersionRef, StableRecordRef};
use erpx_kernel::{ActorAttribution, AuthorityScope, CommandId, CorrelationId, HumanDecisionProof};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

macro_rules! planning_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);
        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, PlanningDomainError> {
                if value.is_nil() || value.get_version_num() != 4 {
                    return Err(PlanningDomainError::InvalidIdentifier($field));
                }
                Ok(Self(value))
            }
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }
    };
}
planning_id!(ScenarioId, "scenario_id");
planning_id!(RunId, "run_id");
planning_id!(DemandId, "demand_id");
planning_id!(SupplyId, "supply_id");
planning_id!(RecommendationId, "recommendation_id");
planning_id!(OverrideId, "override_id");
planning_id!(ReleaseId, "release_id");

pub fn recommendation_approval_decision_ref(
    id: RecommendationId,
    expected_version: u64,
    command_id: CommandId,
) -> String {
    format!(
        "planning.recommendation:{}:approve:v{}:command:{}",
        id.as_uuid(),
        expected_version,
        command_id.as_uuid()
    )
}

pub fn recommendation_rejection_decision_ref(
    id: RecommendationId,
    expected_version: u64,
    command_id: CommandId,
) -> String {
    format!(
        "planning.recommendation:{}:reject:v{}:command:{}",
        id.as_uuid(),
        expected_version,
        command_id.as_uuid()
    )
}

pub fn recommendation_release_decision_ref(
    id: RecommendationId,
    release_id: ReleaseId,
    expected_version: u64,
    command_id: CommandId,
) -> String {
    format!(
        "planning.recommendation:{}:release:{}:v{}:command:{}",
        id.as_uuid(),
        release_id.as_uuid(),
        expected_version,
        command_id.as_uuid()
    )
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanningMode {
    Mrp,
    Drp,
    Capacity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DemandFact {
    pub id: DemandId,
    pub item: StableRecordRef,
    pub location: StableRecordRef,
    pub quantity: Quantity,
    pub required_on: NaiveDate,
    pub source: RecordVersionRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupplyFact {
    pub id: SupplyId,
    pub item: StableRecordRef,
    pub location: StableRecordRef,
    pub quantity: Quantity,
    pub available_on: NaiveDate,
    pub source: RecordVersionRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapacityBucket {
    pub resource: StableRecordRef,
    pub bucket: NaiveDate,
    pub available_seconds: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapacityRequirement {
    pub demand_id: DemandId,
    pub resource: StableRecordRef,
    pub bucket: NaiveDate,
    pub required_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationKind {
    Buy,
    Make,
    Transfer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SourcingRule {
    pub item: StableRecordRef,
    pub kind: RecommendationKind,
    pub source_location: Option<StableRecordRef>,
    pub lead_time_days: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningPolicy {
    pub policy_id: Uuid,
    pub version: u64,
    pub sourcing: Vec<SourcingRule>,
    pub allow_late_recommendations: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScenarioStatus {
    Draft,
    Frozen,
    Running,
    Completed,
    Failed,
    Superseded,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningScenario {
    id: ScenarioId,
    version: u64,
    scope: AuthorityScope,
    modes: BTreeSet<PlanningMode>,
    input_position: u64,
    inputs: Vec<RecordVersionRef>,
    policy: PlanningPolicy,
    demands: Vec<DemandFact>,
    supplies: Vec<SupplyFact>,
    capacity: Vec<CapacityBucket>,
    capacity_requirements: Vec<CapacityRequirement>,
    status: ScenarioStatus,
    last_failure_reason: Option<String>,
    updated_at: DateTime<Utc>,
}

impl PlanningScenario {
    #[allow(clippy::too_many_arguments)]
    pub fn draft(
        id: ScenarioId,
        scope: AuthorityScope,
        modes: BTreeSet<PlanningMode>,
        input_position: u64,
        inputs: Vec<RecordVersionRef>,
        policy: PlanningPolicy,
        demands: Vec<DemandFact>,
        supplies: Vec<SupplyFact>,
        capacity: Vec<CapacityBucket>,
        capacity_requirements: Vec<CapacityRequirement>,
        at: DateTime<Utc>,
    ) -> Result<Self, PlanningDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            modes,
            input_position,
            inputs,
            policy,
            demands,
            supplies,
            capacity,
            capacity_requirements,
            status: ScenarioStatus::Draft,
            last_failure_reason: None,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn freeze(&mut self, at: DateTime<Utc>) -> Result<(), PlanningDomainError> {
        if self.status != ScenarioStatus::Draft || at < self.updated_at {
            return Err(PlanningDomainError::InvalidTransition);
        }
        self.validate()?;
        self.status = ScenarioStatus::Frozen;
        self.advance(at)
    }
    pub fn mark_running(&mut self, at: DateTime<Utc>) -> Result<(), PlanningDomainError> {
        self.transition(ScenarioStatus::Frozen, ScenarioStatus::Running, at)
    }
    pub fn mark_completed(&mut self, at: DateTime<Utc>) -> Result<(), PlanningDomainError> {
        self.transition(ScenarioStatus::Running, ScenarioStatus::Completed, at)
    }
    pub fn mark_failed(
        &mut self,
        reason: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<(), PlanningDomainError> {
        let reason = reason.into();
        if reason.trim().len() < 8 || reason.len() > 500 {
            return Err(PlanningDomainError::InvalidFailureReason);
        }
        self.transition(ScenarioStatus::Running, ScenarioStatus::Failed, at)?;
        self.last_failure_reason = Some(reason);
        Ok(())
    }

    pub fn prepare_retry(
        &mut self,
        reason: impl Into<String>,
        at: DateTime<Utc>,
    ) -> Result<(), PlanningDomainError> {
        let reason = reason.into();
        if self.status != ScenarioStatus::Failed
            || reason.trim().len() < 8
            || reason.len() > 500
            || at < self.updated_at
        {
            return Err(PlanningDomainError::InvalidFailureReason);
        }
        self.status = ScenarioStatus::Frozen;
        self.last_failure_reason = Some(reason);
        self.advance(at)
    }

    pub fn validate(&self) -> Result<(), PlanningDomainError> {
        self.scope.validate()?;
        if self.version == 0
            || self.input_position == 0
            || self.modes.is_empty()
            || self.inputs.is_empty()
            || self.policy.policy_id.is_nil()
            || self.policy.version == 0
            || self.demands.is_empty()
        {
            return Err(PlanningDomainError::InvalidScenario);
        }
        for pin in &self.inputs {
            pin.validate()?;
        }
        let mut ids = BTreeSet::new();
        for demand in &self.demands {
            demand.item.validate()?;
            demand.location.validate()?;
            demand.source.validate()?;
            positive(demand.quantity)?;
            if !ids.insert(demand.id) {
                return Err(PlanningDomainError::DuplicateInput);
            }
        }
        let mut supply_ids = BTreeSet::new();
        for supply in &self.supplies {
            supply.item.validate()?;
            supply.location.validate()?;
            supply.source.validate()?;
            non_negative(supply.quantity)?;
            if !supply_ids.insert(supply.id) {
                return Err(PlanningDomainError::DuplicateInput);
            }
        }
        for rule in &self.policy.sourcing {
            rule.item.validate()?;
            if let Some(source) = &rule.source_location {
                source.validate()?;
            }
        }
        for bucket in &self.capacity {
            bucket.resource.validate()?;
        }
        for value in &self.capacity_requirements {
            value.resource.validate()?;
            if !ids.contains(&value.demand_id) {
                return Err(PlanningDomainError::InvalidCapacityInput);
            }
        }
        Ok(())
    }
    fn transition(
        &mut self,
        expected: ScenarioStatus,
        next: ScenarioStatus,
        at: DateTime<Utc>,
    ) -> Result<(), PlanningDomainError> {
        if self.status != expected || at < self.updated_at {
            return Err(PlanningDomainError::InvalidTransition);
        }
        self.status = next;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), PlanningDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PlanningDomainError::Overflow)?;
        self.updated_at = at;
        Ok(())
    }
    pub const fn id(&self) -> ScenarioId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn status(&self) -> ScenarioStatus {
        self.status
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PeggingFact {
    pub demand_id: DemandId,
    pub supply_id: SupplyId,
    pub quantity: Quantity,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Shortage {
    pub demand_id: DemandId,
    pub item: StableRecordRef,
    pub location: StableRecordRef,
    pub quantity: Quantity,
    pub required_on: NaiveDate,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapacityOverload {
    pub resource: StableRecordRef,
    pub bucket: NaiveDate,
    pub required_seconds: u64,
    pub available_seconds: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RecommendationStatus {
    Proposed,
    Approved,
    Released,
    Rejected,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecommendationOverride {
    pub id: OverrideId,
    pub previous_quantity: Quantity,
    pub revised_quantity: Quantity,
    pub reason: String,
    pub principal_id: Uuid,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningHumanDecisionEvidence {
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub decision_ref: String,
    pub decided_at: DateTime<Utc>,
}

impl PlanningHumanDecisionEvidence {
    fn retain(decision: &HumanDecisionProof) -> Self {
        Self {
            actor: decision.actor().clone(),
            authority_scope: decision.authority_scope().clone(),
            decision_ref: decision.decision_ref().to_owned(),
            decided_at: decision.decided_at(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningRecommendation {
    pub id: RecommendationId,
    pub version: u64,
    /// Legacy snapshots without an owner scope remain readable but cannot be
    /// approved, rejected, or released until republished.
    #[serde(default)]
    pub scope: Option<AuthorityScope>,
    pub kind: RecommendationKind,
    pub item: StableRecordRef,
    pub destination: StableRecordRef,
    pub source: Option<StableRecordRef>,
    pub quantity: Quantity,
    pub required_on: NaiveDate,
    pub demand_id: DemandId,
    pub status: RecommendationStatus,
    pub overrides: Vec<RecommendationOverride>,
    pub release_id: Option<ReleaseId>,
    #[serde(default)]
    pub approval_decision: Option<Box<PlanningHumanDecisionEvidence>>,
    #[serde(default)]
    pub rejection_decision: Option<Box<PlanningHumanDecisionEvidence>>,
    #[serde(default)]
    pub release_decision: Option<Box<PlanningHumanDecisionEvidence>>,
}

impl PlanningRecommendation {
    pub fn override_quantity(
        &mut self,
        value: RecommendationOverride,
    ) -> Result<bool, PlanningDomainError> {
        if self.status != RecommendationStatus::Proposed {
            return Err(PlanningDomainError::InvalidTransition);
        }
        if self
            .overrides
            .iter()
            .any(|existing| existing.id == value.id)
        {
            return Ok(false);
        }
        positive(value.revised_quantity)?;
        same_scale(value.previous_quantity, value.revised_quantity)?;
        if self.quantity != value.previous_quantity
            || value.reason.trim().len() < 8
            || value.reason.len() > 500
            || value.principal_id.is_nil()
        {
            return Err(PlanningDomainError::InvalidOverride);
        }
        self.quantity = value.revised_quantity;
        self.overrides.push(value);
        self.advance()?;
        Ok(true)
    }
    pub fn approve(
        &mut self,
        command_id: CommandId,
        decision: &HumanDecisionProof,
    ) -> Result<(), PlanningDomainError> {
        self.require_decision(
            decision,
            &recommendation_approval_decision_ref(self.id, self.version, command_id),
        )?;
        if self.status != RecommendationStatus::Proposed {
            return Err(PlanningDomainError::InvalidTransition);
        }
        self.status = RecommendationStatus::Approved;
        self.approval_decision = Some(Box::new(PlanningHumanDecisionEvidence::retain(decision)));
        self.advance()
    }
    pub fn reject(
        &mut self,
        command_id: CommandId,
        decision: &HumanDecisionProof,
    ) -> Result<(), PlanningDomainError> {
        self.require_decision(
            decision,
            &recommendation_rejection_decision_ref(self.id, self.version, command_id),
        )?;
        if self.status != RecommendationStatus::Proposed {
            return Err(PlanningDomainError::InvalidTransition);
        }
        self.status = RecommendationStatus::Rejected;
        self.rejection_decision = Some(Box::new(PlanningHumanDecisionEvidence::retain(decision)));
        self.advance()
    }
    pub fn mark_released(
        &mut self,
        release_id: ReleaseId,
        command_id: CommandId,
        decision: &HumanDecisionProof,
    ) -> Result<(), PlanningDomainError> {
        self.require_decision(
            decision,
            &recommendation_release_decision_ref(self.id, release_id, self.version, command_id),
        )?;
        if self.status != RecommendationStatus::Approved || self.release_id.is_some() {
            return Err(PlanningDomainError::InvalidTransition);
        }
        self.status = RecommendationStatus::Released;
        self.release_id = Some(release_id);
        self.release_decision = Some(Box::new(PlanningHumanDecisionEvidence::retain(decision)));
        self.advance()
    }
    fn require_decision(
        &self,
        decision: &HumanDecisionProof,
        expected_ref: &str,
    ) -> Result<(), PlanningDomainError> {
        let scope = self
            .scope
            .as_ref()
            .ok_or(PlanningDomainError::HumanDecisionRequired)?;
        if decision.authority_scope() != scope
            || decision.decision_ref() != expected_ref
            || !decision.actor().is_human()
            || decision.actor().session_id.is_none()
            || decision.actor().represented_principal_ref.is_some()
            || decision.actor().support_capability_id.is_some()
        {
            Err(PlanningDomainError::HumanDecisionRequired)
        } else {
            Ok(())
        }
    }
    fn advance(&mut self) -> Result<(), PlanningDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(PlanningDomainError::Overflow)?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PlanningRun {
    pub id: RunId,
    pub scenario_id: ScenarioId,
    pub scenario_version: u64,
    pub policy_version: u64,
    pub input_position: u64,
    pub input_digest: String,
    pub shortages: Vec<Shortage>,
    pub pegging: Vec<PeggingFact>,
    pub overloads: Vec<CapacityOverload>,
    pub recommendations: Vec<PlanningRecommendation>,
    pub completed_at: DateTime<Utc>,
}

impl PlanningRun {
    pub fn execute(
        id: RunId,
        scenario: &PlanningScenario,
        at: DateTime<Utc>,
    ) -> Result<Self, PlanningDomainError> {
        if scenario.status != ScenarioStatus::Running {
            return Err(PlanningDomainError::ScenarioNotPinned);
        }
        let input_digest = digest_scenario(scenario)?;
        let mut supply = scenario.supplies.clone();
        supply.sort_by_key(|value| {
            (
                value.item.id(),
                value.location.id(),
                value.available_on,
                value.id,
            )
        });
        let mut available: BTreeMap<SupplyId, i64> = supply
            .iter()
            .map(|value| (value.id, value.quantity.units()))
            .collect();
        let mut demands = scenario.demands.clone();
        demands.sort_by_key(|value| {
            (
                value.required_on,
                value.item.id(),
                value.location.id(),
                value.id,
            )
        });
        let mut shortages = Vec::new();
        let mut pegging = Vec::new();
        let mut recommendations = Vec::new();
        for demand in &demands {
            let mut remaining = demand.quantity.units();
            for candidate in supply.iter().filter(|value| {
                value.item == demand.item
                    && value.location == demand.location
                    && value.available_on <= demand.required_on
                    && value.quantity.scale() == demand.quantity.scale()
            }) {
                let balance = available
                    .get_mut(&candidate.id)
                    .expect("supply initialized");
                let used = remaining.min(*balance);
                if used > 0 {
                    pegging.push(PeggingFact {
                        demand_id: demand.id,
                        supply_id: candidate.id,
                        quantity: Quantity::try_new(used, demand.quantity.scale())?,
                    });
                    *balance -= used;
                    remaining -= used;
                }
                if remaining == 0 {
                    break;
                }
            }
            if remaining > 0 {
                let quantity = Quantity::try_new(remaining, demand.quantity.scale())?;
                shortages.push(Shortage {
                    demand_id: demand.id,
                    item: demand.item.clone(),
                    location: demand.location.clone(),
                    quantity,
                    required_on: demand.required_on,
                });
                let rule = scenario
                    .policy
                    .sourcing
                    .iter()
                    .find(|value| value.item == demand.item)
                    .ok_or(PlanningDomainError::MissingSourcingRule)?;
                recommendations.push(PlanningRecommendation {
                    id: deterministic_recommendation_id(scenario.id, demand.id)?,
                    version: 1,
                    scope: Some(scenario.scope.clone()),
                    kind: rule.kind,
                    item: demand.item.clone(),
                    destination: demand.location.clone(),
                    source: rule.source_location.clone(),
                    quantity,
                    required_on: demand.required_on,
                    demand_id: demand.id,
                    status: RecommendationStatus::Proposed,
                    overrides: Vec::new(),
                    release_id: None,
                    approval_decision: None,
                    rejection_decision: None,
                    release_decision: None,
                });
            }
        }
        let mut capacity: BTreeMap<(StableRecordRef, NaiveDate), (u64, u64)> = BTreeMap::new();
        for bucket in &scenario.capacity {
            capacity.insert(
                (bucket.resource.clone(), bucket.bucket),
                (0, bucket.available_seconds),
            );
        }
        for required in &scenario.capacity_requirements {
            let value = capacity
                .entry((required.resource.clone(), required.bucket))
                .or_insert((0, 0));
            value.0 = value
                .0
                .checked_add(required.required_seconds)
                .ok_or(PlanningDomainError::Overflow)?;
        }
        let overloads = capacity
            .into_iter()
            .filter_map(
                |((resource, bucket), (required_seconds, available_seconds))| {
                    (required_seconds > available_seconds).then_some(CapacityOverload {
                        resource,
                        bucket,
                        required_seconds,
                        available_seconds,
                    })
                },
            )
            .collect();
        Ok(Self {
            id,
            scenario_id: scenario.id,
            scenario_version: scenario.version,
            policy_version: scenario.policy.version,
            input_position: scenario.input_position,
            input_digest,
            shortages,
            pegging,
            overloads,
            recommendations,
            completed_at: at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RecommendationRelease {
    pub id: ReleaseId,
    pub recommendation_id: RecommendationId,
    pub recommendation_version: u64,
    pub correlation_id: CorrelationId,
    pub kind: RecommendationKind,
    pub item: StableRecordRef,
    pub source: Option<StableRecordRef>,
    pub destination: StableRecordRef,
    pub quantity: Quantity,
    pub required_on: NaiveDate,
}

impl RecommendationRelease {
    pub fn from_approved(
        id: ReleaseId,
        value: &PlanningRecommendation,
        correlation_id: CorrelationId,
    ) -> Result<Self, PlanningDomainError> {
        if value.status != RecommendationStatus::Approved {
            return Err(PlanningDomainError::RecommendationNotApproved);
        }
        Ok(Self {
            id,
            recommendation_id: value.id,
            recommendation_version: value.version,
            correlation_id,
            kind: value.kind,
            item: value.item.clone(),
            source: value.source.clone(),
            destination: value.destination.clone(),
            quantity: value.quantity,
            required_on: value.required_on,
        })
    }
}

fn digest_scenario(value: &PlanningScenario) -> Result<String, PlanningDomainError> {
    let bytes = serde_json::to_vec(value).map_err(|_| PlanningDomainError::InvalidScenario)?;
    Ok(format!("{:x}", Sha256::digest(bytes)))
}
fn deterministic_recommendation_id(
    scenario: ScenarioId,
    demand: DemandId,
) -> Result<RecommendationId, PlanningDomainError> {
    let mut hasher = Sha256::new();
    hasher.update(scenario.as_uuid().as_bytes());
    hasher.update(demand.as_uuid().as_bytes());
    let hash = hasher.finalize();
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&hash[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x40;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    RecommendationId::try_from_uuid(Uuid::from_bytes(bytes))
}
fn positive(value: Quantity) -> Result<(), PlanningDomainError> {
    if value.units() > 0 {
        Ok(())
    } else {
        Err(PlanningDomainError::InvalidQuantity)
    }
}
fn non_negative(value: Quantity) -> Result<(), PlanningDomainError> {
    if value.units() >= 0 {
        Ok(())
    } else {
        Err(PlanningDomainError::InvalidQuantity)
    }
}
fn same_scale(left: Quantity, right: Quantity) -> Result<(), PlanningDomainError> {
    if left.scale() == right.scale() {
        Ok(())
    } else {
        Err(PlanningDomainError::QuantityScaleMismatch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum PlanningDomainError {
    #[error("invalid first-class planning plan")]
    InvalidPlan,
    #[error("invalid planning identifier: {0}")]
    InvalidIdentifier(&'static str),
    #[error("invalid planning scope")]
    InvalidScope,
    #[error("invalid record reference")]
    InvalidRecord,
    #[error("invalid fixed-precision quantity")]
    InvalidQuantity,
    #[error("quantity scales differ")]
    QuantityScaleMismatch,
    #[error("invalid planning scenario")]
    InvalidScenario,
    #[error("duplicate planning input")]
    DuplicateInput,
    #[error("capacity input references unknown demand")]
    InvalidCapacityInput,
    #[error("invalid planning lifecycle transition")]
    InvalidTransition,
    #[error("scenario inputs or policy are not pinned for execution")]
    ScenarioNotPinned,
    #[error("no sourcing rule exists for shortage")]
    MissingSourcingRule,
    #[error("override requires a reason, principal, and matching prior quantity")]
    InvalidOverride,
    #[error("recommendation is not approved")]
    RecommendationNotApproved,
    #[error("an authenticated direct human decision is required")]
    HumanDecisionRequired,
    #[error("failure or retry reason is invalid")]
    InvalidFailureReason,
    #[error("numeric overflow")]
    Overflow,
}
impl From<erpx_kernel::ScopeError> for PlanningDomainError {
    fn from(_: erpx_kernel::ScopeError) -> Self {
        Self::InvalidScope
    }
}
impl From<erpx_integration_contracts::ReferenceValidationError> for PlanningDomainError {
    fn from(_: erpx_integration_contracts::ReferenceValidationError) -> Self {
        Self::InvalidRecord
    }
}
impl From<erpx_integration_contracts::ContractValidationError> for PlanningDomainError {
    fn from(_: erpx_integration_contracts::ContractValidationError) -> Self {
        Self::InvalidQuantity
    }
}

#[cfg(test)]
mod tests;
