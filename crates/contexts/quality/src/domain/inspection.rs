use chrono::{DateTime, Utc};
use erpx_integration_contracts::{
    StableRecordRef,
    catalog::ContextId,
    reference::{RecordTypeId, ReferenceValidationError},
};
use erpx_kernel::{RecordId, TenantScope};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

pub const MAX_QUALITY_TEXT_BYTES: usize = 255;

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct InspectionId(Uuid);

impl InspectionId {
    pub fn try_from_uuid(value: Uuid) -> Result<Self, QualityDomainError> {
        require_id(value, "inspection_id")?;
        Ok(Self(value))
    }

    pub const fn as_uuid(self) -> Uuid {
        self.0
    }
}

impl TryFrom<Uuid> for InspectionId {
    type Error = QualityDomainError;

    fn try_from(value: Uuid) -> Result<Self, Self::Error> {
        Self::try_from_uuid(value)
    }
}

impl From<InspectionId> for Uuid {
    fn from(value: InspectionId) -> Self {
        value.0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionKind {
    Incoming,
    InProcess,
    Final,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum InspectionStatus {
    Draft,
    InProgress,
    Passed,
    Failed,
    Released,
    Cancelled,
}

impl InspectionStatus {
    const fn can_transition_to(self, next: Self) -> bool {
        matches!(
            (self, next),
            (Self::Draft, Self::InProgress | Self::Cancelled)
                | (
                    Self::InProgress,
                    Self::Passed | Self::Failed | Self::Cancelled
                )
                | (Self::Failed, Self::InProgress | Self::Cancelled)
                | (Self::Passed, Self::Released)
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum QualityDomainError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {maximum} bytes")]
    TooLong { field: &'static str, maximum: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("quality inspections require an exact company or branch scope")]
    InvalidScope,
    #[error("inspection lifecycle transition is not allowed: {from:?} -> {to:?}")]
    InvalidTransition {
        from: InspectionStatus,
        to: InspectionStatus,
    },
    #[error("{field} must be positive")]
    NonPositive { field: &'static str },
    #[error("quality inspection version overflowed")]
    VersionOverflow,
    #[error("inspection result must account for the full inspected quantity")]
    ResultQuantityMismatch,
    #[error("inspection result can only be recorded while in progress")]
    ResultNotRecordable,
    #[error("quality timestamp window is invalid")]
    InvalidWindow,
    #[error("only a passed inspection can be released")]
    InspectionNotPassed,
    #[error("a failed inspection requires bounded disposition evidence")]
    DispositionRequired,
}

/// Authoritative Quality aggregate. Persistence fields stay private so every
/// state change is forced through an invariant-preserving transition.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityInspection {
    id: InspectionId,
    version: u64,
    reference: String,
    scope: TenantScope,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    kind: InspectionKind,
    product_id: Uuid,
    lot_reference: Option<String>,
    quantity: u64,
    passed_quantity: u64,
    failed_quantity: u64,
    status: InspectionStatus,
    disposition_note: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    #[serde(skip)]
    uncommitted_events: Vec<DomainEvent>,
}

/// Typed persistence state used only to rehydrate the aggregate from the
/// Quality-owned normalized write model. It deliberately excludes domain
/// events and any serialized aggregate document.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct QualityInspectionState {
    pub id: InspectionId,
    pub version: u64,
    pub reference: String,
    pub scope: TenantScope,
    pub kind: InspectionKind,
    pub product_id: Uuid,
    pub lot_reference: Option<String>,
    pub quantity: u64,
    pub passed_quantity: u64,
    pub failed_quantity: u64,
    pub status: InspectionStatus,
    pub disposition_note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl QualityInspection {
    pub fn try_rehydrate(state: QualityInspectionState) -> Result<Self, QualityDomainError> {
        let (holding_id, company_id, branch_id) = exact_scope_ids(&state.scope)?;
        let value = Self {
            id: state.id,
            version: state.version,
            reference: state.reference,
            scope: state.scope,
            holding_id,
            company_id,
            branch_id,
            kind: state.kind,
            product_id: state.product_id,
            lot_reference: state.lot_reference,
            quantity: state.quantity,
            passed_quantity: state.passed_quantity,
            failed_quantity: state.failed_quantity,
            status: state.status,
            disposition_note: state.disposition_note,
            created_at: state.created_at,
            updated_at: state.updated_at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        Ok(value)
    }

    #[allow(clippy::too_many_arguments)]
    pub fn create(
        id: InspectionId,
        scope: TenantScope,
        kind: InspectionKind,
        product_id: Uuid,
        lot_reference: Option<String>,
        quantity: u64,
        at: DateTime<Utc>,
    ) -> Result<Self, QualityDomainError> {
        let (holding_id, company_id, branch_id) = exact_scope_ids(&scope)?;
        require_id(product_id, "product_id")?;
        if quantity == 0 {
            return Err(QualityDomainError::NonPositive { field: "quantity" });
        }
        validate_optional_text(lot_reference.as_deref(), "lot_reference")?;
        let reference = deterministic_reference(id);
        let mut value = Self {
            id,
            version: 1,
            reference: reference.clone(),
            scope: scope.clone(),
            holding_id,
            company_id,
            branch_id,
            kind,
            product_id,
            lot_reference,
            quantity,
            passed_quantity: 0,
            failed_quantity: 0,
            status: InspectionStatus::Draft,
            disposition_note: None,
            created_at: at,
            updated_at: at,
            uncommitted_events: Vec::new(),
        };
        value.validate()?;
        value.uncommitted_events.push(DomainEvent::Created {
            inspection_id: id,
            scope,
            product_id,
            reference,
            at,
        });
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), QualityDomainError> {
        require_id(self.id.as_uuid(), "inspection_id")?;
        let (holding_id, company_id, branch_id) = exact_scope_ids(&self.scope)?;
        if (holding_id, company_id, branch_id) != (self.holding_id, self.company_id, self.branch_id)
        {
            return Err(QualityDomainError::InvalidScope);
        }
        require_id(self.product_id, "product_id")?;
        if self.version == 0 {
            return Err(QualityDomainError::NonPositive { field: "version" });
        }
        if self.reference != deterministic_reference(self.id) {
            return Err(QualityDomainError::Required { field: "reference" });
        }
        if self.quantity == 0 {
            return Err(QualityDomainError::NonPositive { field: "quantity" });
        }
        if self.updated_at < self.created_at {
            return Err(QualityDomainError::InvalidWindow);
        }
        validate_optional_text(self.lot_reference.as_deref(), "lot_reference")?;
        validate_optional_text(self.disposition_note.as_deref(), "disposition_note")?;

        let accounted = self
            .passed_quantity
            .checked_add(self.failed_quantity)
            .ok_or(QualityDomainError::ResultQuantityMismatch)?;
        if accounted > self.quantity {
            return Err(QualityDomainError::ResultQuantityMismatch);
        }
        if self.status == InspectionStatus::Failed
            && self
                .disposition_note
                .as_deref()
                .is_none_or(|value| value.trim().is_empty())
        {
            return Err(QualityDomainError::DispositionRequired);
        }
        match self.status {
            InspectionStatus::Draft | InspectionStatus::InProgress
                if self.passed_quantity != 0 || self.failed_quantity != 0 =>
            {
                Err(QualityDomainError::ResultQuantityMismatch)
            }
            InspectionStatus::Passed | InspectionStatus::Released
                if self.passed_quantity != self.quantity || self.failed_quantity != 0 =>
            {
                Err(QualityDomainError::ResultQuantityMismatch)
            }
            InspectionStatus::Failed if self.failed_quantity == 0 || accounted != self.quantity => {
                Err(QualityDomainError::ResultQuantityMismatch)
            }
            _ => Ok(()),
        }
    }

    pub fn start(&mut self, at: DateTime<Utc>) -> Result<(), QualityDomainError> {
        self.transition(InspectionStatus::InProgress, at)?;
        self.uncommitted_events.push(DomainEvent::Started {
            inspection_id: self.id,
            at,
        });
        Ok(())
    }

    pub fn record_result(
        &mut self,
        passed_quantity: u64,
        failed_quantity: u64,
        note: Option<String>,
        at: DateTime<Utc>,
    ) -> Result<(), QualityDomainError> {
        if self.status != InspectionStatus::InProgress {
            return Err(QualityDomainError::ResultNotRecordable);
        }
        if passed_quantity.checked_add(failed_quantity) != Some(self.quantity) {
            return Err(QualityDomainError::ResultQuantityMismatch);
        }
        if at < self.updated_at {
            return Err(QualityDomainError::InvalidWindow);
        }
        validate_optional_text(note.as_deref(), "disposition_note")?;
        if failed_quantity > 0 && note.as_deref().is_none_or(|value| value.trim().is_empty()) {
            return Err(QualityDomainError::DispositionRequired);
        }
        let next_status = if failed_quantity == 0 {
            InspectionStatus::Passed
        } else {
            InspectionStatus::Failed
        };
        let mut candidate = self.clone();
        candidate.version = next_version(candidate.version)?;
        candidate.passed_quantity = passed_quantity;
        candidate.failed_quantity = failed_quantity;
        candidate.disposition_note = note.clone();
        candidate.status = next_status;
        candidate.updated_at = at;
        candidate.validate()?;
        *self = candidate;
        self.uncommitted_events.push(DomainEvent::ResultRecorded {
            inspection_id: self.id,
            product_id: self.product_id,
            reference: self.reference.clone(),
            passed_quantity,
            failed_quantity,
            note,
            at,
        });
        Ok(())
    }

    pub fn release(&mut self, at: DateTime<Utc>) -> Result<(), QualityDomainError> {
        if self.status != InspectionStatus::Passed {
            return Err(QualityDomainError::InspectionNotPassed);
        }
        self.transition(InspectionStatus::Released, at)?;
        self.uncommitted_events.push(DomainEvent::Released {
            inspection_id: self.id,
            product_id: self.product_id,
            reference: self.reference.clone(),
            at,
        });
        Ok(())
    }

    pub fn cancel(&mut self, at: DateTime<Utc>) -> Result<(), QualityDomainError> {
        self.transition(InspectionStatus::Cancelled, at)?;
        self.uncommitted_events.push(DomainEvent::Cancelled {
            inspection_id: self.id,
            at,
        });
        Ok(())
    }

    fn transition(
        &mut self,
        next: InspectionStatus,
        at: DateTime<Utc>,
    ) -> Result<(), QualityDomainError> {
        if !self.status.can_transition_to(next) {
            return Err(QualityDomainError::InvalidTransition {
                from: self.status,
                to: next,
            });
        }
        if at < self.updated_at {
            return Err(QualityDomainError::InvalidWindow);
        }
        let mut candidate = self.clone();
        candidate.version = next_version(candidate.version)?;
        candidate.status = next;
        candidate.updated_at = at;
        if next == InspectionStatus::InProgress {
            candidate.passed_quantity = 0;
            candidate.failed_quantity = 0;
            candidate.disposition_note = None;
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub const fn id(&self) -> InspectionId {
        self.id
    }

    pub const fn version(&self) -> u64 {
        self.version
    }

    pub fn reference(&self) -> &str {
        &self.reference
    }

    pub const fn scope(&self) -> &TenantScope {
        &self.scope
    }

    pub const fn holding_id(&self) -> Uuid {
        self.holding_id
    }

    pub const fn company_id(&self) -> Uuid {
        self.company_id
    }

    pub const fn branch_id(&self) -> Option<Uuid> {
        self.branch_id
    }

    pub const fn kind(&self) -> InspectionKind {
        self.kind
    }

    pub const fn product_id(&self) -> Uuid {
        self.product_id
    }

    pub fn lot_reference(&self) -> Option<&str> {
        self.lot_reference.as_deref()
    }

    pub const fn quantity(&self) -> u64 {
        self.quantity
    }

    pub const fn passed_quantity(&self) -> u64 {
        self.passed_quantity
    }

    pub const fn failed_quantity(&self) -> u64 {
        self.failed_quantity
    }

    pub const fn status(&self) -> InspectionStatus {
        self.status
    }

    pub fn disposition_note(&self) -> Option<&str> {
        self.disposition_note.as_deref()
    }

    pub const fn created_at(&self) -> DateTime<Utc> {
        self.created_at
    }

    pub const fn updated_at(&self) -> DateTime<Utc> {
        self.updated_at
    }

    pub fn record_ref(&self) -> Result<StableRecordRef, ReferenceValidationError> {
        let record_id =
            RecordId::try_from_uuid(self.id.as_uuid()).expect("validated inspection identifier");
        StableRecordRef::try_new(
            record_id,
            ContextId::Quality,
            RecordTypeId::try_new("quality.inspection")?,
        )
    }

    pub(crate) fn take_uncommitted_events(&mut self) -> Vec<DomainEvent> {
        std::mem::take(&mut self.uncommitted_events)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum DomainEvent {
    Created {
        inspection_id: InspectionId,
        scope: TenantScope,
        product_id: Uuid,
        reference: String,
        at: DateTime<Utc>,
    },
    Started {
        inspection_id: InspectionId,
        at: DateTime<Utc>,
    },
    ResultRecorded {
        inspection_id: InspectionId,
        product_id: Uuid,
        reference: String,
        passed_quantity: u64,
        failed_quantity: u64,
        note: Option<String>,
        at: DateTime<Utc>,
    },
    Released {
        inspection_id: InspectionId,
        product_id: Uuid,
        reference: String,
        at: DateTime<Utc>,
    },
    Cancelled {
        inspection_id: InspectionId,
        at: DateTime<Utc>,
    },
}

pub fn deterministic_reference(id: InspectionId) -> String {
    format!(
        "QI-{}",
        id.as_uuid().simple().to_string().to_ascii_uppercase()
    )
}

fn exact_scope_ids(scope: &TenantScope) -> Result<(Uuid, Uuid, Option<Uuid>), QualityDomainError> {
    scope
        .validate()
        .map_err(|_| QualityDomainError::InvalidScope)?;
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } => Ok((*holding_id, *company_id, None)),
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Ok((*holding_id, *company_id, Some(*branch_id))),
        TenantScope::Platform | TenantScope::Holding { .. } | TenantScope::Subtree { .. } => {
            Err(QualityDomainError::InvalidScope)
        }
    }
}

fn require_id(id: Uuid, field: &'static str) -> Result<(), QualityDomainError> {
    if id.is_nil() {
        Err(QualityDomainError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn validate_optional_text(
    value: Option<&str>,
    field: &'static str,
) -> Result<(), QualityDomainError> {
    let Some(value) = value else {
        return Ok(());
    };
    if value.trim().is_empty() {
        return Err(QualityDomainError::Required { field });
    }
    if value.len() > MAX_QUALITY_TEXT_BYTES {
        return Err(QualityDomainError::TooLong {
            field,
            maximum: MAX_QUALITY_TEXT_BYTES,
        });
    }
    if value.chars().any(char::is_control) {
        return Err(QualityDomainError::ControlCharacter { field });
    }
    Ok(())
}

fn next_version(version: u64) -> Result<u64, QualityDomainError> {
    version
        .checked_add(1)
        .ok_or(QualityDomainError::VersionOverflow)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    fn id(value: u128) -> Uuid {
        Uuid::from_u128(value)
    }

    fn at(minute: u32) -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 8, 5, 9, minute, 0)
            .single()
            .unwrap()
    }

    fn scope() -> TenantScope {
        TenantScope::Company {
            holding_id: id(1),
            company_id: id(2),
        }
    }

    fn inspection(quantity: u64) -> QualityInspection {
        QualityInspection::create(
            InspectionId::try_from_uuid(id(3)).unwrap(),
            scope(),
            InspectionKind::Final,
            id(4),
            Some("LOT-001".into()),
            quantity,
            at(0),
        )
        .unwrap()
    }

    #[test]
    fn creation_requires_exact_scope_and_has_deterministic_reference() {
        let inspection = inspection(5);
        assert_eq!(
            inspection.reference(),
            "QI-00000000000000000000000000000003"
        );
        assert_eq!(inspection.version(), 1);
        assert_eq!(inspection.status(), InspectionStatus::Draft);

        let invalid = QualityInspection::create(
            InspectionId::try_from_uuid(id(5)).unwrap(),
            TenantScope::Holding { holding_id: id(1) },
            InspectionKind::Incoming,
            id(4),
            None,
            1,
            at(0),
        );
        assert_eq!(invalid, Err(QualityDomainError::InvalidScope));
    }

    #[test]
    fn pass_and_release_follow_the_lifecycle() {
        let mut inspection = inspection(5);
        inspection.start(at(1)).unwrap();
        inspection
            .record_result(5, 0, Some("all samples passed".into()), at(2))
            .unwrap();
        inspection.release(at(3)).unwrap();
        assert_eq!(inspection.status(), InspectionStatus::Released);
        assert_eq!(inspection.version(), 4);
    }

    #[test]
    fn failed_result_can_be_reinspected_and_cancelled() {
        let mut inspection = inspection(5);
        inspection.start(at(1)).unwrap();
        inspection
            .record_result(4, 1, Some("surface defect quarantined".into()), at(2))
            .unwrap();
        assert_eq!(inspection.status(), InspectionStatus::Failed);
        inspection.start(at(3)).unwrap();
        inspection.cancel(at(4)).unwrap();
        assert_eq!(inspection.status(), InspectionStatus::Cancelled);
    }

    #[test]
    fn rejected_result_does_not_mutate_state_or_events() {
        let mut inspection = inspection(5);
        inspection.start(at(1)).unwrap();
        let before = inspection.clone();
        assert_eq!(
            inspection.record_result(4, 0, None, at(2)),
            Err(QualityDomainError::ResultQuantityMismatch)
        );
        assert_eq!(inspection, before);
    }

    #[test]
    fn failed_result_requires_disposition_evidence_atomically() {
        let mut inspection = inspection(5);
        inspection.start(at(1)).unwrap();
        let before = inspection.clone();
        assert_eq!(
            inspection.record_result(4, 1, None, at(2)),
            Err(QualityDomainError::DispositionRequired)
        );
        assert_eq!(inspection, before);
    }

    #[test]
    fn inspection_exposes_stable_first_class_record_identity() {
        let inspection_id = Uuid::new_v4();
        let value = QualityInspection::create(
            InspectionId::try_from_uuid(inspection_id).unwrap(),
            scope(),
            InspectionKind::Final,
            id(4),
            None,
            1,
            at(0),
        )
        .unwrap();
        let reference = value.record_ref().unwrap();
        assert_eq!(reference.id().as_uuid(), inspection_id);
        assert_eq!(
            reference.canonical_detail_route(),
            format!("/quality/inspection/{inspection_id}")
        );
    }

    #[test]
    fn typed_rehydration_revalidates_every_persisted_invariant() {
        let mut source = inspection(5);
        source.start(at(1)).unwrap();
        let state = QualityInspectionState {
            id: source.id(),
            version: source.version(),
            reference: source.reference().into(),
            scope: source.scope().clone(),
            kind: source.kind(),
            product_id: source.product_id(),
            lot_reference: source.lot_reference().map(str::to_owned),
            quantity: source.quantity(),
            passed_quantity: source.passed_quantity(),
            failed_quantity: source.failed_quantity(),
            status: source.status(),
            disposition_note: source.disposition_note().map(str::to_owned),
            created_at: source.created_at(),
            updated_at: source.updated_at(),
        };
        source.take_uncommitted_events();
        assert_eq!(
            QualityInspection::try_rehydrate(state.clone()).unwrap(),
            source
        );

        let mut corrupt = state;
        corrupt.reference = "forged".into();
        assert!(QualityInspection::try_rehydrate(corrupt).is_err());
    }
}
