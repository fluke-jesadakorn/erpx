use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{CurrencyCode, MoneyAmount, RecordVersionRef};
use erpx_kernel::{AuthorityScope, CorrelationId};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

macro_rules! benefits_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);
        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, BenefitsDomainError> {
                if value.is_nil() || value.get_version_num() != 4 {
                    return Err(BenefitsDomainError::InvalidIdentifier($field));
                }
                Ok(Self(value))
            }
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }
    };
}
benefits_id!(BenefitPlanId, "benefit_plan_id");
benefits_id!(EnrollmentWindowId, "enrollment_window_id");
benefits_id!(BenefitEnrollmentId, "benefit_enrollment_id");
benefits_id!(DependentId, "dependent_id");
benefits_id!(LifeEventId, "life_event_id");
benefits_id!(ElectionId, "election_id");
benefits_id!(ProviderAcknowledgementId, "provider_acknowledgement_id");
benefits_id!(PayrollHandoffId, "payroll_handoff_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PlanStatus {
    Draft,
    Published,
    Retired,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionRule {
    pub employee_basis_points: u16,
    pub employer_basis_points: u16,
    pub employee_flat: MoneyAmount,
    pub employer_flat: MoneyAmount,
    pub employee_cap: Option<MoneyAmount>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenefitPlan {
    id: BenefitPlanId,
    version: u64,
    scope: AuthorityScope,
    name: String,
    currency: CurrencyCode,
    eligibility_class: String,
    minimum_tenure_days: u32,
    contribution: ContributionRule,
    status: PlanStatus,
    updated_at: DateTime<Utc>,
}

impl BenefitPlan {
    #[allow(clippy::too_many_arguments)]
    pub fn draft(
        id: BenefitPlanId,
        scope: AuthorityScope,
        name: impl Into<String>,
        currency: CurrencyCode,
        eligibility_class: impl Into<String>,
        minimum_tenure_days: u32,
        contribution: ContributionRule,
        at: DateTime<Utc>,
    ) -> Result<Self, BenefitsDomainError> {
        let value = Self {
            id,
            version: 1,
            scope,
            name: name.into(),
            currency,
            eligibility_class: eligibility_class.into(),
            minimum_tenure_days,
            contribution,
            status: PlanStatus::Draft,
            updated_at: at,
        };
        value.validate()?;
        Ok(value)
    }
    pub fn publish(&mut self, at: DateTime<Utc>) -> Result<(), BenefitsDomainError> {
        if self.status != PlanStatus::Draft || at < self.updated_at {
            return Err(BenefitsDomainError::InvalidTransition);
        }
        self.status = PlanStatus::Published;
        self.advance(at)
    }
    pub fn validate(&self) -> Result<(), BenefitsDomainError> {
        self.scope.validate()?;
        if self.version == 0
            || self.name.trim().is_empty()
            || self.name.len() > 255
            || self.eligibility_class.trim().is_empty()
            || self.contribution.employee_basis_points > 10_000
            || self.contribution.employer_basis_points > 10_000
            || self.contribution.employee_flat.currency() != self.currency
            || self.contribution.employer_flat.currency() != self.currency
            || self.contribution.employee_flat.minor_units() < 0
            || self.contribution.employer_flat.minor_units() < 0
            || self
                .contribution
                .employee_cap
                .is_some_and(|value| value.currency() != self.currency || value.minor_units() < 0)
        {
            return Err(BenefitsDomainError::InvalidPlan);
        }
        Ok(())
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), BenefitsDomainError> {
        self.version = self
            .version
            .checked_add(1)
            .ok_or(BenefitsDomainError::Overflow)?;
        self.updated_at = at;
        Ok(())
    }
    pub const fn id(&self) -> BenefitPlanId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn status(&self) -> PlanStatus {
        self.status
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenefitPlanRef {
    pub plan_id: BenefitPlanId,
    pub version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EnrollmentWindow {
    pub id: EnrollmentWindowId,
    pub plan: BenefitPlanRef,
    pub opens_on: NaiveDate,
    pub closes_on: NaiveDate,
    pub coverage_starts_on: NaiveDate,
}
impl EnrollmentWindow {
    pub fn validate(&self) -> Result<(), BenefitsDomainError> {
        if self.plan.version == 0
            || self.closes_on < self.opens_on
            || self.coverage_starts_on <= self.closes_on
        {
            Err(BenefitsDomainError::InvalidWindow)
        } else {
            Ok(())
        }
    }
    pub fn contains(&self, date: NaiveDate) -> bool {
        self.opens_on <= date && date <= self.closes_on
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EligibilityDecision {
    pub employee: RecordVersionRef,
    pub employment_class: String,
    pub tenure_days: u32,
    pub eligible: bool,
    pub source: RecordVersionRef,
    pub evaluated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LifeEventKind {
    Marriage,
    BirthOrAdoption,
    LossOfCoverage,
    Divorce,
    EmploymentChange,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifeEvent {
    pub id: LifeEventId,
    pub kind: LifeEventKind,
    pub occurred_on: NaiveDate,
    pub evidence: RecordVersionRef,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Dependent {
    pub id: DependentId,
    pub person_reference: String,
    pub relationship: String,
    pub eligibility_evidence: RecordVersionRef,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CoverageTier {
    Employee,
    EmployeeSpouse,
    EmployeeChildren,
    Family,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContributionCalculation {
    pub employee: MoneyAmount,
    pub employer: MoneyAmount,
    pub rule_version: u64,
    pub base_pay_source: RecordVersionRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenefitElection {
    pub id: ElectionId,
    pub tier: CoverageTier,
    pub dependent_ids: Vec<DependentId>,
    pub contribution: ContributionCalculation,
    pub effective_on: NaiveDate,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProviderAcknowledgement {
    pub id: ProviderAcknowledgementId,
    pub request_key: String,
    pub accepted: bool,
    pub provider_reference: String,
    pub reason: Option<String>,
    pub received_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum BenefitEnrollmentStatus {
    Draft,
    Elected,
    PendingProvider,
    Acknowledged,
    ProviderRejected,
    PayrollHandedOff,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenefitEnrollment {
    id: BenefitEnrollmentId,
    version: u64,
    scope: AuthorityScope,
    plan: BenefitPlanRef,
    window: EnrollmentWindowId,
    employee: RecordVersionRef,
    eligibility: EligibilityDecision,
    life_event: Option<LifeEvent>,
    dependents: Vec<Dependent>,
    election: Option<BenefitElection>,
    acknowledgements: Vec<ProviderAcknowledgement>,
    payroll_handoff_id: Option<PayrollHandoffId>,
    status: BenefitEnrollmentStatus,
    updated_at: DateTime<Utc>,
}

impl BenefitEnrollment {
    #[allow(clippy::too_many_arguments)]
    pub fn open(
        id: BenefitEnrollmentId,
        scope: AuthorityScope,
        plan: &BenefitPlan,
        window: &EnrollmentWindow,
        employee: RecordVersionRef,
        eligibility: EligibilityDecision,
        life_event: Option<LifeEvent>,
        on: NaiveDate,
        at: DateTime<Utc>,
    ) -> Result<Self, BenefitsDomainError> {
        employee.validate()?;
        eligibility.employee.validate()?;
        eligibility.source.validate()?;
        window.validate()?;
        if plan.status != PlanStatus::Published
            || scope != *plan.scope()
            || window.plan
                != (BenefitPlanRef {
                    plan_id: plan.id,
                    version: plan.version,
                })
            || employee != eligibility.employee
            || !eligibility.eligible
            || eligibility.employment_class != plan.eligibility_class
            || eligibility.tenure_days < plan.minimum_tenure_days
        {
            return Err(BenefitsDomainError::NotEligible);
        }
        if !window.contains(on) {
            let event = life_event
                .as_ref()
                .ok_or(BenefitsDomainError::WindowClosed)?;
            event.evidence.validate()?;
            if on
                .signed_duration_since(event.occurred_on)
                .num_days()
                .unsigned_abs()
                > 31
            {
                return Err(BenefitsDomainError::WindowClosed);
            }
        }
        Ok(Self {
            id,
            version: 1,
            scope,
            plan: BenefitPlanRef {
                plan_id: plan.id,
                version: plan.version,
            },
            window: window.id,
            employee,
            eligibility,
            life_event,
            dependents: Vec::new(),
            election: None,
            acknowledgements: Vec::new(),
            payroll_handoff_id: None,
            status: BenefitEnrollmentStatus::Draft,
            updated_at: at,
        })
    }
    pub fn add_dependent(
        &mut self,
        dependent: Dependent,
        at: DateTime<Utc>,
    ) -> Result<(), BenefitsDomainError> {
        if self.status != BenefitEnrollmentStatus::Draft
            || dependent.person_reference.trim().is_empty()
            || dependent.relationship.trim().is_empty()
            || self.dependents.iter().any(|value| value.id == dependent.id)
        {
            return Err(BenefitsDomainError::InvalidDependent);
        }
        dependent.eligibility_evidence.validate()?;
        self.dependents.push(dependent);
        self.advance(at)
    }
    #[allow(clippy::too_many_arguments)]
    pub fn elect(
        &mut self,
        plan: &BenefitPlan,
        election_id: ElectionId,
        tier: CoverageTier,
        dependent_ids: Vec<DependentId>,
        base_pay: MoneyAmount,
        base_pay_source: RecordVersionRef,
        effective_on: NaiveDate,
        at: DateTime<Utc>,
    ) -> Result<(), BenefitsDomainError> {
        if self.status != BenefitEnrollmentStatus::Draft
            || self.plan
                != (BenefitPlanRef {
                    plan_id: plan.id,
                    version: plan.version,
                })
            || effective_on < at.date_naive()
            || base_pay.currency() != plan.currency
            || base_pay.minor_units() < 0
        {
            return Err(BenefitsDomainError::InvalidElection);
        }
        base_pay_source.validate()?;
        let known: std::collections::BTreeSet<_> =
            self.dependents.iter().map(|value| value.id).collect();
        if dependent_ids.iter().any(|id| !known.contains(id)) {
            return Err(BenefitsDomainError::InvalidDependent);
        }
        let employee = calculate(
            base_pay,
            plan.contribution.employee_basis_points,
            plan.contribution.employee_flat,
            plan.contribution.employee_cap,
        )?;
        let employer = calculate(
            base_pay,
            plan.contribution.employer_basis_points,
            plan.contribution.employer_flat,
            None,
        )?;
        self.election = Some(BenefitElection {
            id: election_id,
            tier,
            dependent_ids,
            contribution: ContributionCalculation {
                employee,
                employer,
                rule_version: plan.version,
                base_pay_source,
            },
            effective_on,
        });
        self.status = BenefitEnrollmentStatus::Elected;
        self.advance(at)
    }
    pub fn submit_provider(&mut self, at: DateTime<Utc>) -> Result<(), BenefitsDomainError> {
        if !matches!(
            self.status,
            BenefitEnrollmentStatus::Elected | BenefitEnrollmentStatus::ProviderRejected
        ) {
            return Err(BenefitsDomainError::InvalidTransition);
        }
        self.status = BenefitEnrollmentStatus::PendingProvider;
        self.advance(at)
    }
    pub fn acknowledge_provider(
        &mut self,
        value: ProviderAcknowledgement,
    ) -> Result<bool, BenefitsDomainError> {
        if self.status != BenefitEnrollmentStatus::PendingProvider
            || value.request_key.trim().is_empty()
            || value.provider_reference.trim().is_empty()
            || value.received_at < self.updated_at
            || (!value.accepted
                && value
                    .reason
                    .as_ref()
                    .is_none_or(|reason| reason.trim().len() < 8))
        {
            return Err(BenefitsDomainError::InvalidProviderAcknowledgement);
        }
        if self
            .acknowledgements
            .iter()
            .any(|existing| existing.id == value.id || existing.request_key == value.request_key)
        {
            return Ok(false);
        }
        let accepted = value.accepted;
        let at = value.received_at;
        self.acknowledgements.push(value);
        self.status = if accepted {
            BenefitEnrollmentStatus::Acknowledged
        } else {
            BenefitEnrollmentStatus::ProviderRejected
        };
        self.advance(at)?;
        Ok(true)
    }
    pub fn payroll_handoff(
        &self,
        id: PayrollHandoffId,
        correlation_id: CorrelationId,
    ) -> Result<BenefitsPayrollHandoff, BenefitsDomainError> {
        if self.status != BenefitEnrollmentStatus::Acknowledged || self.payroll_handoff_id.is_some()
        {
            return Err(BenefitsDomainError::ProviderNotAcknowledged);
        }
        let election = self
            .election
            .as_ref()
            .ok_or(BenefitsDomainError::InvalidElection)?;
        Ok(BenefitsPayrollHandoff {
            id,
            enrollment_id: self.id,
            enrollment_version: self.version,
            employee_record_id: self.employee.record().id().as_uuid(),
            election_id: election.id,
            employee_contribution: election.contribution.employee,
            employer_contribution: election.contribution.employer,
            effective_on: election.effective_on,
            correlation_id,
        })
    }
    pub fn mark_payroll_handed_off(
        &mut self,
        id: PayrollHandoffId,
        at: DateTime<Utc>,
    ) -> Result<(), BenefitsDomainError> {
        if self.status != BenefitEnrollmentStatus::Acknowledged || self.payroll_handoff_id.is_some()
        {
            return Err(BenefitsDomainError::InvalidTransition);
        }
        self.payroll_handoff_id = Some(id);
        self.status = BenefitEnrollmentStatus::PayrollHandedOff;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), BenefitsDomainError> {
        if at < self.updated_at {
            return Err(BenefitsDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(BenefitsDomainError::Overflow)?;
        self.updated_at = at;
        Ok(())
    }
    pub const fn id(&self) -> BenefitEnrollmentId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &AuthorityScope {
        &self.scope
    }
    pub const fn status(&self) -> BenefitEnrollmentStatus {
        self.status
    }
    pub const fn plan(&self) -> BenefitPlanRef {
        self.plan
    }
    pub const fn employee(&self) -> &RecordVersionRef {
        &self.employee
    }
    pub const fn election(&self) -> Option<&BenefitElection> {
        self.election.as_ref()
    }
    pub const fn payroll_handoff_id(&self) -> Option<PayrollHandoffId> {
        self.payroll_handoff_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BenefitsPayrollHandoff {
    pub id: PayrollHandoffId,
    pub enrollment_id: BenefitEnrollmentId,
    pub enrollment_version: u64,
    pub employee_record_id: Uuid,
    pub election_id: ElectionId,
    pub employee_contribution: MoneyAmount,
    pub employer_contribution: MoneyAmount,
    pub effective_on: NaiveDate,
    pub correlation_id: CorrelationId,
}

fn calculate(
    base: MoneyAmount,
    basis_points: u16,
    flat: MoneyAmount,
    cap: Option<MoneyAmount>,
) -> Result<MoneyAmount, BenefitsDomainError> {
    let proportional = base
        .minor_units()
        .checked_mul(i64::from(basis_points))
        .and_then(|value| value.checked_div(10_000))
        .ok_or(BenefitsDomainError::Overflow)?;
    let mut total = proportional
        .checked_add(flat.minor_units())
        .ok_or(BenefitsDomainError::Overflow)?;
    if let Some(cap) = cap {
        total = total.min(cap.minor_units());
    }
    Ok(MoneyAmount::new(total, base.currency()))
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum BenefitsDomainError {
    #[error("invalid benefits identifier: {0}")]
    InvalidIdentifier(&'static str),
    #[error("invalid benefits scope")]
    InvalidScope,
    #[error("invalid record reference")]
    InvalidRecord,
    #[error("invalid benefit plan")]
    InvalidPlan,
    #[error("invalid enrollment window")]
    InvalidWindow,
    #[error("invalid benefits handoff")]
    InvalidHandoff,
    #[error("employee is not eligible")]
    NotEligible,
    #[error("enrollment window is closed")]
    WindowClosed,
    #[error("invalid dependent")]
    InvalidDependent,
    #[error("invalid benefit election")]
    InvalidElection,
    #[error("invalid provider acknowledgement")]
    InvalidProviderAcknowledgement,
    #[error("provider has not acknowledged the election")]
    ProviderNotAcknowledged,
    #[error("invalid lifecycle transition")]
    InvalidTransition,
    #[error("invalid timestamp")]
    InvalidTimestamp,
    #[error("numeric overflow")]
    Overflow,
}
impl From<erpx_kernel::ScopeError> for BenefitsDomainError {
    fn from(_: erpx_kernel::ScopeError) -> Self {
        Self::InvalidScope
    }
}
impl From<erpx_integration_contracts::ReferenceValidationError> for BenefitsDomainError {
    fn from(_: erpx_integration_contracts::ReferenceValidationError) -> Self {
        Self::InvalidRecord
    }
}

#[cfg(test)]
mod tests;
