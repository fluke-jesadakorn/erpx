use std::collections::BTreeSet;

use chrono::{DateTime, NaiveDate, Utc};
use erpx_integration_contracts::{MoneyAmount, Quantity, RecordVersionRef, StableRecordRef};
use erpx_kernel::{CorrelationId, PrincipalId, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

macro_rules! field_id {
    ($name:ident, $field:literal) => {
        #[derive(
            Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
        )]
        #[serde(transparent)]
        pub struct $name(Uuid);
        impl $name {
            pub fn try_from_uuid(value: Uuid) -> Result<Self, FieldServiceDomainError> {
                if value.is_nil() || value.get_version_num() != 4 {
                    return Err(FieldServiceDomainError::InvalidIdentifier($field));
                }
                Ok(Self(value))
            }
            pub const fn as_uuid(self) -> Uuid {
                self.0
            }
        }
    };
}
field_id!(ServiceAgreementId, "service_agreement_id");
field_id!(ServiceRequestId, "service_request_id");
field_id!(ServiceOrderId, "service_order_id");
field_id!(DispatchId, "dispatch_id");
field_id!(VisitId, "visit_id");
field_id!(PartFactId, "part_fact_id");
field_id!(ExceptionId, "exception_id");
field_id!(BillingHandoffId, "billing_handoff_id");

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Connectivity {
    Online,
    Offline,
}
fn require_online(value: Connectivity) -> Result<(), FieldServiceDomainError> {
    if value == Connectivity::Online {
        Ok(())
    } else {
        Err(FieldServiceDomainError::OnlineRequired)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceAgreement {
    id: ServiceAgreementId,
    version: u64,
    scope: TenantScope,
    customer: RecordVersionRef,
    covered_assets: BTreeSet<StableRecordRef>,
    covered_locations: BTreeSet<StableRecordRef>,
    service_codes: BTreeSet<String>,
    valid_from: NaiveDate,
    valid_through: NaiveDate,
    active: bool,
}
impl ServiceAgreement {
    #[allow(clippy::too_many_arguments)]
    pub fn activate(
        id: ServiceAgreementId,
        scope: TenantScope,
        customer: RecordVersionRef,
        covered_assets: BTreeSet<StableRecordRef>,
        covered_locations: BTreeSet<StableRecordRef>,
        service_codes: BTreeSet<String>,
        valid_from: NaiveDate,
        valid_through: NaiveDate,
    ) -> Result<Self, FieldServiceDomainError> {
        customer.validate()?;
        scope.validate()?;
        for value in &covered_assets {
            value.validate()?;
        }
        for value in &covered_locations {
            value.validate()?;
        }
        if covered_assets.is_empty()
            || covered_locations.is_empty()
            || service_codes.is_empty()
            || service_codes
                .iter()
                .any(|value| value.trim().is_empty() || value.len() > 64)
            || valid_through < valid_from
        {
            return Err(FieldServiceDomainError::InvalidAgreement);
        }
        Ok(Self {
            id,
            version: 1,
            scope,
            customer,
            covered_assets,
            covered_locations,
            service_codes,
            valid_from,
            valid_through,
            active: true,
        })
    }
    pub fn covers(
        &self,
        customer: &RecordVersionRef,
        asset: &RecordVersionRef,
        location: &StableRecordRef,
        service_code: &str,
        on: NaiveDate,
    ) -> bool {
        self.active
            && &self.customer == customer
            && self.covered_assets.contains(asset.record())
            && self.covered_locations.contains(location)
            && self.service_codes.contains(service_code)
            && self.valid_from <= on
            && on <= self.valid_through
    }
    pub const fn id(&self) -> ServiceAgreementId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceRequest {
    pub id: ServiceRequestId,
    pub version: u64,
    pub scope: TenantScope,
    pub customer: RecordVersionRef,
    pub asset: RecordVersionRef,
    pub location: StableRecordRef,
    pub service_code: String,
    pub requested_at: DateTime<Utc>,
}
impl ServiceRequest {
    pub fn validate(&self) -> Result<(), FieldServiceDomainError> {
        self.scope.validate()?;
        self.customer.validate()?;
        self.asset.validate()?;
        self.location.validate()?;
        if self.version == 0 || self.service_code.trim().is_empty() || self.service_code.len() > 64
        {
            Err(FieldServiceDomainError::InvalidRequest)
        } else {
            Ok(())
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TechnicianQualification {
    pub technician: RecordVersionRef,
    pub principal_id: PrincipalId,
    pub skills: BTreeSet<String>,
    pub valid_through: NaiveDate,
    pub evidence: RecordVersionRef,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartReservation {
    pub part: RecordVersionRef,
    pub reserved: Quantity,
    pub issued: Quantity,
    pub inventory_reservation: RecordVersionRef,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "outcome")]
pub enum CustomerSignature {
    Signed {
        evidence: RecordVersionRef,
    },
    Refused {
        exception_id: ExceptionId,
        reason: String,
        witnessed_by: PrincipalId,
        evidence: RecordVersionRef,
    },
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExceptionKind {
    CustomerMismatch,
    AssetMismatch,
    SignatureRefused,
    CoverageMismatch,
    InventoryMismatch,
}
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceException {
    pub id: ExceptionId,
    pub kind: ExceptionKind,
    pub reason: String,
    pub evidence: RecordVersionRef,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ServiceOrderStatus {
    Draft,
    Scheduled,
    Dispatched,
    InProgress,
    Completed,
    BillingHandedOff,
    Closed,
    Cancelled,
    Exception,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceOrder {
    id: ServiceOrderId,
    version: u64,
    scope: TenantScope,
    request: ServiceRequestId,
    request_version: u64,
    agreement: ServiceAgreementId,
    agreement_version: u64,
    customer: RecordVersionRef,
    asset: RecordVersionRef,
    location: StableRecordRef,
    service_code: String,
    required_skills: BTreeSet<String>,
    scheduled_on: Option<NaiveDate>,
    technician: Option<TechnicianQualification>,
    dispatch_id: Option<DispatchId>,
    visit_id: Option<VisitId>,
    parts: Vec<PartReservation>,
    part_fact_ids: BTreeSet<PartFactId>,
    evidence: Vec<RecordVersionRef>,
    signature: Option<CustomerSignature>,
    exceptions: Vec<ServiceException>,
    billing_handoff_id: Option<BillingHandoffId>,
    status: ServiceOrderStatus,
    updated_at: DateTime<Utc>,
}

impl ServiceOrder {
    pub fn create(
        id: ServiceOrderId,
        request: &ServiceRequest,
        agreement: &ServiceAgreement,
        required_skills: BTreeSet<String>,
        at: DateTime<Utc>,
    ) -> Result<Self, FieldServiceDomainError> {
        request.validate()?;
        if request.scope != *agreement.scope()
            || required_skills.is_empty()
            || !agreement.covers(
                &request.customer,
                &request.asset,
                &request.location,
                &request.service_code,
                at.date_naive(),
            )
        {
            return Err(FieldServiceDomainError::CoverageDenied);
        }
        Ok(Self {
            id,
            version: 1,
            scope: request.scope.clone(),
            request: request.id,
            request_version: request.version,
            agreement: agreement.id,
            agreement_version: agreement.version,
            customer: request.customer.clone(),
            asset: request.asset.clone(),
            location: request.location.clone(),
            service_code: request.service_code.clone(),
            required_skills,
            scheduled_on: None,
            technician: None,
            dispatch_id: None,
            visit_id: None,
            parts: Vec::new(),
            part_fact_ids: BTreeSet::new(),
            evidence: Vec::new(),
            signature: None,
            exceptions: Vec::new(),
            billing_handoff_id: None,
            status: ServiceOrderStatus::Draft,
            updated_at: at,
        })
    }
    pub fn schedule(
        &mut self,
        connectivity: Connectivity,
        on: NaiveDate,
        qualification: TechnicianQualification,
        at: DateTime<Utc>,
    ) -> Result<(), FieldServiceDomainError> {
        require_online(connectivity)?;
        qualification.technician.validate()?;
        qualification.evidence.validate()?;
        if self.status != ServiceOrderStatus::Draft
            || on < at.date_naive()
            || qualification.valid_through < on
            || !self.required_skills.is_subset(&qualification.skills)
        {
            return Err(FieldServiceDomainError::SkillOrScheduleDenied);
        }
        self.scheduled_on = Some(on);
        self.technician = Some(qualification);
        self.status = ServiceOrderStatus::Scheduled;
        self.advance(at)
    }
    pub fn reserve_part(
        &mut self,
        connectivity: Connectivity,
        part: PartReservation,
        at: DateTime<Utc>,
    ) -> Result<(), FieldServiceDomainError> {
        require_online(connectivity)?;
        part.part.validate()?;
        part.inventory_reservation.validate()?;
        positive(part.reserved)?;
        same_scale(part.reserved, part.issued)?;
        if part.issued.units() != 0
            || !matches!(
                self.status,
                ServiceOrderStatus::Draft | ServiceOrderStatus::Scheduled
            )
        {
            return Err(FieldServiceDomainError::InventoryGateDenied);
        }
        self.parts.push(part);
        self.advance(at)
    }
    pub fn dispatch(
        &mut self,
        connectivity: Connectivity,
        id: DispatchId,
        agreement: &ServiceAgreement,
        at: DateTime<Utc>,
    ) -> Result<(), FieldServiceDomainError> {
        require_online(connectivity)?;
        let scheduled = self
            .scheduled_on
            .ok_or(FieldServiceDomainError::SkillOrScheduleDenied)?;
        if self.status != ServiceOrderStatus::Scheduled
            || self.technician.is_none()
            || self.parts.iter().any(|value| value.reserved.units() <= 0)
            || !agreement.covers(
                &self.customer,
                &self.asset,
                &self.location,
                &self.service_code,
                scheduled,
            )
        {
            return Err(FieldServiceDomainError::DispatchGateDenied);
        }
        self.dispatch_id = Some(id);
        self.status = ServiceOrderStatus::Dispatched;
        self.advance(at)
    }
    pub fn start_visit(
        &mut self,
        connectivity: Connectivity,
        id: VisitId,
        presented_customer: &RecordVersionRef,
        presented_asset: &RecordVersionRef,
        at: DateTime<Utc>,
    ) -> Result<(), FieldServiceDomainError> {
        require_online(connectivity)?;
        if self.status != ServiceOrderStatus::Dispatched {
            return Err(FieldServiceDomainError::InvalidTransition);
        }
        if presented_customer != &self.customer {
            return Err(FieldServiceDomainError::ExplicitMismatchRequired(
                ExceptionKind::CustomerMismatch,
            ));
        }
        if presented_asset != &self.asset {
            return Err(FieldServiceDomainError::ExplicitMismatchRequired(
                ExceptionKind::AssetMismatch,
            ));
        }
        self.visit_id = Some(id);
        self.status = ServiceOrderStatus::InProgress;
        self.advance(at)
    }
    pub fn record_exception(
        &mut self,
        connectivity: Connectivity,
        value: ServiceException,
    ) -> Result<(), FieldServiceDomainError> {
        require_online(connectivity)?;
        value.evidence.validate()?;
        if value.reason.trim().len() < 8
            || value.reason.len() > 500
            || value.recorded_at < self.updated_at
        {
            return Err(FieldServiceDomainError::InvalidException);
        }
        let at = value.recorded_at;
        self.exceptions.push(value);
        self.status = ServiceOrderStatus::Exception;
        self.advance(at)
    }
    pub fn issue_part(
        &mut self,
        connectivity: Connectivity,
        id: PartFactId,
        part_record_id: Uuid,
        quantity: Quantity,
        at: DateTime<Utc>,
    ) -> Result<bool, FieldServiceDomainError> {
        require_online(connectivity)?;
        if self.status != ServiceOrderStatus::InProgress {
            return Err(FieldServiceDomainError::InvalidTransition);
        }
        if !self.part_fact_ids.insert(id) {
            return Ok(false);
        }
        let part = self
            .parts
            .iter_mut()
            .find(|value| value.part.record().id().as_uuid() == part_record_id)
            .ok_or(FieldServiceDomainError::InventoryGateDenied)?;
        same_scale(quantity, part.issued)?;
        positive(quantity)?;
        let next = part
            .issued
            .units()
            .checked_add(quantity.units())
            .ok_or(FieldServiceDomainError::Overflow)?;
        if next > part.reserved.units() {
            self.part_fact_ids.remove(&id);
            return Err(FieldServiceDomainError::InventoryGateDenied);
        }
        part.issued = Quantity::try_new(next, quantity.scale())?;
        self.advance(at)?;
        Ok(true)
    }
    pub fn complete_visit(
        &mut self,
        connectivity: Connectivity,
        evidence: Vec<RecordVersionRef>,
        signature: CustomerSignature,
        at: DateTime<Utc>,
    ) -> Result<(), FieldServiceDomainError> {
        require_online(connectivity)?;
        if self.status != ServiceOrderStatus::InProgress || evidence.is_empty() {
            return Err(FieldServiceDomainError::VisitEvidenceRequired);
        }
        for value in &evidence {
            value.validate()?;
        }
        match &signature {
            CustomerSignature::Signed { evidence } => evidence.validate()?,
            CustomerSignature::Refused {
                reason, evidence, ..
            } => {
                evidence.validate()?;
                if reason.trim().len() < 8 || reason.len() > 500 {
                    return Err(FieldServiceDomainError::ExplicitRefusalRequired);
                }
            }
        }
        if let CustomerSignature::Refused {
            exception_id,
            reason,
            evidence,
            ..
        } = &signature
        {
            self.exceptions.push(ServiceException {
                id: *exception_id,
                kind: ExceptionKind::SignatureRefused,
                reason: reason.clone(),
                evidence: evidence.clone(),
                recorded_at: at,
            });
        }
        self.evidence = evidence;
        self.signature = Some(signature);
        self.status = ServiceOrderStatus::Completed;
        self.advance(at)
    }
    pub fn billing_handoff(
        &self,
        id: BillingHandoffId,
        amount: MoneyAmount,
        correlation_id: CorrelationId,
    ) -> Result<FieldServiceBillingHandoff, FieldServiceDomainError> {
        if self.status != ServiceOrderStatus::Completed
            || self.billing_handoff_id.is_some()
            || amount.minor_units() < 0
        {
            return Err(FieldServiceDomainError::BillingGateDenied);
        }
        Ok(FieldServiceBillingHandoff {
            id,
            service_order_id: self.id,
            service_order_version: self.version,
            customer_record_id: self.customer.record().id().as_uuid(),
            asset_record_id: self.asset.record().id().as_uuid(),
            amount,
            completion_evidence: self.evidence.clone(),
            correlation_id,
        })
    }
    pub fn mark_billing_handed_off(
        &mut self,
        id: BillingHandoffId,
        at: DateTime<Utc>,
    ) -> Result<(), FieldServiceDomainError> {
        if self.status != ServiceOrderStatus::Completed || self.billing_handoff_id.is_some() {
            return Err(FieldServiceDomainError::InvalidTransition);
        }
        self.billing_handoff_id = Some(id);
        self.status = ServiceOrderStatus::BillingHandedOff;
        self.advance(at)
    }
    fn advance(&mut self, at: DateTime<Utc>) -> Result<(), FieldServiceDomainError> {
        if at < self.updated_at {
            return Err(FieldServiceDomainError::InvalidTimestamp);
        }
        self.version = self
            .version
            .checked_add(1)
            .ok_or(FieldServiceDomainError::Overflow)?;
        self.updated_at = at;
        Ok(())
    }
    pub const fn id(&self) -> ServiceOrderId {
        self.id
    }
    pub const fn version(&self) -> u64 {
        self.version
    }
    pub fn scope(&self) -> &TenantScope {
        &self.scope
    }
    pub const fn status(&self) -> ServiceOrderStatus {
        self.status
    }
    pub const fn request_id(&self) -> ServiceRequestId {
        self.request
    }
    pub const fn agreement_id(&self) -> ServiceAgreementId {
        self.agreement
    }
    pub const fn customer(&self) -> &RecordVersionRef {
        &self.customer
    }
    pub const fn asset(&self) -> &RecordVersionRef {
        &self.asset
    }
    pub const fn location(&self) -> &StableRecordRef {
        &self.location
    }
    pub fn service_code(&self) -> &str {
        &self.service_code
    }
    pub const fn technician(&self) -> Option<&TechnicianQualification> {
        self.technician.as_ref()
    }
    pub const fn dispatch_id(&self) -> Option<DispatchId> {
        self.dispatch_id
    }
    pub const fn visit_id(&self) -> Option<VisitId> {
        self.visit_id
    }
    pub const fn billing_handoff_id(&self) -> Option<BillingHandoffId> {
        self.billing_handoff_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldServiceBillingHandoff {
    pub id: BillingHandoffId,
    pub service_order_id: ServiceOrderId,
    pub service_order_version: u64,
    pub customer_record_id: Uuid,
    pub asset_record_id: Uuid,
    pub amount: MoneyAmount,
    pub completion_evidence: Vec<RecordVersionRef>,
    pub correlation_id: CorrelationId,
}

fn positive(value: Quantity) -> Result<(), FieldServiceDomainError> {
    if value.units() > 0 {
        Ok(())
    } else {
        Err(FieldServiceDomainError::InvalidQuantity)
    }
}
fn same_scale(left: Quantity, right: Quantity) -> Result<(), FieldServiceDomainError> {
    if left.scale() == right.scale() {
        Ok(())
    } else {
        Err(FieldServiceDomainError::QuantityScaleMismatch)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FieldServiceDomainError {
    #[error("invalid Field Service identifier: {0}")]
    InvalidIdentifier(&'static str),
    #[error("invalid Field Service scope")]
    InvalidScope,
    #[error("invalid record reference")]
    InvalidRecord,
    #[error("invalid agreement")]
    InvalidAgreement,
    #[error("invalid service request")]
    InvalidRequest,
    #[error("invalid first-class field-service record")]
    InvalidGovernedRecord,
    #[error("agreement coverage denied")]
    CoverageDenied,
    #[error("online connectivity is required for Field Service mutations")]
    OnlineRequired,
    #[error("technician skill or schedule gate denied")]
    SkillOrScheduleDenied,
    #[error("inventory gate denied")]
    InventoryGateDenied,
    #[error("dispatch gate denied")]
    DispatchGateDenied,
    #[error("explicit mismatch exception required: {0:?}")]
    ExplicitMismatchRequired(ExceptionKind),
    #[error("invalid service exception")]
    InvalidException,
    #[error("visit evidence is required")]
    VisitEvidenceRequired,
    #[error("explicit customer refusal evidence is required")]
    ExplicitRefusalRequired,
    #[error("billing gate denied")]
    BillingGateDenied,
    #[error("invalid lifecycle transition")]
    InvalidTransition,
    #[error("invalid timestamp")]
    InvalidTimestamp,
    #[error("invalid quantity")]
    InvalidQuantity,
    #[error("quantity scales differ")]
    QuantityScaleMismatch,
    #[error("numeric overflow")]
    Overflow,
}
impl From<erpx_kernel::ScopeError> for FieldServiceDomainError {
    fn from(_: erpx_kernel::ScopeError) -> Self {
        Self::InvalidScope
    }
}
impl From<erpx_integration_contracts::ReferenceValidationError> for FieldServiceDomainError {
    fn from(_: erpx_integration_contracts::ReferenceValidationError) -> Self {
        Self::InvalidRecord
    }
}
impl From<erpx_integration_contracts::ContractValidationError> for FieldServiceDomainError {
    fn from(_: erpx_integration_contracts::ContractValidationError) -> Self {
        Self::InvalidQuantity
    }
}

#[cfg(test)]
mod tests;
