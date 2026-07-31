//! G13 migration persistence contracts.
//!
//! This module is intentionally unregistered in the G13-A packet.  The
//! migration crate should later expose it from `lib.rs`, and the infrastructure
//! crate should then import it from `erpx_migration::g13`.  Keeping the DTOs in
//! the migration crate prevents SQLx adapters from inventing a second phase
//! or owner vocabulary.

use crate::{CommandOwner, MigrationPhase, MigrationRun, MigrationRunError, ReconciliationReport};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, to_vec};
use sha2::{Digest, Sha256};
use thiserror::Error;
use uuid::Uuid;

pub const G13_MAX_REFERENCE_BYTES: usize = 128;
pub const G13_MAX_ACTOR_BYTES: usize = 200;
pub const G13_MAX_IDEMPOTENCY_KEY_BYTES: usize = 200;
pub const G13_MAX_CHECKPOINT_KEY_BYTES: usize = 128;
pub const G13_MAX_CURSOR_BYTES: usize = 512;
pub const G13_MAX_ERROR_BYTES: usize = 512;
pub const G13_MAX_EVIDENCE_BYTES: usize = 64 * 1024;

#[derive(Debug, Error, Clone, PartialEq, Eq)]
pub enum G13ContractError {
    #[error("{0} is empty or exceeds its bounded size")]
    InvalidText(&'static str),
    #[error("{0} is not a 64-character lowercase hexadecimal hash")]
    InvalidHash(&'static str),
    #[error("{0} must be a non-nil UUID")]
    NilUuid(&'static str),
    #[error("{0} is not a supported persisted value")]
    InvalidValue(&'static str),
    #[error("evidence payload exceeds the bounded size")]
    EvidenceTooLarge,
    #[error("migration run contract is invalid: {0}")]
    MigrationRun(#[from] MigrationRunError),
    #[error("evidence payload cannot be serialized")]
    EvidenceSerialization,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G13MigrationRunDto {
    pub run: MigrationRun,
    pub command_owner: CommandOwner,
    pub aggregate_version: u64,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl G13MigrationRunDto {
    pub fn new(
        run: MigrationRun,
        aggregate_version: u64,
        created_at: DateTime<Utc>,
        updated_at: DateTime<Utc>,
    ) -> Result<Self, G13ContractError> {
        if aggregate_version == 0 {
            return Err(G13ContractError::InvalidValue("aggregate version"));
        }
        Ok(Self {
            command_owner: run.command_owner(),
            run,
            aggregate_version,
            created_at,
            updated_at,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G13CohortDto {
    pub cohort_key: String,
    pub source_contract_hash: String,
    pub id_namespace: String,
    pub run_id: Uuid,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

impl G13CohortDto {
    pub fn new(
        cohort_key: impl Into<String>,
        source_contract_hash: impl Into<String>,
        id_namespace: impl Into<String>,
        run_id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Self, G13ContractError> {
        let cohort_key = cohort_key.into();
        let source_contract_hash = source_contract_hash.into();
        let id_namespace = id_namespace.into();
        validate_g13_reference(&cohort_key, "cohort")?;
        validate_g13_hash(&source_contract_hash, "source contract hash")?;
        validate_g13_reference(&id_namespace, "id namespace")?;
        validate_g13_uuid(run_id, "run id")?;
        Ok(Self {
            cohort_key,
            source_contract_hash,
            id_namespace,
            run_id,
            created_at: now,
            updated_at: now,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G13StartRunRequest {
    pub run_id: Uuid,
    pub cohort: String,
    pub source_contract_hash: String,
    pub id_namespace: String,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub idempotency_key: String,
    pub now: DateTime<Utc>,
}

impl G13StartRunRequest {
    pub fn build_run(&self) -> Result<MigrationRun, G13ContractError> {
        validate_g13_actor(&self.actor_ref)?;
        validate_g13_uuid(self.correlation_id, "correlation id")?;
        validate_g13_idempotency_key(&self.idempotency_key)?;
        Ok(MigrationRun::new(
            self.run_id,
            self.cohort.clone(),
            self.source_contract_hash.clone(),
            self.id_namespace.clone(),
            self.now,
        )?)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G13TransitionRequest {
    pub run_id: Uuid,
    pub expected_aggregate_version: u64,
    pub expected_phase: MigrationPhase,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub idempotency_key: String,
    pub now: DateTime<Utc>,
}

impl G13TransitionRequest {
    pub fn validate(&self) -> Result<(), G13ContractError> {
        validate_g13_uuid(self.run_id, "run id")?;
        if self.expected_aggregate_version == 0 {
            return Err(G13ContractError::InvalidValue("expected aggregate version"));
        }
        validate_g13_actor(&self.actor_ref)?;
        validate_g13_uuid(self.correlation_id, "correlation id")?;
        validate_g13_idempotency_key(&self.idempotency_key)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum G13CheckpointState {
    Pending,
    Complete,
    Failed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G13CheckpointDto {
    pub run_id: Uuid,
    pub checkpoint_key: String,
    pub source_contract_hash: String,
    pub cursor: Option<String>,
    pub source_position: u64,
    pub processed_rows: u64,
    pub state: G13CheckpointState,
    pub checkpoint_version: u64,
    pub last_error: Option<String>,
    pub updated_at: DateTime<Utc>,
    pub updated_by: String,
    pub correlation_id: Uuid,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G13CheckpointRequest {
    pub run_id: Uuid,
    pub expected_run_version: u64,
    pub expected_checkpoint_version: Option<u64>,
    pub checkpoint_key: String,
    pub source_contract_hash: String,
    pub cursor: Option<String>,
    pub source_position: u64,
    pub processed_rows: u64,
    pub state: G13CheckpointState,
    pub last_error: Option<String>,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub idempotency_key: String,
    pub now: DateTime<Utc>,
}

impl G13CheckpointRequest {
    pub fn validate(&self) -> Result<(), G13ContractError> {
        validate_g13_uuid(self.run_id, "run id")?;
        if self.expected_run_version == 0
            || self
                .expected_checkpoint_version
                .is_some_and(|version| version == 0)
        {
            return Err(G13ContractError::InvalidValue("checkpoint version"));
        }
        validate_g13_reference(&self.checkpoint_key, "checkpoint key")?;
        validate_g13_hash(&self.source_contract_hash, "source contract hash")?;
        if let Some(cursor) = &self.cursor {
            validate_g13_bounded(cursor, "checkpoint cursor", G13_MAX_CURSOR_BYTES)?;
        }
        if let Some(last_error) = &self.last_error {
            validate_g13_bounded(last_error, "checkpoint error", G13_MAX_ERROR_BYTES)?;
        }
        validate_g13_actor(&self.actor_ref)?;
        validate_g13_uuid(self.correlation_id, "correlation id")?;
        validate_g13_idempotency_key(&self.idempotency_key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G13RetentionWindowDto {
    pub run_id: Uuid,
    pub retention_version: u64,
    pub retention_until: DateTime<Utc>,
    pub scheduled_at: DateTime<Utc>,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G13RetentionRequest {
    pub run_id: Uuid,
    pub expected_run_version: u64,
    pub retention_until: DateTime<Utc>,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub idempotency_key: String,
    pub now: DateTime<Utc>,
}

impl G13RetentionRequest {
    pub fn validate(&self) -> Result<(), G13ContractError> {
        validate_g13_uuid(self.run_id, "run id")?;
        if self.expected_run_version == 0 {
            return Err(G13ContractError::InvalidValue("expected aggregate version"));
        }
        validate_g13_actor(&self.actor_ref)?;
        validate_g13_uuid(self.correlation_id, "correlation id")?;
        validate_g13_idempotency_key(&self.idempotency_key)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G13RollbackEvidencePayload {
    pub rehearsal_id: Uuid,
    pub restored_phase: MigrationPhase,
    pub restored_state_hash: String,
    pub read_back_verified: bool,
    pub writes_blocked: bool,
    pub outbox_replayed: bool,
    pub notes: Option<String>,
}

impl G13RollbackEvidencePayload {
    pub fn validate(&self) -> Result<(), G13ContractError> {
        validate_g13_uuid(self.rehearsal_id, "rollback rehearsal id")?;
        validate_g13_hash(&self.restored_state_hash, "restored state hash")?;
        if !self.read_back_verified || !self.writes_blocked || !self.outbox_replayed {
            return Err(G13ContractError::InvalidValue(
                "rollback evidence must verify read-back, write blocking, and outbox replay",
            ));
        }
        if let Some(notes) = &self.notes {
            validate_g13_bounded(notes, "rollback notes", G13_MAX_ERROR_BYTES)?;
        }
        Ok(())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum G13EvidencePayload {
    Reconciliation(ReconciliationReport),
    Rollback(G13RollbackEvidencePayload),
}

impl G13EvidencePayload {
    pub const fn kind(&self) -> G13EvidenceKind {
        match self {
            Self::Reconciliation(_) => G13EvidenceKind::Reconciliation,
            Self::Rollback(_) => G13EvidenceKind::Rollback,
        }
    }

    pub fn success(&self) -> bool {
        match self {
            Self::Reconciliation(report) => report.is_reconciled(),
            Self::Rollback(evidence) => evidence.validate().is_ok(),
        }
    }

    pub fn validate(&self) -> Result<Value, G13ContractError> {
        if let Self::Rollback(evidence) = self {
            evidence.validate()?;
        }
        let value =
            serde_json::to_value(self).map_err(|_| G13ContractError::EvidenceSerialization)?;
        let bytes = to_vec(&value).map_err(|_| G13ContractError::EvidenceSerialization)?;
        if bytes.len() > G13_MAX_EVIDENCE_BYTES {
            return Err(G13ContractError::EvidenceTooLarge);
        }
        Ok(value)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum G13EvidenceKind {
    Reconciliation,
    Rollback,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G13EvidenceRequest {
    pub run_id: Uuid,
    pub expected_run_version: u64,
    pub evidence_id: Uuid,
    pub payload: G13EvidencePayload,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub idempotency_key: String,
    pub now: DateTime<Utc>,
}

impl G13EvidenceRequest {
    pub fn validate(&self) -> Result<Value, G13ContractError> {
        validate_g13_uuid(self.run_id, "run id")?;
        if self.expected_run_version == 0 {
            return Err(G13ContractError::InvalidValue("expected aggregate version"));
        }
        validate_g13_uuid(self.evidence_id, "evidence id")?;
        validate_g13_actor(&self.actor_ref)?;
        validate_g13_uuid(self.correlation_id, "correlation id")?;
        validate_g13_idempotency_key(&self.idempotency_key)?;
        self.payload.validate()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G13EvidenceDto {
    pub evidence_id: Uuid,
    pub run_id: Uuid,
    pub evidence_kind: G13EvidenceKind,
    pub content_hash: String,
    pub payload: Value,
    pub success: bool,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub recorded_at: DateTime<Utc>,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G13TransitionDto {
    pub transition_id: Uuid,
    pub run_id: Uuid,
    pub transition_sequence: u64,
    pub from_phase: Option<MigrationPhase>,
    pub to_phase: MigrationPhase,
    pub command_owner: CommandOwner,
    pub aggregate_version: u64,
    pub occurred_at: DateTime<Utc>,
    pub actor_ref: String,
    pub correlation_id: Uuid,
    pub idempotency_key: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct G13IdempotencyDto {
    pub idempotency_key: String,
    pub operation: String,
    pub request_hash: String,
    pub run_id: Option<Uuid>,
    pub status: String,
    pub response: Option<Value>,
    pub created_at: DateTime<Utc>,
    pub completed_at: Option<DateTime<Utc>>,
}

pub fn phase_to_wire(phase: MigrationPhase) -> &'static str {
    match phase {
        MigrationPhase::SourceFrozen => "source_frozen",
        MigrationPhase::IdsMapped => "ids_mapped",
        MigrationPhase::WriteBackfilled => "write_backfilled",
        MigrationPhase::ReadProjected => "read_projected",
        MigrationPhase::ShadowReplaying => "shadow_replaying",
        MigrationPhase::Reconciled => "reconciled",
        MigrationPhase::ReadOnlyPilot => "read_only_pilot",
        MigrationPhase::WorkflowAccepted => "workflow_accepted",
        MigrationPhase::RollbackRehearsed => "rollback_rehearsed",
        MigrationPhase::MutationFrozen => "mutation_frozen",
        MigrationPhase::FinalDeltaImported => "final_delta_imported",
        MigrationPhase::ErpxCommandOwner => "erpx_command_owner",
        MigrationPhase::FolioReadOnly => "folio_read_only",
        MigrationPhase::RetentionComplete => "retention_complete",
        MigrationPhase::FolioRetired => "folio_retired",
    }
}

pub fn phase_from_wire(value: &str) -> Result<MigrationPhase, G13ContractError> {
    match value {
        "source_frozen" => Ok(MigrationPhase::SourceFrozen),
        "ids_mapped" => Ok(MigrationPhase::IdsMapped),
        "write_backfilled" => Ok(MigrationPhase::WriteBackfilled),
        "read_projected" => Ok(MigrationPhase::ReadProjected),
        "shadow_replaying" => Ok(MigrationPhase::ShadowReplaying),
        "reconciled" => Ok(MigrationPhase::Reconciled),
        "read_only_pilot" => Ok(MigrationPhase::ReadOnlyPilot),
        "workflow_accepted" => Ok(MigrationPhase::WorkflowAccepted),
        "rollback_rehearsed" => Ok(MigrationPhase::RollbackRehearsed),
        "mutation_frozen" => Ok(MigrationPhase::MutationFrozen),
        "final_delta_imported" => Ok(MigrationPhase::FinalDeltaImported),
        "erpx_command_owner" => Ok(MigrationPhase::ErpxCommandOwner),
        "folio_read_only" => Ok(MigrationPhase::FolioReadOnly),
        "retention_complete" => Ok(MigrationPhase::RetentionComplete),
        "folio_retired" => Ok(MigrationPhase::FolioRetired),
        _ => Err(G13ContractError::InvalidValue("migration phase")),
    }
}

pub fn owner_to_wire(owner: CommandOwner) -> &'static str {
    match owner {
        CommandOwner::Folio => "folio",
        CommandOwner::Erpx => "erpx",
    }
}

pub fn owner_from_wire(value: &str) -> Result<CommandOwner, G13ContractError> {
    match value {
        "folio" => Ok(CommandOwner::Folio),
        "erpx" => Ok(CommandOwner::Erpx),
        _ => Err(G13ContractError::InvalidValue("command owner")),
    }
}

pub fn checkpoint_state_to_wire(state: G13CheckpointState) -> &'static str {
    match state {
        G13CheckpointState::Pending => "pending",
        G13CheckpointState::Complete => "complete",
        G13CheckpointState::Failed => "failed",
    }
}

pub fn checkpoint_state_from_wire(value: &str) -> Result<G13CheckpointState, G13ContractError> {
    match value {
        "pending" => Ok(G13CheckpointState::Pending),
        "complete" => Ok(G13CheckpointState::Complete),
        "failed" => Ok(G13CheckpointState::Failed),
        _ => Err(G13ContractError::InvalidValue("checkpoint state")),
    }
}

pub fn evidence_kind_to_wire(kind: G13EvidenceKind) -> &'static str {
    match kind {
        G13EvidenceKind::Reconciliation => "reconciliation",
        G13EvidenceKind::Rollback => "rollback",
    }
}

pub fn evidence_kind_from_wire(value: &str) -> Result<G13EvidenceKind, G13ContractError> {
    match value {
        "reconciliation" => Ok(G13EvidenceKind::Reconciliation),
        "rollback" => Ok(G13EvidenceKind::Rollback),
        _ => Err(G13ContractError::InvalidValue("evidence kind")),
    }
}

pub fn validate_g13_reference(value: &str, field: &'static str) -> Result<(), G13ContractError> {
    validate_g13_bounded(value, field, G13_MAX_REFERENCE_BYTES)
}

pub fn validate_g13_actor(value: &str) -> Result<(), G13ContractError> {
    validate_g13_bounded(value, "actor", G13_MAX_ACTOR_BYTES)
}

pub fn validate_g13_idempotency_key(value: &str) -> Result<(), G13ContractError> {
    validate_g13_bounded(value, "idempotency key", G13_MAX_IDEMPOTENCY_KEY_BYTES)
}

pub fn validate_g13_hash(value: &str, field: &'static str) -> Result<(), G13ContractError> {
    if value.len() != 64 || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(G13ContractError::InvalidHash(field));
    }
    Ok(())
}

pub fn validate_g13_uuid(value: Uuid, field: &'static str) -> Result<(), G13ContractError> {
    if value.is_nil() {
        return Err(G13ContractError::NilUuid(field));
    }
    Ok(())
}

pub fn validate_g13_bounded(
    value: &str,
    field: &'static str,
    max_bytes: usize,
) -> Result<(), G13ContractError> {
    if value.trim().is_empty()
        || value.len() > max_bytes
        || value.chars().any(|character| character.is_control())
    {
        return Err(G13ContractError::InvalidText(field));
    }
    Ok(())
}

pub fn content_hash(value: &Value) -> Result<String, G13ContractError> {
    let bytes = to_vec(value).map_err(|_| G13ContractError::EvidenceSerialization)?;
    if bytes.len() > G13_MAX_EVIDENCE_BYTES {
        return Err(G13ContractError::EvidenceTooLarge);
    }
    let mut digest = Sha256::new();
    digest.update(bytes);
    Ok(digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::Duration;
    use std::collections::BTreeMap;

    fn run() -> MigrationRun {
        MigrationRun::new(
            Uuid::new_v4(),
            "cohort-g13",
            "0123456789abcdef0123456789abcdef0123456789abcdef0123456789abcdef",
            "folio-g13",
            Utc::now(),
        )
        .expect("run")
    }

    #[test]
    fn persisted_wire_names_round_trip_every_phase_and_owner() {
        let phases = [
            MigrationPhase::SourceFrozen,
            MigrationPhase::IdsMapped,
            MigrationPhase::WriteBackfilled,
            MigrationPhase::ReadProjected,
            MigrationPhase::ShadowReplaying,
            MigrationPhase::Reconciled,
            MigrationPhase::ReadOnlyPilot,
            MigrationPhase::WorkflowAccepted,
            MigrationPhase::RollbackRehearsed,
            MigrationPhase::MutationFrozen,
            MigrationPhase::FinalDeltaImported,
            MigrationPhase::ErpxCommandOwner,
            MigrationPhase::FolioReadOnly,
            MigrationPhase::RetentionComplete,
            MigrationPhase::FolioRetired,
        ];
        for phase in phases {
            assert_eq!(phase_from_wire(phase_to_wire(phase)).unwrap(), phase);
        }
        assert_eq!(
            owner_from_wire(owner_to_wire(CommandOwner::Folio)).unwrap(),
            CommandOwner::Folio
        );
        assert_eq!(
            owner_from_wire(owner_to_wire(CommandOwner::Erpx)).unwrap(),
            CommandOwner::Erpx
        );
    }

    #[test]
    fn run_dto_derives_owner_from_phase_and_rejects_zero_version() {
        let now = Utc::now();
        let dto = G13MigrationRunDto::new(run(), 1, now, now).expect("dto");
        assert_eq!(dto.command_owner, CommandOwner::Folio);
        assert!(matches!(
            G13MigrationRunDto::new(dto.run, 0, now, now),
            Err(G13ContractError::InvalidValue("aggregate version"))
        ));
    }

    #[test]
    fn rollback_and_reconciliation_evidence_are_bounded_and_hashed() {
        let mut counts: BTreeMap<String, i32> = BTreeMap::new();
        counts.insert("users".into(), 2);
        let report = ReconciliationReport {
            differences: Vec::new(),
            unexplained: Vec::new(),
        };
        let reconciliation = G13EvidencePayload::Reconciliation(report);
        let value = reconciliation.validate().expect("reconciliation payload");
        assert!(reconciliation.success());
        assert_eq!(content_hash(&value).unwrap().len(), 64);
        assert_eq!(counts.get("users"), Some(&2));

        let rollback = G13EvidencePayload::Rollback(G13RollbackEvidencePayload {
            rehearsal_id: Uuid::new_v4(),
            restored_phase: MigrationPhase::WorkflowAccepted,
            restored_state_hash: "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd"
                .into(),
            read_back_verified: true,
            writes_blocked: true,
            outbox_replayed: true,
            notes: Some("bounded".into()),
        });
        assert!(rollback.validate().is_ok());
        assert!(rollback.success());
        assert!(
            G13EvidencePayload::Rollback(G13RollbackEvidencePayload {
                rehearsal_id: Uuid::new_v4(),
                restored_phase: MigrationPhase::WorkflowAccepted,
                restored_state_hash:
                    "abcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcdefabcd".into(),
                read_back_verified: false,
                writes_blocked: true,
                outbox_replayed: true,
                notes: None,
            })
            .validate()
            .is_err()
        );
        let _ = Duration::seconds(1);
    }
}
