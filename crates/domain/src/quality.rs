//! Tenant-scoped Quality Inspection contracts.
//!
//! Quality owns inspection disposition and traceability facts. Product master,
//! inventory quantity, and manufacturing execution remain references owned by
//! their respective bounded contexts. The aggregate is deliberately small so
//! it can be persisted and replayed without copying those source records.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{CommandContext, TenantScope};

pub const QUALITY_CONTRACT_VERSION: u16 = 1;
pub const MAX_QUALITY_TEXT_BYTES: usize = 255;
pub const MAX_QUALITY_REFERENCE_BYTES: usize = 128;
pub const MAX_QUALITY_IDEMPOTENCY_BYTES: usize = 512;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum QualityError {
    #[error("{field} is required")]
    Required { field: &'static str },
    #[error("{field} exceeds the maximum length of {max} bytes")]
    TooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("quality records require an explicit company or branch scope")]
    InvalidScope,
    #[error("command scope {command_scope:?} does not contain record scope {record_scope:?}")]
    CommandScopeOutside {
        command_scope: Box<TenantScope>,
        record_scope: Box<TenantScope>,
    },
    #[error("command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("{aggregate_type} lifecycle transition is not allowed: {from} -> {to}")]
    InvalidTransition {
        aggregate_type: &'static str,
        from: String,
        to: String,
    },
    #[error("{field} must be positive")]
    NonPositive { field: &'static str },
    #[error("quality inspection version overflowed")]
    VersionOverflow,
    #[error("{field} cannot be negative")]
    Negative { field: &'static str },
    #[error("inspection result must account for the full inspected quantity")]
    ResultQuantityMismatch,
    #[error("inspection result can only be recorded while in progress")]
    ResultNotRecordable,
    #[error("quality timestamp window is invalid")]
    InvalidWindow,
    #[error("only a passed inspection can be released")]
    InspectionNotPassed,
    #[error("{field} is not the deterministic value expected for this aggregate")]
    InvalidDeterministicField { field: &'static str },
}

/// Facts used to decide whether the disposable Quality read model is safe to
/// serve. A non-zero difference requires clear-and-replay of the read side;
/// authoritative source rows are never repaired by reconciliation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct QualityProjectionReconciliation {
    pub source_event_count: u64,
    pub source_aggregate_count: u64,
    pub projected_aggregate_count: u64,
    pub missing_aggregate_count: u64,
    pub stale_aggregate_count: u64,
    pub ahead_aggregate_count: u64,
    pub orphan_aggregate_count: u64,
    pub missing_checkpoint_count: u64,
    pub orphan_checkpoint_count: u64,
    pub invalid_projection_count: u64,
}

impl QualityProjectionReconciliation {
    pub const fn difference_count(self) -> u64 {
        let aggregate_count_gap = self
            .source_aggregate_count
            .abs_diff(self.projected_aggregate_count);
        aggregate_count_gap
            .saturating_add(self.missing_aggregate_count)
            .saturating_add(self.stale_aggregate_count)
            .saturating_add(self.ahead_aggregate_count)
            .saturating_add(self.orphan_aggregate_count)
            .saturating_add(self.missing_checkpoint_count)
            .saturating_add(self.orphan_checkpoint_count)
            .saturating_add(self.invalid_projection_count)
    }

    pub const fn is_consistent(self) -> bool {
        self.source_aggregate_count == self.projected_aggregate_count
            && self.missing_aggregate_count == 0
            && self.stale_aggregate_count == 0
            && self.ahead_aggregate_count == 0
            && self.orphan_aggregate_count == 0
            && self.missing_checkpoint_count == 0
            && self.orphan_checkpoint_count == 0
            && self.invalid_projection_count == 0
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityInspectionKind {
    Incoming,
    InProcess,
    Final,
}

impl QualityInspectionKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Incoming => "incoming",
            Self::InProcess => "in_process",
            Self::Final => "final",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityInspectionStatus {
    Draft,
    InProgress,
    Passed,
    Failed,
    Released,
    Cancelled,
}

impl QualityInspectionStatus {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Draft => "draft",
            Self::InProgress => "in_progress",
            Self::Passed => "passed",
            Self::Failed => "failed",
            Self::Released => "released",
            Self::Cancelled => "cancelled",
        }
    }

    pub const fn can_transition_to(self, next: Self) -> bool {
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct QualityInspection {
    pub id: Uuid,
    pub version: u64,
    pub reference: String,
    pub idempotency_key: String,
    pub command_id: Uuid,
    pub scope: TenantScope,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub kind: QualityInspectionKind,
    pub product_id: Uuid,
    pub lot_reference: Option<String>,
    pub quantity: u64,
    pub passed_quantity: u64,
    pub failed_quantity: u64,
    pub status: QualityInspectionStatus,
    pub disposition_note: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl QualityInspection {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: Uuid,
        scope: TenantScope,
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
        kind: QualityInspectionKind,
        product_id: Uuid,
        lot_reference: Option<String>,
        quantity: u64,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<Self, QualityError> {
        validate_context(context)?;
        let value = Self {
            id,
            version: 1,
            reference: deterministic_reference(id),
            idempotency_key: deterministic_idempotency_key(context)?,
            command_id: context.command_id,
            scope,
            holding_id,
            company_id,
            branch_id,
            kind,
            product_id,
            lot_reference,
            quantity,
            passed_quantity: 0,
            failed_quantity: 0,
            status: QualityInspectionStatus::Draft,
            disposition_note: None,
            created_at: at,
            updated_at: at,
        };
        value.validate()?;
        validate_command_scope(context, &value.scope)?;
        Ok(value)
    }

    pub fn validate(&self) -> Result<(), QualityError> {
        require_id(self.id, "inspection_id")?;
        if self.version == 0 {
            return Err(QualityError::NonPositive { field: "version" });
        }
        require_id(self.command_id, "command_id")?;
        require_id(self.product_id, "product_id")?;
        validate_scope_binding(
            &self.scope,
            self.holding_id,
            self.company_id,
            self.branch_id,
        )?;
        if self.reference != deterministic_reference(self.id) {
            return Err(QualityError::InvalidDeterministicField { field: "reference" });
        }
        bounded(
            &self.idempotency_key,
            "idempotency_key",
            MAX_QUALITY_IDEMPOTENCY_BYTES,
        )?;
        if self.idempotency_key != self.idempotency_key.trim() {
            return Err(QualityError::InvalidDeterministicField {
                field: "idempotency_key",
            });
        }
        if self.quantity == 0 {
            return Err(QualityError::NonPositive { field: "quantity" });
        }
        if self.updated_at < self.created_at {
            return Err(QualityError::InvalidWindow);
        }
        let accounted_quantity = self
            .passed_quantity
            .checked_add(self.failed_quantity)
            .ok_or(QualityError::ResultQuantityMismatch)?;
        if accounted_quantity > self.quantity {
            return Err(QualityError::ResultQuantityMismatch);
        }
        if let Some(lot_reference) = &self.lot_reference {
            bounded(lot_reference, "lot_reference", MAX_QUALITY_TEXT_BYTES)?;
        }
        if let Some(note) = &self.disposition_note {
            bounded(note, "disposition_note", MAX_QUALITY_TEXT_BYTES)?;
        }
        match self.status {
            QualityInspectionStatus::Draft | QualityInspectionStatus::InProgress
                if self.passed_quantity != 0 || self.failed_quantity != 0 =>
            {
                return Err(QualityError::InvalidDeterministicField {
                    field: "inspection_result",
                });
            }
            QualityInspectionStatus::Passed | QualityInspectionStatus::Released
                if self.passed_quantity != self.quantity || self.failed_quantity != 0 =>
            {
                return Err(QualityError::InvalidDeterministicField {
                    field: "inspection_result",
                });
            }
            QualityInspectionStatus::Failed
                if self.failed_quantity == 0 || accounted_quantity != self.quantity =>
            {
                return Err(QualityError::InvalidDeterministicField {
                    field: "inspection_result",
                });
            }
            QualityInspectionStatus::Cancelled | QualityInspectionStatus::Draft => {}
            _ => {}
        }
        Ok(())
    }

    pub fn start(
        &mut self,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<(), QualityError> {
        self.transition(QualityInspectionStatus::InProgress, at, context)
    }

    pub fn record_result(
        &mut self,
        passed_quantity: u64,
        failed_quantity: u64,
        note: Option<String>,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<(), QualityError> {
        validate_context_for_scope(context, &self.scope)?;
        if !matches!(self.status, QualityInspectionStatus::InProgress) {
            return Err(QualityError::ResultNotRecordable);
        }
        if passed_quantity.checked_add(failed_quantity) != Some(self.quantity) {
            return Err(QualityError::ResultQuantityMismatch);
        }
        if at < self.updated_at {
            return Err(QualityError::InvalidWindow);
        }
        let candidate = Self {
            version: next_version(self.version)?,
            passed_quantity,
            failed_quantity,
            disposition_note: note,
            updated_at: at,
            status: if failed_quantity == 0 {
                QualityInspectionStatus::Passed
            } else {
                QualityInspectionStatus::Failed
            },
            ..self.clone()
        };
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }

    pub fn release(
        &mut self,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<(), QualityError> {
        if !matches!(self.status, QualityInspectionStatus::Passed) {
            return Err(QualityError::InspectionNotPassed);
        }
        self.transition(QualityInspectionStatus::Released, at, context)
    }

    pub fn cancel(
        &mut self,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<(), QualityError> {
        self.transition(QualityInspectionStatus::Cancelled, at, context)
    }

    fn transition(
        &mut self,
        next: QualityInspectionStatus,
        at: DateTime<Utc>,
        context: &CommandContext,
    ) -> Result<(), QualityError> {
        validate_context_for_scope(context, &self.scope)?;
        if !self.status.can_transition_to(next) {
            return Err(QualityError::InvalidTransition {
                aggregate_type: "quality_inspection",
                from: self.status.as_str().into(),
                to: next.as_str().into(),
            });
        }
        if at < self.updated_at {
            return Err(QualityError::InvalidWindow);
        }
        let mut candidate = self.clone();
        candidate.version = next_version(candidate.version)?;
        candidate.status = next;
        candidate.updated_at = at;
        if matches!(next, QualityInspectionStatus::InProgress) {
            candidate.passed_quantity = 0;
            candidate.failed_quantity = 0;
            candidate.disposition_note = None;
        }
        candidate.validate()?;
        *self = candidate;
        Ok(())
    }
}

pub fn deterministic_reference(id: Uuid) -> String {
    format!("QI-{}", id.simple().to_string().to_ascii_uppercase())
}

pub fn deterministic_idempotency_key(context: &CommandContext) -> Result<String, QualityError> {
    validate_context(context)?;
    let value = format!("quality/inspection/{}", context.idempotency_key.trim());
    bounded(&value, "idempotency_key", MAX_QUALITY_IDEMPOTENCY_BYTES)?;
    Ok(value)
}

fn validate_context(context: &CommandContext) -> Result<(), QualityError> {
    context
        .validate()
        .map_err(|error| QualityError::InvalidCommandContext(error.to_string()))
}

fn validate_context_for_scope(
    context: &CommandContext,
    record_scope: &TenantScope,
) -> Result<(), QualityError> {
    validate_context(context)?;
    validate_command_scope(context, record_scope)
}

fn validate_command_scope(
    context: &CommandContext,
    record_scope: &TenantScope,
) -> Result<(), QualityError> {
    if context.tenant_scope.contains(record_scope) {
        Ok(())
    } else {
        Err(QualityError::CommandScopeOutside {
            command_scope: Box::new(context.tenant_scope.clone()),
            record_scope: Box::new(record_scope.clone()),
        })
    }
}

fn validate_scope_binding(
    scope: &TenantScope,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
) -> Result<(), QualityError> {
    require_id(holding_id, "holding_id")?;
    require_id(company_id, "company_id")?;
    if branch_id.is_some_and(|id| id.is_nil()) {
        return Err(QualityError::NilIdentifier { field: "branch_id" });
    }
    let matches_scope = match scope {
        TenantScope::Company {
            holding_id: scope_holding,
            company_id: scope_company,
        } => *scope_holding == holding_id && *scope_company == company_id && branch_id.is_none(),
        TenantScope::Branch {
            holding_id: scope_holding,
            company_id: scope_company,
            branch_id: scope_branch,
        } => {
            *scope_holding == holding_id
                && *scope_company == company_id
                && branch_id == Some(*scope_branch)
        }
        TenantScope::Subtree { .. } => branch_id.is_none(),
        TenantScope::Platform | TenantScope::Holding { .. } => false,
    };
    if matches_scope {
        Ok(())
    } else {
        Err(QualityError::InvalidScope)
    }
}

fn require_id(id: Uuid, field: &'static str) -> Result<(), QualityError> {
    if id.is_nil() {
        Err(QualityError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn next_version(version: u64) -> Result<u64, QualityError> {
    version.checked_add(1).ok_or(QualityError::VersionOverflow)
}

fn bounded(value: &str, field: &'static str, max: usize) -> Result<(), QualityError> {
    if value.trim().is_empty() {
        return Err(QualityError::Required { field });
    }
    if value.len() > max {
        return Err(QualityError::TooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(QualityError::ControlCharacter { field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind};

    fn context(scope: TenantScope, key: &str) -> CommandContext {
        CommandContext::try_new(
            key,
            ActorAttribution {
                principal_ref: "quality-test-actor".into(),
                kind: ActorKind::TenantUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope,
            "quality.test",
        )
        .expect("valid command context")
    }

    #[test]
    fn inspection_requires_scope_and_has_deterministic_identity() {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let scope = TenantScope::Company {
            holding_id,
            company_id,
        };
        let command = context(scope.clone(), "create-1");
        let id = Uuid::new_v4();
        let inspection = QualityInspection::new(
            id,
            scope,
            holding_id,
            company_id,
            None,
            QualityInspectionKind::Incoming,
            Uuid::new_v4(),
            Some("LOT-1".into()),
            10,
            Utc::now(),
            &command,
        )
        .expect("valid inspection");
        assert_eq!(inspection.reference, deterministic_reference(id));
        assert_eq!(inspection.idempotency_key, "quality/inspection/create-1");
    }

    #[test]
    fn result_and_release_follow_quality_lifecycle() {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let scope = TenantScope::Company {
            holding_id,
            company_id,
        };
        let mut inspection = QualityInspection::new(
            Uuid::new_v4(),
            scope.clone(),
            holding_id,
            company_id,
            None,
            QualityInspectionKind::Final,
            Uuid::new_v4(),
            None,
            5,
            Utc::now(),
            &context(scope.clone(), "create-2"),
        )
        .expect("valid inspection");
        inspection
            .start(Utc::now(), &context(scope.clone(), "start-2"))
            .expect("start");
        inspection
            .record_result(
                5,
                0,
                Some("all samples passed".into()),
                Utc::now(),
                &context(scope.clone(), "result-2"),
            )
            .expect("result");
        inspection
            .release(Utc::now(), &context(scope, "release-2"))
            .expect("release");
        assert_eq!(inspection.status, QualityInspectionStatus::Released);
    }

    #[test]
    fn mixed_result_is_failed_but_must_account_for_the_lot() {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let scope = TenantScope::Company {
            holding_id,
            company_id,
        };
        let mut inspection = QualityInspection::new(
            Uuid::new_v4(),
            scope.clone(),
            holding_id,
            company_id,
            None,
            QualityInspectionKind::InProcess,
            Uuid::new_v4(),
            None,
            5,
            Utc::now(),
            &context(scope.clone(), "create-3"),
        )
        .expect("valid inspection");
        inspection
            .start(Utc::now(), &context(scope.clone(), "start-3"))
            .expect("start");
        inspection
            .record_result(4, 1, None, Utc::now(), &context(scope.clone(), "result-3"))
            .expect("mixed result");
        assert_eq!(inspection.status, QualityInspectionStatus::Failed);
        assert!(
            inspection
                .record_result(4, 0, None, Utc::now(), &context(scope, "result-3-retry"))
                .is_err()
        );
    }

    #[test]
    fn result_identity_and_timestamp_are_immutable_on_rejected_write() {
        let scope = TenantScope::Company {
            holding_id: Uuid::from_u128(101),
            company_id: Uuid::from_u128(102),
        };
        let created_at = Utc::now();
        let creation_context = context(scope.clone(), "quality-authority-create");
        let mut inspection = QualityInspection::new(
            Uuid::from_u128(103),
            scope.clone(),
            Uuid::from_u128(101),
            Uuid::from_u128(102),
            None,
            QualityInspectionKind::Final,
            Uuid::from_u128(104),
            None,
            2,
            created_at,
            &creation_context,
        )
        .expect("inspection");
        inspection
            .start(
                created_at + chrono::Duration::minutes(1),
                &context(scope.clone(), "quality-authority-start"),
            )
            .expect("start");
        let before = inspection.clone();
        assert_eq!(
            inspection.record_result(
                2,
                0,
                None,
                created_at,
                &context(scope.clone(), "quality-authority-result-too-early"),
            ),
            Err(QualityError::InvalidWindow)
        );
        assert_eq!(inspection, before);

        let result_context = context(scope.clone(), "quality-authority-result");
        inspection
            .record_result(
                2,
                0,
                None,
                created_at + chrono::Duration::minutes(2),
                &result_context,
            )
            .expect("result");
        assert_eq!(
            inspection.idempotency_key,
            "quality/inspection/quality-authority-create"
        );
        assert_eq!(inspection.command_id, creation_context.command_id);
    }

    #[test]
    fn reconciliation_counts_cannot_report_a_missing_projection_as_green() {
        let reconciliation = QualityProjectionReconciliation {
            source_aggregate_count: 3,
            projected_aggregate_count: 2,
            ..QualityProjectionReconciliation::default()
        };
        assert!(!reconciliation.is_consistent());
        assert_eq!(reconciliation.difference_count(), 1);
    }

    #[test]
    fn result_quantity_overflow_is_rejected_without_mutation() {
        let holding_id = Uuid::from_u128(201);
        let company_id = Uuid::from_u128(202);
        let scope = TenantScope::Company {
            holding_id,
            company_id,
        };
        let at = Utc::now();
        let mut inspection = QualityInspection::new(
            Uuid::from_u128(203),
            scope.clone(),
            holding_id,
            company_id,
            None,
            QualityInspectionKind::Final,
            Uuid::from_u128(204),
            None,
            u64::MAX,
            at,
            &context(scope.clone(), "quality-overflow-create"),
        )
        .expect("maximum representable inspection quantity is valid");
        inspection
            .start(at, &context(scope.clone(), "quality-overflow-start"))
            .expect("start");
        let before = inspection.clone();

        assert_eq!(
            inspection.record_result(
                u64::MAX,
                1,
                None,
                at,
                &context(scope, "quality-overflow-result"),
            ),
            Err(QualityError::ResultQuantityMismatch)
        );
        assert_eq!(inspection, before);
    }
}
