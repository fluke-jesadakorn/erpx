use std::collections::{BTreeMap, BTreeSet};

use chrono::{DateTime, Utc};
use erpx_integration_contracts::{MoneyAmount, Quantity, StableRecordRef};
use erpx_kernel::{ActorAttribution, AuthorityScope, CommandId, CorrelationId, HumanDecisionProof};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

macro_rules! manufacturing_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);

        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, ManufacturingDomainError> {
                if value.is_nil() || value.get_version_num() != 4 {
                    return Err(ManufacturingDomainError::InvalidIdentifier($field));
                }
                Ok(Self(value))
            }

            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }
    };
}

manufacturing_id!(SpecificationId, "specification_id");
manufacturing_id!(RoutingId, "routing_id");
manufacturing_id!(ProductionOrderId, "production_order_id");
manufacturing_id!(MaterialFactId, "material_fact_id");
manufacturing_id!(ResultFactId, "result_fact_id");
manufacturing_id!(LaborFactId, "labor_fact_id");
manufacturing_id!(CostFactId, "cost_fact_id");
manufacturing_id!(DispositionId, "disposition_id");

pub fn specification_publish_decision_ref(
    id: SpecificationId,
    expected_version: u64,
    command_id: CommandId,
) -> String {
    format!(
        "manufacturing.specification:{}:publish:v{}:command:{}",
        id.as_uuid(),
        expected_version,
        command_id.as_uuid()
    )
}

pub fn production_order_release_decision_ref(
    id: ProductionOrderId,
    expected_version: u64,
    command_id: CommandId,
) -> String {
    format!(
        "manufacturing.order:{}:release:v{}:command:{}",
        id.as_uuid(),
        expected_version,
        command_id.as_uuid()
    )
}

pub fn production_order_close_decision_ref(
    id: ProductionOrderId,
    expected_version: u64,
    command_id: CommandId,
) -> String {
    format!(
        "manufacturing.order:{}:close:v{}:command:{}",
        id.as_uuid(),
        expected_version,
        command_id.as_uuid()
    )
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManufacturingHumanDecisionEvidence {
    pub actor: ActorAttribution,
    pub authority_scope: AuthorityScope,
    pub decision_ref: String,
    pub decided_at: DateTime<Utc>,
}

impl ManufacturingHumanDecisionEvidence {
    fn retain(decision: &HumanDecisionProof) -> Self {
        Self {
            actor: decision.actor().clone(),
            authority_scope: decision.authority_scope().clone(),
            decision_ref: decision.decision_ref().to_owned(),
            decided_at: decision.decided_at(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ManufacturingMode {
    Discrete,
    Process,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaterialRole {
    Component,
    ProcessInput,
    CoProduct,
    ByProduct,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialLine {
    pub line: u16,
    pub item: StableRecordRef,
    pub quantity: Quantity,
    pub role: MaterialRole,
    pub expected_scrap_basis_points: u16,
}

impl MaterialLine {
    fn validate(&self, mode: ManufacturingMode) -> Result<(), ManufacturingDomainError> {
        self.item.validate()?;
        positive(self.quantity)?;
        if self.line == 0 || self.expected_scrap_basis_points > 10_000 {
            return Err(ManufacturingDomainError::InvalidSpecification);
        }
        match (mode, self.role) {
            (ManufacturingMode::Discrete, MaterialRole::ProcessInput)
            | (ManufacturingMode::Process, MaterialRole::Component) => {
                Err(ManufacturingDomainError::InvalidMaterialRole)
            }
            _ => Ok(()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RoutingStep {
    pub sequence: u16,
    pub operation_code: String,
    pub work_center: StableRecordRef,
    pub setup_seconds: u64,
    pub run_seconds_per_output: u64,
}

impl RoutingStep {
    fn validate(&self) -> Result<(), ManufacturingDomainError> {
        self.work_center.validate()?;
        if self.sequence == 0
            || self.operation_code.trim().is_empty()
            || self.operation_code.len() > 64
            || self.run_seconds_per_output == 0
        {
            return Err(ManufacturingDomainError::InvalidRouting);
        }
        Ok(())
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpecificationStatus {
    Draft,
    Released,
    Retired,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct SpecificationRef {
    pub specification_id: SpecificationId,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManufacturingSpecification {
    id: SpecificationId,
    version: u64,
    scope: AuthorityScope,
    mode: ManufacturingMode,
    output_item: StableRecordRef,
    output_quantity: Quantity,
    materials: Vec<MaterialLine>,
    routing_id: RoutingId,
    routing: Vec<RoutingStep>,
    status: SpecificationStatus,
    updated_at: DateTime<Utc>,
    #[serde(default)]
    publish_decision: Option<Box<ManufacturingHumanDecisionEvidence>>,
}

impl ManufacturingSpecification {
    #[allow(clippy::too_many_arguments)]
    pub fn draft(
        id: SpecificationId,
        scope: AuthorityScope,
        mode: ManufacturingMode,
        output_item: StableRecordRef,
        output_quantity: Quantity,
        materials: Vec<MaterialLine>,
        routing_id: RoutingId,
        routing: Vec<RoutingStep>,
        at: DateTime<Utc>,
    ) -> Result<Self, ManufacturingDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            mode,
            output_item,
            output_quantity,
            materials,
            routing_id,
            routing,
            status: SpecificationStatus::Draft,
            updated_at: at,
            publish_decision: None,
        };
        value.validate()?;
        Ok(value)
    }

    pub fn release(
        &mut self,
        command_id: CommandId,
        decision: &HumanDecisionProof,
    ) -> Result<(), ManufacturingDomainError> {
        require_exact_human_decision(
            decision,
            &self.scope,
            &specification_publish_decision_ref(self.id, self.version, command_id),
        )?;
        let at = decision.decided_at();
        if self.status != SpecificationStatus::Draft || at < self.updated_at {
            return Err(ManufacturingDomainError::InvalidTransition);
        }
        self.validate()?;
        self.status = SpecificationStatus::Released;
        self.publish_decision = Some(Box::new(ManufacturingHumanDecisionEvidence::retain(
            decision,
        )));
        self.advance(at)
    }

    pub fn retire(&mut self, at: DateTime<Utc>) -> Result<(), ManufacturingDomainError> {
        if self.status != SpecificationStatus::Released || at < self.updated_at {
            return Err(ManufacturingDomainError::InvalidTransition);
        }
        self.status = SpecificationStatus::Retired;
        self.advance(at)
    }

    pub fn validate(&self) -> Result<(), ManufacturingDomainError> {
        self.scope.validate()?;
        self.output_item.validate()?;
        positive(self.output_quantity)?;
        if self.version == 0 || self.materials.is_empty() || self.routing.is_empty() {
            return Err(ManufacturingDomainError::InvalidSpecification);
        }
        let mut lines = BTreeSet::new();
        for material in &self.materials {
            material.validate(self.mode)?;
            if !lines.insert(material.line) {
                return Err(ManufacturingDomainError::InvalidSpecification);
            }
        }
        if self.mode == ManufacturingMode::Process
            && !self
                .materials
                .iter()
                .any(|line| line.role == MaterialRole::ProcessInput)
        {
            return Err(ManufacturingDomainError::InvalidSpecification);
        }
        let mut steps = BTreeSet::new();
        for step in &self.routing {
            step.validate()?;
            if !steps.insert(step.sequence) {
                return Err(ManufacturingDomainError::InvalidRouting);
            }
        }
        Ok(())
    }

    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), ManufacturingDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(ManufacturingDomainError::Overflow)?;
        self.updated_at = at;
        Ok(())
    }

    pub const fn reference(&self) -> SpecificationRef {
        SpecificationRef {
            specification_id: self.id,
            version: self.version,
        }
    }
    pub const fn status(&self) -> SpecificationStatus {
        self.status
    }
    pub const fn mode(&self) -> ManufacturingMode {
        self.mode
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub fn materials(&self) -> &[MaterialLine] {
        &self.materials
    }
    pub fn routing(&self) -> &[RoutingStep] {
        &self.routing
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductionStatus {
    Draft,
    Released,
    InProgress,
    Completed,
    Closed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaterialBalance {
    pub line: u16,
    pub required: Quantity,
    pub reserved: Quantity,
    pub issued: Quantity,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionResult {
    pub id: ResultFactId,
    pub item: StableRecordRef,
    pub lot_or_serial: String,
    pub good: Quantity,
    pub scrap: Quantity,
    pub is_co_product: bool,
    pub source_lots: Vec<String>,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaborResult {
    pub id: LaborFactId,
    pub routing_sequence: u16,
    pub labor_seconds: u64,
    pub machine_seconds: u64,
    pub at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Disposition {
    Accepted,
    Rejected,
    Hold,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityDisposition {
    pub id: DispositionId,
    pub result_id: ResultFactId,
    pub quantity: Quantity,
    pub disposition: Disposition,
    pub authority_record: StableRecordRef,
    pub source_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ManufacturingCostFact {
    pub id: CostFactId,
    pub amount: MoneyAmount,
    pub category: String,
    pub owner_record: StableRecordRef,
    pub owner_version: u64,
    pub reconciled: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductionOrder {
    id: ProductionOrderId,
    version: u64,
    scope: AuthorityScope,
    specification: SpecificationRef,
    mode: ManufacturingMode,
    planned_output: Quantity,
    status: ProductionStatus,
    material: BTreeMap<u16, MaterialBalance>,
    material_fact_ids: BTreeSet<MaterialFactId>,
    results: BTreeMap<ResultFactId, ProductionResult>,
    labor: BTreeMap<LaborFactId, LaborResult>,
    dispositions: BTreeMap<DispositionId, QualityDisposition>,
    costs: BTreeMap<CostFactId, ManufacturingCostFact>,
    correlation_id: CorrelationId,
    updated_at: DateTime<Utc>,
    #[serde(default)]
    release_decision: Option<Box<ManufacturingHumanDecisionEvidence>>,
    #[serde(default)]
    close_decision: Option<Box<ManufacturingHumanDecisionEvidence>>,
}

impl ProductionOrder {
    pub fn draft(
        id: ProductionOrderId,
        scope: AuthorityScope,
        specification: &ManufacturingSpecification,
        planned_output: Quantity,
        correlation_id: CorrelationId,
        at: DateTime<Utc>,
    ) -> Result<Self, ManufacturingDomainError> {
        scope.validate()?;
        if scope != *specification.scope()
            || specification.status() != SpecificationStatus::Released
        {
            return Err(ManufacturingDomainError::SpecificationNotReleased);
        }
        positive(planned_output)?;
        same_scale(planned_output, specification.output_quantity)?;
        let material = specification
            .materials()
            .iter()
            .filter(|line| {
                matches!(
                    line.role,
                    MaterialRole::Component | MaterialRole::ProcessInput
                )
            })
            .map(|line| {
                let units = scaled_requirement(
                    line.quantity,
                    planned_output,
                    specification.output_quantity,
                )?;
                let zero = Quantity::try_new(0, line.quantity.scale())?;
                Ok((
                    line.line,
                    MaterialBalance {
                        line: line.line,
                        required: units,
                        reserved: zero,
                        issued: zero,
                    },
                ))
            })
            .collect::<Result<BTreeMap<_, _>, ManufacturingDomainError>>()?;
        Ok(Self {
            id,
            version: 1,
            scope,
            specification: specification.reference(),
            mode: specification.mode(),
            planned_output,
            status: ProductionStatus::Draft,
            material,
            material_fact_ids: BTreeSet::new(),
            results: BTreeMap::new(),
            labor: BTreeMap::new(),
            dispositions: BTreeMap::new(),
            costs: BTreeMap::new(),
            correlation_id,
            updated_at: at,
            release_decision: None,
            close_decision: None,
        })
    }

    pub fn release(
        &mut self,
        command_id: CommandId,
        decision: &HumanDecisionProof,
    ) -> Result<(), ManufacturingDomainError> {
        require_exact_human_decision(
            decision,
            &self.scope,
            &production_order_release_decision_ref(self.id, self.version, command_id),
        )?;
        self.transition(
            ProductionStatus::Draft,
            ProductionStatus::Released,
            decision.decided_at(),
        )?;
        self.release_decision = Some(Box::new(ManufacturingHumanDecisionEvidence::retain(
            decision,
        )));
        Ok(())
    }

    pub fn reserve_material(
        &mut self,
        line: u16,
        quantity: Quantity,
        at: DateTime<Utc>,
    ) -> Result<(), ManufacturingDomainError> {
        if !matches!(
            self.status,
            ProductionStatus::Released | ProductionStatus::InProgress
        ) {
            return Err(ManufacturingDomainError::InvalidTransition);
        }
        let balance = self
            .material
            .get_mut(&line)
            .ok_or(ManufacturingDomainError::UnknownMaterialLine)?;
        same_scale(quantity, balance.required)?;
        non_negative(quantity)?;
        if quantity.units() > balance.required.units() {
            return Err(ManufacturingDomainError::MaterialOverReservation);
        }
        balance.reserved = quantity;
        self.advance(at)
    }

    pub fn start(&mut self, at: DateTime<Utc>) -> Result<(), ManufacturingDomainError> {
        if self
            .material
            .values()
            .any(|value| value.reserved.units() < value.required.units())
        {
            return Err(ManufacturingDomainError::MaterialNotReserved);
        }
        self.transition(ProductionStatus::Released, ProductionStatus::InProgress, at)
    }

    pub fn issue_material(
        &mut self,
        fact_id: MaterialFactId,
        line: u16,
        quantity: Quantity,
        at: DateTime<Utc>,
    ) -> Result<bool, ManufacturingDomainError> {
        if self.status != ProductionStatus::InProgress {
            return Err(ManufacturingDomainError::InvalidTransition);
        }
        if !self.material_fact_ids.insert(fact_id) {
            return Ok(false);
        }
        let balance = self
            .material
            .get_mut(&line)
            .ok_or(ManufacturingDomainError::UnknownMaterialLine)?;
        same_scale(quantity, balance.issued)?;
        positive(quantity)?;
        let next = balance
            .issued
            .units()
            .checked_add(quantity.units())
            .ok_or(ManufacturingDomainError::Overflow)?;
        if next > balance.reserved.units() {
            self.material_fact_ids.remove(&fact_id);
            return Err(ManufacturingDomainError::MaterialOverIssue);
        }
        balance.issued = Quantity::try_new(next, quantity.scale())?;
        self.advance(at)?;
        Ok(true)
    }

    pub fn report_labor(&mut self, value: LaborResult) -> Result<bool, ManufacturingDomainError> {
        if self.status != ProductionStatus::InProgress || value.labor_seconds == 0 {
            return Err(ManufacturingDomainError::InvalidResult);
        }
        if self.labor.contains_key(&value.id) {
            return Ok(false);
        }
        let at = value.at;
        self.labor.insert(value.id, value);
        self.advance(at)?;
        Ok(true)
    }

    pub fn report_result(
        &mut self,
        value: ProductionResult,
    ) -> Result<bool, ManufacturingDomainError> {
        if self.status != ProductionStatus::InProgress {
            return Err(ManufacturingDomainError::InvalidTransition);
        }
        validate_result(&value, self.mode)?;
        if self.results.contains_key(&value.id) {
            return Ok(false);
        }
        let at = value.at;
        self.results.insert(value.id, value);
        self.advance(at)?;
        Ok(true)
    }

    pub fn apply_disposition(
        &mut self,
        value: QualityDisposition,
        at: DateTime<Utc>,
    ) -> Result<bool, ManufacturingDomainError> {
        let result = self
            .results
            .get(&value.result_id)
            .ok_or(ManufacturingDomainError::UnknownResult)?;
        value.authority_record.validate()?;
        positive(value.quantity)?;
        same_scale(value.quantity, result.good)?;
        if value.source_version == 0 || value.quantity.units() > result.good.units() {
            return Err(ManufacturingDomainError::InvalidDisposition);
        }
        if self.dispositions.contains_key(&value.id) {
            return Ok(false);
        }
        self.dispositions.insert(value.id, value);
        self.advance(at)?;
        Ok(true)
    }

    pub fn record_cost(
        &mut self,
        fact: ManufacturingCostFact,
        at: DateTime<Utc>,
    ) -> Result<bool, ManufacturingDomainError> {
        fact.owner_record.validate()?;
        if fact.owner_version == 0
            || fact.amount.minor_units() < 0
            || fact.category.trim().is_empty()
        {
            return Err(ManufacturingDomainError::InvalidCost);
        }
        if self.costs.contains_key(&fact.id) {
            return Ok(false);
        }
        self.costs.insert(fact.id, fact);
        self.advance(at)?;
        Ok(true)
    }

    pub fn complete(&mut self, at: DateTime<Utc>) -> Result<(), ManufacturingDomainError> {
        if self.results.is_empty()
            || self
                .material
                .values()
                .any(|value| value.issued.units() == 0)
        {
            return Err(ManufacturingDomainError::IncompleteReconciliation);
        }
        self.transition(
            ProductionStatus::InProgress,
            ProductionStatus::Completed,
            at,
        )
    }

    pub fn close(
        &mut self,
        command_id: CommandId,
        decision: &HumanDecisionProof,
    ) -> Result<(), ManufacturingDomainError> {
        require_exact_human_decision(
            decision,
            &self.scope,
            &production_order_close_decision_ref(self.id, self.version, command_id),
        )?;
        if self
            .dispositions
            .values()
            .any(|value| value.disposition == Disposition::Hold)
            || self.costs.is_empty()
            || self.costs.values().any(|value| !value.reconciled)
        {
            return Err(ManufacturingDomainError::IncompleteReconciliation);
        }
        self.transition(
            ProductionStatus::Completed,
            ProductionStatus::Closed,
            decision.decided_at(),
        )?;
        self.close_decision = Some(Box::new(ManufacturingHumanDecisionEvidence::retain(
            decision,
        )));
        Ok(())
    }

    fn transition(
        &mut self,
        expected: ProductionStatus,
        next: ProductionStatus,
        at: DateTime<Utc>,
    ) -> Result<(), ManufacturingDomainError> {
        if self.status != expected || at < self.updated_at {
            return Err(ManufacturingDomainError::InvalidTransition);
        }
        self.status = next;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), ManufacturingDomainError> {
        if at < self.updated_at {
            return Err(ManufacturingDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(ManufacturingDomainError::Overflow)?;
        self.updated_at = at;
        Ok(())
    }
    pub const fn id(&self) -> ProductionOrderId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub const fn status(&self) -> ProductionStatus {
        self.status
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn specification(&self) -> SpecificationRef {
        self.specification
    }
    pub const fn correlation_id(&self) -> CorrelationId {
        self.correlation_id
    }
}

fn validate_result(
    value: &ProductionResult,
    mode: ManufacturingMode,
) -> Result<(), ManufacturingDomainError> {
    value.item.validate()?;
    positive(value.good)?;
    non_negative(value.scrap)?;
    same_scale(value.good, value.scrap)?;
    if value.lot_or_serial.trim().is_empty()
        || value.lot_or_serial.len() > 128
        || value
            .source_lots
            .iter()
            .any(|lot| lot.trim().is_empty() || lot.len() > 128)
    {
        return Err(ManufacturingDomainError::InvalidGenealogy);
    }
    if mode == ManufacturingMode::Discrete && value.is_co_product {
        return Err(ManufacturingDomainError::InvalidResult);
    }
    Ok(())
}

fn scaled_requirement(
    line: Quantity,
    planned: Quantity,
    base: Quantity,
) -> Result<Quantity, ManufacturingDomainError> {
    same_scale(planned, base)?;
    let units = line
        .units()
        .checked_mul(planned.units())
        .and_then(|value| value.checked_div(base.units()))
        .ok_or(ManufacturingDomainError::Overflow)?;
    Quantity::try_new(units, line.scale()).map_err(Into::into)
}
fn positive(value: Quantity) -> Result<(), ManufacturingDomainError> {
    if value.units() > 0 {
        Ok(())
    } else {
        Err(ManufacturingDomainError::InvalidQuantity)
    }
}
fn non_negative(value: Quantity) -> Result<(), ManufacturingDomainError> {
    if value.units() >= 0 {
        Ok(())
    } else {
        Err(ManufacturingDomainError::InvalidQuantity)
    }
}
fn same_scale(left: Quantity, right: Quantity) -> Result<(), ManufacturingDomainError> {
    if left.scale() == right.scale() {
        Ok(())
    } else {
        Err(ManufacturingDomainError::QuantityScaleMismatch)
    }
}

fn require_exact_human_decision(
    decision: &HumanDecisionProof,
    scope: &AuthorityScope,
    expected_ref: &str,
) -> Result<(), ManufacturingDomainError> {
    if decision.authority_scope() != scope
        || decision.decision_ref() != expected_ref
        || !decision.actor().is_human()
        || decision.actor().session_id.is_none()
        || decision.actor().represented_principal_ref.is_some()
        || decision.actor().support_capability_id.is_some()
    {
        Err(ManufacturingDomainError::HumanDecisionRequired)
    } else {
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ManufacturingDomainError {
    #[error("invalid manufacturing identifier: {0}")]
    InvalidIdentifier(&'static str),
    #[error("invalid manufacturing scope")]
    InvalidScope,
    #[error("invalid record reference")]
    InvalidRecord,
    #[error("invalid fixed-precision quantity")]
    InvalidQuantity,
    #[error("quantity scales differ")]
    QuantityScaleMismatch,
    #[error("invalid specification")]
    InvalidSpecification,
    #[error("invalid material role for manufacturing mode")]
    InvalidMaterialRole,
    #[error("invalid routing")]
    InvalidRouting,
    #[error("specification is not released in the order scope")]
    SpecificationNotReleased,
    #[error("invalid lifecycle transition")]
    InvalidTransition,
    #[error("an authenticated direct human decision is required")]
    HumanDecisionRequired,
    #[error("invalid event timestamp")]
    InvalidTimestamp,
    #[error("unknown material line")]
    UnknownMaterialLine,
    #[error("material is not fully reserved")]
    MaterialNotReserved,
    #[error("material reservation exceeds requirement")]
    MaterialOverReservation,
    #[error("material issue exceeds reservation")]
    MaterialOverIssue,
    #[error("invalid production result")]
    InvalidResult,
    #[error("invalid lot/serial genealogy")]
    InvalidGenealogy,
    #[error("unknown production result")]
    UnknownResult,
    #[error("invalid quality disposition")]
    InvalidDisposition,
    #[error("invalid manufacturing cost fact")]
    InvalidCost,
    #[error("invalid first-class manufacturing record lifecycle")]
    InvalidRecordLifecycle,
    #[error("WIP, quality, or cost reconciliation is incomplete")]
    IncompleteReconciliation,
    #[error("numeric overflow")]
    Overflow,
}

impl From<erpx_kernel::ScopeError> for ManufacturingDomainError {
    fn from(_: erpx_kernel::ScopeError) -> Self {
        Self::InvalidScope
    }
}
impl From<erpx_integration_contracts::ReferenceValidationError> for ManufacturingDomainError {
    fn from(_: erpx_integration_contracts::ReferenceValidationError) -> Self {
        Self::InvalidRecord
    }
}
impl From<erpx_integration_contracts::ContractValidationError> for ManufacturingDomainError {
    fn from(_: erpx_integration_contracts::ContractValidationError) -> Self {
        Self::InvalidQuantity
    }
}

#[cfg(test)]
mod tests;
