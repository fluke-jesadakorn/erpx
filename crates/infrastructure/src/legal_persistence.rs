//! Holding-write SQLx persistence for legal contracts and document metadata.
//!
//! This adapter stores only bounded legal facts.  Document content is never
//! accepted by the adapter: object references and content digests are opaque
//! metadata, and access history is append-only source state.  Every mutation
//! uses the holding-scoped write transaction and commits its normalized rows,
//! idempotency record, and typed outbox event together.

use std::collections::{BTreeMap, HashSet};

use chrono::{DateTime, Duration, Utc};
use erpx_domain::contracts::{
    ActorAttribution, ActorKind, CONTRACT_VERSION, CommandContext, EventEnvelope,
};
use erpx_domain::legal::{
    ContractStatus, DocumentVersion, LegalAccessHistoryEntry, LegalContract, LegalDocument,
    LegalScope, MAX_DOCUMENT_VERSIONS, MAX_LEGAL_PARTIES, MAX_LEGAL_PURPOSE_BYTES,
    MAX_LEGAL_REFERENCE_BYTES, MAX_LEGAL_TITLE_BYTES,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::begin_scoped;

pub const LEGAL_CONTRACT_AGGREGATE: &str = "legal_contract";
pub const LEGAL_DOCUMENT_AGGREGATE: &str = "legal_document";
pub const LEGAL_ACCESS_HISTORY_AGGREGATE: &str = "legal_access_history";
pub const LEGAL_CONTRACT_SAVED_EVENT: &str = "legal.contract_saved.v1";
pub const LEGAL_DOCUMENT_SAVED_EVENT: &str = "legal.document_saved.v1";
pub const LEGAL_ACCESS_RECORDED_EVENT: &str = "legal.access_recorded.v1";

const MAX_DATA_PROFILE_KEY_BYTES: usize = 255;
const MAX_IDEMPOTENCY_KEY_BYTES: usize = 512;
const MAX_ACTOR_REF_BYTES: usize = MAX_LEGAL_REFERENCE_BYTES;
const MAX_RETENTION_HORIZON: Duration = Duration::days(36_525);
const MAX_EVENT_VERSION: u64 = i64::MAX as u64;
const MAX_RETENTION_SWEEP_BATCH: u32 = 100;
const LEGAL_ACCESS_HISTORY_AGGREGATE_VERSION: u64 = 1;

/// A closed set of legal source aggregate identities used by the outbox.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegalAggregateType {
    Contract,
    Document,
    AccessHistory,
}

impl LegalAggregateType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Contract => LEGAL_CONTRACT_AGGREGATE,
            Self::Document => LEGAL_DOCUMENT_AGGREGATE,
            Self::AccessHistory => LEGAL_ACCESS_HISTORY_AGGREGATE,
        }
    }

    fn parse(value: &str) -> Result<Self, PersistenceError> {
        match value {
            LEGAL_CONTRACT_AGGREGATE => Ok(Self::Contract),
            LEGAL_DOCUMENT_AGGREGATE => Ok(Self::Document),
            LEGAL_ACCESS_HISTORY_AGGREGATE => Ok(Self::AccessHistory),
            _ => Err(invalid("unknown legal aggregate type")),
        }
    }
}

/// Typed legal source events.  The string representation is stored in the
/// existing holding outbox envelope for compatibility with later projectors.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegalEventType {
    ContractSaved,
    DocumentSaved,
    AccessRecorded,
}

impl LegalEventType {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::ContractSaved => LEGAL_CONTRACT_SAVED_EVENT,
            Self::DocumentSaved => LEGAL_DOCUMENT_SAVED_EVENT,
            Self::AccessRecorded => LEGAL_ACCESS_RECORDED_EVENT,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalContractPartyEvent {
    pub party_id: Uuid,
    pub role: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalContractEvent {
    pub contract_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub reference: String,
    pub title: String,
    pub object_ref: String,
    pub content_sha256: String,
    pub retention_until: DateTime<Utc>,
    pub status: ContractStatus,
    pub created_by: String,
    pub approved_by: Option<String>,
    pub parties: Vec<LegalContractPartyEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalDocumentVersionEvent {
    pub version: u64,
    pub object_ref: String,
    pub content_sha256: String,
    pub created_at: DateTime<Utc>,
    pub created_by: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalDocumentEvent {
    pub document_id: Uuid,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub title: String,
    pub current_version: u64,
    pub archived: bool,
    pub versions: Vec<LegalDocumentVersionEvent>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalAccessHistoryEvent {
    pub entry_id: Uuid,
    pub document_id: Uuid,
    pub version: u64,
    pub actor_ref: String,
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub action: String,
    pub purpose: String,
    pub observed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", tag = "kind", content = "value")]
pub enum LegalOutboxPayload {
    ContractSaved(LegalContractEvent),
    DocumentSaved(LegalDocumentEvent),
    AccessRecorded(LegalAccessHistoryEvent),
}

/// Typed, transport-neutral event produced by this source adapter.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegalOutboxEvent {
    pub event_id: Uuid,
    pub aggregate_type: LegalAggregateType,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub event_type: LegalEventType,
    pub payload: LegalOutboxPayload,
    pub occurred_at: DateTime<Utc>,
}

/// Receipt for an accepted legal source mutation or an exact idempotency
/// replay.  It contains no aggregate JSON or document content.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LegalWriteReceipt {
    pub aggregate_type: LegalAggregateType,
    pub aggregate_id: Uuid,
    pub version: u64,
    pub idempotency_key: String,
    pub event_id: Uuid,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq)]
struct LegalReplaySourceFact {
    event_type: String,
    aggregate_type: String,
    aggregate_id: Uuid,
    aggregate_version: u64,
    payload: serde_json::Value,
    contract_version: Option<i32>,
    event_version: Option<i32>,
    active_scope: Option<serde_json::Value>,
    actor: Option<serde_json::Value>,
    correlation_id: Option<Uuid>,
    causation_id: Option<Uuid>,
    idempotency_key: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LegalRetentionSweepReport {
    pub scanned: u32,
    pub expired: u32,
    pub conflicts: u32,
}

#[derive(Debug, Clone)]
pub struct SqlxLegalPersistence {
    write_pool: PgPool,
    data_profile_key: String,
}

impl SqlxLegalPersistence {
    pub fn new(
        write_pool: PgPool,
        data_profile_key: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let data_profile_key = data_profile_key.into();
        validate_data_profile_key(&data_profile_key)?;
        Ok(Self {
            write_pool,
            data_profile_key,
        })
    }

    pub fn data_profile_key(&self) -> &str {
        &self.data_profile_key
    }

    /// Expire active contracts whose retention deadline has passed. Each
    /// mutation is rebuilt as a typed domain command and saved with the
    /// current source version, so concurrent workers cannot double-advance an
    /// aggregate. A conflict is reported and left for the next sweep.
    pub async fn expire_due_contracts(
        &self,
        now: DateTime<Utc>,
        batch_size: u32,
    ) -> Result<LegalRetentionSweepReport, PersistenceError> {
        self.expire_due_contracts_with_correlation(now, batch_size, Uuid::new_v4())
            .await
    }

    /// Expire due contracts while preserving the worker invocation's
    /// correlation ID in every authoritative command/outbox record.
    pub async fn expire_due_contracts_with_correlation(
        &self,
        now: DateTime<Utc>,
        batch_size: u32,
        correlation_id: Uuid,
    ) -> Result<LegalRetentionSweepReport, PersistenceError> {
        if batch_size == 0 || batch_size > MAX_RETENTION_SWEEP_BATCH {
            return Err(invalid(format!(
                "legal retention sweep batch must be 1..={MAX_RETENTION_SWEEP_BATCH}"
            )));
        }
        if correlation_id.is_nil() {
            return Err(invalid("legal retention sweep correlation ID is invalid"));
        }
        let mut source_tx = self.begin_write().await?;
        let rows = sqlx::query(
            "SELECT contract_id, holding_id, company_id, branch_id, reference,
                    title, object_ref, content_sha256, retention_until,
                    created_by, approved_by, version, last_command_id
             FROM erpx_holding.legal_contracts
             WHERE data_profile_key=$1 AND status='active' AND retention_until <= $2
             ORDER BY retention_until ASC, contract_id ASC
             LIMIT $3",
        )
        .bind(&self.data_profile_key)
        .bind(now)
        .bind(i64::from(batch_size))
        .fetch_all(&mut *source_tx)
        .await?;

        let mut candidates = Vec::with_capacity(rows.len());
        for row in rows {
            let contract_id: Uuid = row.try_get("contract_id")?;
            let holding_id: Uuid = row.try_get("holding_id")?;
            let company_id: Uuid = row.try_get("company_id")?;
            let branch_id: Option<Uuid> = row.try_get("branch_id")?;
            let scope = LegalScope::new(holding_id, company_id, branch_id)
                .map_err(|error| invalid(format!("invalid retained Legal scope: {error}")))?;
            let version = as_u64(row.try_get("version")?, "legal contract version")?;
            let mut contract = LegalContract {
                id: contract_id,
                reference: row.try_get("reference")?,
                title: row.try_get("title")?,
                scope,
                object_ref: row.try_get("object_ref")?,
                content_sha256: row.try_get("content_sha256")?,
                retention_until: row.try_get("retention_until")?,
                status: ContractStatus::Active,
                parties: Vec::new(),
                created_by: row.try_get("created_by")?,
                approved_by: row.try_get("approved_by")?,
                version,
                last_command_id: row.try_get("last_command_id")?,
            };
            let party_rows = sqlx::query(
                "SELECT party_id, party_role
                 FROM erpx_holding.legal_contract_parties
                 WHERE data_profile_key=$1 AND contract_id=$2
                 ORDER BY party_id ASC",
            )
            .bind(&self.data_profile_key)
            .bind(contract_id)
            .fetch_all(&mut *source_tx)
            .await?;
            contract.parties = party_rows
                .into_iter()
                .map(|party| {
                    erpx_domain::legal::ContractParty::new(
                        party.try_get("party_id")?,
                        party.try_get::<String, _>("party_role")?,
                    )
                    .map_err(|error| invalid(format!("invalid retained Legal party: {error}")))
                })
                .collect::<Result<Vec<_>, PersistenceError>>()?;
            candidates.push(contract);
        }
        source_tx.commit().await?;

        let mut report = LegalRetentionSweepReport {
            scanned: u32::try_from(candidates.len()).unwrap_or(MAX_RETENTION_SWEEP_BATCH),
            expired: 0,
            conflicts: 0,
        };
        for mut contract in candidates {
            let contract_id = contract.id;
            let version = contract.version;
            let mut context = CommandContext::try_new(
                format!("legal-retention:{contract_id}:{version}"),
                ActorAttribution {
                    principal_ref: "system-worker:legal-retention".into(),
                    kind: ActorKind::SystemWorker,
                    session_id: None,
                    represented_principal_ref: None,
                    support_capability_id: None,
                },
                contract.scope.active_scope(),
                "expire legal contract retention",
            )
            .map_err(|error| invalid(format!("invalid Legal retention context: {error}")))?;
            context.correlation_id = correlation_id;
            context.audit.correlation_id = correlation_id;
            context.expected_aggregate_version = Some(version);
            contract
                .expire(now, &context)
                .map_err(|error| invalid(format!("Legal retention expiry is invalid: {error}")))?;
            match self.save_contract(&contract, &context, Some(version)).await {
                Ok(_) => report.expired = report.expired.saturating_add(1),
                Err(PersistenceError::OptimisticConcurrency { .. })
                | Err(PersistenceError::NotFound { .. }) => {
                    report.conflicts = report.conflicts.saturating_add(1)
                }
                Err(error) => return Err(error),
            }
        }
        Ok(report)
    }

    /// Persist a normalized contract, its parties, approval fact, retention
    /// state, idempotency record, and outbox event atomically.
    pub async fn save_contract(
        &self,
        contract: &LegalContract,
        context: &CommandContext,
        expected_version: Option<u64>,
    ) -> Result<LegalWriteReceipt, PersistenceError> {
        validate_contract(contract)?;
        validate_command_context(context, &contract.scope, Some(contract.last_command_id))?;
        validate_contract_command_actor_binding(contract, context)?;
        let idempotency_key = context.idempotency_key.as_str();
        validate_idempotency_key(idempotency_key)?;
        let request_hash = request_hash(
            LegalAggregateType::Contract,
            contract.id,
            contract,
            expected_version,
        )?;
        let replay_payload = LegalOutboxPayload::ContractSaved(contract_event(contract));
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
            LegalAggregateType::Contract,
            contract.id,
            &replay_payload,
            context,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }

        let current = sqlx::query(
            "SELECT holding_id, company_id, branch_id, version, retention_until, status, created_by
             FROM erpx_holding.legal_contracts
             WHERE data_profile_key=$1 AND contract_id=$2
             FOR UPDATE",
        )
        .bind(&self.data_profile_key)
        .bind(contract.id)
        .fetch_optional(&mut *tx)
        .await?;
        let current_version = current
            .as_ref()
            .map(|row| row.try_get::<i64, _>("version"))
            .transpose()?
            .map(|value| as_u64(value, "legal contract version"))
            .transpose()?;
        if let Some(row) = &current {
            ensure_immutable_scope(
                &contract.scope,
                row.try_get("holding_id")?,
                row.try_get("company_id")?,
                row.try_get("branch_id")?,
                "legal contract",
            )?;
        }
        ensure_expected_version(
            LegalAggregateType::Contract,
            contract.id,
            expected_version,
            current_version,
        )?;
        let next_version = if let Some(current_version) = current_version {
            current_version
                .checked_add(1)
                .ok_or_else(|| invalid("legal contract version overflow"))?
        } else {
            if contract.status != ContractStatus::Draft {
                return Err(invalid("a new legal contract must start in draft"));
            }
            contract.version
        };
        ensure_domain_version(
            LegalAggregateType::Contract,
            contract.id,
            contract.version,
            next_version,
        )?;
        if let Some(row) = &current {
            let stored_retention: DateTime<Utc> = row.try_get("retention_until")?;
            if stored_retention != contract.retention_until {
                return Err(invalid(
                    "legal contract retention cannot be shortened or changed",
                ));
            }
            if row.try_get::<String, _>("status")? == "archived" {
                return Err(invalid("archived legal contracts are immutable"));
            }
            let stored_status = parse_contract_status(&row.try_get::<String, _>("status")?)?;
            validate_contract_transition(stored_status, contract.status)?;
            let stored_creator: String = row.try_get("created_by")?;
            if stored_creator != contract.created_by {
                return Err(invalid("legal contract creator is immutable"));
            }
            if stored_status == ContractStatus::Draft {
                sync_contract_parties(&mut tx, &self.data_profile_key, contract).await?;
            } else {
                ensure_contract_parties_unchanged(&mut tx, &self.data_profile_key, contract)
                    .await?;
            }
            update_contract(
                &mut tx,
                &self.data_profile_key,
                contract,
                next_version,
                current_version.expect("current contract version is present"),
                idempotency_key,
            )
            .await?;
        } else {
            insert_contract(
                &mut tx,
                &self.data_profile_key,
                contract,
                next_version,
                idempotency_key,
            )
            .await?;
            sync_contract_parties(&mut tx, &self.data_profile_key, contract).await?;
        }
        persist_approval(&mut tx, &self.data_profile_key, contract, next_version).await?;
        persist_retention(
            &mut tx,
            &self.data_profile_key,
            contract,
            next_version,
            current_version,
        )
        .await?;

        let event = LegalOutboxEvent {
            event_id: Uuid::new_v4(),
            aggregate_type: LegalAggregateType::Contract,
            aggregate_id: contract.id,
            aggregate_version: next_version,
            event_type: LegalEventType::ContractSaved,
            payload: replay_payload,
            occurred_at: Utc::now(),
        };
        append_legal_event(&mut tx, &self.data_profile_key, &event, context).await?;
        let receipt = LegalWriteReceipt {
            aggregate_type: LegalAggregateType::Contract,
            aggregate_id: contract.id,
            version: next_version,
            idempotency_key: idempotency_key.into(),
            event_id: event.event_id,
            replayed: false,
        };
        record_idempotency(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Persist document metadata and any newly observed immutable versions.
    /// `source_version` is the optimistic mutation version; it is separate
    /// from `current_version`, which identifies the current opaque object.
    pub async fn save_document(
        &self,
        document: &LegalDocument,
        context: &CommandContext,
        expected_version: Option<u64>,
    ) -> Result<LegalWriteReceipt, PersistenceError> {
        validate_document(document)?;
        validate_command_context(context, &document.scope, None)?;
        let idempotency_key = context.idempotency_key.as_str();
        validate_idempotency_key(idempotency_key)?;
        let request_hash = request_hash(
            LegalAggregateType::Document,
            document.id,
            document,
            expected_version,
        )?;
        let replay_payload = LegalOutboxPayload::DocumentSaved(document_event(document));
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
            LegalAggregateType::Document,
            document.id,
            &replay_payload,
            context,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }

        let current = sqlx::query(
            "SELECT holding_id, company_id, branch_id, source_version, current_version, archived
             FROM erpx_holding.legal_documents
             WHERE data_profile_key=$1 AND document_id=$2
             FOR UPDATE",
        )
        .bind(&self.data_profile_key)
        .bind(document.id)
        .fetch_optional(&mut *tx)
        .await?;
        let current_version = current
            .as_ref()
            .map(|row| row.try_get::<i64, _>("source_version"))
            .transpose()?
            .map(|value| as_u64(value, "legal document source version"))
            .transpose()?;
        let stored_current_version = current
            .as_ref()
            .map(|row| row.try_get::<i64, _>("current_version"))
            .transpose()?
            .map(|value| as_u64(value, "legal document current version"))
            .transpose()?;
        if let Some(row) = &current {
            ensure_immutable_scope(
                &document.scope,
                row.try_get("holding_id")?,
                row.try_get("company_id")?,
                row.try_get("branch_id")?,
                "legal document",
            )?;
        }
        validate_document_command_actor_binding(
            document,
            &context.actor.principal_ref,
            stored_current_version,
        )?;
        ensure_expected_version(
            LegalAggregateType::Document,
            document.id,
            expected_version,
            current_version,
        )?;
        if current
            .as_ref()
            .is_some_and(|row| row.try_get::<bool, _>("archived").unwrap_or(false))
        {
            return Err(invalid("archived legal documents are immutable"));
        }
        let next_version = current_version.map_or(Ok(1), |value| {
            value
                .checked_add(1)
                .ok_or_else(|| invalid("legal document source version exceeds BIGINT"))
        })?;
        if let Some(row) = &current {
            let stored_current: u64 = as_u64(
                row.try_get("current_version")?,
                "legal document current version",
            )?;
            if stored_current > document.current_version {
                return Err(invalid("legal document versions cannot be removed"));
            }
            update_document(
                &mut tx,
                &self.data_profile_key,
                document,
                next_version,
                current_version.expect("current document version is present"),
                idempotency_key,
            )
            .await?;
        } else {
            if document.archived {
                return Err(invalid("a new legal document cannot be archived"));
            }
            insert_document(
                &mut tx,
                &self.data_profile_key,
                document,
                next_version,
                idempotency_key,
            )
            .await?;
        }
        persist_document_versions(&mut tx, &self.data_profile_key, document).await?;

        let event = LegalOutboxEvent {
            event_id: Uuid::new_v4(),
            aggregate_type: LegalAggregateType::Document,
            aggregate_id: document.id,
            aggregate_version: next_version,
            event_type: LegalEventType::DocumentSaved,
            payload: replay_payload,
            occurred_at: Utc::now(),
        };
        append_legal_event(&mut tx, &self.data_profile_key, &event, context).await?;
        let receipt = LegalWriteReceipt {
            aggregate_type: LegalAggregateType::Document,
            aggregate_id: document.id,
            version: next_version,
            idempotency_key: idempotency_key.into(),
            event_id: event.event_id,
            replayed: false,
        };
        record_idempotency(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Append one immutable access-history fact for an already persisted
    /// document version.
    pub async fn record_access_history(
        &self,
        entry: &LegalAccessHistoryEntry,
        context: &CommandContext,
    ) -> Result<LegalWriteReceipt, PersistenceError> {
        validate_access_history(entry)?;
        validate_command_context(context, &entry.scope, None)?;
        if context.actor.principal_ref != entry.actor_ref {
            return Err(invalid("legal access actor does not match command actor"));
        }
        let idempotency_key = context.idempotency_key.as_str();
        validate_idempotency_key(idempotency_key)?;
        let request_hash = request_hash(
            LegalAggregateType::AccessHistory,
            entry.entry_id,
            entry,
            None,
        )?;
        let replay_payload = LegalOutboxPayload::AccessRecorded(access_event(entry));
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
            LegalAggregateType::AccessHistory,
            entry.entry_id,
            &replay_payload,
            context,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }

        let document = sqlx::query(
            "SELECT holding_id, company_id, branch_id, current_version
             FROM erpx_holding.legal_documents
             WHERE data_profile_key=$1 AND document_id=$2
             FOR SHARE",
        )
        .bind(&self.data_profile_key)
        .bind(entry.document_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "legal document",
            id: entry.document_id,
        })?;
        let stored_holding_id: Uuid = document.try_get("holding_id")?;
        let stored_company_id: Uuid = document.try_get("company_id")?;
        let stored_branch_id: Option<Uuid> = document.try_get("branch_id")?;
        ensure_scope_matches(
            &entry.scope,
            stored_holding_id,
            stored_company_id,
            stored_branch_id,
        )?;
        let current_version: u64 = as_u64(
            document.try_get("current_version")?,
            "legal document current version",
        )?;
        if entry.version > current_version {
            return Err(invalid(
                "legal access history references a future document version",
            ));
        }
        let version_exists: bool = sqlx::query_scalar(
            "SELECT EXISTS(
                 SELECT 1 FROM erpx_holding.legal_document_versions
                 WHERE data_profile_key=$1 AND document_id=$2 AND version=$3
             )",
        )
        .bind(&self.data_profile_key)
        .bind(entry.document_id)
        .bind(as_i64(entry.version, "legal access document version")?)
        .fetch_one(&mut *tx)
        .await?;
        if !version_exists {
            return Err(invalid(
                "legal access history references a missing document version",
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_holding.legal_access_history
                (data_profile_key, entry_id, document_id, holding_id, company_id, branch_id,
                 document_version, actor_ref, action, purpose, observed_at)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)",
        )
        .bind(&self.data_profile_key)
        .bind(entry.entry_id)
        .bind(entry.document_id)
        .bind(entry.scope.holding_id)
        .bind(entry.scope.company_id)
        .bind(entry.scope.branch_id)
        .bind(as_i64(entry.version, "legal access document version")?)
        .bind(&entry.actor_ref)
        .bind(&entry.action)
        .bind(&entry.purpose)
        .bind(entry.observed_at)
        .execute(&mut *tx)
        .await?;

        let event = LegalOutboxEvent {
            event_id: Uuid::new_v4(),
            aggregate_type: LegalAggregateType::AccessHistory,
            aggregate_id: entry.entry_id,
            // Each access-history entry is its own immutable aggregate. The
            // referenced document version belongs to the payload, not to the
            // access-entry aggregate history.
            aggregate_version: LEGAL_ACCESS_HISTORY_AGGREGATE_VERSION,
            event_type: LegalEventType::AccessRecorded,
            payload: replay_payload,
            occurred_at: Utc::now(),
        };
        append_legal_event(&mut tx, &self.data_profile_key, &event, context).await?;
        let receipt = LegalWriteReceipt {
            aggregate_type: LegalAggregateType::AccessHistory,
            aggregate_id: entry.entry_id,
            version: LEGAL_ACCESS_HISTORY_AGGREGATE_VERSION,
            idempotency_key: idempotency_key.into(),
            event_id: event.event_id,
            replayed: false,
        };
        record_idempotency(
            &mut tx,
            &self.data_profile_key,
            idempotency_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Alias kept explicit for callers that name the source fact rather than
    /// the history collection.
    pub async fn save_access_history(
        &self,
        entry: &LegalAccessHistoryEntry,
        context: &CommandContext,
    ) -> Result<LegalWriteReceipt, PersistenceError> {
        self.record_access_history(entry, context).await
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.write_pool, &self.data_profile_key).await
    }
}

async fn insert_contract(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    contract: &LegalContract,
    version: u64,
    idempotency_key: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_holding.legal_contracts
            (data_profile_key, contract_id, holding_id, company_id, branch_id, reference, title,
             object_ref, content_sha256, retention_until, status, created_by, approved_by,
             version, last_command_id, idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
    )
    .bind(data_profile_key)
    .bind(contract.id)
    .bind(contract.scope.holding_id)
    .bind(contract.scope.company_id)
    .bind(contract.scope.branch_id)
    .bind(&contract.reference)
    .bind(&contract.title)
    .bind(&contract.object_ref)
    .bind(&contract.content_sha256)
    .bind(contract.retention_until)
    .bind(contract_status(contract.status))
    .bind(&contract.created_by)
    .bind(&contract.approved_by)
    .bind(as_i64(version, "legal contract version")?)
    .bind(contract.last_command_id)
    .bind(idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn update_contract(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    contract: &LegalContract,
    version: u64,
    previous_version: u64,
    idempotency_key: &str,
) -> Result<(), PersistenceError> {
    let result = sqlx::query(
        "UPDATE erpx_holding.legal_contracts
         SET holding_id=$3, company_id=$4, branch_id=$5, reference=$6, title=$7,
             object_ref=$8, content_sha256=$9, retention_until=$10, status=$11,
             approved_by=$12, version=$13, last_command_id=$14,
             idempotency_key=$15, updated_at=now()
         WHERE data_profile_key=$1 AND contract_id=$2 AND version=$16",
    )
    .bind(data_profile_key)
    .bind(contract.id)
    .bind(contract.scope.holding_id)
    .bind(contract.scope.company_id)
    .bind(contract.scope.branch_id)
    .bind(&contract.reference)
    .bind(&contract.title)
    .bind(&contract.object_ref)
    .bind(&contract.content_sha256)
    .bind(contract.retention_until)
    .bind(contract_status(contract.status))
    .bind(&contract.approved_by)
    .bind(as_i64(version, "legal contract version")?)
    .bind(contract.last_command_id)
    .bind(idempotency_key)
    .bind(as_i64(previous_version, "legal contract version")?)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(invalid("legal contract version update was not applied"));
    }
    Ok(())
}

async fn sync_contract_parties(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    contract: &LegalContract,
) -> Result<(), PersistenceError> {
    let party_ids: Vec<Uuid> = contract
        .parties
        .iter()
        .map(|party| party.party_id)
        .collect();
    sqlx::query(
        "DELETE FROM erpx_holding.legal_contract_parties
         WHERE data_profile_key=$1 AND contract_id=$2
           AND NOT (party_id = ANY($3))",
    )
    .bind(data_profile_key)
    .bind(contract.id)
    .bind(&party_ids)
    .execute(&mut **tx)
    .await?;
    for party in &contract.parties {
        sqlx::query(
            "INSERT INTO erpx_holding.legal_contract_parties
                (data_profile_key, contract_id, holding_id, party_id, party_role)
             VALUES ($1,$2,$3,$4,$5)
             ON CONFLICT (data_profile_key, contract_id, party_id)
             DO UPDATE SET holding_id=EXCLUDED.holding_id, party_role=EXCLUDED.party_role",
        )
        .bind(data_profile_key)
        .bind(contract.id)
        .bind(contract.scope.holding_id)
        .bind(party.party_id)
        .bind(&party.role)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}

async fn ensure_contract_parties_unchanged(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    contract: &LegalContract,
) -> Result<(), PersistenceError> {
    let stored: Vec<(Uuid, String)> = sqlx::query_as(
        "SELECT party_id, party_role
         FROM erpx_holding.legal_contract_parties
         WHERE data_profile_key=$1 AND contract_id=$2
         ORDER BY party_id",
    )
    .bind(data_profile_key)
    .bind(contract.id)
    .fetch_all(&mut **tx)
    .await?;
    let expected: Vec<(Uuid, String)> = contract
        .parties
        .iter()
        .map(|party| (party.party_id, party.role.clone()))
        .collect::<BTreeMap<_, _>>()
        .into_iter()
        .collect();
    if stored != expected {
        return Err(invalid(
            "legal contract parties are immutable outside draft status",
        ));
    }
    Ok(())
}

async fn persist_approval(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    contract: &LegalContract,
    version: u64,
) -> Result<(), PersistenceError> {
    let Some(approved_by) = contract.approved_by.as_deref() else {
        return Ok(());
    };
    let existing: Option<(Uuid, String, i64)> = sqlx::query_as(
        "SELECT holding_id, approved_by, contract_version
         FROM erpx_holding.legal_contract_approvals
         WHERE data_profile_key=$1 AND contract_id=$2",
    )
    .bind(data_profile_key)
    .bind(contract.id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some((existing_holding_id, existing_approved_by, existing_version)) = existing {
        if existing_holding_id != contract.scope.holding_id {
            return Err(invalid(
                "legal contract approval fact holding binding is inconsistent",
            ));
        }
        if existing_approved_by != approved_by {
            return Err(invalid("legal contract approval cannot be reassigned"));
        }
        if as_u64(existing_version, "legal approval version")? > version {
            return Err(invalid(
                "legal contract approval fact version is ahead of the aggregate",
            ));
        }
        return Ok(());
    }
    sqlx::query(
        "INSERT INTO erpx_holding.legal_contract_approvals
            (data_profile_key, contract_id, holding_id, approved_by, contract_version)
         VALUES ($1,$2,$3,$4,$5)",
    )
    .bind(data_profile_key)
    .bind(contract.id)
    .bind(contract.scope.holding_id)
    .bind(approved_by)
    .bind(as_i64(version, "legal approval version")?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn persist_retention(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    contract: &LegalContract,
    version: u64,
    previous_version: Option<u64>,
) -> Result<(), PersistenceError> {
    let existing = sqlx::query(
        "SELECT holding_id, retention_until, contract_version
         FROM erpx_holding.legal_contract_retention
         WHERE data_profile_key=$1 AND contract_id=$2
         FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(contract.id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(row) = existing {
        let stored_holding_id: Uuid = row.try_get("holding_id")?;
        if stored_holding_id != contract.scope.holding_id {
            return Err(invalid(
                "legal contract retention fact holding binding is inconsistent",
            ));
        }
        let stored_retention: DateTime<Utc> = row.try_get("retention_until")?;
        if stored_retention != contract.retention_until {
            return Err(invalid(
                "legal contract retention fact deadline is inconsistent",
            ));
        }
        let stored_version = as_u64(row.try_get("contract_version")?, "legal retention version")?;
        if previous_version != Some(stored_version) {
            return Err(invalid(
                "legal contract retention fact version is inconsistent",
            ));
        }
    } else if previous_version.is_some() {
        return Err(invalid("legal contract retention fact is missing"));
    }
    let retention_state = retention_state(contract.retention_until, Utc::now());
    sqlx::query(
        "INSERT INTO erpx_holding.legal_contract_retention
            (data_profile_key, contract_id, holding_id, retention_until, retention_state,
             contract_version)
         VALUES ($1,$2,$3,$4,$5,$6)
         ON CONFLICT (data_profile_key, contract_id) DO UPDATE SET
             holding_id=EXCLUDED.holding_id,
             retention_state=EXCLUDED.retention_state,
             contract_version=EXCLUDED.contract_version,
             updated_at=now()",
    )
    .bind(data_profile_key)
    .bind(contract.id)
    .bind(contract.scope.holding_id)
    .bind(contract.retention_until)
    .bind(retention_state)
    .bind(as_i64(version, "legal retention version")?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_document(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    document: &LegalDocument,
    source_version: u64,
    idempotency_key: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_holding.legal_documents
            (data_profile_key, document_id, holding_id, company_id, branch_id, title,
             current_version, archived, source_version, idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)",
    )
    .bind(data_profile_key)
    .bind(document.id)
    .bind(document.scope.holding_id)
    .bind(document.scope.company_id)
    .bind(document.scope.branch_id)
    .bind(document.title.as_str())
    .bind(as_i64(
        document.current_version,
        "legal document current version",
    )?)
    .bind(document.archived)
    .bind(as_i64(source_version, "legal document source version")?)
    .bind(idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn update_document(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    document: &LegalDocument,
    source_version: u64,
    previous_source_version: u64,
    idempotency_key: &str,
) -> Result<(), PersistenceError> {
    let result = sqlx::query(
        "UPDATE erpx_holding.legal_documents
         SET holding_id=$3, company_id=$4, branch_id=$5, title=$6, current_version=$7,
             archived=$8, source_version=$9, idempotency_key=$10, updated_at=now()
         WHERE data_profile_key=$1 AND document_id=$2 AND source_version=$11",
    )
    .bind(data_profile_key)
    .bind(document.id)
    .bind(document.scope.holding_id)
    .bind(document.scope.company_id)
    .bind(document.scope.branch_id)
    .bind(document.title.as_str())
    .bind(as_i64(
        document.current_version,
        "legal document current version",
    )?)
    .bind(document.archived)
    .bind(as_i64(source_version, "legal document source version")?)
    .bind(idempotency_key)
    .bind(as_i64(
        previous_source_version,
        "legal document source version",
    )?)
    .execute(&mut **tx)
    .await?;
    if result.rows_affected() != 1 {
        return Err(invalid("legal document version update was not applied"));
    }
    Ok(())
}

async fn persist_document_versions(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    document: &LegalDocument,
) -> Result<(), PersistenceError> {
    for version in &document.versions {
        let existing = sqlx::query(
            "SELECT holding_id, object_ref, content_sha256, created_at, created_by
             FROM erpx_holding.legal_document_versions
             WHERE data_profile_key=$1 AND document_id=$2 AND version=$3",
        )
        .bind(data_profile_key)
        .bind(document.id)
        .bind(as_i64(version.version, "legal document version")?)
        .fetch_optional(&mut **tx)
        .await?;
        if let Some(row) = existing {
            let same = row.try_get::<Uuid, _>("holding_id")? == document.scope.holding_id
                && row.try_get::<String, _>("object_ref")? == version.object_ref
                && row.try_get::<String, _>("content_sha256")? == version.content_sha256
                && row.try_get::<DateTime<Utc>, _>("created_at")? == version.created_at
                && row.try_get::<String, _>("created_by")? == version.created_by;
            if !same {
                return Err(invalid("legal document versions are immutable"));
            }
            continue;
        }
        sqlx::query(
            "INSERT INTO erpx_holding.legal_document_versions
                (data_profile_key, document_id, holding_id, version, object_ref,
                 content_sha256, created_at, created_by)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(data_profile_key)
        .bind(document.id)
        .bind(document.scope.holding_id)
        .bind(as_i64(version.version, "legal document version")?)
        .bind(&version.object_ref)
        .bind(&version.content_sha256)
        .bind(version.created_at)
        .bind(&version.created_by)
        .execute(&mut **tx)
        .await?;
    }
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM erpx_holding.legal_document_versions
         WHERE data_profile_key=$1 AND document_id=$2",
    )
    .bind(data_profile_key)
    .bind(document.id)
    .fetch_one(&mut **tx)
    .await?;
    if usize::try_from(count).unwrap_or(usize::MAX) != document.versions.len() {
        return Err(invalid("legal document versions cannot be removed"));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn find_replay(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    idempotency_key: &str,
    request_hash: &str,
    expected_aggregate_type: LegalAggregateType,
    expected_aggregate_id: Uuid,
    expected_payload: &LegalOutboxPayload,
    context: &CommandContext,
) -> Result<Option<LegalWriteReceipt>, PersistenceError> {
    let row = sqlx::query(
        "SELECT request_hash, aggregate_type, aggregate_id, aggregate_version, event_id
         FROM erpx_holding.legal_command_idempotency
         WHERE data_profile_key=$1 AND idempotency_key=$2
         FOR UPDATE",
    )
    .bind(data_profile_key)
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else { return Ok(None) };
    if row.try_get::<String, _>("request_hash")? != request_hash {
        return Err(PersistenceError::IdempotencyConflict {
            key: idempotency_key.into(),
        });
    }
    let stored_aggregate_type =
        LegalAggregateType::parse(&row.try_get::<String, _>("aggregate_type")?)?;
    let stored_aggregate_id: Uuid = row.try_get("aggregate_id")?;
    let stored_version = as_u64(
        row.try_get("aggregate_version")?,
        "legal idempotency aggregate version",
    )?;
    validate_positive_version(stored_version, "legal idempotency aggregate version")?;
    let event_id: Uuid = row.try_get("event_id")?;
    validate_uuid(event_id, "legal idempotency event ID")?;
    if stored_aggregate_type != expected_aggregate_type
        || stored_aggregate_id != expected_aggregate_id
    {
        return Err(invalid(
            "legal idempotency record is bound to a different aggregate",
        ));
    }
    let receipt = LegalWriteReceipt {
        aggregate_type: stored_aggregate_type,
        aggregate_id: stored_aggregate_id,
        version: stored_version,
        idempotency_key: idempotency_key.into(),
        event_id,
        replayed: true,
    };
    let source = sqlx::query(
        "SELECT event_type, aggregate_type, aggregate_id, aggregate_version,
                payload, contract_version, event_version, active_scope, actor,
                correlation_id, causation_id, idempotency_key
         FROM erpx_holding.outbox_events
         WHERE data_profile_key=$1 AND event_id=$2
         FOR SHARE",
    )
    .bind(data_profile_key)
    .bind(event_id)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| invalid("legal idempotency replay is missing its source outbox event"))?;
    let source_fact = LegalReplaySourceFact {
        event_type: source.try_get("event_type")?,
        aggregate_type: source.try_get("aggregate_type")?,
        aggregate_id: source.try_get("aggregate_id")?,
        aggregate_version: as_u64(
            source.try_get("aggregate_version")?,
            "legal replay source version",
        )?,
        payload: source.try_get("payload")?,
        contract_version: source.try_get("contract_version")?,
        event_version: source.try_get("event_version")?,
        active_scope: source.try_get("active_scope")?,
        actor: source.try_get("actor")?,
        correlation_id: source.try_get("correlation_id")?,
        causation_id: source.try_get("causation_id")?,
        idempotency_key: source.try_get("idempotency_key")?,
    };
    validate_replay_source_fact(&receipt, expected_payload, context, &source_fact)?;
    Ok(Some(receipt))
}

fn validate_replay_source_fact(
    receipt: &LegalWriteReceipt,
    expected_payload: &LegalOutboxPayload,
    context: &CommandContext,
    source: &LegalReplaySourceFact,
) -> Result<(), PersistenceError> {
    let expected_event_type = match (receipt.aggregate_type, expected_payload) {
        (LegalAggregateType::Contract, LegalOutboxPayload::ContractSaved(_)) => {
            LEGAL_CONTRACT_SAVED_EVENT
        }
        (LegalAggregateType::Document, LegalOutboxPayload::DocumentSaved(_)) => {
            LEGAL_DOCUMENT_SAVED_EVENT
        }
        (LegalAggregateType::AccessHistory, LegalOutboxPayload::AccessRecorded(_)) => {
            LEGAL_ACCESS_RECORDED_EVENT
        }
        _ => {
            return Err(invalid(
                "legal idempotency replay payload does not match its aggregate type",
            ));
        }
    };
    let expected_payload = serde_json::to_value(expected_payload)?;
    let expected_scope = serde_json::to_value(&context.active_scope)?;
    let expected_actor = serde_json::to_value(&context.actor)?;
    let expected_contract_version = i32::from(CONTRACT_VERSION);
    if source.event_type != expected_event_type
        || source.aggregate_type != receipt.aggregate_type.as_str()
        || source.aggregate_id != receipt.aggregate_id
        || source.aggregate_version != receipt.version
        || source.payload != expected_payload
        || source.contract_version != Some(expected_contract_version)
        || source.event_version != Some(1)
        || source.active_scope != Some(expected_scope)
        || source.actor != Some(expected_actor)
        || source.correlation_id != Some(context.correlation_id)
        || source.causation_id != context.causation_id
        || source.idempotency_key.as_deref() != Some(receipt.idempotency_key.as_str())
    {
        return Err(invalid(
            "legal idempotency replay does not match its authoritative outbox event",
        ));
    }
    Ok(())
}

async fn record_idempotency(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    idempotency_key: &str,
    request_hash: &str,
    receipt: &LegalWriteReceipt,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_holding.legal_command_idempotency
            (data_profile_key, idempotency_key, request_hash, aggregate_type,
             aggregate_id, aggregate_version, event_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(data_profile_key)
    .bind(idempotency_key)
    .bind(request_hash)
    .bind(receipt.aggregate_type.as_str())
    .bind(receipt.aggregate_id)
    .bind(as_i64(
        receipt.version,
        "legal idempotency aggregate version",
    )?)
    .bind(receipt.event_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn append_legal_event(
    tx: &mut Transaction<'_, Postgres>,
    data_profile_key: &str,
    event: &LegalOutboxEvent,
    context: &CommandContext,
) -> Result<(), PersistenceError> {
    let envelope = EventEnvelope {
        contract_version: context.contract_version,
        event_id: event.event_id,
        event_type: event.event_type.as_str().into(),
        event_version: 1,
        occurred_at: event.occurred_at,
        aggregate_type: event.aggregate_type.as_str().into(),
        aggregate_id: event.aggregate_id,
        aggregate_version: event.aggregate_version,
        active_scope: context.active_scope.clone(),
        actor: context.actor.clone(),
        correlation_id: context.correlation_id,
        causation_id: context.causation_id,
        idempotency_key: Some(context.idempotency_key.clone()),
        payload: serde_json::to_value(&event.payload)?,
    };
    if context.contract_version != CONTRACT_VERSION {
        return Err(invalid("legal event contract version is unsupported"));
    }
    sqlx::query(
        "INSERT INTO erpx_holding.outbox_events
            (event_id, aggregate_type, aggregate_id, data_profile_key, aggregate_version,
             event_type, payload, occurred_at, contract_version, event_version,
             active_scope, actor, correlation_id, causation_id, idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
    )
    .bind(envelope.event_id)
    .bind(&envelope.aggregate_type)
    .bind(envelope.aggregate_id)
    .bind(data_profile_key)
    .bind(as_i64(
        envelope.aggregate_version,
        "legal outbox aggregate version",
    )?)
    .bind(&envelope.event_type)
    .bind(&envelope.payload)
    .bind(envelope.occurred_at)
    .bind(i32::from(envelope.contract_version))
    .bind(i32::from(envelope.event_version))
    .bind(serde_json::to_value(&envelope.active_scope)?)
    .bind(serde_json::to_value(&envelope.actor)?)
    .bind(envelope.correlation_id)
    .bind(envelope.causation_id)
    .bind(envelope.idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn contract_event(contract: &LegalContract) -> LegalContractEvent {
    LegalContractEvent {
        contract_id: contract.id,
        holding_id: contract.scope.holding_id,
        company_id: contract.scope.company_id,
        branch_id: contract.scope.branch_id,
        reference: contract.reference.clone(),
        title: contract.title.clone(),
        object_ref: contract.object_ref.clone(),
        content_sha256: contract.content_sha256.clone(),
        retention_until: contract.retention_until,
        status: contract.status,
        created_by: contract.created_by.clone(),
        approved_by: contract.approved_by.clone(),
        parties: contract
            .parties
            .iter()
            .map(|party| LegalContractPartyEvent {
                party_id: party.party_id,
                role: party.role.clone(),
            })
            .collect(),
    }
}

fn document_event(document: &LegalDocument) -> LegalDocumentEvent {
    LegalDocumentEvent {
        document_id: document.id,
        holding_id: document.scope.holding_id,
        company_id: document.scope.company_id,
        branch_id: document.scope.branch_id,
        title: document.title.clone(),
        current_version: document.current_version,
        archived: document.archived,
        versions: document
            .versions
            .iter()
            .map(document_version_event)
            .collect(),
    }
}

fn document_version_event(version: &DocumentVersion) -> LegalDocumentVersionEvent {
    LegalDocumentVersionEvent {
        version: version.version,
        object_ref: version.object_ref.clone(),
        content_sha256: version.content_sha256.clone(),
        created_at: version.created_at,
        created_by: version.created_by.clone(),
    }
}

fn access_event(entry: &LegalAccessHistoryEntry) -> LegalAccessHistoryEvent {
    LegalAccessHistoryEvent {
        entry_id: entry.entry_id,
        document_id: entry.document_id,
        version: entry.version,
        actor_ref: entry.actor_ref.clone(),
        holding_id: entry.scope.holding_id,
        company_id: entry.scope.company_id,
        branch_id: entry.scope.branch_id,
        action: entry.action.clone(),
        purpose: entry.purpose.clone(),
        observed_at: entry.observed_at,
    }
}

fn validate_command_context(
    context: &CommandContext,
    scope: &LegalScope,
    expected_command_id: Option<Uuid>,
) -> Result<(), PersistenceError> {
    context
        .validate()
        .map_err(|error| invalid(format!("invalid legal command context: {error}")))?;
    if context.contract_version != CONTRACT_VERSION {
        return Err(invalid("legal command contract version is unsupported"));
    }
    if context.command_id.is_nil() {
        return Err(invalid("legal command ID cannot be nil"));
    }
    if context.actor.principal_ref.trim().is_empty() {
        return Err(invalid("legal command actor cannot be empty"));
    }
    if context.actor.principal_ref.len() > MAX_ACTOR_REF_BYTES
        || context.actor.principal_ref.chars().any(char::is_control)
    {
        return Err(invalid("legal command actor is invalid"));
    }
    validate_uuid(context.correlation_id, "legal command correlation ID")?;
    if let Some(causation_id) = context.causation_id {
        validate_uuid(causation_id, "legal command causation ID")?;
    }
    if matches!(
        context.actor.kind,
        ActorKind::PlatformUser | ActorKind::HoldingUser
    ) && context.actor.session_id.is_none()
    {
        return Err(invalid(
            "legal human commands require authenticated session provenance",
        ));
    }
    if !context.active_scope.contains(&scope.active_scope()) {
        return Err(invalid("legal command scope does not contain record scope"));
    }
    if expected_command_id.is_some_and(|command_id| command_id != context.command_id) {
        return Err(invalid("legal aggregate command ID does not match context"));
    }
    Ok(())
}

fn parse_contract_status(value: &str) -> Result<ContractStatus, PersistenceError> {
    match value {
        "draft" => Ok(ContractStatus::Draft),
        "review" => Ok(ContractStatus::Review),
        "approved" => Ok(ContractStatus::Approved),
        "active" => Ok(ContractStatus::Active),
        "expired" => Ok(ContractStatus::Expired),
        "terminated" => Ok(ContractStatus::Terminated),
        "archived" => Ok(ContractStatus::Archived),
        _ => Err(invalid("legal contract status is invalid")),
    }
}

fn validate_contract_transition(
    from: ContractStatus,
    to: ContractStatus,
) -> Result<(), PersistenceError> {
    let allowed = matches!(
        (from, to),
        (ContractStatus::Draft, ContractStatus::Draft)
            | (ContractStatus::Draft, ContractStatus::Review)
            | (ContractStatus::Review, ContractStatus::Approved)
            | (ContractStatus::Approved, ContractStatus::Active)
            | (ContractStatus::Active, ContractStatus::Expired)
            | (ContractStatus::Active, ContractStatus::Terminated)
            | (ContractStatus::Expired, ContractStatus::Archived)
            | (ContractStatus::Terminated, ContractStatus::Archived)
    );
    if !allowed {
        return Err(invalid("legal contract lifecycle transition is invalid"));
    }
    Ok(())
}

fn validate_contract(contract: &LegalContract) -> Result<(), PersistenceError> {
    validate_uuid(contract.id, "legal contract ID")?;
    validate_scope(&contract.scope)?;
    validate_bounded(
        &contract.reference,
        "legal contract reference",
        MAX_LEGAL_REFERENCE_BYTES,
    )?;
    validate_bounded(
        &contract.title,
        "legal contract title",
        MAX_LEGAL_TITLE_BYTES,
    )?;
    validate_object_metadata(&contract.object_ref, "legal contract object reference")?;
    validate_digest(&contract.content_sha256, "legal contract content digest")?;
    validate_bounded(
        &contract.created_by,
        "legal contract creator",
        MAX_ACTOR_REF_BYTES,
    )?;
    if let Some(approved_by) = &contract.approved_by {
        validate_bounded(approved_by, "legal contract approver", MAX_ACTOR_REF_BYTES)?;
        if approved_by == &contract.created_by {
            return Err(invalid(
                "legal contract approval requires a different principal",
            ));
        }
    }
    validate_uuid(contract.last_command_id, "legal contract command ID")?;
    validate_positive_version(contract.version, "legal contract version")?;
    validate_retention(contract.retention_until)?;
    if matches!(
        contract.status,
        ContractStatus::Expired | ContractStatus::Archived
    ) && Utc::now() < contract.retention_until
    {
        return Err(invalid("legal contract retention is still active"));
    }
    if matches!(
        contract.status,
        ContractStatus::Draft | ContractStatus::Review
    ) && contract.approved_by.is_some()
    {
        return Err(invalid(
            "draft or review legal contracts cannot be approved",
        ));
    }
    if matches!(
        contract.status,
        ContractStatus::Approved
            | ContractStatus::Active
            | ContractStatus::Expired
            | ContractStatus::Terminated
            | ContractStatus::Archived
    ) && contract.approved_by.is_none()
    {
        return Err(invalid(
            "approved legal lifecycle states require an approver",
        ));
    }
    if contract.parties.len() > MAX_LEGAL_PARTIES {
        return Err(invalid("legal contract party limit was reached"));
    }
    let mut party_ids = HashSet::with_capacity(contract.parties.len());
    for party in &contract.parties {
        validate_uuid(party.party_id, "legal contract party ID")?;
        validate_bounded(
            &party.role,
            "legal contract party role",
            MAX_LEGAL_REFERENCE_BYTES,
        )?;
        if !party_ids.insert(party.party_id) {
            return Err(invalid("duplicate legal contract party"));
        }
    }
    Ok(())
}

fn validate_contract_command_actor_binding(
    contract: &LegalContract,
    context: &CommandContext,
) -> Result<(), PersistenceError> {
    if contract.version == 1 && contract.created_by != context.actor.principal_ref {
        return Err(invalid(
            "the initial Legal contract actor must match its creator",
        ));
    }
    if contract.status == ContractStatus::Approved
        && contract.approved_by.as_deref() != Some(context.actor.principal_ref.as_str())
    {
        return Err(invalid(
            "the Legal approval actor must match the approved-by principal",
        ));
    }
    Ok(())
}

fn validate_document(document: &LegalDocument) -> Result<(), PersistenceError> {
    validate_uuid(document.id, "legal document ID")?;
    validate_scope(&document.scope)?;
    validate_bounded(
        &document.title,
        "legal document title",
        MAX_LEGAL_TITLE_BYTES,
    )?;
    if document.versions.len() > MAX_DOCUMENT_VERSIONS {
        return Err(invalid("legal document version limit was reached"));
    }
    if document.current_version
        != document
            .versions
            .last()
            .map_or(0, |version| version.version)
    {
        return Err(invalid(
            "legal document current version is not the latest version",
        ));
    }
    if document.archived && document.current_version == 0 {
        return Err(invalid("an archived legal document requires a version"));
    }
    for (index, version) in document.versions.iter().enumerate() {
        let expected = u64::try_from(index + 1)
            .map_err(|_| invalid("legal document version index overflow"))?;
        if version.version != expected {
            return Err(invalid("legal document versions must be contiguous"));
        }
        validate_object_metadata(&version.object_ref, "legal document object reference")?;
        validate_digest(&version.content_sha256, "legal document content digest")?;
        validate_bounded(
            &version.created_by,
            "legal document version creator",
            MAX_ACTOR_REF_BYTES,
        )?;
        if version.created_at > Utc::now() + Duration::minutes(5) {
            return Err(invalid(
                "legal document version timestamp is too far in the future",
            ));
        }
    }
    Ok(())
}

fn validate_document_command_actor_binding(
    document: &LegalDocument,
    actor_ref: &str,
    stored_current_version: Option<u64>,
) -> Result<(), PersistenceError> {
    let first_unpersisted_version = match stored_current_version {
        Some(version) => version
            .checked_add(1)
            .ok_or_else(|| invalid("legal document current version overflow"))?,
        None => 1,
    };
    if document
        .versions
        .iter()
        .filter(|version| version.version >= first_unpersisted_version)
        .any(|version| version.created_by != actor_ref)
    {
        return Err(invalid(
            "legal document version creator must match the command actor",
        ));
    }
    Ok(())
}

fn validate_access_history(entry: &LegalAccessHistoryEntry) -> Result<(), PersistenceError> {
    validate_uuid(entry.entry_id, "legal access-history entry ID")?;
    validate_uuid(entry.document_id, "legal access-history document ID")?;
    validate_positive_version(entry.version, "legal access-history document version")?;
    validate_scope(&entry.scope)?;
    validate_bounded(
        &entry.actor_ref,
        "legal access-history actor",
        MAX_ACTOR_REF_BYTES,
    )?;
    validate_bounded(
        &entry.action,
        "legal access-history action",
        MAX_LEGAL_REFERENCE_BYTES,
    )?;
    validate_bounded(
        &entry.purpose,
        "legal access-history purpose",
        MAX_LEGAL_PURPOSE_BYTES,
    )?;
    if entry.observed_at > Utc::now() + Duration::minutes(5) {
        return Err(invalid(
            "legal access-history timestamp is too far in the future",
        ));
    }
    Ok(())
}

fn validate_scope(scope: &LegalScope) -> Result<(), PersistenceError> {
    scope
        .validate()
        .map_err(|error| invalid(format!("invalid legal scope: {error}")))
}

fn validate_retention(retention_until: DateTime<Utc>) -> Result<(), PersistenceError> {
    let latest_allowed = Utc::now()
        .checked_add_signed(MAX_RETENTION_HORIZON)
        .ok_or_else(|| invalid("legal retention horizon overflow"))?;
    if retention_until > latest_allowed {
        return Err(invalid("legal retention exceeds the bounded horizon"));
    }
    Ok(())
}

fn validate_object_metadata(value: &str, field: &'static str) -> Result<(), PersistenceError> {
    validate_bounded(value, field, MAX_LEGAL_REFERENCE_BYTES)?;
    let lower = value.to_ascii_lowercase();
    if value.starts_with('/')
        || value.contains("..")
        || value.contains("://")
        || lower.contains("data:")
        || lower.contains("base64")
        || value.bytes().any(|character| b"?#%=".contains(&character))
        || !value
            .bytes()
            .all(|character| character.is_ascii_alphanumeric() || b"/._-".contains(&character))
    {
        return Err(invalid(format!(
            "{field} must be an opaque storage reference"
        )));
    }
    Ok(())
}

fn validate_digest(value: &str, field: &'static str) -> Result<(), PersistenceError> {
    validate_bounded(value, field, MAX_LEGAL_REFERENCE_BYTES)?;
    let digest = value
        .strip_prefix("sha256:")
        .ok_or_else(|| invalid(format!("{field} must be an opaque sha256 digest reference")))?;
    if digest.len() != 64
        || !digest
            .chars()
            .all(|character| character.is_ascii_hexdigit())
    {
        return Err(invalid(format!(
            "{field} must contain exactly 64 hexadecimal digest characters"
        )));
    }
    Ok(())
}

fn validate_bounded(value: &str, field: &'static str, max: usize) -> Result<(), PersistenceError> {
    if value.trim().is_empty() {
        return Err(invalid(format!("{field} cannot be empty")));
    }
    if value.len() > max {
        return Err(invalid(format!("{field} exceeds its bounded length")));
    }
    if value.chars().any(char::is_control) {
        return Err(invalid(format!("{field} contains a control character")));
    }
    if contains_sensitive_marker(value) {
        return Err(invalid(format!("{field} contains sensitive material")));
    }
    Ok(())
}

fn contains_sensitive_marker(value: &str) -> bool {
    let lower = value.to_ascii_lowercase();
    [
        "password",
        "secret",
        "token",
        "private_key",
        "payment_instrument",
        "credential",
        "bearer ",
        "authorization",
    ]
    .iter()
    .any(|marker| lower.contains(marker))
}

fn validate_idempotency_key(value: &str) -> Result<(), PersistenceError> {
    validate_bounded(value, "legal idempotency key", MAX_IDEMPOTENCY_KEY_BYTES)
}

fn validate_data_profile_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > MAX_DATA_PROFILE_KEY_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn validate_uuid(value: Uuid, field: &'static str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(invalid(format!("{field} cannot be nil")));
    }
    Ok(())
}

fn validate_positive_version(value: u64, field: &'static str) -> Result<(), PersistenceError> {
    if value == 0 || value > MAX_EVENT_VERSION {
        return Err(invalid(format!("{field} must fit a positive BIGINT")));
    }
    Ok(())
}

fn ensure_expected_version(
    aggregate_type: LegalAggregateType,
    aggregate_id: Uuid,
    expected: Option<u64>,
    actual: Option<u64>,
) -> Result<(), PersistenceError> {
    if expected != actual {
        return Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: aggregate_type.as_str().into(),
            aggregate_id,
            expected,
            actual,
        });
    }
    Ok(())
}

fn ensure_domain_version(
    aggregate_type: LegalAggregateType,
    aggregate_id: Uuid,
    supplied: u64,
    expected: u64,
) -> Result<(), PersistenceError> {
    if supplied != expected {
        return Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: aggregate_type.as_str().into(),
            aggregate_id,
            expected: Some(expected),
            actual: Some(supplied),
        });
    }
    Ok(())
}

fn ensure_scope_matches(
    scope: &LegalScope,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
) -> Result<(), PersistenceError> {
    if scope.holding_id != holding_id
        || scope.company_id != company_id
        || scope.branch_id != branch_id
    {
        return Err(invalid(
            "legal scope is not contained by the persisted document scope",
        ));
    }
    Ok(())
}

fn ensure_immutable_scope(
    scope: &LegalScope,
    holding_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    entity: &'static str,
) -> Result<(), PersistenceError> {
    if scope.holding_id != holding_id
        || scope.company_id != company_id
        || scope.branch_id != branch_id
    {
        return Err(invalid(format!("{entity} scope is immutable")));
    }
    Ok(())
}

fn request_hash<T: Serialize>(
    aggregate_type: LegalAggregateType,
    aggregate_id: Uuid,
    value: &T,
    expected_version: Option<u64>,
) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        aggregate_type,
        aggregate_id,
        value,
        expected_version,
    ))?)))
}

fn contract_status(status: ContractStatus) -> &'static str {
    match status {
        ContractStatus::Draft => "draft",
        ContractStatus::Review => "review",
        ContractStatus::Approved => "approved",
        ContractStatus::Active => "active",
        ContractStatus::Expired => "expired",
        ContractStatus::Terminated => "terminated",
        ContractStatus::Archived => "archived",
    }
}

fn retention_state(retention_until: DateTime<Utc>, now: DateTime<Utc>) -> &'static str {
    if now >= retention_until {
        "ended"
    } else {
        "active"
    }
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| invalid(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| invalid(format!("{field} is negative")))
}

fn invalid(message: impl Into<String>) -> PersistenceError {
    PersistenceError::InvalidData(message.into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::schema_test;
    use chrono::Duration;
    use erpx_domain::contracts::{ActorAttribution, ActorKind, AuthorityScope, CommandContext};
    use erpx_domain::legal::ContractParty;

    fn scope() -> LegalScope {
        LegalScope::new(Uuid::new_v4(), Uuid::new_v4(), Some(Uuid::new_v4())).expect("scope")
    }

    fn context(scope: &LegalScope, actor: &str, key: &str) -> CommandContext {
        CommandContext::try_new(
            key,
            ActorAttribution {
                principal_ref: actor.into(),
                kind: ActorKind::HoldingUser,
                session_id: Some(Uuid::new_v4()),
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope.active_scope(),
            "legal_persistence_test",
        )
        .expect("context")
    }

    fn contract() -> LegalContract {
        let scope = scope();
        let context = context(&scope, "creator", "contract-create");
        LegalContract::new(
            Uuid::new_v4(),
            "contract-001",
            "Supplier agreement",
            scope,
            "opaque/contracts/001-v1",
            "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
            Utc::now() + Duration::days(30),
            &context,
        )
        .expect("contract")
    }

    #[tokio::test]
    async fn constructor_rejects_unbounded_or_cross_holding_data_profile_references() {
        let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
        assert!(SqlxLegalPersistence::new(pool.clone(), " ").is_err());
        assert!(SqlxLegalPersistence::new(pool, "holding\nref").is_err());
        assert!(validate_data_profile_key(&"x".repeat(MAX_DATA_PROFILE_KEY_BYTES + 1)).is_err());
    }

    #[test]
    fn validation_rejects_secret_material_without_echoing_it() {
        let mut legal_contract = contract();
        legal_contract.object_ref = "opaque/token-value".into();
        let error = validate_contract(&legal_contract).expect_err("secret must be rejected");
        let rendered = error.to_string();
        assert!(rendered.contains("sensitive material"));
        assert!(!rendered.contains("token-value"));

        let scope = scope();
        let command = context(&scope, "reader", "document-create");
        let document =
            LegalDocument::new(Uuid::new_v4(), scope, "safe title", &command).expect("document");
        assert!(validate_document(&document).is_ok());
        assert!(validate_digest("raw-document-bytes", "digest").is_err());
    }

    #[test]
    fn scope_retention_and_approval_invariants_are_enforced_at_the_write_boundary() {
        let mut legal_contract = contract();
        legal_contract.status = ContractStatus::Approved;
        legal_contract.approved_by = Some(legal_contract.created_by.clone());
        let error = validate_contract(&legal_contract).expect_err("approval separation");
        assert!(error.to_string().contains("different principal"));

        legal_contract.approved_by = Some("reviewer".into());
        legal_contract.retention_until = Utc::now() + Duration::days(36_526);
        let error = validate_contract(&legal_contract).expect_err("retention horizon");
        assert!(error.to_string().contains("bounded horizon"));

        let document_scope = scope();
        let other_scope = LegalScope::new(
            document_scope.holding_id,
            Uuid::new_v4(),
            document_scope.branch_id,
        )
        .expect("other scope");
        assert!(
            ensure_scope_matches(
                &other_scope,
                document_scope.holding_id,
                document_scope.company_id,
                document_scope.branch_id,
            )
            .is_err()
        );
        assert!(
            ensure_immutable_scope(
                &other_scope,
                document_scope.holding_id,
                document_scope.company_id,
                document_scope.branch_id,
                "legal document",
            )
            .is_err()
        );
        ensure_immutable_scope(
            &document_scope,
            document_scope.holding_id,
            document_scope.company_id,
            document_scope.branch_id,
            "legal document",
        )
        .expect("an unchanged legal scope remains bound");
    }

    #[test]
    fn document_versions_are_bound_to_the_authoritative_command_actor() {
        let scope = scope();
        let creator = context(&scope, "creator", "document-create");
        let mut document =
            LegalDocument::new(Uuid::new_v4(), scope, "Document actor binding", &creator)
                .expect("document");
        document
            .add_version(
                "opaque/documents/actor-binding-v1",
                "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa",
                &creator,
            )
            .expect("version");
        assert!(validate_document_command_actor_binding(&document, "creator", None).is_ok());
        assert!(validate_document_command_actor_binding(&document, "other", None).is_err());

        let mut second = document.clone();
        let other = context(&second.scope, "other", "document-v2");
        second
            .add_version(
                "opaque/documents/actor-binding-v2",
                "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb",
                &other,
            )
            .expect("second version");
        assert!(validate_document_command_actor_binding(&second, "other", Some(1)).is_ok());
        assert!(validate_document_command_actor_binding(&second, "creator", Some(1)).is_err());
    }

    #[test]
    fn human_legal_commands_require_session_provenance_but_workers_do_not() {
        let scope = scope();
        let mut human = context(&scope, "human", "session-required");
        human.actor.session_id = None;
        let error = validate_command_context(&human, &scope, None)
            .expect_err("human Legal commands without a session must fail closed");
        assert!(
            error
                .to_string()
                .contains("authenticated session provenance")
        );

        let worker = CommandContext::try_new(
            "worker-command",
            ActorAttribution {
                principal_ref: "system-worker:legal-retention".into(),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope.active_scope(),
            "legal retention sweep",
        )
        .expect("worker context");
        validate_command_context(&worker, &scope, None)
            .expect("system workers use durable worker identity instead of a user session");

        let mut malformed = context(&scope, "human", "malformed-envelope");
        malformed.actor.principal_ref = "human\nprincipal".into();
        assert!(validate_command_context(&malformed, &scope, None).is_err());
        malformed = context(&scope, "human", "missing-correlation");
        malformed.correlation_id = Uuid::nil();
        assert!(validate_command_context(&malformed, &scope, None).is_err());
    }

    #[test]
    fn contract_lifecycle_actor_bindings_are_fail_closed() {
        let legal_contract = contract();
        let wrong_creator = context(&legal_contract.scope, "other-creator", "wrong-creator");
        assert!(validate_contract_command_actor_binding(&legal_contract, &wrong_creator).is_err());

        let mut approved = legal_contract;
        approved.status = ContractStatus::Approved;
        approved.approved_by = Some("reviewer".into());
        approved.version = 2;
        let reviewer = context(&approved.scope, "reviewer", "approval");
        validate_contract_command_actor_binding(&approved, &reviewer)
            .expect("approved-by actor binding");
        assert!(validate_contract_command_actor_binding(&approved, &wrong_creator).is_err());
    }

    #[test]
    fn domain_approval_and_retention_rules_are_preserved() {
        let mut legal_contract = contract();
        legal_contract
            .add_party(
                ContractParty::new(Uuid::new_v4(), "supplier").expect("party"),
                &context(&legal_contract.scope, "creator", "party"),
            )
            .expect("party add");
        legal_contract
            .submit_for_review(&context(&legal_contract.scope, "creator", "review"))
            .expect("review");
        assert_eq!(
            legal_contract.approve(&context(&legal_contract.scope, "creator", "approve")),
            Err(erpx_domain::legal::LegalError::ApprovalSeparation)
        );
    }

    #[test]
    fn idempotency_hashes_bind_operation_identity_and_typed_events_redact_no_bytes() {
        let value = contract();
        let first =
            request_hash(LegalAggregateType::Contract, value.id, &value, None).expect("hash");
        let second =
            request_hash(LegalAggregateType::Contract, value.id, &value, None).expect("hash");
        let changed =
            request_hash(LegalAggregateType::Contract, value.id, &value, Some(1)).expect("hash");
        assert_eq!(first, second);
        assert_ne!(first, changed);
        assert_eq!(
            LegalEventType::ContractSaved.as_str(),
            LEGAL_CONTRACT_SAVED_EVENT
        );

        let event = LegalOutboxPayload::ContractSaved(contract_event(&value));
        let encoded = serde_json::to_string(&event).expect("event JSON");
        assert!(!encoded.contains("bytea"));
        assert!(!encoded.contains("password"));
        assert!(!encoded.contains("token"));
    }

    #[test]
    fn idempotency_replay_requires_exact_authoritative_event_and_context_fence() {
        let value = contract();
        let command = context(&value.scope, "creator", "contract-create");
        let payload = LegalOutboxPayload::ContractSaved(contract_event(&value));
        let receipt = LegalWriteReceipt {
            aggregate_type: LegalAggregateType::Contract,
            aggregate_id: value.id,
            version: value.version,
            idempotency_key: command.idempotency_key.clone(),
            event_id: Uuid::new_v4(),
            replayed: true,
        };
        let source = LegalReplaySourceFact {
            event_type: LEGAL_CONTRACT_SAVED_EVENT.into(),
            aggregate_type: LEGAL_CONTRACT_AGGREGATE.into(),
            aggregate_id: value.id,
            aggregate_version: value.version,
            payload: serde_json::to_value(&payload).expect("payload"),
            contract_version: Some(i32::from(CONTRACT_VERSION)),
            event_version: Some(1),
            active_scope: Some(serde_json::to_value(&command.active_scope).expect("scope")),
            actor: Some(serde_json::to_value(&command.actor).expect("actor")),
            correlation_id: Some(command.correlation_id),
            causation_id: command.causation_id,
            idempotency_key: Some(command.idempotency_key.clone()),
        };
        validate_replay_source_fact(&receipt, &payload, &command, &source)
            .expect("exact source replay evidence");

        let mut tampered = source.clone();
        tampered.payload = serde_json::json!({"kind": "contract_saved", "value": {}});
        assert!(validate_replay_source_fact(&receipt, &payload, &command, &tampered).is_err());

        let mut wrong_scope = source.clone();
        wrong_scope.active_scope = Some(
            serde_json::to_value(AuthorityScope::Company {
                holding_id: Uuid::new_v4(),
                company_id: Uuid::new_v4(),
            })
            .expect("scope"),
        );
        assert!(validate_replay_source_fact(&receipt, &payload, &command, &wrong_scope).is_err());

        let mut wrong_event = source;
        wrong_event.event_type = LEGAL_DOCUMENT_SAVED_EVENT.into();
        assert!(validate_replay_source_fact(&receipt, &payload, &command, &wrong_event).is_err());
    }

    #[test]
    fn migration_defines_normalized_forced_rls_and_append_only_legal_state() {
        let baseline = include_str!("../../migration/migrations_holding_write/0001_baseline.sql");
        let sql = schema_test::select_statements(baseline, &["erpx_holding.legal_"]);
        for table in [
            "legal_contracts",
            "legal_contract_parties",
            "legal_contract_approvals",
            "legal_contract_retention",
            "legal_documents",
            "legal_document_versions",
            "legal_access_history",
            "legal_command_idempotency",
        ] {
            assert!(sql.contains(&format!("CREATE TABLE IF NOT EXISTS erpx_holding.{table}")));
            assert!(sql.contains(&format!(
                "ALTER TABLE erpx_holding.{table} FORCE ROW LEVEL SECURITY"
            )));
        }
        assert!(sql.contains("legal_document_versions_immutable"));
        assert!(sql.contains("legal_access_history_immutable"));
        assert!(sql.contains("data_profile_key = current_setting('erpx.data_profile_key', true)"));
        assert!(sql.contains("^sha256:[0-9A-Fa-f]{64}$"));
        assert!(!sql.to_ascii_lowercase().contains("bytea"));
        assert!(!sql.to_ascii_lowercase().contains("document_bytes"));
        assert!(!sql.contains("read_pool"));
        let envelope = baseline;
        for column in [
            "contract_version",
            "event_version",
            "active_scope",
            "actor",
            "correlation_id",
            "causation_id",
            "idempotency_key",
        ] {
            assert!(envelope.contains(&format!("ADD COLUMN IF NOT EXISTS {column}")));
        }
    }

    #[test]
    fn typed_aggregate_names_are_closed() {
        assert_eq!(
            LegalAggregateType::Contract.as_str(),
            LEGAL_CONTRACT_AGGREGATE
        );
        assert!(LegalAggregateType::parse("legal_contract").is_ok());
        assert!(LegalAggregateType::parse("document_bytes").is_err());
        assert_eq!(contract_status(ContractStatus::Archived), "archived");
        assert_eq!(retention_state(Utc::now(), Utc::now()), "ended");
        let _ = AuthorityScope::Company {
            holding_id: Uuid::new_v4(),
            company_id: Uuid::new_v4(),
        };
    }
}
