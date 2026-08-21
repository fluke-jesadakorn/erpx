use async_trait::async_trait;
use erpx_cqrs_sqlx::{
    AppliedGuardContext, AtomicCommand, AtomicCommandGuard, AtomicCommandOutcome, AuditWrite,
    CommandStore, CqrsSqlxError, GuardCommandContext, OutboxEventWrite, RequestHash, WritePool,
};
use erpx_integration_contracts::{
    ITEM_DIRECTORY_FACT_VERSION, ITEM_DIRECTORY_OWNER_EVENT_TYPE, IntegrationEvent,
    ItemDescriptorDraft, ItemDescriptorDraftV2, record_policy_id,
};
use erpx_kernel::{AggregateId, AuthorityScope, CommandId, DataClassification, EventId};
use erpx_quality::{
    domain::{
        InspectionId, InspectionKind, InspectionStatus, QualityInspection, QualityInspectionState,
    },
    ports::{
        CommandFingerprint, CommandIdentity, CommandUnitOfWork, PortError, QualityCommit,
        StoredCommit, WriteRepository,
    },
};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{
    AGGREGATE_TYPE, CONTEXT_NAME, EVENT_VERSION, SNAPSHOT_EVENT_TYPE, inspection_partition,
    projection::{QualityOutboxPayload, QualitySnapshot},
};

#[derive(Debug, Clone)]
pub struct QualityWriteRepository {
    pool: WritePool,
}

impl QualityWriteRepository {
    pub fn new(pool: WritePool) -> Self {
        Self { pool }
    }
}

#[derive(Debug)]
struct AuthoritySqlScope {
    mode: &'static str,
    holding_id: Option<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
    company_ids: Vec<Uuid>,
    branch_ids: Vec<Uuid>,
}

impl AuthoritySqlScope {
    fn try_from_scope(scope: &AuthorityScope) -> Result<Self, PortError> {
        scope.validate().map_err(|_| PortError::Denied)?;
        Ok(match scope {
            AuthorityScope::Platform => Self {
                mode: "platform",
                holding_id: None,
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Holding { holding_id } => Self {
                mode: "holding",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Company {
                holding_id,
                company_id,
            } => Self {
                mode: "company",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: None,
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            } => Self {
                mode: "branch",
                holding_id: Some(*holding_id),
                company_id: Some(*company_id),
                branch_id: Some(*branch_id),
                company_ids: Vec::new(),
                branch_ids: Vec::new(),
            },
            AuthorityScope::Subtree {
                holding_id,
                organization_node_id,
            } => Self {
                mode: "subtree",
                holding_id: Some(*holding_id),
                company_id: None,
                branch_id: None,
                company_ids: vec![*organization_node_id],
                branch_ids: Vec::new(),
            },
        })
    }
}

#[derive(Debug, Clone)]
struct QualityInspectionGuard {
    aggregate: QualityInspection,
}

#[async_trait]
impl AtomicCommandGuard for QualityInspectionGuard {
    async fn authoritative_version(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<Option<u64>, CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)?;
        let current: Option<i64> = sqlx::query_scalar(
            "SELECT aggregate_version
             FROM erpx_quality_write.inspection_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND inspection_id=$1
             FOR UPDATE",
        )
        .bind(self.aggregate.id().as_uuid())
        .fetch_optional(&mut **transaction)
        .await?;
        current.map(from_i64_store).transpose()
    }

    async fn validate(
        &self,
        _transaction: &mut Transaction<'_, Postgres>,
        context: &GuardCommandContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        validate_guard_context(&self.aggregate, context)
    }

    async fn apply(
        &self,
        transaction: &mut Transaction<'_, Postgres>,
        context: &AppliedGuardContext<'_>,
    ) -> Result<(), CqrsSqlxError> {
        if context.context != CONTEXT_NAME
            || context.aggregate_type != AGGREGATE_TYPE
            || context.aggregate_id.as_uuid() != self.aggregate.id().as_uuid()
            || context.partition != inspection_partition(self.aggregate.id())
            || context.aggregate_version != self.aggregate.version()
        {
            return Err(CqrsSqlxError::GuardRejected(
                "Quality normalized apply metadata mismatch",
            ));
        }
        let result = sqlx::query(
            "INSERT INTO erpx_quality_write.inspection_authority
                (holding_id,inspection_id,aggregate_version,reference,holding_id,company_id,
                 branch_id,inspection_kind,product_id,lot_reference,quantity,passed_quantity,
                 failed_quantity,inspection_status,disposition_note,created_at,updated_at)
             VALUES (erpx_cqrs_write.current_holding_id(),$1,$2,$3,$4,$5,$6,$7,$8,$9,
                     $10,$11,$12,$13,$14,$15,$16)
             ON CONFLICT (holding_id,inspection_id) DO UPDATE SET
                aggregate_version=EXCLUDED.aggregate_version,
                reference=EXCLUDED.reference,holding_id=EXCLUDED.holding_id,
                company_id=EXCLUDED.company_id,branch_id=EXCLUDED.branch_id,
                inspection_kind=EXCLUDED.inspection_kind,product_id=EXCLUDED.product_id,
                lot_reference=EXCLUDED.lot_reference,quantity=EXCLUDED.quantity,
                passed_quantity=EXCLUDED.passed_quantity,failed_quantity=EXCLUDED.failed_quantity,
                inspection_status=EXCLUDED.inspection_status,
                disposition_note=EXCLUDED.disposition_note,created_at=EXCLUDED.created_at,
                updated_at=EXCLUDED.updated_at
             WHERE erpx_quality_write.inspection_authority.holding_id=
                       erpx_cqrs_write.current_holding_id()
               AND erpx_quality_write.inspection_authority.aggregate_version + 1 =
                       EXCLUDED.aggregate_version",
        )
        .bind(self.aggregate.id().as_uuid())
        .bind(to_i64(self.aggregate.version())?)
        .bind(self.aggregate.reference())
        .bind(self.aggregate.holding_id())
        .bind(self.aggregate.company_id())
        .bind(self.aggregate.branch_id())
        .bind(kind_name(self.aggregate.kind()))
        .bind(self.aggregate.product_id())
        .bind(self.aggregate.lot_reference())
        .bind(to_i64(self.aggregate.quantity())?)
        .bind(to_i64(self.aggregate.passed_quantity())?)
        .bind(to_i64(self.aggregate.failed_quantity())?)
        .bind(status_name(self.aggregate.status()))
        .bind(self.aggregate.disposition_note())
        .bind(self.aggregate.created_at())
        .bind(self.aggregate.updated_at())
        .execute(&mut **transaction)
        .await?;
        if result.rows_affected() != 1 {
            return Err(CqrsSqlxError::GuardRejected(
                "Quality normalized version did not advance exactly once",
            ));
        }
        Ok(())
    }
}

fn validate_guard_context(
    aggregate: &QualityInspection,
    context: &GuardCommandContext<'_>,
) -> Result<(), CqrsSqlxError> {
    if context.context != CONTEXT_NAME
        || context.aggregate_type != AGGREGATE_TYPE
        || context.aggregate_id.as_uuid() != aggregate.id().as_uuid()
        || context.partition != inspection_partition(aggregate.id())
    {
        return Err(CqrsSqlxError::GuardRejected(
            "Quality normalized command metadata mismatch",
        ));
    }
    Ok(())
}

#[async_trait]
impl WriteRepository for QualityWriteRepository {
    async fn load(
        &self,
        authority_scope: &AuthorityScope,
        inspection_id: InspectionId,
    ) -> Result<Option<QualityInspection>, PortError> {
        let authority = AuthoritySqlScope::try_from_scope(authority_scope)?;
        let mut transaction = self.pool.begin().await.map_err(map_store_error)?;
        let row = sqlx::query(
            "SELECT aggregate_version,reference,holding_id,company_id,branch_id,
                    inspection_kind,product_id,lot_reference,quantity,passed_quantity,
                    failed_quantity,inspection_status,disposition_note,created_at,updated_at
             FROM erpx_quality_write.inspection_authority
             WHERE holding_id=erpx_cqrs_write.current_holding_id() AND inspection_id=$1
               AND (
                 $2='platform'
                 OR ($2='holding' AND holding_id=$3)
                 OR ($2='company' AND holding_id=$3 AND company_id=$4)
                 OR ($2='branch' AND holding_id=$3 AND company_id=$4 AND branch_id=$5)
                 OR ($2='subtree' AND holding_id=$3 AND company_id=ANY($6)
                     AND ((branch_id IS NULL AND cardinality($7::uuid[])=0)
                          OR branch_id=ANY($7)))
               )",
        )
        .bind(inspection_id.as_uuid())
        .bind(authority.mode)
        .bind(authority.holding_id)
        .bind(authority.company_id)
        .bind(authority.branch_id)
        .bind(&authority.company_ids)
        .bind(&authority.branch_ids)
        .fetch_optional(&mut *transaction)
        .await
        .map_err(|_| PortError::Unavailable)?;
        transaction
            .commit()
            .await
            .map_err(|_| PortError::Unavailable)?;
        let Some(row) = row else {
            return Ok(None);
        };
        let holding_id: Uuid = row.try_get("holding_id").map_err(|_| PortError::Internal)?;
        let company_id: Uuid = row.try_get("company_id").map_err(|_| PortError::Internal)?;
        let branch_id: Option<Uuid> = row.try_get("branch_id").map_err(|_| PortError::Internal)?;
        let scope = match branch_id {
            Some(branch_id) => AuthorityScope::Branch {
                holding_id,
                company_id,
                branch_id,
            },
            None => AuthorityScope::Company {
                holding_id,
                company_id,
            },
        };
        let state = QualityInspectionState {
            id: inspection_id,
            version: from_i64(
                row.try_get("aggregate_version")
                    .map_err(|_| PortError::Internal)?,
            )?,
            reference: row.try_get("reference").map_err(|_| PortError::Internal)?,
            scope,
            kind: parse_kind(
                &row.try_get::<String, _>("inspection_kind")
                    .map_err(|_| PortError::Internal)?,
            )?,
            product_id: row.try_get("product_id").map_err(|_| PortError::Internal)?,
            lot_reference: row
                .try_get("lot_reference")
                .map_err(|_| PortError::Internal)?,
            quantity: from_i64(row.try_get("quantity").map_err(|_| PortError::Internal)?)?,
            passed_quantity: from_i64(
                row.try_get("passed_quantity")
                    .map_err(|_| PortError::Internal)?,
            )?,
            failed_quantity: from_i64(
                row.try_get("failed_quantity")
                    .map_err(|_| PortError::Internal)?,
            )?,
            status: parse_status(
                &row.try_get::<String, _>("inspection_status")
                    .map_err(|_| PortError::Internal)?,
            )?,
            disposition_note: row
                .try_get("disposition_note")
                .map_err(|_| PortError::Internal)?,
            created_at: row.try_get("created_at").map_err(|_| PortError::Internal)?,
            updated_at: row.try_get("updated_at").map_err(|_| PortError::Internal)?,
        };
        let aggregate = QualityInspection::try_rehydrate(state).map_err(|_| PortError::Internal)?;
        if !authority_scope.contains(aggregate.scope()) {
            return Err(PortError::Denied);
        }
        Ok(Some(aggregate))
    }
}

#[derive(Debug, Clone)]
pub struct QualityCommandAdapter {
    store: CommandStore,
}

impl QualityCommandAdapter {
    pub fn new(pool: WritePool) -> Self {
        Self {
            store: CommandStore::new(pool),
        }
    }
}

#[async_trait]
impl CommandUnitOfWork for QualityCommandAdapter {
    async fn find_receipt(
        &self,
        identity: &CommandIdentity,
    ) -> Result<Option<StoredCommit>, PortError> {
        let inspection_id = fingerprint_inspection_id(&identity.fingerprint);
        let receipt = self
            .store
            .find_receipt::<QualityInspection>(
                CONTEXT_NAME,
                &inspection_partition(inspection_id),
                &identity.idempotency_key,
                identity.command_id,
                request_hash(&identity.fingerprint)?,
            )
            .await
            .map_err(map_store_error)?;
        receipt.map(receipt_to_stored).transpose()
    }

    async fn commit(&self, request: QualityCommit) -> Result<StoredCommit, PortError> {
        validate_commit(&request)?;
        let inspection_id = request.aggregate.id();
        let partition = inspection_partition(inspection_id);
        let aggregate_id = aggregate_id(inspection_id)?;
        let occurred_at = request.aggregate.updated_at();
        let command_id = request.identity.command_id;
        let request_hash = request_hash(&request.identity.fingerprint)?;
        let audit_action = audit_action(&request.identity.fingerprint);
        let idempotency_key = request.identity.idempotency_key.clone();
        let mut events = request
            .integration_events
            .iter()
            .enumerate()
            .map(|(index, event)| OutboxEventWrite {
                event_id: deterministic_event_id(command_id, event.event_type(), index),
                event_type: event.event_type().into(),
                event_version: u32::from(event.schema_version()),
                partition: partition.clone(),
                causation_id: request.causation_id,
                occurred_at,
                payload: QualityOutboxPayload::Integration(event.clone()),
            })
            .collect::<Vec<_>>();

        // The snapshot is intentionally last: CommandReceipt::projection_target
        // must identify the event consumed by this context's read projector.
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(command_id, SNAPSHOT_EVENT_TYPE, events.len()),
            event_type: SNAPSHOT_EVENT_TYPE.into(),
            event_version: EVENT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: QualityOutboxPayload::Snapshot(QualitySnapshot {
                inspection: (&request.aggregate).into(),
            }),
        });
        let directory_draft = quality_directory_draft(&request.aggregate)?;
        events.push(OutboxEventWrite {
            event_id: deterministic_event_id(
                command_id,
                ITEM_DIRECTORY_OWNER_EVENT_TYPE,
                events.len(),
            ),
            event_type: ITEM_DIRECTORY_OWNER_EVENT_TYPE.into(),
            event_version: ITEM_DIRECTORY_FACT_VERSION,
            partition: partition.clone(),
            causation_id: request.causation_id,
            occurred_at,
            payload: QualityOutboxPayload::Directory(directory_draft),
        });

        let active_scope = request.aggregate.scope().clone();
        let aggregate = request.aggregate;
        let command = AtomicCommand {
            context: CONTEXT_NAME.into(),
            partition,
            command_id,
            correlation_id: request.correlation_id,
            idempotency_key,
            request_hash,
            active_scope,
            actor: request.actor,
            audit_context: request.audit,
            aggregate_type: AGGREGATE_TYPE.into(),
            aggregate_id,
            expected_version: request.expected_version,
            response: aggregate,
            audit: AuditWrite {
                audit_id: deterministic_uuid(command_id.as_uuid(), b"quality-audit-v1"),
                action: audit_action.into(),
                occurred_at,
                details: json!({
                    "inspection_id": inspection_id.as_uuid(),
                    "expected_version": request.expected_version,
                }),
            },
            events,
        };
        let guard = QualityInspectionGuard {
            aggregate: command.response.clone(),
        };
        let outcome = self
            .store
            .commit_normalized(command, &guard)
            .await
            .map_err(map_store_error)?;
        let replayed = matches!(outcome, AtomicCommandOutcome::Replayed(_));
        let receipt = match outcome {
            AtomicCommandOutcome::Committed(receipt) | AtomicCommandOutcome::Replayed(receipt) => {
                receipt
            }
        };
        let mut stored = receipt_to_stored(receipt)?;
        stored.replayed = replayed;
        Ok(stored)
    }
}

fn quality_directory_draft(
    aggregate: &QualityInspection,
) -> Result<ItemDescriptorDraftV2, PortError> {
    let record = aggregate.record_ref().map_err(|_| PortError::Internal)?;
    let value = ItemDescriptorDraft {
        authorization_policy: record_policy_id(&record).map_err(|_| PortError::Internal)?,
        record,
        scope: aggregate.scope().clone(),
        lifecycle_state: crate::projection::status_name(aggregate.status()).into(),
        classification: DataClassification::Confidential,
        descriptor_version: aggregate.version(),
        authorized_search_text: aggregate.reference().to_owned(),
    };
    ItemDescriptorDraftV2::try_new(value, aggregate.reference()).map_err(|_| PortError::Internal)
}

fn validate_commit(request: &QualityCommit) -> Result<(), PortError> {
    let next = request
        .expected_version
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(PortError::Internal)?;
    if request.aggregate.validate().is_err()
        || fingerprint_inspection_id(&request.identity.fingerprint) != request.aggregate.id()
        || !request.authority_scope.contains(request.aggregate.scope())
        || request.aggregate.version() != next
    {
        return Err(PortError::Internal);
    }
    Ok(())
}

const fn kind_name(value: InspectionKind) -> &'static str {
    match value {
        InspectionKind::Incoming => "incoming",
        InspectionKind::InProcess => "in_process",
        InspectionKind::Final => "final",
    }
}

fn parse_kind(value: &str) -> Result<InspectionKind, PortError> {
    match value {
        "incoming" => Ok(InspectionKind::Incoming),
        "in_process" => Ok(InspectionKind::InProcess),
        "final" => Ok(InspectionKind::Final),
        _ => Err(PortError::Internal),
    }
}

const fn status_name(value: InspectionStatus) -> &'static str {
    match value {
        InspectionStatus::Draft => "draft",
        InspectionStatus::InProgress => "in_progress",
        InspectionStatus::Passed => "passed",
        InspectionStatus::Failed => "failed",
        InspectionStatus::Released => "released",
        InspectionStatus::Cancelled => "cancelled",
    }
}

fn parse_status(value: &str) -> Result<InspectionStatus, PortError> {
    match value {
        "draft" => Ok(InspectionStatus::Draft),
        "in_progress" => Ok(InspectionStatus::InProgress),
        "passed" => Ok(InspectionStatus::Passed),
        "failed" => Ok(InspectionStatus::Failed),
        "released" => Ok(InspectionStatus::Released),
        "cancelled" => Ok(InspectionStatus::Cancelled),
        _ => Err(PortError::Internal),
    }
}

fn to_i64(value: u64) -> Result<i64, CqrsSqlxError> {
    i64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn from_i64(value: i64) -> Result<u64, PortError> {
    u64::try_from(value).map_err(|_| PortError::Internal)
}

fn from_i64_store(value: i64) -> Result<u64, CqrsSqlxError> {
    u64::try_from(value).map_err(|_| CqrsSqlxError::NumericRange)
}

fn aggregate_id(inspection_id: InspectionId) -> Result<AggregateId, PortError> {
    AggregateId::try_from_uuid(inspection_id.as_uuid()).map_err(|_| PortError::Internal)
}

fn fingerprint_inspection_id(fingerprint: &CommandFingerprint) -> InspectionId {
    match fingerprint {
        CommandFingerprint::Create { inspection_id, .. }
        | CommandFingerprint::Start { inspection_id }
        | CommandFingerprint::RecordResult { inspection_id, .. }
        | CommandFingerprint::Release { inspection_id }
        | CommandFingerprint::Cancel { inspection_id } => *inspection_id,
    }
}

fn request_hash(fingerprint: &CommandFingerprint) -> Result<RequestHash, PortError> {
    let bytes = serde_json::to_vec(fingerprint).map_err(|_| PortError::Internal)?;
    let mut digest = Sha256::new();
    digest.update(b"erpx.quality.command-fingerprint.v1\0");
    digest.update(bytes);
    Ok(RequestHash::new(digest.finalize().into()))
}

fn deterministic_event_id(command_id: CommandId, event_type: &str, index: usize) -> EventId {
    let mut input = Vec::with_capacity(event_type.len() + 16);
    input.extend_from_slice(event_type.as_bytes());
    input.extend_from_slice(&index.to_be_bytes());
    EventId::try_from_uuid(deterministic_uuid(command_id.as_uuid(), &input))
        .expect("a SHA-256-derived UUID is non-nil")
}

fn deterministic_uuid(seed: Uuid, label: &[u8]) -> Uuid {
    let mut digest = Sha256::new();
    digest.update(seed.as_bytes());
    digest.update(label);
    let digest = digest.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

fn audit_action(fingerprint: &CommandFingerprint) -> &'static str {
    match fingerprint {
        CommandFingerprint::Create { .. } => "quality.inspection.create",
        CommandFingerprint::Start { .. } => "quality.inspection.start",
        CommandFingerprint::RecordResult { .. } => "quality.inspection.record_result",
        CommandFingerprint::Release { .. } => "quality.inspection.release",
        CommandFingerprint::Cancel { .. } => "quality.inspection.cancel",
    }
}

fn receipt_to_stored(
    receipt: erpx_cqrs::CommandReceipt<QualityInspection>,
) -> Result<StoredCommit, PortError> {
    if receipt.aggregate.context != CONTEXT_NAME
        || receipt.aggregate.aggregate_type != AGGREGATE_TYPE
        || receipt.aggregate.aggregate_id.as_uuid() != receipt.response.id().as_uuid()
        || receipt.aggregate.version != receipt.response.version()
        || receipt.response.validate().is_err()
    {
        return Err(PortError::Internal);
    }
    Ok(StoredCommit {
        command_id: receipt.command_id,
        correlation_id: receipt.correlation_id,
        aggregate: receipt.response,
        committed_events: receipt.committed_events,
        committed_at: receipt.committed_at,
        replayed: receipt.replayed,
    })
}

fn map_store_error(error: CqrsSqlxError) -> PortError {
    match error {
        CqrsSqlxError::IdempotencyConflict | CqrsSqlxError::CommandReplayConflict => {
            PortError::IdempotencyConflict
        }
        CqrsSqlxError::OptimisticConcurrency {
            expected: Some(expected),
            actual,
            ..
        } => PortError::Conflict {
            expected,
            actual: actual.unwrap_or(0),
        },
        CqrsSqlxError::Database(_) => PortError::Unavailable,
        _ => PortError::Internal,
    }
}

#[cfg(test)]
mod tests {
    use erpx_quality::domain::InspectionKind;

    use super::*;

    fn inspection_id(value: u128) -> InspectionId {
        InspectionId::try_from_uuid(Uuid::from_u128(value)).unwrap()
    }

    fn fingerprint(quantity: u64) -> CommandFingerprint {
        CommandFingerprint::Create {
            inspection_id: inspection_id(1),
            scope: AuthorityScope::Company {
                holding_id: Uuid::from_u128(2),
                company_id: Uuid::from_u128(3),
            },
            kind: InspectionKind::Incoming,
            product_id: Uuid::from_u128(4),
            lot_reference: Some("LOT-1".into()),
            quantity,
        }
    }

    #[test]
    fn fingerprint_hash_is_stable_and_payload_sensitive() {
        let first = request_hash(&fingerprint(5)).unwrap();
        let retry = request_hash(&fingerprint(5)).unwrap();
        let changed = request_hash(&fingerprint(6)).unwrap();
        assert_eq!(first, retry);
        assert_ne!(first, changed);
        assert_eq!(first.as_bytes().len(), 32);
    }

    #[test]
    fn deterministic_ids_are_stable_and_namespaced() {
        let command = CommandId::try_from_uuid(Uuid::from_u128(9)).unwrap();
        let snapshot = deterministic_event_id(command, SNAPSHOT_EVENT_TYPE, 0);
        assert_eq!(
            snapshot,
            deterministic_event_id(command, SNAPSHOT_EVENT_TYPE, 0)
        );
        assert_ne!(
            snapshot,
            deterministic_event_id(command, "quality.inspection_completed", 0)
        );
    }
}
