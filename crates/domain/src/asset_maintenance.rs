//! Asset and maintenance contracts.
//!
//! This module owns the lifecycle and accounting invariants for financial and
//! operational assets.  It deliberately contains no persistence or transport
//! concerns.  Asset records are scoped to a holding/company and, when
//! applicable, a branch.  Maintenance emits an immutable handoff reference
//! for Operations; Operations remains the owner of capacity projections.

use std::collections::BTreeMap;

use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

use crate::contracts::{CommandContext, TenantScope};
use crate::operations::SourceTransactionRef;

pub const ASSET_MAINTENANCE_CONTRACT_VERSION: u16 = 1;
pub const MAX_ASSET_REFERENCE_BYTES: usize = 64;
pub const MAX_ASSET_NAME_BYTES: usize = 255;
pub const MAX_ASSET_CATEGORY_BYTES: usize = 128;
pub const MAX_PART_REFERENCE_BYTES: usize = 128;
pub const MAX_LOCATION_REFERENCE_BYTES: usize = 255;
pub const MAX_CUSTODIAN_REFERENCE_BYTES: usize = 255;
pub const MAX_REASON_BYTES: usize = 1_024;
pub const MAX_COMMAND_HISTORY: usize = 4_096;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum AssetMaintenanceError {
    #[error("{field} cannot be empty")]
    EmptyField { field: &'static str },
    #[error("{field} exceeds the maximum length of {max} bytes")]
    FieldTooLong { field: &'static str, max: usize },
    #[error("{field} contains a control character")]
    ControlCharacter { field: &'static str },
    #[error("{field} cannot be nil")]
    NilIdentifier { field: &'static str },
    #[error("asset scope is invalid")]
    InvalidScope,
    #[error("command context is invalid: {0}")]
    InvalidCommandContext(String),
    #[error("command scope {context:?} does not contain required scope {required:?}")]
    ScopeNotContained {
        context: Box<TenantScope>,
        required: Box<TenantScope>,
    },
    #[error("actor principal reference cannot be empty")]
    EmptyActorReference,
    #[error("{aggregate_type} cannot transition from {from:?} to {to:?}")]
    InvalidTransition {
        aggregate_type: &'static str,
        from: String,
        to: String,
    },
    #[error("asset {0} is already disposed")]
    AssetDisposed(Uuid),
    #[error("{field} must be non-negative")]
    NegativeAmount { field: &'static str },
    #[error("{field} must be greater than zero")]
    ZeroAmount { field: &'static str },
    #[error("accounting arithmetic overflowed")]
    ArithmeticOverflow,
    #[error("book value cannot be negative")]
    NegativeBookValue,
    #[error("depreciation would reduce book value below the residual value")]
    DepreciationBelowResidual,
    #[error("impairment would reduce book value below the residual value")]
    ImpairmentBelowResidual,
    #[error("revaluation would reduce book value below the residual value")]
    RevaluationBelowResidual,
    #[error("book value is below the residual value")]
    BookValueBelowResidual,
    #[error("{field} is outside the asset's accounting bounds")]
    InvalidAccountingValue { field: &'static str },
    #[error("{field} is not the deterministic value expected for this asset")]
    InvalidDeterministicField { field: &'static str },
    #[error("{field} must end after it starts")]
    InvalidWindow { field: &'static str },
    #[error("transfer must change custody, location, or scope")]
    NoTransferChange,
    #[error("duplicate {entity} identifier {id}")]
    DuplicateIdentifier { entity: &'static str, id: Uuid },
    #[error("downtime interval overlaps an existing interval")]
    OverlappingDowntime,
    #[error("idempotency key is already bound to a different command")]
    IdempotencyConflict,
    #[error("aggregate version mismatch: expected {expected}, actual {actual}")]
    VersionConflict { expected: u64, actual: u64 },
    #[error("command history limit was reached")]
    CommandHistoryFull,
    #[error("source event ID cannot be nil")]
    NilSourceEvent,
    #[error("Operations capacity handoff is invalid")]
    InvalidOperationsHandoff,
}

/// Tenant-scoped source/read reconciliation facts for the disposable
/// Asset/Maintenance projection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct AssetMaintenanceProjectionReconciliation {
    pub source_event_count: u64,
    pub source_aggregate_count: u64,
    pub projected_aggregate_count: u64,
    pub missing_aggregate_count: u64,
    pub stale_aggregate_count: u64,
    pub orphan_aggregate_count: u64,
    /// Number of source aggregates whose outbox versions do not form a
    /// contiguous sequence starting at one.
    pub source_version_gap_count: u64,
    /// Number of read rows claiming a source version newer than the
    /// authoritative outbox high-water mark.
    pub projected_ahead_aggregate_count: u64,
    /// Number of projected assets whose normalized custody history does not
    /// exactly match the custody history carried by the aggregate read row.
    pub custody_history_mismatch_count: u64,
}

impl AssetMaintenanceProjectionReconciliation {
    pub const fn is_consistent(self) -> bool {
        self.missing_aggregate_count == 0
            && self.stale_aggregate_count == 0
            && self.orphan_aggregate_count == 0
            && self.source_version_gap_count == 0
            && self.projected_ahead_aggregate_count == 0
            && self.custody_history_mismatch_count == 0
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetScope {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
}

impl AssetScope {
    pub fn new(
        holding_id: Uuid,
        company_id: Uuid,
        branch_id: Option<Uuid>,
    ) -> Result<Self, AssetMaintenanceError> {
        let scope = Self {
            holding_id,
            company_id,
            branch_id,
        };
        scope.validate()?;
        Ok(scope)
    }

    pub fn validate(&self) -> Result<(), AssetMaintenanceError> {
        if self.holding_id.is_nil() {
            return Err(AssetMaintenanceError::NilIdentifier {
                field: "holding_id",
            });
        }
        if self.company_id.is_nil() {
            return Err(AssetMaintenanceError::NilIdentifier {
                field: "company_id",
            });
        }
        if self.branch_id.is_some_and(|branch_id| branch_id.is_nil()) {
            return Err(AssetMaintenanceError::NilIdentifier { field: "branch_id" });
        }
        Ok(())
    }

    pub fn tenant_scope(&self) -> TenantScope {
        match self.branch_id {
            Some(branch_id) => TenantScope::Branch {
                holding_id: self.holding_id,
                company_id: self.company_id,
                branch_id,
            },
            None => TenantScope::Company {
                holding_id: self.holding_id,
                company_id: self.company_id,
            },
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CustodyTransfer {
    pub from_scope: AssetScope,
    pub to_scope: AssetScope,
    pub from_custodian_ref: String,
    pub to_custodian_ref: String,
    pub from_location_ref: String,
    pub to_location_ref: String,
    pub transferred_at: DateTime<Utc>,
    pub actor_ref: String,
    pub reason: String,
    pub command_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AssetStatus {
    Draft,
    InService,
    UnderMaintenance,
    Impaired,
    Disposed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DepreciationMethod {
    StraightLine,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DisposalRecord {
    pub disposed_at: NaiveDate,
    pub proceeds_minor: i64,
    pub reason: String,
    pub command_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AssetMutationReceipt {
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub command_id: Uuid,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandOutcome {
    pub receipt: AssetMutationReceipt,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct CommandRecord {
    fingerprint: String,
    receipt: AssetMutationReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Asset {
    pub id: Uuid,
    pub reference: String,
    pub name: String,
    pub category: String,
    pub scope: AssetScope,
    pub status: AssetStatus,
    pub acquired_on: NaiveDate,
    pub placed_in_service_on: Option<NaiveDate>,
    pub acquisition_cost_minor: i64,
    pub residual_value_minor: i64,
    pub accumulated_depreciation_minor: i64,
    pub impairment_loss_minor: i64,
    pub revaluation_adjustment_minor: i64,
    pub useful_life_months: u32,
    pub depreciation_method: DepreciationMethod,
    pub custodian_ref: String,
    pub location_ref: String,
    pub custody_history: Vec<CustodyTransfer>,
    pub disposal: Option<DisposalRecord>,
    pub version: u64,
    command_log: BTreeMap<String, CommandRecord>,
}

impl Asset {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        context: &CommandContext,
        id: Uuid,
        name: impl Into<String>,
        category: impl Into<String>,
        scope: AssetScope,
        acquired_on: NaiveDate,
        acquisition_cost_minor: i64,
        residual_value_minor: i64,
        useful_life_months: u32,
        custodian_ref: impl Into<String>,
        location_ref: impl Into<String>,
    ) -> Result<Self, AssetMaintenanceError> {
        let name = name.into();
        let category = category.into();
        let custodian_ref = custodian_ref.into();
        let location_ref = location_ref.into();
        validate_context_for_scope(context, &scope)?;
        require_id(id, "asset_id")?;
        require_text(&name, "asset_name", MAX_ASSET_NAME_BYTES)?;
        require_text(&category, "asset_category", MAX_ASSET_CATEGORY_BYTES)?;
        require_text(
            &custodian_ref,
            "custodian_ref",
            MAX_CUSTODIAN_REFERENCE_BYTES,
        )?;
        require_text(&location_ref, "location_ref", MAX_LOCATION_REFERENCE_BYTES)?;
        if acquisition_cost_minor < 0 {
            return Err(AssetMaintenanceError::NegativeAmount {
                field: "acquisition_cost_minor",
            });
        }
        if residual_value_minor < 0 {
            return Err(AssetMaintenanceError::NegativeAmount {
                field: "residual_value_minor",
            });
        }
        if residual_value_minor > acquisition_cost_minor {
            return Err(AssetMaintenanceError::InvalidAccountingValue {
                field: "residual_value_minor",
            });
        }
        if useful_life_months == 0 {
            return Err(AssetMaintenanceError::ZeroAmount {
                field: "useful_life_months",
            });
        }

        let value = Self {
            id,
            reference: format!("AST-{}", id.simple()),
            name,
            category,
            scope,
            status: AssetStatus::Draft,
            acquired_on,
            placed_in_service_on: None,
            acquisition_cost_minor,
            residual_value_minor,
            accumulated_depreciation_minor: 0,
            impairment_loss_minor: 0,
            revaluation_adjustment_minor: 0,
            useful_life_months,
            depreciation_method: DepreciationMethod::StraightLine,
            custodian_ref,
            location_ref,
            custody_history: Vec::new(),
            disposal: None,
            version: 0,
            command_log: BTreeMap::new(),
        };
        value.validate()?;
        Ok(value)
    }

    /// Validate the complete source aggregate before a command is admitted.
    /// Public fields are deserializable at the persistence boundary, so the
    /// command path must not assume that a previously stored snapshot is
    /// internally consistent.
    pub fn validate(&self) -> Result<(), AssetMaintenanceError> {
        require_id(self.id, "asset_id")?;
        if self.reference != format!("AST-{}", self.id.simple()) {
            return Err(AssetMaintenanceError::InvalidAccountingValue { field: "reference" });
        }
        self.scope.validate()?;
        require_text(&self.name, "asset_name", MAX_ASSET_NAME_BYTES)?;
        require_text(&self.category, "asset_category", MAX_ASSET_CATEGORY_BYTES)?;
        require_text(
            &self.custodian_ref,
            "custodian_ref",
            MAX_CUSTODIAN_REFERENCE_BYTES,
        )?;
        require_text(
            &self.location_ref,
            "location_ref",
            MAX_LOCATION_REFERENCE_BYTES,
        )?;
        if self.acquisition_cost_minor < 0 {
            return Err(AssetMaintenanceError::NegativeAmount {
                field: "acquisition_cost_minor",
            });
        }
        if self.residual_value_minor < 0 {
            return Err(AssetMaintenanceError::NegativeAmount {
                field: "residual_value_minor",
            });
        }
        if self.residual_value_minor > self.acquisition_cost_minor {
            return Err(AssetMaintenanceError::InvalidAccountingValue {
                field: "residual_value_minor",
            });
        }
        if self.accumulated_depreciation_minor < 0 {
            return Err(AssetMaintenanceError::NegativeAmount {
                field: "accumulated_depreciation_minor",
            });
        }
        if self.impairment_loss_minor < 0 {
            return Err(AssetMaintenanceError::NegativeAmount {
                field: "impairment_loss_minor",
            });
        }
        if self.useful_life_months == 0 {
            return Err(AssetMaintenanceError::ZeroAmount {
                field: "useful_life_months",
            });
        }
        if self
            .placed_in_service_on
            .is_some_and(|placed| placed < self.acquired_on)
        {
            return Err(AssetMaintenanceError::InvalidWindow {
                field: "placed_in_service_on",
            });
        }
        if matches!(self.status, AssetStatus::Draft) && self.placed_in_service_on.is_some() {
            return Err(AssetMaintenanceError::InvalidDeterministicField {
                field: "asset_status",
            });
        }
        if !matches!(self.status, AssetStatus::Draft) && self.placed_in_service_on.is_none() {
            return Err(AssetMaintenanceError::InvalidDeterministicField {
                field: "placed_in_service_on",
            });
        }
        if self.status == AssetStatus::Disposed {
            if self.disposal.is_none() {
                return Err(AssetMaintenanceError::InvalidDeterministicField { field: "disposal" });
            }
        } else if self.disposal.is_some() {
            return Err(AssetMaintenanceError::InvalidDeterministicField { field: "disposal" });
        }
        if let Some(disposal) = &self.disposal {
            if disposal.disposed_at < self.acquired_on || disposal.proceeds_minor < 0 {
                return Err(AssetMaintenanceError::InvalidWindow {
                    field: "disposed_at",
                });
            }
            require_id(disposal.command_id, "disposal_command_id")?;
            require_text(&disposal.reason, "disposal_reason", MAX_REASON_BYTES)?;
        }

        let acquired_at = self
            .acquired_on
            .and_hms_opt(0, 0, 0)
            .ok_or(AssetMaintenanceError::InvalidWindow {
                field: "acquired_on",
            })?
            .and_utc();
        let mut previous_scope: Option<AssetScope> = None;
        for transfer in &self.custody_history {
            transfer.from_scope.validate()?;
            transfer.to_scope.validate()?;
            if previous_scope.as_ref() != Some(&transfer.from_scope) && previous_scope.is_some() {
                return Err(AssetMaintenanceError::InvalidDeterministicField {
                    field: "custody_history",
                });
            }
            require_text(
                &transfer.from_custodian_ref,
                "from_custodian_ref",
                MAX_CUSTODIAN_REFERENCE_BYTES,
            )?;
            require_text(
                &transfer.to_custodian_ref,
                "to_custodian_ref",
                MAX_CUSTODIAN_REFERENCE_BYTES,
            )?;
            require_text(
                &transfer.from_location_ref,
                "from_location_ref",
                MAX_LOCATION_REFERENCE_BYTES,
            )?;
            require_text(
                &transfer.to_location_ref,
                "to_location_ref",
                MAX_LOCATION_REFERENCE_BYTES,
            )?;
            require_text(
                &transfer.actor_ref,
                "transfer_actor_ref",
                MAX_CUSTODIAN_REFERENCE_BYTES,
            )?;
            require_text(&transfer.reason, "transfer_reason", MAX_REASON_BYTES)?;
            require_id(transfer.command_id, "transfer_command_id")?;
            if transfer.aggregate_version == 0 || transfer.transferred_at < acquired_at {
                return Err(AssetMaintenanceError::InvalidWindow {
                    field: "transferred_at",
                });
            }
            previous_scope = Some(transfer.to_scope.clone());
        }
        if let Some(last) = self.custody_history.last()
            && (last.to_scope != self.scope
                || last.to_custodian_ref != self.custodian_ref
                || last.to_location_ref != self.location_ref)
        {
            return Err(AssetMaintenanceError::InvalidDeterministicField {
                field: "custody_history",
            });
        }
        self.book_value_minor()?;
        Ok(())
    }

    pub fn book_value_minor(&self) -> Result<i64, AssetMaintenanceError> {
        let gross = self
            .acquisition_cost_minor
            .checked_add(self.revaluation_adjustment_minor)
            .ok_or(AssetMaintenanceError::ArithmeticOverflow)?;
        let after_depreciation = gross
            .checked_sub(self.accumulated_depreciation_minor)
            .ok_or(AssetMaintenanceError::ArithmeticOverflow)?;
        let book_value = after_depreciation
            .checked_sub(self.impairment_loss_minor)
            .ok_or(AssetMaintenanceError::ArithmeticOverflow)?;
        if book_value < 0 {
            Err(AssetMaintenanceError::NegativeBookValue)
        } else if book_value < self.residual_value_minor {
            Err(AssetMaintenanceError::BookValueBelowResidual)
        } else {
            Ok(book_value)
        }
    }

    pub fn straight_line_monthly_depreciation_minor(&self) -> Result<i64, AssetMaintenanceError> {
        let depreciable = self
            .acquisition_cost_minor
            .checked_sub(self.residual_value_minor)
            .ok_or(AssetMaintenanceError::ArithmeticOverflow)?;
        Ok(depreciable / i64::from(self.useful_life_months))
    }

    pub fn apply(
        &mut self,
        context: &CommandContext,
        command: AssetCommand,
    ) -> Result<CommandOutcome, AssetMaintenanceError> {
        self.validate()?;
        validate_context_for_scope(context, &self.scope)?;
        if let AssetCommand::Transfer { to_scope, .. } = &command {
            validate_context_for_scope(context, to_scope)?;
        }
        if let Some(expected) = context.expected_aggregate_version
            && expected != self.version
        {
            return Err(AssetMaintenanceError::VersionConflict {
                expected,
                actual: self.version,
            });
        }

        let fingerprint = fingerprint(&command);
        if let Some(existing) = self.command_log.get(&context.idempotency_key) {
            if existing.fingerprint == fingerprint {
                return Ok(CommandOutcome {
                    receipt: existing.receipt.clone(),
                    applied: false,
                });
            }
            return Err(AssetMaintenanceError::IdempotencyConflict);
        }
        if self.command_log.len() >= MAX_COMMAND_HISTORY {
            return Err(AssetMaintenanceError::CommandHistoryFull);
        }

        self.validate_command(&command)?;
        let next_version = self
            .version
            .checked_add(1)
            .ok_or(AssetMaintenanceError::ArithmeticOverflow)?;
        let mut candidate = self.clone();
        candidate.apply_command(command, context, next_version)?;
        candidate.version = next_version;
        let receipt = AssetMutationReceipt {
            aggregate_id: candidate.id,
            aggregate_version: candidate.version,
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
        };
        candidate.command_log.insert(
            context.idempotency_key.clone(),
            CommandRecord {
                fingerprint,
                receipt: receipt.clone(),
            },
        );
        candidate.validate()?;
        *self = candidate;
        Ok(CommandOutcome {
            receipt,
            applied: true,
        })
    }

    fn validate_command(&self, command: &AssetCommand) -> Result<(), AssetMaintenanceError> {
        match command {
            AssetCommand::PlaceInService { at } => {
                if self.status != AssetStatus::Draft || *at < self.acquired_on {
                    return Err(AssetMaintenanceError::InvalidTransition {
                        aggregate_type: "asset",
                        from: format!("{:?}", self.status),
                        to: "in_service".into(),
                    });
                }
            }
            AssetCommand::ReturnToService => {
                require_asset_transition(self.status, AssetStatus::InService)?;
            }
            AssetCommand::BeginMaintenance => {
                require_asset_transition(self.status, AssetStatus::UnderMaintenance)?;
            }
            AssetCommand::RecordDepreciation { amount_minor } => {
                if !matches!(
                    self.status,
                    AssetStatus::InService | AssetStatus::UnderMaintenance | AssetStatus::Impaired
                ) {
                    return Err(AssetMaintenanceError::InvalidTransition {
                        aggregate_type: "asset",
                        from: format!("{:?}", self.status),
                        to: "depreciated".into(),
                    });
                }
                validate_positive(*amount_minor, "depreciation_minor")?;
                let available = self
                    .book_value_minor()?
                    .checked_sub(self.residual_value_minor)
                    .ok_or(AssetMaintenanceError::ArithmeticOverflow)?;
                if *amount_minor > available {
                    return Err(AssetMaintenanceError::DepreciationBelowResidual);
                }
            }
            AssetCommand::RecordImpairment { amount_minor } => {
                if !matches!(
                    self.status,
                    AssetStatus::InService | AssetStatus::UnderMaintenance
                ) {
                    return Err(AssetMaintenanceError::InvalidTransition {
                        aggregate_type: "asset",
                        from: format!("{:?}", self.status),
                        to: "impaired".into(),
                    });
                }
                validate_positive(*amount_minor, "impairment_minor")?;
                let available = self
                    .book_value_minor()?
                    .checked_sub(self.residual_value_minor)
                    .ok_or(AssetMaintenanceError::ArithmeticOverflow)?;
                if *amount_minor > available {
                    return Err(AssetMaintenanceError::ImpairmentBelowResidual);
                }
            }
            AssetCommand::Revalue {
                target_book_value_minor,
            } => {
                if !matches!(
                    self.status,
                    AssetStatus::InService | AssetStatus::UnderMaintenance | AssetStatus::Impaired
                ) {
                    return Err(AssetMaintenanceError::InvalidTransition {
                        aggregate_type: "asset",
                        from: format!("{:?}", self.status),
                        to: "revalued".into(),
                    });
                }
                if *target_book_value_minor < 0 {
                    return Err(AssetMaintenanceError::NegativeAmount {
                        field: "target_book_value_minor",
                    });
                }
                if *target_book_value_minor < self.residual_value_minor {
                    return Err(AssetMaintenanceError::RevaluationBelowResidual);
                }
            }
            AssetCommand::Transfer {
                to_scope,
                custodian_ref,
                location_ref,
                reason,
                at,
            } => {
                if self.status == AssetStatus::Disposed {
                    return Err(AssetMaintenanceError::AssetDisposed(self.id));
                }
                to_scope.validate()?;
                require_text(
                    custodian_ref,
                    "custodian_ref",
                    MAX_CUSTODIAN_REFERENCE_BYTES,
                )?;
                require_text(location_ref, "location_ref", MAX_LOCATION_REFERENCE_BYTES)?;
                require_text(reason, "transfer_reason", MAX_REASON_BYTES)?;
                if *at < self.acquired_on.and_hms_opt(0, 0, 0).unwrap().and_utc() {
                    return Err(AssetMaintenanceError::InvalidWindow {
                        field: "transferred_at",
                    });
                }
                if *to_scope == self.scope
                    && custodian_ref == &self.custodian_ref
                    && location_ref == &self.location_ref
                {
                    return Err(AssetMaintenanceError::NoTransferChange);
                }
            }
            AssetCommand::Dispose {
                disposed_at,
                proceeds_minor,
                reason,
            } => {
                if !matches!(
                    self.status,
                    AssetStatus::InService | AssetStatus::UnderMaintenance | AssetStatus::Impaired
                ) {
                    return Err(AssetMaintenanceError::InvalidTransition {
                        aggregate_type: "asset",
                        from: format!("{:?}", self.status),
                        to: "disposed".into(),
                    });
                }
                if *disposed_at < self.acquired_on {
                    return Err(AssetMaintenanceError::InvalidWindow {
                        field: "disposed_at",
                    });
                }
                if *proceeds_minor < 0 {
                    return Err(AssetMaintenanceError::NegativeAmount {
                        field: "proceeds_minor",
                    });
                }
                require_text(reason, "disposal_reason", MAX_REASON_BYTES)?;
            }
        }
        Ok(())
    }

    fn apply_command(
        &mut self,
        command: AssetCommand,
        context: &CommandContext,
        next_version: u64,
    ) -> Result<(), AssetMaintenanceError> {
        match command {
            AssetCommand::PlaceInService { at } => {
                self.status = AssetStatus::InService;
                self.placed_in_service_on = Some(at);
            }
            AssetCommand::ReturnToService => self.status = AssetStatus::InService,
            AssetCommand::BeginMaintenance => self.status = AssetStatus::UnderMaintenance,
            AssetCommand::RecordDepreciation { amount_minor } => {
                self.accumulated_depreciation_minor = self
                    .accumulated_depreciation_minor
                    .checked_add(amount_minor)
                    .ok_or(AssetMaintenanceError::ArithmeticOverflow)?;
            }
            AssetCommand::RecordImpairment { amount_minor } => {
                self.impairment_loss_minor = self
                    .impairment_loss_minor
                    .checked_add(amount_minor)
                    .ok_or(AssetMaintenanceError::ArithmeticOverflow)?;
                self.status = AssetStatus::Impaired;
            }
            AssetCommand::Revalue {
                target_book_value_minor,
            } => {
                let current = self.book_value_minor()?;
                let delta = target_book_value_minor
                    .checked_sub(current)
                    .ok_or(AssetMaintenanceError::ArithmeticOverflow)?;
                self.revaluation_adjustment_minor = self
                    .revaluation_adjustment_minor
                    .checked_add(delta)
                    .ok_or(AssetMaintenanceError::ArithmeticOverflow)?;
            }
            AssetCommand::Transfer {
                to_scope,
                custodian_ref,
                location_ref,
                reason,
                at,
            } => {
                self.custody_history.push(CustodyTransfer {
                    from_scope: self.scope.clone(),
                    to_scope: to_scope.clone(),
                    from_custodian_ref: self.custodian_ref.clone(),
                    to_custodian_ref: custodian_ref.clone(),
                    from_location_ref: self.location_ref.clone(),
                    to_location_ref: location_ref.clone(),
                    transferred_at: at,
                    actor_ref: context.actor.principal_ref.clone(),
                    reason,
                    command_id: context.command_id,
                    aggregate_version: next_version,
                });
                self.scope = to_scope;
                self.custodian_ref = custodian_ref;
                self.location_ref = location_ref;
            }
            AssetCommand::Dispose {
                disposed_at,
                proceeds_minor,
                reason,
            } => {
                self.status = AssetStatus::Disposed;
                self.disposal = Some(DisposalRecord {
                    disposed_at,
                    proceeds_minor,
                    reason,
                    command_id: context.command_id,
                });
            }
        }
        self.book_value_minor()?;
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum AssetCommand {
    PlaceInService {
        at: NaiveDate,
    },
    ReturnToService,
    BeginMaintenance,
    RecordDepreciation {
        amount_minor: i64,
    },
    RecordImpairment {
        amount_minor: i64,
    },
    Revalue {
        target_book_value_minor: i64,
    },
    Transfer {
        to_scope: AssetScope,
        custodian_ref: String,
        location_ref: String,
        reason: String,
        at: DateTime<Utc>,
    },
    Dispose {
        disposed_at: NaiveDate,
        proceeds_minor: i64,
        reason: String,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceKind {
    Preventive,
    Corrective,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MaintenanceStatus {
    Draft,
    Scheduled,
    InProgress,
    OnHold,
    Completed,
    Cancelled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PartUsage {
    pub id: Uuid,
    pub part_ref: String,
    pub quantity: i64,
    pub unit_cost_minor: i64,
}

impl PartUsage {
    pub fn total_minor(&self) -> Result<i64, AssetMaintenanceError> {
        if self.quantity <= 0 {
            return Err(AssetMaintenanceError::ZeroAmount {
                field: "part_quantity",
            });
        }
        if self.unit_cost_minor < 0 {
            return Err(AssetMaintenanceError::NegativeAmount {
                field: "part_unit_cost_minor",
            });
        }
        self.quantity
            .checked_mul(self.unit_cost_minor)
            .ok_or(AssetMaintenanceError::ArithmeticOverflow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaborEntry {
    pub id: Uuid,
    pub worker_ref: String,
    pub minutes: i64,
    pub hourly_rate_minor: i64,
}

impl LaborEntry {
    pub fn total_minor(&self) -> Result<i64, AssetMaintenanceError> {
        if self.minutes <= 0 {
            return Err(AssetMaintenanceError::ZeroAmount {
                field: "labor_minutes",
            });
        }
        if self.hourly_rate_minor < 0 {
            return Err(AssetMaintenanceError::NegativeAmount {
                field: "labor_hourly_rate_minor",
            });
        }
        self.minutes
            .checked_mul(self.hourly_rate_minor)
            .and_then(|amount| amount.checked_add(59))
            .map(|amount| amount / 60)
            .ok_or(AssetMaintenanceError::ArithmeticOverflow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DowntimeRecord {
    pub id: Uuid,
    pub started_at: DateTime<Utc>,
    pub ended_at: DateTime<Utc>,
    pub reason: String,
}

impl DowntimeRecord {
    pub fn validate(&self) -> Result<(), AssetMaintenanceError> {
        require_id(self.id, "downtime_id")?;
        if self.ended_at <= self.started_at {
            return Err(AssetMaintenanceError::InvalidWindow { field: "downtime" });
        }
        if self.ended_at - self.started_at < Duration::minutes(1) {
            return Err(AssetMaintenanceError::InvalidWindow { field: "downtime" });
        }
        require_text(&self.reason, "downtime_reason", MAX_REASON_BYTES)
    }

    fn overlaps(&self, other: &Self) -> bool {
        self.started_at < other.ended_at && other.started_at < self.ended_at
    }

    pub fn minutes(&self) -> Result<i64, AssetMaintenanceError> {
        self.validate()?;
        Ok((self.ended_at - self.started_at).num_minutes())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReliabilityRecord {
    pub id: Uuid,
    pub observed_from: DateTime<Utc>,
    pub observed_until: DateTime<Utc>,
    pub failure_count: u32,
    pub operating_minutes: i64,
    pub repair_minutes: i64,
}

impl ReliabilityRecord {
    pub fn validate(&self) -> Result<(), AssetMaintenanceError> {
        require_id(self.id, "reliability_id")?;
        if self.observed_until <= self.observed_from {
            return Err(AssetMaintenanceError::InvalidWindow {
                field: "reliability_observation",
            });
        }
        if self.operating_minutes < 0 {
            return Err(AssetMaintenanceError::NegativeAmount {
                field: "operating_minutes",
            });
        }
        if self.repair_minutes < 0 {
            return Err(AssetMaintenanceError::NegativeAmount {
                field: "repair_minutes",
            });
        }
        if self.operating_minutes == 0 && self.repair_minutes == 0 {
            return Err(AssetMaintenanceError::ZeroAmount {
                field: "reliability_observed_minutes",
            });
        }
        Ok(())
    }

    pub fn mean_time_between_failures_minutes(&self) -> Result<Option<i64>, AssetMaintenanceError> {
        self.validate()?;
        if self.failure_count == 0 {
            Ok(None)
        } else {
            Ok(Some(self.operating_minutes / i64::from(self.failure_count)))
        }
    }

    pub fn availability_basis_points(&self) -> Result<u16, AssetMaintenanceError> {
        self.validate()?;
        let observed = self
            .operating_minutes
            .checked_add(self.repair_minutes)
            .ok_or(AssetMaintenanceError::ArithmeticOverflow)?;
        let basis_points = self
            .operating_minutes
            .checked_mul(10_000)
            .ok_or(AssetMaintenanceError::ArithmeticOverflow)?
            / observed;
        u16::try_from(basis_points).map_err(|_| AssetMaintenanceError::ArithmeticOverflow)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceMutationReceipt {
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub command_id: Uuid,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceCommandOutcome {
    pub receipt: MaintenanceMutationReceipt,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
struct MaintenanceCommandRecord {
    fingerprint: String,
    receipt: MaintenanceMutationReceipt,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MaintenanceWorkOrder {
    pub id: Uuid,
    pub reference: String,
    pub asset_id: Uuid,
    pub scope: AssetScope,
    pub kind: MaintenanceKind,
    pub title: String,
    pub status: MaintenanceStatus,
    pub scheduled_for: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub completed_at: Option<DateTime<Utc>>,
    pub parts: Vec<PartUsage>,
    pub labor: Vec<LaborEntry>,
    pub downtime: Vec<DowntimeRecord>,
    pub reliability: Vec<ReliabilityRecord>,
    pub version: u64,
    command_log: BTreeMap<String, MaintenanceCommandRecord>,
}

impl MaintenanceWorkOrder {
    pub fn new(
        context: &CommandContext,
        id: Uuid,
        asset: &Asset,
        kind: MaintenanceKind,
        title: impl Into<String>,
    ) -> Result<Self, AssetMaintenanceError> {
        let title = title.into();
        // A work order copies the asset identity and scope into its own
        // aggregate.  Validate the source first so a malformed snapshot
        // cannot become an authoritative maintenance record merely because
        // its status is not Disposed.
        asset.validate()?;
        validate_context_for_scope(context, &asset.scope)?;
        require_id(id, "maintenance_work_order_id")?;
        if asset.status == AssetStatus::Disposed {
            return Err(AssetMaintenanceError::AssetDisposed(asset.id));
        }
        require_text(&title, "maintenance_title", MAX_ASSET_NAME_BYTES)?;
        Ok(Self {
            id,
            reference: format!("MWO-{}", id.simple()),
            asset_id: asset.id,
            scope: asset.scope.clone(),
            kind,
            title,
            status: MaintenanceStatus::Draft,
            scheduled_for: None,
            started_at: None,
            completed_at: None,
            parts: Vec::new(),
            labor: Vec::new(),
            downtime: Vec::new(),
            reliability: Vec::new(),
            version: 0,
            command_log: BTreeMap::new(),
        })
    }

    pub fn apply(
        &mut self,
        context: &CommandContext,
        command: MaintenanceCommand,
    ) -> Result<MaintenanceCommandOutcome, AssetMaintenanceError> {
        validate_context_for_scope(context, &self.scope)?;
        if let Some(expected) = context.expected_aggregate_version
            && expected != self.version
        {
            return Err(AssetMaintenanceError::VersionConflict {
                expected,
                actual: self.version,
            });
        }
        let fingerprint = fingerprint(&command);
        if let Some(existing) = self.command_log.get(&context.idempotency_key) {
            if existing.fingerprint == fingerprint {
                return Ok(MaintenanceCommandOutcome {
                    receipt: existing.receipt.clone(),
                    applied: false,
                });
            }
            return Err(AssetMaintenanceError::IdempotencyConflict);
        }
        if self.command_log.len() >= MAX_COMMAND_HISTORY {
            return Err(AssetMaintenanceError::CommandHistoryFull);
        }
        self.validate_command(&command)?;
        let next_version = self
            .version
            .checked_add(1)
            .ok_or(AssetMaintenanceError::ArithmeticOverflow)?;
        let mut candidate = self.clone();
        candidate.apply_command(command)?;
        candidate.version = next_version;
        let receipt = MaintenanceMutationReceipt {
            aggregate_id: candidate.id,
            aggregate_version: candidate.version,
            command_id: context.command_id,
            idempotency_key: context.idempotency_key.clone(),
        };
        candidate.command_log.insert(
            context.idempotency_key.clone(),
            MaintenanceCommandRecord {
                fingerprint,
                receipt: receipt.clone(),
            },
        );
        *self = candidate;
        Ok(MaintenanceCommandOutcome {
            receipt,
            applied: true,
        })
    }

    fn validate_command(&self, command: &MaintenanceCommand) -> Result<(), AssetMaintenanceError> {
        match command {
            MaintenanceCommand::Schedule { scheduled_for } => {
                require_maintenance_transition(self.status, MaintenanceStatus::Scheduled)?;
                if scheduled_for.is_none() {
                    return Err(AssetMaintenanceError::EmptyField {
                        field: "scheduled_for",
                    });
                }
            }
            MaintenanceCommand::Start { at } => {
                require_maintenance_transition(self.status, MaintenanceStatus::InProgress)?;
                if let Some(scheduled_for) = self.scheduled_for
                    && *at < scheduled_for
                {
                    return Err(AssetMaintenanceError::InvalidWindow {
                        field: "maintenance_started_at",
                    });
                }
            }
            MaintenanceCommand::PutOnHold { reason } => {
                require_maintenance_transition(self.status, MaintenanceStatus::OnHold)?;
                require_text(reason, "hold_reason", MAX_REASON_BYTES)?;
            }
            MaintenanceCommand::Resume => {
                require_maintenance_transition(self.status, MaintenanceStatus::InProgress)?;
            }
            MaintenanceCommand::Complete { at } => {
                require_maintenance_transition(self.status, MaintenanceStatus::Completed)?;
                if self.started_at.is_none()
                    || self.started_at.is_some_and(|started| *at <= started)
                {
                    return Err(AssetMaintenanceError::InvalidWindow {
                        field: "maintenance_completed_at",
                    });
                }
            }
            MaintenanceCommand::Cancel { reason } => {
                require_maintenance_transition(self.status, MaintenanceStatus::Cancelled)?;
                require_text(reason, "cancellation_reason", MAX_REASON_BYTES)?;
            }
            MaintenanceCommand::AddPart { part } => {
                if !matches!(
                    self.status,
                    MaintenanceStatus::Scheduled
                        | MaintenanceStatus::InProgress
                        | MaintenanceStatus::OnHold
                ) {
                    return Err(AssetMaintenanceError::InvalidTransition {
                        aggregate_type: "maintenance_work_order",
                        from: format!("{:?}", self.status),
                        to: "part_added".into(),
                    });
                }
                require_id(part.id, "part_usage_id")?;
                require_text(&part.part_ref, "part_ref", MAX_PART_REFERENCE_BYTES)?;
                part.total_minor()?;
                if self.parts.iter().any(|existing| existing.id == part.id) {
                    return Err(AssetMaintenanceError::DuplicateIdentifier {
                        entity: "part_usage",
                        id: part.id,
                    });
                }
            }
            MaintenanceCommand::AddLabor { labor } => {
                if !matches!(
                    self.status,
                    MaintenanceStatus::Scheduled
                        | MaintenanceStatus::InProgress
                        | MaintenanceStatus::OnHold
                ) {
                    return Err(AssetMaintenanceError::InvalidTransition {
                        aggregate_type: "maintenance_work_order",
                        from: format!("{:?}", self.status),
                        to: "labor_added".into(),
                    });
                }
                require_id(labor.id, "labor_entry_id")?;
                require_text(
                    &labor.worker_ref,
                    "worker_ref",
                    MAX_CUSTODIAN_REFERENCE_BYTES,
                )?;
                labor.total_minor()?;
                if self.labor.iter().any(|existing| existing.id == labor.id) {
                    return Err(AssetMaintenanceError::DuplicateIdentifier {
                        entity: "labor_entry",
                        id: labor.id,
                    });
                }
            }
            MaintenanceCommand::RecordDowntime { downtime } => {
                if !matches!(
                    self.status,
                    MaintenanceStatus::Scheduled
                        | MaintenanceStatus::InProgress
                        | MaintenanceStatus::OnHold
                ) {
                    return Err(AssetMaintenanceError::InvalidTransition {
                        aggregate_type: "maintenance_work_order",
                        from: format!("{:?}", self.status),
                        to: "downtime_recorded".into(),
                    });
                }
                downtime.validate()?;
                if self
                    .downtime
                    .iter()
                    .any(|existing| existing.id == downtime.id)
                {
                    return Err(AssetMaintenanceError::DuplicateIdentifier {
                        entity: "downtime",
                        id: downtime.id,
                    });
                }
                if self
                    .downtime
                    .iter()
                    .any(|existing| existing.overlaps(downtime))
                {
                    return Err(AssetMaintenanceError::OverlappingDowntime);
                }
            }
            MaintenanceCommand::RecordReliability { record } => {
                if !matches!(
                    self.status,
                    MaintenanceStatus::Scheduled
                        | MaintenanceStatus::InProgress
                        | MaintenanceStatus::OnHold
                ) {
                    return Err(AssetMaintenanceError::InvalidTransition {
                        aggregate_type: "maintenance_work_order",
                        from: format!("{:?}", self.status),
                        to: "reliability_recorded".into(),
                    });
                }
                record.validate()?;
                if self
                    .reliability
                    .iter()
                    .any(|existing| existing.id == record.id)
                {
                    return Err(AssetMaintenanceError::DuplicateIdentifier {
                        entity: "reliability",
                        id: record.id,
                    });
                }
            }
        }
        Ok(())
    }

    fn apply_command(&mut self, command: MaintenanceCommand) -> Result<(), AssetMaintenanceError> {
        match command {
            MaintenanceCommand::Schedule { scheduled_for } => {
                self.status = MaintenanceStatus::Scheduled;
                self.scheduled_for = scheduled_for;
            }
            MaintenanceCommand::Start { at } => {
                self.status = MaintenanceStatus::InProgress;
                self.started_at = Some(at);
            }
            MaintenanceCommand::PutOnHold { .. } => self.status = MaintenanceStatus::OnHold,
            MaintenanceCommand::Resume => self.status = MaintenanceStatus::InProgress,
            MaintenanceCommand::Complete { at } => {
                self.status = MaintenanceStatus::Completed;
                self.completed_at = Some(at);
            }
            MaintenanceCommand::Cancel { .. } => self.status = MaintenanceStatus::Cancelled,
            MaintenanceCommand::AddPart { part } => self.parts.push(part),
            MaintenanceCommand::AddLabor { labor } => self.labor.push(labor),
            MaintenanceCommand::RecordDowntime { downtime } => self.downtime.push(downtime),
            MaintenanceCommand::RecordReliability { record } => self.reliability.push(record),
        }
        Ok(())
    }

    pub fn parts_cost_minor(&self) -> Result<i64, AssetMaintenanceError> {
        self.parts.iter().try_fold(0_i64, |total, part| {
            total
                .checked_add(part.total_minor()?)
                .ok_or(AssetMaintenanceError::ArithmeticOverflow)
        })
    }

    pub fn labor_cost_minor(&self) -> Result<i64, AssetMaintenanceError> {
        self.labor.iter().try_fold(0_i64, |total, labor| {
            total
                .checked_add(labor.total_minor()?)
                .ok_or(AssetMaintenanceError::ArithmeticOverflow)
        })
    }

    pub fn total_maintenance_cost_minor(&self) -> Result<i64, AssetMaintenanceError> {
        self.parts_cost_minor()?
            .checked_add(self.labor_cost_minor()?)
            .ok_or(AssetMaintenanceError::ArithmeticOverflow)
    }

    pub fn total_downtime_minutes(&self) -> Result<i64, AssetMaintenanceError> {
        self.downtime.iter().try_fold(0_i64, |total, downtime| {
            total
                .checked_add(downtime.minutes()?)
                .ok_or(AssetMaintenanceError::ArithmeticOverflow)
        })
    }

    pub fn operations_capacity_handoff(
        &self,
        context: &CommandContext,
        source_event_id: Uuid,
        idempotency_key: impl Into<String>,
        occurred_at: DateTime<Utc>,
    ) -> Result<OperationsCapacityImpactHandoff, AssetMaintenanceError> {
        validate_context_for_scope(context, &self.scope)?;
        if source_event_id.is_nil() {
            return Err(AssetMaintenanceError::NilSourceEvent);
        }
        let idempotency_key = idempotency_key.into();
        require_text(
            &idempotency_key,
            "operations_handoff_idempotency_key",
            MAX_ASSET_REFERENCE_BYTES,
        )?;
        let downtime_minutes = self.total_downtime_minutes()?;
        if downtime_minutes <= 0 {
            return Err(AssetMaintenanceError::InvalidOperationsHandoff);
        }
        let source =
            SourceTransactionRef::new("assets", "maintenance_work_order", self.id, source_event_id)
                .map_err(|_| AssetMaintenanceError::InvalidOperationsHandoff)?;
        OperationsCapacityImpactHandoff::new(
            source,
            self.asset_id,
            self.scope.clone(),
            downtime_minutes,
            occurred_at,
            idempotency_key,
        )
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MaintenanceCommand {
    Schedule {
        scheduled_for: Option<DateTime<Utc>>,
    },
    Start {
        at: DateTime<Utc>,
    },
    PutOnHold {
        reason: String,
    },
    Resume,
    Complete {
        at: DateTime<Utc>,
    },
    Cancel {
        reason: String,
    },
    AddPart {
        part: PartUsage,
    },
    AddLabor {
        labor: LaborEntry,
    },
    RecordDowntime {
        downtime: DowntimeRecord,
    },
    RecordReliability {
        record: ReliabilityRecord,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsCapacityImpactHandoff {
    pub reference: String,
    pub source: SourceTransactionRef,
    pub asset_id: Uuid,
    pub scope: AssetScope,
    pub downtime_minutes: i64,
    pub occurred_at: DateTime<Utc>,
    pub idempotency_key: String,
}

impl OperationsCapacityImpactHandoff {
    fn new(
        source: SourceTransactionRef,
        asset_id: Uuid,
        scope: AssetScope,
        downtime_minutes: i64,
        occurred_at: DateTime<Utc>,
        idempotency_key: String,
    ) -> Result<Self, AssetMaintenanceError> {
        scope.validate()?;
        require_id(asset_id, "asset_id")?;
        if source.source_type != "maintenance_work_order"
            || source.source_domain != "assets"
            || source.source_id.is_nil()
            || source.source_event_id.is_nil()
            || downtime_minutes <= 0
        {
            return Err(AssetMaintenanceError::InvalidOperationsHandoff);
        }
        require_text(
            &idempotency_key,
            "operations_handoff_idempotency_key",
            MAX_ASSET_REFERENCE_BYTES,
        )?;
        Ok(Self {
            reference: format!(
                "OPS-CAP-{}-{}",
                source.source_id.simple(),
                source.source_event_id.simple()
            ),
            source,
            asset_id,
            scope,
            downtime_minutes,
            occurred_at,
            idempotency_key,
        })
    }
}

fn require_asset_transition(
    from: AssetStatus,
    to: AssetStatus,
) -> Result<(), AssetMaintenanceError> {
    let allowed = matches!(
        (from, to),
        (AssetStatus::Draft, AssetStatus::InService)
            | (AssetStatus::InService, AssetStatus::UnderMaintenance)
            | (AssetStatus::UnderMaintenance, AssetStatus::InService)
    );
    if allowed {
        Ok(())
    } else {
        Err(AssetMaintenanceError::InvalidTransition {
            aggregate_type: "asset",
            from: format!("{from:?}"),
            to: format!("{to:?}"),
        })
    }
}

fn require_maintenance_transition(
    from: MaintenanceStatus,
    to: MaintenanceStatus,
) -> Result<(), AssetMaintenanceError> {
    let allowed = matches!(
        (from, to),
        (MaintenanceStatus::Draft, MaintenanceStatus::Scheduled)
            | (MaintenanceStatus::Scheduled, MaintenanceStatus::InProgress)
            | (MaintenanceStatus::InProgress, MaintenanceStatus::OnHold)
            | (MaintenanceStatus::OnHold, MaintenanceStatus::InProgress)
            | (MaintenanceStatus::InProgress, MaintenanceStatus::Completed)
            | (MaintenanceStatus::Draft, MaintenanceStatus::Cancelled)
            | (MaintenanceStatus::Scheduled, MaintenanceStatus::Cancelled)
            | (MaintenanceStatus::InProgress, MaintenanceStatus::Cancelled)
            | (MaintenanceStatus::OnHold, MaintenanceStatus::Cancelled)
    );
    if allowed {
        Ok(())
    } else {
        Err(AssetMaintenanceError::InvalidTransition {
            aggregate_type: "maintenance_work_order",
            from: format!("{from:?}"),
            to: format!("{to:?}"),
        })
    }
}

fn validate_context_for_scope(
    context: &CommandContext,
    required: &AssetScope,
) -> Result<(), AssetMaintenanceError> {
    context
        .validate()
        .map_err(|error| AssetMaintenanceError::InvalidCommandContext(error.to_string()))?;
    if context.actor.principal_ref.trim().is_empty() {
        return Err(AssetMaintenanceError::EmptyActorReference);
    }
    required.validate()?;
    if context.tenant_scope.contains(&required.tenant_scope()) {
        Ok(())
    } else {
        Err(AssetMaintenanceError::ScopeNotContained {
            context: Box::new(context.tenant_scope.clone()),
            required: Box::new(required.tenant_scope()),
        })
    }
}

fn require_id(id: Uuid, field: &'static str) -> Result<(), AssetMaintenanceError> {
    if id.is_nil() {
        Err(AssetMaintenanceError::NilIdentifier { field })
    } else {
        Ok(())
    }
}

fn require_text(value: &str, field: &'static str, max: usize) -> Result<(), AssetMaintenanceError> {
    if value.trim().is_empty() {
        return Err(AssetMaintenanceError::EmptyField { field });
    }
    if value.len() > max {
        return Err(AssetMaintenanceError::FieldTooLong { field, max });
    }
    if value.chars().any(char::is_control) {
        return Err(AssetMaintenanceError::ControlCharacter { field });
    }
    Ok(())
}

fn validate_positive(value: i64, field: &'static str) -> Result<(), AssetMaintenanceError> {
    if value < 0 {
        Err(AssetMaintenanceError::NegativeAmount { field })
    } else if value == 0 {
        Err(AssetMaintenanceError::ZeroAmount { field })
    } else {
        Ok(())
    }
}

fn fingerprint<T: Serialize>(value: &T) -> String {
    let bytes = serde_json::to_vec(value).expect("domain command serialization is infallible");
    hex::encode(Sha256::digest(bytes))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::contracts::{ActorAttribution, ActorKind};

    fn ids() -> (Uuid, Uuid, Uuid, Uuid, Uuid) {
        (
            Uuid::from_u128(1),
            Uuid::from_u128(2),
            Uuid::from_u128(3),
            Uuid::from_u128(4),
            Uuid::from_u128(5),
        )
    }

    fn context(scope: TenantScope, key: &str, expected: Option<u64>) -> CommandContext {
        let actor = ActorAttribution {
            principal_ref: "user:asset-admin".into(),
            kind: ActorKind::TenantUser,
            session_id: Some(Uuid::from_u128(99)),
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let mut value = CommandContext::try_new(key, actor, scope, "asset maintenance test")
            .expect("valid context");
        value.expected_aggregate_version = expected;
        value
    }

    fn scope(branch_id: Option<Uuid>) -> AssetScope {
        let (holding, company, branch, _, _) = ids();
        AssetScope::new(holding, company, branch_id.or(Some(branch))).expect("valid scope")
    }

    fn asset() -> Asset {
        let value = ids();
        let asset_scope = scope(Some(value.2));
        let context = context(asset_scope.tenant_scope(), "asset-create", None);
        Asset::new(
            &context,
            value.4,
            "Forklift",
            "material_handling",
            asset_scope,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            10_000,
            1_000,
            60,
            "employee:warehouse",
            "warehouse-a",
        )
        .expect("valid asset")
    }

    #[test]
    fn scope_is_required_and_branch_authority_cannot_cross_siblings() {
        let value = ids();
        let asset_scope = scope(Some(value.2));
        let wrong_scope = TenantScope::Branch {
            holding_id: value.0,
            company_id: value.1,
            branch_id: Uuid::from_u128(6),
        };
        let context = context(wrong_scope, "asset-create", None);
        let result = Asset::new(
            &context,
            value.4,
            "Forklift",
            "material_handling",
            asset_scope,
            NaiveDate::from_ymd_opt(2026, 1, 1).unwrap(),
            10_000,
            1_000,
            60,
            "employee:warehouse",
            "warehouse-a",
        );
        assert!(matches!(
            result,
            Err(AssetMaintenanceError::ScopeNotContained { .. })
        ));
    }

    #[test]
    fn company_authority_can_transfer_between_branches_but_branch_cannot() {
        let mut value = asset();
        let (_, company, _, branch_b, _) = ids();
        let destination = AssetScope::new(value.scope.holding_id, company, Some(branch_b)).unwrap();
        let wrong_context = context(value.scope.tenant_scope(), "transfer-branch", Some(0));
        let transfer = AssetCommand::Transfer {
            to_scope: destination.clone(),
            custodian_ref: "employee:other".into(),
            location_ref: "warehouse-b".into(),
            reason: "branch reassignment".into(),
            at: Utc::now(),
        };
        assert!(matches!(
            value.apply(&wrong_context, transfer.clone()),
            Err(AssetMaintenanceError::ScopeNotContained { .. })
        ));

        let company_context = context(
            TenantScope::Company {
                holding_id: value.scope.holding_id,
                company_id: company,
            },
            "transfer-company",
            Some(0),
        );
        value.apply(&company_context, transfer).expect("transfer");
        assert_eq!(value.scope, destination);
        assert_eq!(value.custody_history.len(), 1);
        assert_eq!(value.custody_history[0].from_location_ref, "warehouse-a");
        assert_eq!(value.location_ref, "warehouse-b");
    }

    #[test]
    fn accounting_arithmetic_preserves_non_negative_book_value_and_is_idempotent() {
        let mut value = asset();
        let base = value.scope.tenant_scope();
        value
            .apply(
                &context(base.clone(), "place", Some(0)),
                AssetCommand::PlaceInService {
                    at: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
                },
            )
            .unwrap();
        value
            .apply(
                &context(base.clone(), "depreciate", Some(1)),
                AssetCommand::RecordDepreciation {
                    amount_minor: 3_000,
                },
            )
            .unwrap();
        value
            .apply(
                &context(base.clone(), "impair", Some(2)),
                AssetCommand::RecordImpairment {
                    amount_minor: 2_000,
                },
            )
            .unwrap();
        assert_eq!(value.book_value_minor().unwrap(), 5_000);
        value
            .apply(
                &context(base.clone(), "revalue", Some(3)),
                AssetCommand::Revalue {
                    target_book_value_minor: 9_000,
                },
            )
            .unwrap();
        assert_eq!(value.book_value_minor().unwrap(), 9_000);
        let duplicate = value
            .apply(
                &context(base.clone(), "revalue", Some(4)),
                AssetCommand::Revalue {
                    target_book_value_minor: 9_000,
                },
            )
            .unwrap();
        assert!(!duplicate.applied);
        assert!(matches!(
            value.apply(
                &context(base, "too-much", Some(4)),
                AssetCommand::RecordDepreciation {
                    amount_minor: 10_000
                },
            ),
            Err(AssetMaintenanceError::DepreciationBelowResidual)
        ));
    }

    #[test]
    fn impairment_and_revaluation_cannot_cross_the_residual_value_boundary() {
        let mut value = asset();
        let base = value.scope.tenant_scope();
        value
            .apply(
                &context(base.clone(), "residual-place", Some(0)),
                AssetCommand::PlaceInService {
                    at: NaiveDate::from_ymd_opt(2026, 1, 2).unwrap(),
                },
            )
            .unwrap();
        let before = value.clone();
        assert_eq!(
            value.apply(
                &context(base.clone(), "residual-impairment", Some(1)),
                AssetCommand::RecordImpairment {
                    amount_minor: 9_001,
                },
            ),
            Err(AssetMaintenanceError::ImpairmentBelowResidual)
        );
        assert_eq!(value, before);
        assert_eq!(
            value.apply(
                &context(base, "residual-revalue", Some(1)),
                AssetCommand::Revalue {
                    target_book_value_minor: 999,
                },
            ),
            Err(AssetMaintenanceError::RevaluationBelowResidual)
        );
        assert_eq!(value, before);
    }

    #[test]
    fn custody_transfer_before_acquisition_is_rejected_without_partial_state() {
        let mut value = asset();
        let before = value.clone();
        let context = context(
            value.scope.tenant_scope(),
            "custody-before-acquisition",
            Some(0),
        );
        let result = value.apply(
            &context,
            AssetCommand::Transfer {
                to_scope: value.scope.clone(),
                custodian_ref: "employee:other".into(),
                location_ref: "warehouse-b".into(),
                reason: "invalid historical transfer".into(),
                at: value.acquired_on.and_hms_opt(0, 0, 0).unwrap().and_utc() - Duration::days(1),
            },
        );
        assert!(matches!(
            result,
            Err(AssetMaintenanceError::InvalidWindow {
                field: "transferred_at"
            })
        ));
        assert_eq!(value, before);
    }

    #[test]
    fn maintenance_lifecycle_is_closed_and_costs_are_accounted() {
        let value = ids();
        let asset = asset();
        let base = asset.scope.tenant_scope();
        let create_context = context(base.clone(), "mwo-create", None);
        let mut work_order = MaintenanceWorkOrder::new(
            &create_context,
            value.3,
            &asset,
            MaintenanceKind::Preventive,
            "Quarterly service",
        )
        .unwrap();
        let scheduled_at = Utc::now() + Duration::hours(1);
        work_order
            .apply(
                &context(base.clone(), "schedule", Some(0)),
                MaintenanceCommand::Schedule {
                    scheduled_for: Some(scheduled_at),
                },
            )
            .unwrap();
        assert!(matches!(
            work_order.apply(
                &context(base.clone(), "complete-too-early", Some(1)),
                MaintenanceCommand::Complete {
                    at: scheduled_at + Duration::hours(1),
                }
            ),
            Err(AssetMaintenanceError::InvalidTransition { .. })
        ));
        work_order
            .apply(
                &context(base.clone(), "start", Some(1)),
                MaintenanceCommand::Start { at: scheduled_at },
            )
            .unwrap();
        work_order
            .apply(
                &context(base.clone(), "part", Some(2)),
                MaintenanceCommand::AddPart {
                    part: PartUsage {
                        id: Uuid::from_u128(20),
                        part_ref: "filter".into(),
                        quantity: 2,
                        unit_cost_minor: 1_500,
                    },
                },
            )
            .unwrap();
        work_order
            .apply(
                &context(base.clone(), "labor", Some(3)),
                MaintenanceCommand::AddLabor {
                    labor: LaborEntry {
                        id: Uuid::from_u128(21),
                        worker_ref: "employee:tech".into(),
                        minutes: 60,
                        hourly_rate_minor: 6_000,
                    },
                },
            )
            .unwrap();
        work_order
            .apply(
                &context(base.clone(), "hold", Some(4)),
                MaintenanceCommand::PutOnHold {
                    reason: "waiting for inspection".into(),
                },
            )
            .unwrap();
        work_order
            .apply(
                &context(base.clone(), "resume", Some(5)),
                MaintenanceCommand::Resume,
            )
            .unwrap();
        work_order
            .apply(
                &context(base.clone(), "downtime", Some(6)),
                MaintenanceCommand::RecordDowntime {
                    downtime: DowntimeRecord {
                        id: Uuid::from_u128(22),
                        started_at: scheduled_at,
                        ended_at: scheduled_at + Duration::minutes(90),
                        reason: "service".into(),
                    },
                },
            )
            .unwrap();
        work_order
            .apply(
                &context(base.clone(), "finish", Some(7)),
                MaintenanceCommand::Complete {
                    at: scheduled_at + Duration::hours(2),
                },
            )
            .unwrap();
        assert_eq!(work_order.status, MaintenanceStatus::Completed);
        assert_eq!(work_order.total_maintenance_cost_minor().unwrap(), 9_000);
    }

    #[test]
    fn reliability_record_exposes_mtbf_and_availability_without_negative_metrics() {
        let value = ids();
        let asset = asset();
        let scope = asset.scope.tenant_scope();
        let mut work_order = MaintenanceWorkOrder::new(
            &context(scope.clone(), "mwo-create", None),
            value.3,
            &asset,
            MaintenanceKind::Preventive,
            "Reliability observation",
        )
        .unwrap();
        let start = Utc::now() + Duration::hours(1);
        work_order
            .apply(
                &context(scope.clone(), "schedule", Some(0)),
                MaintenanceCommand::Schedule {
                    scheduled_for: Some(start),
                },
            )
            .unwrap();
        let record = ReliabilityRecord {
            id: Uuid::from_u128(50),
            observed_from: start,
            observed_until: start + Duration::hours(10),
            failure_count: 2,
            operating_minutes: 480,
            repair_minutes: 120,
        };
        work_order
            .apply(
                &context(scope, "reliability", Some(1)),
                MaintenanceCommand::RecordReliability {
                    record: record.clone(),
                },
            )
            .unwrap();
        assert_eq!(
            record.mean_time_between_failures_minutes().unwrap(),
            Some(240)
        );
        assert_eq!(record.availability_basis_points().unwrap(), 8_000);
        assert_eq!(work_order.reliability.len(), 1);
    }

    #[test]
    fn downtime_is_aggregated_without_overlap_and_handed_to_operations() {
        let value = ids();
        let asset = asset();
        let scope = asset.scope.tenant_scope();
        let mut work_order = MaintenanceWorkOrder::new(
            &context(scope.clone(), "mwo-create", None),
            value.3,
            &asset,
            MaintenanceKind::Corrective,
            "Repair motor",
        )
        .unwrap();
        let start = Utc::now() + Duration::hours(1);
        work_order
            .apply(
                &context(scope.clone(), "schedule", Some(0)),
                MaintenanceCommand::Schedule {
                    scheduled_for: Some(start),
                },
            )
            .unwrap();
        work_order
            .apply(
                &context(scope.clone(), "downtime-a", Some(1)),
                MaintenanceCommand::RecordDowntime {
                    downtime: DowntimeRecord {
                        id: Uuid::from_u128(30),
                        started_at: start,
                        ended_at: start + Duration::minutes(60),
                        reason: "repair".into(),
                    },
                },
            )
            .unwrap();
        work_order
            .apply(
                &context(scope.clone(), "downtime-b", Some(2)),
                MaintenanceCommand::RecordDowntime {
                    downtime: DowntimeRecord {
                        id: Uuid::from_u128(31),
                        started_at: start + Duration::minutes(60),
                        ended_at: start + Duration::minutes(90),
                        reason: "testing".into(),
                    },
                },
            )
            .unwrap();
        assert_eq!(work_order.total_downtime_minutes().unwrap(), 90);
        let overlap = work_order.apply(
            &context(scope.clone(), "downtime-overlap", Some(3)),
            MaintenanceCommand::RecordDowntime {
                downtime: DowntimeRecord {
                    id: Uuid::from_u128(32),
                    started_at: start + Duration::minutes(45),
                    ended_at: start + Duration::minutes(100),
                    reason: "duplicate window".into(),
                },
            },
        );
        assert!(matches!(
            overlap,
            Err(AssetMaintenanceError::OverlappingDowntime)
        ));
        let handoff = work_order
            .operations_capacity_handoff(
                &context(scope, "handoff", Some(3)),
                Uuid::from_u128(40),
                "ops-handoff-1",
                start + Duration::minutes(100),
            )
            .unwrap();
        assert_eq!(handoff.downtime_minutes, 90);
        assert!(handoff.reference.starts_with("OPS-CAP-"));
        assert_eq!(handoff.source.source_domain, "assets");
    }

    #[test]
    fn maintenance_version_overflow_rejects_without_mutating_state_or_idempotency() {
        let value = ids();
        let asset = asset();
        let scope = asset.scope.tenant_scope();
        let mut work_order = MaintenanceWorkOrder::new(
            &context(scope.clone(), "mwo-overflow-create", None),
            value.3,
            &asset,
            MaintenanceKind::Preventive,
            "Overflow guard",
        )
        .unwrap();
        work_order.version = u64::MAX;
        let result = work_order.apply(
            &context(scope, "mwo-overflow-schedule", Some(u64::MAX)),
            MaintenanceCommand::Schedule {
                scheduled_for: Some(Utc::now() + Duration::hours(1)),
            },
        );

        assert_eq!(result, Err(AssetMaintenanceError::ArithmeticOverflow));
        assert_eq!(work_order.status, MaintenanceStatus::Draft);
        assert_eq!(work_order.scheduled_for, None);
        assert_eq!(work_order.version, u64::MAX);
        assert!(work_order.command_log.is_empty());
    }

    #[test]
    fn projection_reconciliation_is_green_only_without_mismatches() {
        assert!(AssetMaintenanceProjectionReconciliation::default().is_consistent());
        assert!(
            !AssetMaintenanceProjectionReconciliation {
                orphan_aggregate_count: 1,
                ..AssetMaintenanceProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !AssetMaintenanceProjectionReconciliation {
                source_version_gap_count: 1,
                ..AssetMaintenanceProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !AssetMaintenanceProjectionReconciliation {
                projected_ahead_aggregate_count: 1,
                ..AssetMaintenanceProjectionReconciliation::default()
            }
            .is_consistent()
        );
        assert!(
            !AssetMaintenanceProjectionReconciliation {
                custody_history_mismatch_count: 1,
                ..AssetMaintenanceProjectionReconciliation::default()
            }
            .is_consistent()
        );
    }
}
