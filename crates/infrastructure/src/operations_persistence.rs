use std::collections::BTreeMap;

use chrono::{DateTime, Duration, Utc};
use erpx_domain::access::Scope;
use erpx_domain::contracts::{
    ActorAttribution, ActorKind, CONTRACT_VERSION, EventEnvelope, TenantScope,
};
use erpx_domain::operations::completion::{
    OperationsAuthorizationProof, OperationsDetectionRequest,
    OperationsExpiredPrincipalReassignment, OperationsProjectionFreshnessState,
    OperationsQueueItemState, OperationsQueueState, OperationsSlaEscalationRequest,
    OperationsWorkflowAction,
};
use erpx_domain::operations::{
    AggregateMetadata, ExceptionCase, ExceptionLifecycleCommand, ExceptionStatus,
    MAX_OPERATIONS_POLICY_DOMAINS, OPERATIONS_CONTRACT_VERSION, OperationsAggregate,
    OperationsEvent, OperationsSourceDomain, OperationsSourceEvent, ProjectionEventMetadata,
    SourceTransactionRef, WorkQueuePolicy, event_for,
};
use erpx_migration::LegacySourceSnapshot;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{PersistenceError, provisioning::TENANT_SOURCE_REBUILD_LOCK_PREFIX};

pub const OPERATIONS_READ_PROJECTION: &str = "operations_read_model";
/// Durable high-water mark for the source-owned outbox feed consumed by the
/// Operations detection scheduler. This uses the existing rebuildable read
/// checkpoint table and is deliberately separate from the Operations aggregate
/// projection checkpoints.
pub const OPERATIONS_DETECTION_CURSOR_PROJECTION: &str = "operations_detection_cursor";
const OPERATIONS_DETECTION_CURSOR_AGGREGATE_TYPE: &str = "operations.source_outbox_cursor";
pub const MAX_OPERATIONS_EVENT_PAGE_SIZE: u32 = 500;
/// Maximum number of active exception cases returned by one read page.
pub const MAX_OPERATIONS_EXCEPTION_PAGE_SIZE: u32 = 100;
/// Maximum number of immutable source references returned by one reconciliation
/// page. The source payload itself remains in the owning bounded context.
pub const MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE: u32 = 100;
/// Maximum number of due Operations SLA notifications claimed by one worker
/// transaction.
pub const MAX_OPERATIONS_SLA_NOTIFICATION_PAGE_SIZE: u32 = 100;
pub const MIN_OPERATIONS_SLA_NOTIFICATION_LEASE_SECONDS: i64 = 30;
pub const MAX_OPERATIONS_SLA_NOTIFICATION_LEASE_SECONDS: i64 = 900;
pub const OPERATIONS_SLA_NOTIFICATION_RETENTION: Duration = Duration::days(7);
const OPERATIONS_NOTIFICATION_AGGREGATE_TYPE: &str = "operations_notification";
const OPERATIONS_NOTIFICATION_DELIVERY_EVENT_TYPE: &str =
    "operations.sla_notification_delivered.v1";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationsSlaNotificationKind {
    SlaEscalated,
}

impl OperationsSlaNotificationKind {
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::SlaEscalated => "sla_escalated",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsSlaNotification {
    pub notification_key: String,
    pub holding_ref: String,
    pub exception_id: Uuid,
    pub exception_version: u64,
    pub recipient_ref: String,
    pub kind: OperationsSlaNotificationKind,
    pub not_before: DateTime<Utc>,
    pub expires_at: DateTime<Utc>,
    pub payload: Value,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClaimedOperationsSlaNotification {
    pub notification: OperationsSlaNotification,
    pub claimed_by: Uuid,
    pub claim_token: Uuid,
    pub claim_expires_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
pub struct SqlxOperationsStore {
    write_pool: PgPool,
    read_pool: PgPool,
    holding_ref: String,
}

/// Compatibility name for callers that refer to the boundary as persistence.
pub type SqlxOperationsPersistence = SqlxOperationsStore;

#[derive(Debug, Clone, PartialEq)]
pub struct OperationsSnapshotRecord {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub holding_ref: String,
    pub scope: Value,
    pub aggregate_version: u64,
    pub lifecycle_status: String,
    pub state: Value,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsEventApplication {
    pub event_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

/// Receipt for one source fact applied to an Operations orchestration
/// aggregate. `event_id` is the source event ID by contract, making retries
/// deterministic without making Operations the source transaction owner.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsSourceEventApplication {
    pub source_event_id: Uuid,
    pub source_domain: OperationsSourceDomain,
    pub event_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsSourceProjectionApplication {
    pub ingestion: OperationsSourceEventApplication,
    pub projection: OperationsProjectionApplication,
}

/// Cursor for immutable source provenance. Recorded time and source event ID
/// provide a stable order even when source contexts publish at the same time.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationsSourceEventCursor {
    pub recorded_at: DateTime<Utc>,
    pub source_event_id: Uuid,
}

impl OperationsSourceEventCursor {
    pub fn new(
        recorded_at: DateTime<Utc>,
        source_event_id: Uuid,
    ) -> Result<Self, PersistenceError> {
        validate_uuid(source_event_id, "source event cursor event_id")?;
        Ok(Self {
            recorded_at,
            source_event_id,
        })
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsSourceReferenceRecord {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub holding_ref: String,
    pub source: SourceTransactionRef,
    pub operations_event_id: Option<Uuid>,
    pub recorded_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsSourceEventPage {
    pub references: Vec<OperationsSourceReferenceRecord>,
    pub next_cursor: Option<OperationsSourceEventCursor>,
    pub has_more: bool,
}

/// Cursor for the source-owned tenant outbox feed. Recorded time and event ID
/// form a total order without exposing SQL implementation details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationsSourceOutboxCursor {
    pub occurred_at: DateTime<Utc>,
    pub source_event_id: Uuid,
}

impl OperationsSourceOutboxCursor {
    pub fn new(
        occurred_at: DateTime<Utc>,
        source_event_id: Uuid,
    ) -> Result<Self, PersistenceError> {
        validate_uuid(source_event_id, "source outbox cursor event_id")?;
        Ok(Self {
            occurred_at,
            source_event_id,
        })
    }
}

fn operations_detection_cursor_id(holding_ref: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("erpx:operations:detection-cursor:{holding_ref}").as_bytes(),
    )
}

/// A validated source-owned outbox event prepared for Operations detection.
/// The payload remains owned by its bounded context; this adapter only
/// preserves the immutable envelope and provenance needed by Operations.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationsSourceOutboxEvent {
    pub source_event: OperationsSourceEvent,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationsSourceOutboxEventPage {
    pub events: Vec<OperationsSourceOutboxEvent>,
    pub next_cursor: Option<OperationsSourceOutboxCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsExceptionReconciliationApplication {
    pub source_event_id: Uuid,
    pub lifecycle: ExceptionLifecycleApplication,
}

/// Cursor for the tenant Operations event stream. The source stream has one
/// event per aggregate version, so aggregate ID and version are sufficient to
/// resume an ordered page without exposing database implementation details.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationsEventCursor {
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

impl OperationsEventCursor {
    pub fn new(aggregate_id: Uuid, aggregate_version: u64) -> Result<Self, PersistenceError> {
        validate_uuid(aggregate_id, "event cursor aggregate_id")?;
        Ok(Self {
            aggregate_id,
            aggregate_version,
        })
    }
}

/// A typed, transport-neutral event record read from the tenant write-side
/// Operations stream. Projection metadata is intentionally not synthesized
/// here; a worker chooses the projection it is rebuilding before applying the
/// record.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationsEventRecord {
    pub envelope: EventEnvelope,
    pub holding_ref: String,
    pub applied_at: DateTime<Utc>,
}

impl OperationsEventRecord {
    pub fn validate_for_holding(&self, expected_holding_ref: &str) -> Result<(), PersistenceError> {
        validate_holding_ref(expected_holding_ref)?;
        if self.holding_ref != expected_holding_ref {
            return Err(PersistenceError::InvalidData(
                "operations event holding reference does not match the routed tenant".into(),
            ));
        }
        Ok(())
    }

    pub fn cursor(&self) -> OperationsEventCursor {
        OperationsEventCursor {
            aggregate_id: self.envelope.aggregate_id,
            aggregate_version: self.envelope.aggregate_version,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationsEventPage {
    pub events: Vec<OperationsEventRecord>,
    pub next_cursor: Option<OperationsEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsProjectionRebuildPreparation {
    pub projection_name: String,
    pub holding_ref: String,
    pub read_rows_cleared: u64,
    pub checkpoints_cleared: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsProjectionApplication {
    pub projection_name: String,
    pub event_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsProjectionCheckpoint {
    pub projection_name: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub holding_ref: String,
    pub last_event_id: Option<Uuid>,
    pub last_aggregate_version: u64,
    pub projected_at: DateTime<Utc>,
}

/// Resume cursor for the active exception queue. The queue is ordered newest
/// first; the aggregate ID makes equal timestamps deterministic.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationsExceptionCursor {
    pub updated_at: DateTime<Utc>,
    pub aggregate_id: Uuid,
}

/// A bounded page of active Operations exception cases.
#[derive(Debug, Clone, PartialEq)]
pub struct OperationsExceptionPage {
    pub cases: Vec<OperationsReadRow<ExceptionCase>>,
    pub next_cursor: Option<OperationsExceptionCursor>,
    pub has_more: bool,
}

/// Result of one tenant-scoped ExceptionCase lifecycle command. A replay with
/// the same idempotency key returns `applied = false` and never mutates the
/// snapshot or emits another event.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExceptionLifecycleApplication {
    pub event_id: Uuid,
    pub exception_id: Uuid,
    pub from: ExceptionStatus,
    pub to: ExceptionStatus,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationsReadRow<A> {
    pub projection_name: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub holding_ref: String,
    pub aggregate_version: u64,
    pub lifecycle_status: String,
    pub value: A,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct OperationsJsonReadCursor {
    pub updated_at: DateTime<Utc>,
    pub aggregate_id: Uuid,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationsJsonReadRow {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub holding_ref: String,
    pub aggregate_version: u64,
    pub lifecycle_status: String,
    pub state: Value,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationsJsonReadPage {
    pub rows: Vec<OperationsJsonReadRow>,
    pub has_more: bool,
    pub next_cursor: Option<OperationsJsonReadCursor>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsIdempotencyRecord {
    pub event_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsWorkQueuePolicyPublishApplication {
    pub policy_id: Uuid,
    pub status: String,
    pub aggregate_version: u64,
    pub event_id: Uuid,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsReadSummary {
    pub read_row_count: u64,
    pub exception_count: u64,
    pub projection_checkpoint_count: u64,
    pub event_count: u64,
    pub source_reference_count: u64,
    pub source_domain_counts: BTreeMap<String, u64>,
    pub latest_event_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsDetectionApplication {
    pub ingestion: OperationsSourceProjectionApplication,
    pub assignment: ExceptionLifecycleApplication,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsReassignmentApplication {
    pub lifecycle: ExceptionLifecycleApplication,
    pub expired_principal_ref: String,
    pub replacement_principal_ref: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsSlaEscalationApplication {
    pub lifecycle: ExceptionLifecycleApplication,
    pub due_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OperationsReconciliationReport {
    pub holding_ref: String,
    pub events_observed: u64,
    pub aggregate_count: u64,
    pub read_row_count: u64,
    pub exception_count: u64,
    pub checkpoint_count: u64,
    pub outbox_event_count: u64,
    pub missing_outbox_events: u64,
    /// Outbox rows whose immutable envelope cannot be certified against the
    /// Operations event stream. This includes both an existing row whose
    /// envelope differs from its event and an orphan row with no authoritative
    /// Operations event. It also includes a delivered notification whose
    /// append-only downstream handoff is missing. Delivery state such as
    /// `published_at` is deliberately excluded because it is mutable transport
    /// state, not event identity.
    pub outbox_mismatch_events: u64,
    pub stale_aggregate_count: u64,
    pub last_event_at: Option<DateTime<Utc>>,
    pub last_projected_at: Option<DateTime<Utc>>,
    pub projection_lag_seconds: Option<i64>,
    pub healthy: bool,
}

#[derive(Debug, Default)]
struct OperationsProjectionVersionSet {
    event: Option<u64>,
    snapshot: Option<u64>,
    checkpoint: Option<u64>,
    read_row: Option<u64>,
}

impl OperationsProjectionVersionSet {
    fn is_aligned(&self) -> bool {
        self.event.is_some()
            && self.snapshot.is_some()
            && self.checkpoint.is_some()
            && self.read_row.is_some()
            && self.event == self.snapshot
            && self.snapshot == self.checkpoint
            && self.checkpoint == self.read_row
    }
}

/// The small, PII-free Legacy baseline handed off by the read-only Legacy
/// extractor. Operations never connects to or queries Legacy from this store;
/// it compares the immutable source snapshot evidence with its own
/// authoritative/read-model reconciliation instead.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LegacyOperationsBaseline {
    /// The holding this source snapshot was extracted for. `None` is retained
    /// for backwards-compatible construction of unscoped evidence, but the
    /// Operations completion worker must reject it before reconciliation.
    #[serde(default)]
    pub holding_ref: Option<String>,
    pub source_snapshot_ref: String,
    pub operations_exception_count: u64,
}

impl LegacyOperationsBaseline {
    pub fn new(
        source_snapshot_ref: impl Into<String>,
        operations_exception_count: u64,
    ) -> Result<Self, PersistenceError> {
        let baseline = Self {
            holding_ref: None,
            source_snapshot_ref: source_snapshot_ref.into(),
            operations_exception_count,
        };
        baseline.validate()?;
        Ok(baseline)
    }

    /// Bind source evidence to exactly one routed Operations holding. This is
    /// deliberately explicit: a global Legacy snapshot cannot be presented as
    /// holding-scoped parity evidence by omission.
    pub fn bind_to_holding(
        mut self,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        self.holding_ref = Some(holding_ref);
        self.validate()?;
        Ok(self)
    }

    /// Require the baseline to be bound to the exact holding being
    /// reconciled. This check is intentionally separate from `validate()` so
    /// existing persistence callers can still inspect legacy unscoped facts;
    /// completion/certification callers must use this fail-closed variant.
    pub fn validate_for_holding(&self, expected_holding_ref: &str) -> Result<(), PersistenceError> {
        validate_holding_ref(expected_holding_ref)?;
        self.validate()?;
        match self.holding_ref.as_deref() {
            Some(actual) if actual == expected_holding_ref => Ok(()),
            Some(_) => Err(PersistenceError::InvalidData(
                "Legacy Operations baseline holding does not match the routed holding".into(),
            )),
            None => Err(PersistenceError::InvalidData(
                "Legacy Operations baseline is global or unscoped".into(),
            )),
        }
    }

    /// Convert only the bounded Operations metric from a read-only Legacy
    /// source snapshot. Missing source evidence is a hard error, never an
    /// implicit zero that could make reconciliation appear healthy.
    pub fn from_legacy_snapshot(snapshot: &LegacySourceSnapshot) -> Result<Self, PersistenceError> {
        let operations_exception_count = snapshot
            .counts
            .get("operations_exceptions")
            .copied()
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "Legacy source snapshot lacks the operations_exceptions count".into(),
                )
            })?;
        Self::new(
            snapshot.source_contract_hash.clone(),
            operations_exception_count,
        )
    }

    /// Build holding-bound evidence from a read-only Legacy snapshot without
    /// allowing the caller to forget the routing identity.
    pub fn from_legacy_snapshot_for_holding(
        snapshot: &LegacySourceSnapshot,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        Self::from_legacy_snapshot(snapshot)?.bind_to_holding(holding_ref)
    }

    pub fn validate(&self) -> Result<(), PersistenceError> {
        validate_opaque_reference(&self.source_snapshot_ref, "source_snapshot_ref")?;
        if let Some(holding_ref) = &self.holding_ref {
            validate_holding_ref(holding_ref)?;
        }
        Ok(())
    }
}

/// Explicit Legacy/ERPX parity evidence for the Operations exception aggregate.
/// This is comparison evidence only; it does not grant command ownership or
/// mutate Legacy or any Operations write-side record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OperationsLegacyReconciliation {
    pub source_snapshot_ref: String,
    pub legacy_operations_exception_count: u64,
    pub erpx_operations_exception_count: u64,
    pub exception_count_delta: i64,
    pub operations_reconciliation_healthy: bool,
    pub exception_count_match: bool,
    pub healthy: bool,
}

impl OperationsReconciliationReport {
    pub fn reconcile_legacy_baseline(
        &self,
        baseline: &LegacyOperationsBaseline,
    ) -> Result<OperationsLegacyReconciliation, PersistenceError> {
        baseline.validate()?;
        if let Some(baseline_holding_ref) = &baseline.holding_ref
            && baseline_holding_ref != &self.holding_ref
        {
            return Err(PersistenceError::InvalidData(
                "Legacy Operations baseline holding does not match the reconciliation holding"
                    .into(),
            ));
        }
        let exception_count_delta =
            signed_count_delta(self.exception_count, baseline.operations_exception_count);
        let exception_count_match = exception_count_delta == 0;
        Ok(OperationsLegacyReconciliation {
            source_snapshot_ref: baseline.source_snapshot_ref.clone(),
            legacy_operations_exception_count: baseline.operations_exception_count,
            erpx_operations_exception_count: self.exception_count,
            exception_count_delta,
            operations_reconciliation_healthy: self.healthy,
            exception_count_match,
            healthy: self.healthy && exception_count_match,
        })
    }
}

impl SqlxOperationsStore {
    /// Build a server-side tenant store from already-routed pools. Pool URLs,
    /// database keys, and credentials are deliberately not part of this API.
    pub fn new(
        write_pool: PgPool,
        read_pool: PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        Ok(Self {
            write_pool,
            read_pool,
            holding_ref,
        })
    }

    pub fn from_pools(
        write_pool: &PgPool,
        read_pool: &PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        Self::new(write_pool.clone(), read_pool.clone(), holding_ref)
    }

    pub fn holding_ref(&self) -> &str {
        &self.holding_ref
    }

    pub async fn idempotency_record(
        &self,
        idempotency_key: &str,
    ) -> Result<Option<OperationsIdempotencyRecord>, PersistenceError> {
        validate_non_empty(idempotency_key, "idempotency_key")?;
        let mut tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT event_id, aggregate_type, aggregate_id, aggregate_version
             FROM erpx_tenant.operations_events
             WHERE idempotency_key = $1",
        )
        .bind(idempotency_key)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        row.map(|row| {
            Ok(OperationsIdempotencyRecord {
                event_id: row.try_get("event_id")?,
                aggregate_type: row.try_get("aggregate_type")?,
                aggregate_id: row.try_get("aggregate_id")?,
                aggregate_version: positive_u64(
                    row.try_get("aggregate_version")?,
                    "idempotency aggregate version",
                )?,
            })
        })
        .transpose()
    }

    pub async fn read_summary(&self) -> Result<OperationsReadSummary, PersistenceError> {
        let mut read_tx = self.begin_read().await?;
        let read_counts = sqlx::query(
            "SELECT
                count(*)::BIGINT AS read_row_count,
                count(*) FILTER (WHERE aggregate_type = $1)::BIGINT AS exception_count
             FROM erpx_tenant.operations_read_rows
             WHERE projection_name = $2 AND holding_ref = $3",
        )
        .bind(ExceptionCase::AGGREGATE_TYPE)
        .bind(OPERATIONS_READ_PROJECTION)
        .bind(&self.holding_ref)
        .fetch_one(&mut *read_tx)
        .await?;
        let checkpoint_count: i64 = sqlx::query_scalar(
            "SELECT count(*)::BIGINT
             FROM erpx_tenant.operations_projection_checkpoints
             WHERE projection_name = $1 AND holding_ref = $2",
        )
        .bind(OPERATIONS_READ_PROJECTION)
        .bind(&self.holding_ref)
        .fetch_one(&mut *read_tx)
        .await?;
        read_tx.commit().await?;

        let mut write_tx = self.begin_write().await?;
        let write_counts = sqlx::query(
            "SELECT count(*)::BIGINT AS event_count, max(occurred_at) AS latest_event_at
             FROM erpx_tenant.operations_events
             WHERE holding_ref = $1",
        )
        .bind(&self.holding_ref)
        .fetch_one(&mut *write_tx)
        .await?;
        let source_rows = sqlx::query(
            "SELECT source_domain, count(*)::BIGINT AS source_count
             FROM erpx_tenant.operations_source_references
             WHERE holding_ref = $1
             GROUP BY source_domain
             ORDER BY source_domain",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *write_tx)
        .await?;
        write_tx.commit().await?;

        let as_count = |value: i64, field: &'static str| {
            u64::try_from(value)
                .map_err(|_| PersistenceError::InvalidData(format!("{field} is invalid")))
        };
        let mut source_domain_counts = BTreeMap::new();
        for row in source_rows {
            source_domain_counts.insert(
                row.try_get("source_domain")?,
                as_count(row.try_get("source_count")?, "source count")?,
            );
        }
        Ok(OperationsReadSummary {
            read_row_count: as_count(read_counts.try_get("read_row_count")?, "read row count")?,
            exception_count: as_count(read_counts.try_get("exception_count")?, "exception count")?,
            projection_checkpoint_count: as_count(checkpoint_count, "checkpoint count")?,
            event_count: as_count(write_counts.try_get("event_count")?, "event count")?,
            source_reference_count: source_domain_counts.values().sum(),
            source_domain_counts,
            latest_event_at: write_counts.try_get("latest_event_at")?,
        })
    }

    /// Detect, assign, and project one source-owned operational exception.
    /// The source fact remains immutable provenance; the exception snapshot,
    /// Operations event, tenant outbox row, and initial read projection all
    /// use the existing typed seams. A retry can finish after an interrupted
    /// assignment because ingestion and lifecycle idempotency are independent
    /// but both are bound to the request/source identities.
    pub async fn detect_exception(
        &self,
        request: &OperationsDetectionRequest,
    ) -> Result<OperationsDetectionApplication, PersistenceError> {
        request
            .validate()
            .map_err(|error| invalid_domain("detection request", error))?;
        self.detect_exception_at(request, request.source_event.occurred_at)
            .await
    }

    /// Detect one source-owned exception during a scheduled sweep. Source
    /// occurrence time remains the immutable fact timestamp; `at` is the
    /// current worker time used for authorization, principal windows, and the
    /// assignment event. Both the source ingestion and assignment are
    /// idempotent, and each Operations event is written with its tenant
    /// outbox envelope before projection is advanced.
    pub async fn detect_exception_at(
        &self,
        request: &OperationsDetectionRequest,
        at: DateTime<Utc>,
    ) -> Result<OperationsDetectionApplication, PersistenceError> {
        request
            .validate_at(at)
            .map_err(|error| invalid_domain("scheduled detection request", error))?;
        let exception = request
            .exception_at(at)
            .map_err(|error| invalid_domain("scheduled detection exception", error))?;
        let ingestion = self
            .ingest_source_event_and_project(&request.source_event, &exception, None)
            .await?;
        let assignment = self
            .apply_exception_lifecycle_command(
                &request
                    .assignment_command_at(at)
                    .map_err(|error| invalid_domain("scheduled detection assignment", error))?,
            )
            .await?;
        Ok(OperationsDetectionApplication {
            ingestion,
            assignment,
        })
    }

    /// Reassign a live exception whose previous assignee has expired. The
    /// caller supplies a current role/manager proof from the shared
    /// authorization evaluator; this store only verifies the persisted case,
    /// scope, expected version, and immutable lifecycle transition.
    pub async fn reassign_expired_principal(
        &self,
        request: &OperationsExpiredPrincipalReassignment,
    ) -> Result<OperationsReassignmentApplication, PersistenceError> {
        let exception = self
            .get_aggregate::<ExceptionCase>(request.exception_id)
            .await?
            .ok_or(PersistenceError::NotFound {
                entity: "exception case",
                id: request.exception_id,
            })?;
        request
            .validate(&exception.meta.scope)
            .map_err(|error| invalid_domain("expired-principal reassignment", error))?;
        let lifecycle = self
            .apply_exception_lifecycle_command_with_assignee_guard(
                &request.command(),
                Some(&request.expired_principal_ref),
                None,
            )
            .await?;
        Ok(OperationsReassignmentApplication {
            lifecycle,
            expired_principal_ref: request.expired_principal_ref.clone(),
            replacement_principal_ref: request.replacement.principal_ref.clone(),
        })
    }

    /// Escalate a due exception using a current manager-chain proof. SLA age
    /// is evaluated against the write-side snapshot, never a stale UI row.
    pub async fn escalate_due_exception(
        &self,
        request: &OperationsSlaEscalationRequest,
    ) -> Result<OperationsSlaEscalationApplication, PersistenceError> {
        let exception = self
            .get_aggregate::<ExceptionCase>(request.exception_id)
            .await?
            .ok_or(PersistenceError::NotFound {
                entity: "exception case",
                id: request.exception_id,
            })?;
        request
            .validate(&exception)
            .map_err(|error| invalid_domain("SLA escalation", error))?;
        let due_at = exception.due_at.ok_or_else(|| {
            PersistenceError::InvalidData("exception has no persisted SLA due time".into())
        })?;
        if due_at > request.at {
            return Err(PersistenceError::InvalidData(
                "exception SLA is not due at the requested sweep time".into(),
            ));
        }
        let lifecycle = self
            .apply_exception_lifecycle_command_with_notification(
                &request.command(),
                Some(&request.manager.principal_ref),
            )
            .await?;
        Ok(OperationsSlaEscalationApplication { lifecycle, due_at })
    }

    /// Claim due SLA notifications for one routed holding. The claim and
    /// lease transition are atomic and expired claims are returned to the
    /// pending state before selection. Notification payloads are bounded
    /// metadata only; provider credentials and message bodies never leave the
    /// tenant write database.
    pub async fn claim_sla_notifications(
        &self,
        worker_id: Uuid,
        at: DateTime<Utc>,
        lease: Duration,
        page_size: u32,
    ) -> Result<Vec<ClaimedOperationsSlaNotification>, PersistenceError> {
        if worker_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "Operations notification worker identity is required".into(),
            ));
        }
        if !(MIN_OPERATIONS_SLA_NOTIFICATION_LEASE_SECONDS
            ..=MAX_OPERATIONS_SLA_NOTIFICATION_LEASE_SECONDS)
            .contains(&lease.num_seconds())
        {
            return Err(PersistenceError::InvalidData(
                "Operations notification lease is outside its bound".into(),
            ));
        }
        if page_size == 0 || page_size > MAX_OPERATIONS_SLA_NOTIFICATION_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "Operations notification page size is outside its bound".into(),
            ));
        }
        let mut tx = self.begin_write().await?;
        sqlx::query(
            "UPDATE erpx_tenant.operations_sla_notifications
             SET status = CASE WHEN expires_at <= $2 THEN 'expired' ELSE 'pending' END,
                 claimed_by = NULL, claim_token = NULL, claimed_at = NULL,
                 claim_expires_at = NULL, updated_at = $2
             WHERE holding_ref = $1 AND status = 'claimed'
               AND (claim_expires_at <= $2
                    OR expires_at <= $2)",
        )
        .bind(&self.holding_ref)
        .bind(at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "UPDATE erpx_tenant.operations_sla_notifications
             SET status = 'expired', updated_at = $2
             WHERE holding_ref = $1
               AND status = 'pending' AND expires_at <= $2",
        )
        .bind(&self.holding_ref)
        .bind(at)
        .execute(&mut *tx)
        .await?;

        let rows = sqlx::query(
            "SELECT notification_key, holding_ref, exception_id,
                    exception_version, recipient_ref, kind, not_before,
                    expires_at, payload
             FROM erpx_tenant.operations_sla_notifications
             WHERE holding_ref = $3
               AND status = 'pending'
               AND not_before <= $1
               AND expires_at > $1
             ORDER BY not_before, notification_key
             LIMIT $2
             FOR UPDATE SKIP LOCKED",
        )
        .bind(at)
        .bind(i64::from(page_size))
        .bind(&self.holding_ref)
        .fetch_all(&mut *tx)
        .await?;

        let mut claimed = Vec::with_capacity(rows.len());
        for row in rows {
            let notification_key: String = row.try_get("notification_key")?;
            let claim_token = Uuid::new_v4();
            let changed = sqlx::query(
                "UPDATE erpx_tenant.operations_sla_notifications
                 SET status = 'claimed', claimed_by = $3, claim_token = $4,
                     claimed_at = $5,
                     claim_expires_at = LEAST(
                         $5 + ($6::double precision * interval '1 second'),
                         expires_at
                     ),
                     updated_at = $5
                 WHERE notification_key = $1 AND holding_ref = $2
                   AND status = 'pending'
                   AND not_before <= $5 AND expires_at > $5
                 RETURNING claim_expires_at",
            )
            .bind(&notification_key)
            .bind(&self.holding_ref)
            .bind(worker_id)
            .bind(claim_token)
            .bind(at)
            .bind(lease.num_seconds() as f64)
            .fetch_optional(&mut *tx)
            .await?;
            let Some(changed) = changed else {
                continue;
            };
            let claim_expires_at: DateTime<Utc> = changed.try_get("claim_expires_at")?;
            claimed.push(ClaimedOperationsSlaNotification {
                notification: OperationsSlaNotification {
                    notification_key,
                    holding_ref: row.try_get("holding_ref")?,
                    exception_id: row.try_get("exception_id")?,
                    exception_version: positive_u64(
                        row.try_get("exception_version")?,
                        "Operations notification exception version",
                    )?,
                    recipient_ref: row.try_get("recipient_ref")?,
                    kind: operations_notification_kind(row.try_get("kind")?)?,
                    not_before: row.try_get("not_before")?,
                    expires_at: row.try_get("expires_at")?,
                    payload: row.try_get("payload")?,
                },
                claimed_by: worker_id,
                claim_token,
                claim_expires_at,
            });
        }
        tx.commit().await?;
        Ok(claimed)
    }

    pub async fn acknowledge_sla_notification(
        &self,
        claim: &ClaimedOperationsSlaNotification,
        delivered_at: DateTime<Utc>,
    ) -> Result<(), PersistenceError> {
        validate_operations_notification_claim(claim)?;
        let mut tx = self.begin_write().await?;
        let changed = sqlx::query(
            "UPDATE erpx_tenant.operations_sla_notifications
             SET status = 'delivered', delivered_at = $6, claimed_by = NULL,
                 claim_token = NULL, claimed_at = NULL, claim_expires_at = NULL,
                 updated_at = clock_timestamp()
             WHERE notification_key = $1 AND holding_ref = $2
               AND status = 'claimed' AND claimed_by = $3 AND claim_token = $4
               AND claim_expires_at > clock_timestamp()
               AND expires_at > clock_timestamp()
               AND $5 >= not_before
               AND $5 <= clock_timestamp()",
        )
        .bind(&claim.notification.notification_key)
        .bind(&claim.notification.holding_ref)
        .bind(claim.claimed_by)
        .bind(claim.claim_token)
        .bind(delivered_at)
        .bind(delivered_at)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(PersistenceError::ClaimLost {
                worker_id: claim.claimed_by,
                event_id: claim.notification.exception_id,
            });
        }
        tx.commit().await?;
        Ok(())
    }

    pub async fn release_sla_notification(
        &self,
        claim: &ClaimedOperationsSlaNotification,
    ) -> Result<(), PersistenceError> {
        validate_operations_notification_claim(claim)?;
        let mut tx = self.begin_write().await?;
        let changed = sqlx::query(
            "UPDATE erpx_tenant.operations_sla_notifications
             SET status = CASE WHEN expires_at <= clock_timestamp() THEN 'expired' ELSE 'pending' END,
                 claimed_by = NULL, claim_token = NULL, claimed_at = NULL,
                 claim_expires_at = NULL, updated_at = clock_timestamp()
             WHERE notification_key = $1 AND holding_ref = $2
               AND status = 'claimed' AND claimed_by = $3 AND claim_token = $4
               AND claim_expires_at > clock_timestamp()",
        )
        .bind(&claim.notification.notification_key)
        .bind(&claim.notification.holding_ref)
        .bind(claim.claimed_by)
        .bind(claim.claim_token)
        .execute(&mut *tx)
        .await?;
        if changed.rows_affected() != 1 {
            return Err(PersistenceError::ClaimLost {
                worker_id: claim.claimed_by,
                event_id: claim.notification.exception_id,
            });
        }
        tx.commit().await?;
        Ok(())
    }

    /// Append a safe delivery handoff event. The notification inbox remains
    /// the delivery state authority; this event is the durable downstream
    /// integration boundary and is idempotent by notification key.
    pub async fn append_sla_notification_delivery_event(
        &self,
        notification: &OperationsSlaNotification,
        actor_ref: &str,
    ) -> Result<Uuid, PersistenceError> {
        validate_operations_notification(notification)?;
        if actor_ref.trim().is_empty() || actor_ref.len() > 255 {
            return Err(PersistenceError::InvalidData(
                "Operations notification delivery actor is invalid".into(),
            ));
        }
        let event_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            format!(
                "operations-sla-notification-delivered:{}",
                notification.notification_key
            )
            .as_bytes(),
        );
        let correlation_id = Uuid::new_v5(
            &Uuid::NAMESPACE_URL,
            notification.notification_key.as_bytes(),
        );
        let payload = json!({
            "notification_key": notification.notification_key,
            "exception_id": notification.exception_id,
            "exception_version": notification.exception_version,
            "recipient_ref": notification.recipient_ref,
            "kind": notification.kind.as_str(),
        });
        let actor = ActorAttribution {
            principal_ref: actor_ref.to_owned(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let tenant_scope = serde_json::to_value(TenantScope::Platform)?;
        let actor_value = serde_json::to_value(&actor)?;
        let mut tx = self.begin_write().await?;
        let authoritative = sqlx::query(
            "SELECT holding_ref, exception_id, exception_version, recipient_ref,
                    kind, not_before, expires_at, status, payload
             FROM erpx_tenant.operations_sla_notifications
             WHERE notification_key = $1 AND holding_ref = $2
               AND (status = 'delivered'
                    OR (status = 'claimed' AND claim_expires_at > clock_timestamp()))
             FOR SHARE",
        )
        .bind(&notification.notification_key)
        .bind(&notification.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        let Some(authoritative) = authoritative else {
            return Err(PersistenceError::NotFound {
                entity: "Operations SLA notification",
                id: notification.exception_id,
            });
        };
        let status: String = authoritative.try_get("status")?;
        let matches = status == "claimed" || status == "delivered";
        let matches = matches
            && authoritative.try_get::<Uuid, _>("exception_id")? == notification.exception_id
            && positive_u64(
                authoritative.try_get("exception_version")?,
                "Operations notification exception version",
            )? == notification.exception_version
            && authoritative.try_get::<String, _>("recipient_ref")? == notification.recipient_ref
            && authoritative.try_get::<String, _>("kind")? == notification.kind.as_str()
            && authoritative.try_get::<DateTime<Utc>, _>("not_before")? == notification.not_before
            && authoritative.try_get::<DateTime<Utc>, _>("expires_at")? == notification.expires_at
            && authoritative.try_get::<Value, _>("payload")? == notification.payload;
        if !matches {
            return Err(PersistenceError::InvalidData(
                "Operations notification handoff does not match authoritative state".into(),
            ));
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.outbox_events
                (event_id, aggregate_type, aggregate_id, holding_ref,
                 aggregate_version, event_type, payload, occurred_at,
                 contract_version, event_version, tenant_scope, actor,
                 correlation_id, causation_id, idempotency_key)
             VALUES ($1, $2, $3, $4, $5,
                     $6, $7, $8, $9, $10, $11, $12, $13, NULL, $14)
             ON CONFLICT (event_id) DO NOTHING",
        )
        .bind(event_id)
        .bind(OPERATIONS_NOTIFICATION_AGGREGATE_TYPE)
        .bind(notification.exception_id)
        .bind(&notification.holding_ref)
        .bind(as_i64(
            notification.exception_version,
            "notification version",
        )?)
        .bind(OPERATIONS_NOTIFICATION_DELIVERY_EVENT_TYPE)
        .bind(&payload)
        .bind(notification.not_before)
        .bind(i32::from(OPERATIONS_CONTRACT_VERSION))
        .bind(1_i32)
        .bind(&tenant_scope)
        .bind(&actor_value)
        .bind(correlation_id)
        .bind(&notification.notification_key)
        .execute(&mut *tx)
        .await?;
        ensure_sla_notification_delivery_outbox_matches(
            &mut tx,
            event_id,
            notification,
            &payload,
            &tenant_scope,
            &actor_value,
            correlation_id,
        )
        .await?;
        tx.commit().await?;
        Ok(event_id)
    }

    /// Repair only missing tenant outbox envelopes from the immutable
    /// Operations event stream. This is append-only and idempotent; it never
    /// edits or deletes Operations events and can be replayed after a crash.
    pub async fn repair_missing_outbox_events(
        &self,
        authorization: &OperationsAuthorizationProof,
        requested_scope: &erpx_domain::access::Scope,
        at: DateTime<Utc>,
        limit: u32,
    ) -> Result<u64, PersistenceError> {
        authorization
            .validate_for(OperationsWorkflowAction::Reconcile, requested_scope, at)
            .map_err(|error| invalid_domain("outbox repair authorization", error))?;
        require_holding_scope(requested_scope, "outbox repair")?;
        let limit = bounded_source_event_page_size(limit)?;
        let mut tx = self.begin_write().await?;
        let result = sqlx::query(
            "INSERT INTO erpx_tenant.outbox_events
                (event_id, aggregate_type, aggregate_id, holding_ref,
                 aggregate_version, event_type, payload, occurred_at,
                 contract_version, event_version, tenant_scope, actor,
                 correlation_id, causation_id, idempotency_key)
             SELECT e.event_id, e.aggregate_type, e.aggregate_id, e.holding_ref,
                    e.aggregate_version, e.event_type, e.payload, e.occurred_at,
                    e.contract_version, e.event_version, e.tenant_scope, e.actor,
                    e.correlation_id, e.causation_id, e.idempotency_key
             FROM erpx_tenant.operations_events e
             LEFT JOIN erpx_tenant.outbox_events o ON o.event_id = e.event_id
             WHERE e.holding_ref = $1 AND o.event_id IS NULL
             ORDER BY e.occurred_at, e.event_id
             LIMIT $2
             ON CONFLICT (event_id) DO NOTHING",
        )
        .bind(&self.holding_ref)
        .bind(i64::from(limit))
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(result.rows_affected())
    }

    /// Rebuild a missing Operations SLA notification handoff from the durable
    /// delivered inbox row. This is an append-only, holding-scoped recovery
    /// action: it never marks a pending/claimed notification delivered and it
    /// never edits or deletes an existing outbox envelope. A retry observes
    /// the deterministic event identity and is therefore idempotent.
    pub async fn repair_missing_sla_notification_handoff_events(
        &self,
        authorization: &OperationsAuthorizationProof,
        requested_scope: &erpx_domain::access::Scope,
        at: DateTime<Utc>,
        actor_ref: &str,
        limit: u32,
    ) -> Result<u64, PersistenceError> {
        authorization
            .validate_for(OperationsWorkflowAction::Reconcile, requested_scope, at)
            .map_err(|error| invalid_domain("notification handoff repair authorization", error))?;
        require_holding_scope(requested_scope, "notification handoff repair")?;
        validate_non_empty(actor_ref, "notification handoff repair actor")?;
        if actor_ref.len() > 255 || actor_ref.chars().any(char::is_control) {
            return Err(PersistenceError::InvalidData(
                "notification handoff repair actor is invalid".into(),
            ));
        }
        let limit = bounded_source_event_page_size(limit)?;
        let mut tx = self.begin_write().await?;
        let rows = sqlx::query(
            "SELECT n.notification_key, n.holding_ref, n.exception_id,
                    n.exception_version, n.recipient_ref, n.kind,
                    n.not_before, n.expires_at, n.payload
             FROM erpx_tenant.operations_sla_notifications n
             WHERE n.holding_ref = $1 AND n.status = 'delivered'
               AND NOT EXISTS (
                    SELECT 1
                    FROM erpx_tenant.outbox_events o
                    WHERE o.holding_ref = n.holding_ref
                      AND o.event_type = $2
                      AND o.idempotency_key = n.notification_key
               )
             ORDER BY n.updated_at, n.notification_key
             LIMIT $3
             FOR UPDATE OF n SKIP LOCKED",
        )
        .bind(&self.holding_ref)
        .bind(OPERATIONS_NOTIFICATION_DELIVERY_EVENT_TYPE)
        .bind(i64::from(limit))
        .fetch_all(&mut *tx)
        .await?;

        let tenant_scope = serde_json::to_value(TenantScope::Platform)?;
        let actor = ActorAttribution {
            principal_ref: actor_ref.to_owned(),
            kind: ActorKind::SystemWorker,
            session_id: None,
            represented_principal_ref: None,
            support_capability_id: None,
        };
        let actor_value = serde_json::to_value(&actor)?;
        let mut repaired = 0_u64;
        for row in rows {
            let notification = OperationsSlaNotification {
                notification_key: row.try_get("notification_key")?,
                holding_ref: row.try_get("holding_ref")?,
                exception_id: row.try_get("exception_id")?,
                exception_version: positive_u64(
                    row.try_get("exception_version")?,
                    "notification exception version",
                )?,
                recipient_ref: row.try_get("recipient_ref")?,
                kind: operations_notification_kind(row.try_get("kind")?)?,
                not_before: row.try_get("not_before")?,
                expires_at: row.try_get("expires_at")?,
                payload: row.try_get("payload")?,
            };
            validate_operations_notification(&notification)?;
            let event_id = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                format!(
                    "operations-sla-notification-delivered:{}",
                    notification.notification_key
                )
                .as_bytes(),
            );
            let correlation_id = Uuid::new_v5(
                &Uuid::NAMESPACE_URL,
                notification.notification_key.as_bytes(),
            );
            let payload = json!({
                "notification_key": notification.notification_key,
                "exception_id": notification.exception_id,
                "exception_version": notification.exception_version,
                "recipient_ref": notification.recipient_ref,
                "kind": notification.kind.as_str(),
            });
            let inserted = sqlx::query(
                "INSERT INTO erpx_tenant.outbox_events
                    (event_id, aggregate_type, aggregate_id, holding_ref,
                     aggregate_version, event_type, payload, occurred_at,
                     contract_version, event_version, tenant_scope, actor,
                     correlation_id, causation_id, idempotency_key)
                 VALUES ($1, $2, $3, $4, $5,
                         $6, $7, $8, $9, $10, $11, $12, $13, NULL, $14)
                 ON CONFLICT (event_id) DO NOTHING",
            )
            .bind(event_id)
            .bind(OPERATIONS_NOTIFICATION_AGGREGATE_TYPE)
            .bind(notification.exception_id)
            .bind(&notification.holding_ref)
            .bind(as_i64(
                notification.exception_version,
                "notification version",
            )?)
            .bind(OPERATIONS_NOTIFICATION_DELIVERY_EVENT_TYPE)
            .bind(&payload)
            .bind(notification.not_before)
            .bind(i32::from(OPERATIONS_CONTRACT_VERSION))
            .bind(1_i32)
            .bind(&tenant_scope)
            .bind(&actor_value)
            .bind(correlation_id)
            .bind(&notification.notification_key)
            .execute(&mut *tx)
            .await?;
            ensure_sla_notification_delivery_outbox_matches(
                &mut tx,
                event_id,
                &notification,
                &payload,
                &tenant_scope,
                &actor_value,
                correlation_id,
            )
            .await?;
            repaired = repaired
                .checked_add(inserted.rows_affected())
                .ok_or_else(|| {
                    PersistenceError::InvalidData("notification repair count overflow".into())
                })?;
        }
        tx.commit().await?;
        Ok(repaired)
    }

    /// Compare the immutable write stream/outbox with the disposable read
    /// model and checkpoints. This is the authoritative G7 reconciliation
    /// report consumed by operations UI and the coordinator's certification
    /// evidence; a healthy report has no missing outbox rows or stale
    /// aggregate checkpoints and is fresh within the five-minute contract.
    pub async fn reconciliation_report(
        &self,
        authorization: &OperationsAuthorizationProof,
        requested_scope: &erpx_domain::access::Scope,
        at: DateTime<Utc>,
    ) -> Result<OperationsReconciliationReport, PersistenceError> {
        authorization
            .validate_for(OperationsWorkflowAction::Reconcile, requested_scope, at)
            .map_err(|error| invalid_domain("Operations reconciliation authorization", error))?;
        require_holding_scope(requested_scope, "Operations reconciliation")?;
        let mut write_tx = self.begin_write().await?;
        let write_counts = sqlx::query(
            "SELECT
                count(*)::BIGINT AS event_count,
                count(DISTINCT (aggregate_type, aggregate_id))::BIGINT AS aggregate_count,
                max(occurred_at) AS last_event_at
             FROM erpx_tenant.operations_events
             WHERE holding_ref = $1",
        )
        .bind(&self.holding_ref)
        .fetch_one(&mut *write_tx)
        .await?;
        let write_event_versions = sqlx::query(
            "SELECT aggregate_type, aggregate_id, max(aggregate_version) AS aggregate_version
             FROM erpx_tenant.operations_events
             WHERE holding_ref = $1
             GROUP BY aggregate_type, aggregate_id",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *write_tx)
        .await?;
        let outbox_count: i64 = sqlx::query_scalar(
            "SELECT count(*)::BIGINT FROM erpx_tenant.outbox_events WHERE holding_ref = $1",
        )
        .bind(&self.holding_ref)
        .fetch_one(&mut *write_tx)
        .await?;
        let missing_outbox: i64 = sqlx::query_scalar(
            "SELECT count(*)::BIGINT
             FROM erpx_tenant.operations_events e
             LEFT JOIN erpx_tenant.outbox_events o ON o.event_id = e.event_id
             WHERE e.holding_ref = $1 AND o.event_id IS NULL",
        )
        .bind(&self.holding_ref)
        .fetch_one(&mut *write_tx)
        .await?;
        // The Operations event is authoritative for every immutable outbox
        // envelope field. A row with the right event_id but altered payload,
        // routing, causation, or actor metadata must fail reconciliation just
        // as a missing row does; otherwise the worker could certify a corrupt
        // delivery envelope as healthy.
        let outbox_mismatches: i64 = sqlx::query_scalar(
            "SELECT count(*)::BIGINT
             FROM erpx_tenant.operations_events e
             JOIN erpx_tenant.outbox_events o ON o.event_id = e.event_id
             WHERE e.holding_ref = $1
               AND (
                    o.aggregate_type IS DISTINCT FROM e.aggregate_type
                 OR o.aggregate_id IS DISTINCT FROM e.aggregate_id
                 OR o.holding_ref IS DISTINCT FROM e.holding_ref
                 OR o.aggregate_version IS DISTINCT FROM e.aggregate_version
                 OR o.event_type IS DISTINCT FROM e.event_type
                 OR o.payload IS DISTINCT FROM e.payload
                 OR o.occurred_at IS DISTINCT FROM e.occurred_at
                 OR o.contract_version IS DISTINCT FROM e.contract_version
                 OR o.event_version IS DISTINCT FROM e.event_version
                 OR o.tenant_scope IS DISTINCT FROM e.tenant_scope
                 OR o.actor IS DISTINCT FROM e.actor
                 OR o.correlation_id IS DISTINCT FROM e.correlation_id
                 OR o.causation_id IS DISTINCT FROM e.causation_id
                 OR o.idempotency_key IS DISTINCT FROM e.idempotency_key
               )",
        )
        .bind(&self.holding_ref)
        .fetch_one(&mut *write_tx)
        .await?;
        // A rogue or partially committed outbox row is not made authoritative
        // merely because it has a valid row shape. It must have a matching
        // immutable Operations event in this holding; otherwise a consumer
        // could deliver an event that was never accepted by Operations while
        // reconciliation still reported the tenant healthy.
        let orphan_outbox_rows = sqlx::query(
            "SELECT o.event_id, o.aggregate_type, o.aggregate_id, o.holding_ref,
                    o.aggregate_version, o.event_type, o.payload, o.occurred_at,
                    o.contract_version, o.event_version, o.tenant_scope, o.actor,
                    o.correlation_id, o.causation_id, o.idempotency_key,
                    n.notification_key AS notification_key,
                    n.holding_ref AS notification_holding_ref,
                    n.exception_id AS notification_exception_id,
                    n.exception_version AS notification_exception_version,
                    n.recipient_ref AS notification_recipient_ref,
                    n.kind AS notification_kind,
                    n.not_before AS notification_not_before,
                    n.status AS notification_status
             FROM erpx_tenant.outbox_events o
             LEFT JOIN erpx_tenant.operations_events e ON e.event_id = o.event_id
             LEFT JOIN erpx_tenant.operations_sla_notifications n
               ON n.notification_key = o.idempotency_key
              AND n.holding_ref = o.holding_ref
             WHERE o.holding_ref = $1
               AND e.event_id IS NULL
               -- Upstream source outbox rows are intentionally not Operations
               -- aggregate events. They are certified by source-feed
               -- reconciliation, while this orphan check covers only
               -- Operations-owned envelopes and the notification handoff.
                   AND (o.aggregate_type LIKE 'operations.%'
                    OR o.aggregate_type LIKE 'organization.%'
                    OR o.aggregate_type = $2)",
        )
        .bind(&self.holding_ref)
        .bind(OPERATIONS_NOTIFICATION_AGGREGATE_TYPE)
        .fetch_all(&mut *write_tx)
        .await?;
        let orphan_outbox = orphan_outbox_rows
            .into_iter()
            .try_fold(0_i64, |count, row| {
                let valid_delivery = valid_sla_notification_delivery_handoff(&row)?;
                if valid_delivery {
                    Ok(count)
                } else {
                    count.checked_add(1).ok_or_else(|| {
                        PersistenceError::InvalidData("outbox mismatch count overflow".into())
                    })
                }
            })?;
        // A delivered inbox row is itself durable evidence that a provider
        // handoff was acknowledged. If its deterministic outbox envelope is
        // missing, the normal orphan query cannot see it; count it as drift so
        // the append-only repair path can restore the handoff before a pass is
        // certified healthy.
        let missing_notification_handoffs: i64 = sqlx::query_scalar(
            "SELECT count(*)::BIGINT
             FROM erpx_tenant.operations_sla_notifications n
             WHERE n.holding_ref = $1 AND n.status = 'delivered'
               AND NOT EXISTS (
                    SELECT 1
                    FROM erpx_tenant.outbox_events o
                    WHERE o.holding_ref = n.holding_ref
                      AND o.event_type = $2
                      AND o.idempotency_key = n.notification_key
               )",
        )
        .bind(&self.holding_ref)
        .bind(OPERATIONS_NOTIFICATION_DELIVERY_EVENT_TYPE)
        .fetch_one(&mut *write_tx)
        .await?;
        let write_versions = sqlx::query(
            "SELECT aggregate_type, aggregate_id, aggregate_version
             FROM erpx_tenant.operations_aggregate_snapshots
             WHERE holding_ref = $1",
        )
        .bind(&self.holding_ref)
        .fetch_all(&mut *write_tx)
        .await?;
        write_tx.commit().await?;

        let mut read_tx = self.begin_read().await?;
        let read_counts = sqlx::query(
            "SELECT
                (SELECT count(*)::BIGINT FROM erpx_tenant.operations_read_rows
                 WHERE projection_name = $1 AND holding_ref = $2) AS read_row_count,
                (SELECT count(*)::BIGINT FROM erpx_tenant.operations_read_rows
                 WHERE projection_name = $1 AND holding_ref = $2
                   AND aggregate_type = $3) AS exception_count,
                (SELECT count(*)::BIGINT FROM erpx_tenant.operations_projection_checkpoints
                 WHERE projection_name = $1 AND holding_ref = $2) AS checkpoint_count,
                (SELECT max(projected_at) FROM erpx_tenant.operations_projection_checkpoints
                 WHERE projection_name = $1 AND holding_ref = $2) AS last_projected_at",
        )
        .bind(OPERATIONS_READ_PROJECTION)
        .bind(&self.holding_ref)
        .bind(ExceptionCase::AGGREGATE_TYPE)
        .fetch_one(&mut *read_tx)
        .await?;
        let read_versions = sqlx::query(
            "SELECT aggregate_type, aggregate_id, last_aggregate_version
             FROM erpx_tenant.operations_projection_checkpoints
             WHERE projection_name = $1 AND holding_ref = $2",
        )
        .bind(OPERATIONS_READ_PROJECTION)
        .bind(&self.holding_ref)
        .fetch_all(&mut *read_tx)
        .await?;
        let read_row_versions = sqlx::query(
            "SELECT aggregate_type, aggregate_id, aggregate_version
             FROM erpx_tenant.operations_read_rows
             WHERE projection_name = $1 AND holding_ref = $2",
        )
        .bind(OPERATIONS_READ_PROJECTION)
        .bind(&self.holding_ref)
        .fetch_all(&mut *read_tx)
        .await?;
        read_tx.commit().await?;

        // Reconciliation must compare the complete projection key set, not
        // only rows that happen to join. A deleted row replaced by a rogue
        // row could otherwise preserve the two counts while the write-side
        // aggregate still looked fully projected. The write snapshot,
        // checkpoint, and read row are one logical key and must all exist at
        // the same version before the projection can be certified healthy.
        let mut projection_versions: BTreeMap<(String, Uuid), OperationsProjectionVersionSet> =
            BTreeMap::new();
        for row in write_event_versions {
            let key = (
                row.try_get::<String, _>("aggregate_type")?,
                row.try_get::<Uuid, _>("aggregate_id")?,
            );
            projection_versions.entry(key).or_default().event = Some(positive_u64(
                row.try_get("aggregate_version")?,
                "event version",
            )?);
        }
        for row in write_versions {
            let key = (
                row.try_get::<String, _>("aggregate_type")?,
                row.try_get::<Uuid, _>("aggregate_id")?,
            );
            projection_versions.entry(key).or_default().snapshot = Some(positive_u64(
                row.try_get("aggregate_version")?,
                "snapshot version",
            )?);
        }
        for row in read_versions {
            let key = (
                row.try_get::<String, _>("aggregate_type")?,
                row.try_get::<Uuid, _>("aggregate_id")?,
            );
            projection_versions.entry(key).or_default().checkpoint = Some(positive_u64(
                row.try_get("last_aggregate_version")?,
                "checkpoint version",
            )?);
        }
        for row in read_row_versions {
            let key = (
                row.try_get::<String, _>("aggregate_type")?,
                row.try_get::<Uuid, _>("aggregate_id")?,
            );
            projection_versions.entry(key).or_default().read_row = Some(positive_u64(
                row.try_get("aggregate_version")?,
                "read row version",
            )?);
        }
        let stale = u64::try_from(
            projection_versions
                .values()
                .filter(|versions| !versions.is_aligned())
                .count(),
        )
        .map_err(|_| {
            PersistenceError::InvalidData("projection mismatch count is invalid".into())
        })?;

        let events_observed = positive_count(
            write_counts.try_get("event_count")?,
            "Operations event count",
        )?;
        let aggregate_count = positive_count(
            write_counts.try_get("aggregate_count")?,
            "Operations aggregate count",
        )?;
        let read_row_count = positive_count(
            read_counts.try_get("read_row_count")?,
            "Operations read row count",
        )?;
        let exception_count = positive_count(
            read_counts.try_get("exception_count")?,
            "Operations exception count",
        )?;
        let checkpoint_count = positive_count(
            read_counts.try_get("checkpoint_count")?,
            "Operations checkpoint count",
        )?;
        let outbox_event_count = positive_count(outbox_count, "Operations outbox count")?;
        let missing_outbox_events = positive_count(missing_outbox, "missing outbox count")?;
        let outbox_mismatch_events = positive_count(
            outbox_mismatches
                .checked_add(orphan_outbox)
                .and_then(|count| count.checked_add(missing_notification_handoffs))
                .ok_or_else(|| {
                    PersistenceError::InvalidData("outbox mismatch count overflow".into())
                })?,
            "outbox mismatch count",
        )?;
        let last_event_at: Option<DateTime<Utc>> = write_counts.try_get("last_event_at")?;
        let last_projected_at: Option<DateTime<Utc>> = read_counts.try_get("last_projected_at")?;
        let freshness =
            OperationsProjectionFreshnessState::at(last_event_at, last_projected_at, at);
        let healthy = missing_outbox_events == 0
            && outbox_mismatch_events == 0
            && stale == 0
            && read_row_count == checkpoint_count
            && freshness.healthy;
        Ok(OperationsReconciliationReport {
            holding_ref: self.holding_ref.clone(),
            events_observed,
            aggregate_count,
            read_row_count,
            exception_count,
            checkpoint_count,
            outbox_event_count,
            missing_outbox_events,
            outbox_mismatch_events,
            stale_aggregate_count: stale,
            last_event_at,
            last_projected_at,
            projection_lag_seconds: freshness.lag_seconds,
            healthy,
        })
    }

    /// Build the bounded state consumed by the Operations queue UI/API. The
    /// values are read-model snapshots only; all mutations still go through
    /// the write-side lifecycle commands above.
    pub async fn queue_state(
        &self,
        authorization: &OperationsAuthorizationProof,
        requested_scope: &erpx_domain::access::Scope,
        at: DateTime<Utc>,
        cursor: Option<OperationsExceptionCursor>,
        page_size: u32,
    ) -> Result<OperationsQueueState, PersistenceError> {
        authorization
            .validate_for(OperationsWorkflowAction::ReadQueue, requested_scope, at)
            .map_err(|error| invalid_domain("Operations queue authorization", error))?;
        require_holding_scope(requested_scope, "Operations queue")?;
        let page = self
            .read_active_exception_page(OPERATIONS_READ_PROJECTION, cursor, page_size)
            .await?;
        let summary = self.read_summary().await?;
        let mut read_tx = self.begin_read().await?;
        let last_projected_at: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT max(projected_at)
             FROM erpx_tenant.operations_projection_checkpoints
             WHERE projection_name = $1 AND holding_ref = $2",
        )
        .bind(OPERATIONS_READ_PROJECTION)
        .bind(&self.holding_ref)
        .fetch_one(&mut *read_tx)
        .await?;
        read_tx.commit().await?;
        let freshness =
            OperationsProjectionFreshnessState::at(summary.latest_event_at, last_projected_at, at);
        let items = page
            .cases
            .into_iter()
            .map(|row| OperationsQueueItemState {
                exception_id: row.aggregate_id,
                version: row.aggregate_version,
                status: row.lifecycle_status,
                category: row.value.category,
                severity: row.value.severity,
                assigned_to: row.value.assigned_to,
                manager_ref: row.value.manager_ref,
                due_at: row.value.due_at,
                updated_at: row.updated_at,
                source: row.value.source,
            })
            .collect();
        let next_cursor = page.next_cursor.map(|next| {
            format!(
                "{}:{}",
                next.updated_at
                    .to_rfc3339_opts(chrono::SecondsFormat::Nanos, true),
                next.aggregate_id
            )
        });
        let state = OperationsQueueState {
            holding_ref: self.holding_ref.clone(),
            projection_name: OPERATIONS_READ_PROJECTION.into(),
            as_of: at,
            freshness,
            items,
            next_cursor,
            has_more: page.has_more,
        };
        state
            .validate()
            .map_err(|error| invalid_domain("Operations queue state", error))?;
        Ok(state)
    }

    /// Reject the legacy snapshot-only write shape.
    ///
    /// Operations state is authoritative only when an immutable event, its
    /// tenant outbox envelope, and the aggregate snapshot are committed in one
    /// transaction. Keeping this compatibility method as a fail-closed
    /// boundary prevents older callers from silently creating a snapshot that
    /// cannot be projected, reconciled, or replayed. Callers must construct an
    /// `OperationsEvent` and use `apply_event` (or one of the typed command
    /// methods) instead.
    pub async fn save_aggregate<A: OperationsAggregate>(
        &self,
        aggregate: &A,
        expected_version: Option<u64>,
    ) -> Result<u64, PersistenceError> {
        self.save_aggregate_with_source_refs(
            aggregate,
            expected_version,
            aggregate.source_references(),
        )
        .await
    }

    /// Compatibility entry point for ExceptionCase that is deliberately
    /// fail-closed. Exception lifecycle state must enter through an
    /// authoritative event/outbox transaction, never a snapshot-only write.
    pub async fn save_exception_case(
        &self,
        exception: &ExceptionCase,
        expected_version: Option<u64>,
        source_refs: &[SourceTransactionRef],
    ) -> Result<u64, PersistenceError> {
        self.save_aggregate_with_source_refs(exception, expected_version, source_refs)
            .await
    }

    /// Publish one tenant-scoped work-queue policy through the same immutable
    /// Operations event, outbox, snapshot, and read-projection path used by
    /// every other aggregate. The server is responsible for authorization;
    /// this adapter remains responsible for optimistic versioning,
    /// idempotency, scope binding, and durable projection advancement.
    #[allow(clippy::too_many_arguments)]
    pub async fn publish_work_queue_policy(
        &self,
        policy_id: Uuid,
        scope: Scope,
        name: impl Into<String>,
        allowed_domains: Vec<String>,
        escalation_after_minutes: u32,
        expected_version: u64,
        actor: ActorAttribution,
        idempotency_key: String,
        correlation_id: Uuid,
        causation_id: Option<Uuid>,
        at: DateTime<Utc>,
    ) -> Result<OperationsWorkQueuePolicyPublishApplication, PersistenceError> {
        validate_uuid(policy_id, "work queue policy id")?;
        validate_non_empty(&idempotency_key, "work queue policy idempotency key")?;
        validate_uuid(correlation_id, "work queue policy correlation id")?;
        validate_work_queue_policy_domains(&allowed_domains)?;
        let expected = (expected_version != 0).then_some(expected_version);

        if let Some(record) = self.idempotency_record(&idempotency_key).await? {
            if record.aggregate_type != WorkQueuePolicy::AGGREGATE_TYPE
                || record.aggregate_id != policy_id
            {
                return Err(PersistenceError::IdempotencyConflict {
                    key: idempotency_key,
                });
            }
            return Ok(OperationsWorkQueuePolicyPublishApplication {
                policy_id,
                status: "active".into(),
                aggregate_version: record.aggregate_version,
                event_id: record.event_id,
                applied: false,
            });
        }

        let policy =
            if let Some(mut current) = self.get_aggregate::<WorkQueuePolicy>(policy_id).await? {
                if current.meta.scope != scope {
                    return Err(PersistenceError::InvalidData(
                        "work queue policy scope does not match the requested scope".into(),
                    ));
                }
                current.name = name.into();
                current.allowed_domains = allowed_domains;
                current.escalation_after_minutes = escalation_after_minutes;
                current.status = erpx_domain::operations::WorkQueuePolicyStatus::Active;
                current.meta.version = current.meta.version.checked_add(1).ok_or_else(|| {
                    PersistenceError::InvalidData("policy version overflow".into())
                })?;
                current.meta.updated_at = at;
                current
            } else {
                let mut metadata =
                    AggregateMetadata::new(policy_id, self.holding_ref.clone(), scope, at)
                        .map_err(|error| invalid_domain("work queue policy metadata", error))?;
                metadata.version = 1;
                WorkQueuePolicy {
                    meta: metadata,
                    name: name.into(),
                    allowed_domains,
                    escalation_after_minutes,
                    source_refs: Vec::new(),
                    status: erpx_domain::operations::WorkQueuePolicyStatus::Active,
                }
            };

        let event = event_for(
            &policy,
            "operations.work_queue_policy.published",
            1,
            actor,
            Some(idempotency_key),
            correlation_id,
            causation_id,
            at,
        )
        .map_err(|error| invalid_domain("work queue policy event", error))?;
        let application = self
            .apply_event(&event, &policy, expected, policy.source_references())
            .await?;
        self.project_event(&event, &policy).await?;
        Ok(OperationsWorkQueuePolicyPublishApplication {
            policy_id: application.aggregate_id,
            status: policy.lifecycle_status().into(),
            aggregate_version: application.aggregate_version,
            event_id: application.event_id,
            applied: application.applied,
        })
    }

    /// Ingest one source-owned fact and apply the corresponding Operations
    /// orchestration event atomically with the aggregate snapshot and source
    /// provenance. The source event ID is the immutable provenance and
    /// causation identity; the Operations application event gets its own
    /// deterministic identity so a source-owned outbox row cannot collide
    /// with the Operations outbox row. No source transaction or source
    /// payload is inserted into Operations.
    pub async fn ingest_source_event<A: OperationsAggregate>(
        &self,
        source_event: &OperationsSourceEvent,
        aggregate: &A,
        expected_version: Option<u64>,
    ) -> Result<OperationsSourceEventApplication, PersistenceError> {
        self.ensure_authoritative_source_event(source_event).await?;
        let event = source_event_for_aggregate(source_event, aggregate, &self.holding_ref)?;
        let application = self
            .apply_event(
                &event,
                aggregate,
                expected_version,
                aggregate.source_references(),
            )
            .await?;
        Ok(OperationsSourceEventApplication {
            source_event_id: source_event.source_event_id,
            source_domain: source_event.source_domain,
            event_id: application.event_id,
            aggregate_type: application.aggregate_type,
            aggregate_id: application.aggregate_id,
            aggregate_version: application.aggregate_version,
            applied: application.applied,
        })
    }

    /// Convenience boundary for the worker: the write-side source-event
    /// application and read-side checkpoint advancement are separate physical
    /// transactions, but both are typed and returned together. A missing or
    /// stale projection is therefore observable and cannot be mistaken for a
    /// successful source ingestion.
    pub async fn ingest_source_event_and_project<A: OperationsAggregate>(
        &self,
        source_event: &OperationsSourceEvent,
        aggregate: &A,
        expected_version: Option<u64>,
    ) -> Result<OperationsSourceProjectionApplication, PersistenceError> {
        self.ensure_authoritative_source_event(source_event).await?;
        let event = source_event_for_aggregate(source_event, aggregate, &self.holding_ref)?;
        let ingestion = self
            .apply_event(
                &event,
                aggregate,
                expected_version,
                aggregate.source_references(),
            )
            .await?;
        let projection = self.project_event(&event, aggregate).await?;
        Ok(OperationsSourceProjectionApplication {
            ingestion: OperationsSourceEventApplication {
                source_event_id: source_event.source_event_id,
                source_domain: source_event.source_domain,
                event_id: ingestion.event_id,
                aggregate_type: ingestion.aggregate_type,
                aggregate_id: ingestion.aggregate_id,
                aggregate_version: ingestion.aggregate_version,
                applied: ingestion.applied,
            },
            projection,
        })
    }

    /// Create a detected ExceptionCase from a source fact and apply it through
    /// the same source-event boundary as every other Operations aggregate.
    /// Exception creation is orchestration state; the source context remains
    /// authoritative for the triggering transaction.
    pub async fn create_exception_from_source_event(
        &self,
        source_event: &OperationsSourceEvent,
        exception_id: Uuid,
        category: impl Into<String>,
        severity: impl Into<String>,
    ) -> Result<OperationsSourceEventApplication, PersistenceError> {
        let exception =
            ExceptionCase::from_source_event(source_event, exception_id, category, severity)
                .map_err(|error| invalid_domain("source exception", error))?;
        self.ingest_source_event(source_event, &exception, None)
            .await
    }

    /// Verify that a presented source fact is the complete envelope already
    /// committed by its owning bounded context. Operations never accepts a
    /// caller-created source event as authority: the source outbox row is the
    /// immutable fact, while Operations stores only its provenance and
    /// orchestration consequences.
    async fn ensure_authoritative_source_event(
        &self,
        source_event: &OperationsSourceEvent,
    ) -> Result<(), PersistenceError> {
        source_event
            .validate()
            .map_err(|error| invalid_domain("source event", error))?;
        if source_event.holding_ref != self.holding_ref {
            return Err(PersistenceError::InvalidData(
                "source event holding reference does not match the routed tenant".into(),
            ));
        }
        let mut tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT event_id, aggregate_type, aggregate_id, holding_ref,
                    aggregate_version, event_type, contract_version,
                    event_version, tenant_scope, actor, correlation_id,
                    causation_id, payload, occurred_at
             FROM erpx_tenant.outbox_events
             WHERE event_id = $1 AND holding_ref = $2
               AND event_type NOT LIKE 'operations.%'
               AND event_type NOT LIKE 'organization.%'
             FOR SHARE",
        )
        .bind(source_event.source_event_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or_else(|| {
            PersistenceError::InvalidData(
                "source event is not present in the authoritative source outbox".into(),
            )
        })?;
        let authoritative = source_outbox_event_from_row(row, &self.holding_ref)?.source_event;
        if !source_events_match_authority(source_event, &authoritative) {
            return Err(PersistenceError::InvalidData(
                "source event does not match the authoritative source outbox envelope".into(),
            ));
        }
        tx.commit().await?;
        Ok(())
    }

    /// Reconcile an existing exception from a source fact. The command actor,
    /// assignment windows, and expected version are still supplied by the
    /// authoritative Operations/RBAC boundary; only source causation and
    /// idempotency are bound here.
    pub async fn reconcile_exception_from_source_event(
        &self,
        source_event: &OperationsSourceEvent,
        command: &ExceptionLifecycleCommand,
    ) -> Result<OperationsExceptionReconciliationApplication, PersistenceError> {
        source_event
            .validate()
            .map_err(|error| invalid_domain("source event", error))?;
        let exception = self
            .get_aggregate::<ExceptionCase>(command.exception_id)
            .await?
            .ok_or(PersistenceError::NotFound {
                entity: "exception case",
                id: command.exception_id,
            })?;
        exception
            .validate_source_event_link(source_event)
            .map_err(|error| invalid_domain("exception source reconciliation", error))?;
        let bound_command = command
            .bound_to_source_event(source_event)
            .map_err(|error| invalid_domain("exception source reconciliation command", error))?;
        let lifecycle = self
            .apply_exception_lifecycle_command(&bound_command)
            .await?;
        Ok(OperationsExceptionReconciliationApplication {
            source_event_id: source_event.source_event_id,
            lifecycle,
        })
    }

    /// Apply one validated ExceptionCase lifecycle command atomically on the
    /// tenant write database. The aggregate row is locked before idempotency
    /// lookup so concurrent retries cannot race into duplicate transitions.
    /// This helper owns orchestration state only; source transaction records
    /// remain immutable references on the ExceptionCase.
    pub async fn apply_exception_lifecycle_command(
        &self,
        command: &ExceptionLifecycleCommand,
    ) -> Result<ExceptionLifecycleApplication, PersistenceError> {
        self.apply_exception_lifecycle_command_with_notification(command, None)
            .await
    }

    async fn apply_exception_lifecycle_command_with_notification(
        &self,
        command: &ExceptionLifecycleCommand,
        notification_recipient: Option<&str>,
    ) -> Result<ExceptionLifecycleApplication, PersistenceError> {
        self.apply_exception_lifecycle_command_with_assignee_guard(
            command,
            None,
            notification_recipient,
        )
        .await
    }

    async fn apply_exception_lifecycle_command_with_assignee_guard(
        &self,
        command: &ExceptionLifecycleCommand,
        expected_assignee: Option<&str>,
        notification_recipient: Option<&str>,
    ) -> Result<ExceptionLifecycleApplication, PersistenceError> {
        command
            .validate()
            .map_err(|error| invalid_domain("exception lifecycle command", error))?;
        let mut tx = self.begin_write().await?;
        let snapshot = sqlx::query(
            "SELECT aggregate_id, holding_ref, scope, aggregate_version,
                    lifecycle_status, state, created_at, updated_at
             FROM erpx_tenant.operations_aggregate_snapshots
             WHERE aggregate_type = $1 AND aggregate_id = $2
             FOR UPDATE",
        )
        .bind(ExceptionCase::AGGREGATE_TYPE)
        .bind(command.exception_id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "exception case",
            id: command.exception_id,
        })?;
        let current_version = positive_u64(
            snapshot.try_get("aggregate_version")?,
            "exception aggregate version",
        )?;
        let snapshot_id: Uuid = snapshot.try_get("aggregate_id")?;
        let state: Value = snapshot.try_get("state")?;
        let mut exception: ExceptionCase = serde_json::from_value(state)?;
        let metadata = validate_aggregate(&exception, &self.holding_ref)?;
        if metadata.id != snapshot_id
            || metadata.version != current_version
            || snapshot.try_get::<String, _>("holding_ref")? != self.holding_ref
        {
            return Err(PersistenceError::InvalidData(
                "exception snapshot metadata does not match its routed key".into(),
            ));
        }

        if let Some(row) = sqlx::query(
            "SELECT event_id, aggregate_type, aggregate_id, aggregate_version
             FROM erpx_tenant.operations_events
             WHERE idempotency_key = $1",
        )
        .bind(&command.idempotency_key)
        .fetch_optional(&mut *tx)
        .await?
        {
            let aggregate_type: String = row.try_get("aggregate_type")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            if aggregate_type != ExceptionCase::AGGREGATE_TYPE
                || aggregate_id != command.exception_id
            {
                return Err(PersistenceError::IdempotencyConflict {
                    key: command.idempotency_key.clone(),
                });
            }
            let aggregate_version = positive_u64(
                row.try_get("aggregate_version")?,
                "exception event aggregate version",
            )?;
            let event_id: Uuid = row.try_get("event_id")?;
            append_existing_operations_event_outbox(&mut tx, event_id, &self.holding_ref).await?;
            tx.commit().await?;
            return Ok(ExceptionLifecycleApplication {
                event_id,
                exception_id: command.exception_id,
                from: exception.status,
                to: exception.status,
                aggregate_version,
                applied: false,
            });
        }

        if let Some(expected_assignee) = expected_assignee
            && exception.assigned_to.as_deref() != Some(expected_assignee)
        {
            return Err(PersistenceError::InvalidData(
                "exception is not currently assigned to the expired principal".into(),
            ));
        }

        if current_version != command.expected_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: ExceptionCase::AGGREGATE_TYPE.into(),
                aggregate_id: command.exception_id,
                expected: Some(command.expected_version),
                actual: Some(current_version),
            });
        }
        let transition = exception
            .apply_lifecycle_command(command)
            .map_err(|error| map_exception_command_error(error, command, current_version))?;
        let event = event_for(
            &exception,
            command.action.event_type(),
            1,
            command.actor_attribution.clone(),
            Some(command.idempotency_key.clone()),
            command.correlation_id,
            command.causation_id,
            command.at,
        )
        .map_err(|error| invalid_domain("exception lifecycle event", error))?;
        let state = normalized_state(&exception, transition.aggregate_version)?;
        sqlx::query(
            "INSERT INTO erpx_tenant.operations_events
                (event_id, contract_version, event_type, event_version, occurred_at,
                 aggregate_type, aggregate_id, aggregate_version, holding_ref,
                 tenant_scope, actor, correlation_id, causation_id, idempotency_key,
                 payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(event.envelope.event_id)
        .bind(i32::from(event.envelope.contract_version))
        .bind(&event.envelope.event_type)
        .bind(i32::from(event.envelope.event_version))
        .bind(event.envelope.occurred_at)
        .bind(ExceptionCase::AGGREGATE_TYPE)
        .bind(command.exception_id)
        .bind(as_i64(
            transition.aggregate_version,
            "exception event aggregate version",
        )?)
        .bind(&self.holding_ref)
        .bind(serde_json::to_value(&event.envelope.tenant_scope)?)
        .bind(serde_json::to_value(&event.envelope.actor)?)
        .bind(event.envelope.correlation_id)
        .bind(event.envelope.causation_id)
        .bind(&event.envelope.idempotency_key)
        .bind(&event.envelope.payload)
        .execute(&mut *tx)
        .await
        .map_err(map_event_insert_error)?;
        append_operations_outbox_event(&mut tx, &event.envelope, &self.holding_ref).await?;
        write_snapshot(
            &mut tx,
            exception.metadata(),
            ExceptionCase::AGGREGATE_TYPE,
            transition.aggregate_version,
            serde_json::to_value(&exception.meta.scope)?,
            exception.lifecycle_status(),
            state,
        )
        .await?;
        if let Some(recipient_ref) = notification_recipient {
            let notification = OperationsSlaNotification {
                notification_key: format!(
                    "operations-sla:{}:{}",
                    command.exception_id, transition.aggregate_version
                ),
                holding_ref: self.holding_ref.clone(),
                exception_id: command.exception_id,
                exception_version: transition.aggregate_version,
                recipient_ref: recipient_ref.to_owned(),
                kind: OperationsSlaNotificationKind::SlaEscalated,
                not_before: command.at,
                expires_at: command.at + OPERATIONS_SLA_NOTIFICATION_RETENTION,
                payload: json!({
                    "exception_id": command.exception_id,
                    "exception_version": transition.aggregate_version,
                    "recipient_ref": recipient_ref,
                    "kind": OperationsSlaNotificationKind::SlaEscalated.as_str(),
                }),
            };
            enqueue_operations_sla_notification_tx(&mut tx, &notification).await?;
        }
        tx.commit().await?;
        Ok(ExceptionLifecycleApplication {
            event_id: event.envelope.event_id,
            exception_id: command.exception_id,
            from: transition.from,
            to: transition.to,
            aggregate_version: transition.aggregate_version,
            applied: true,
        })
    }

    /// Preserve the old API shape without preserving its unsafe behavior.
    ///
    /// This method used to write only `operations_aggregate_snapshots` and
    /// `operations_source_references`, leaving no immutable event or outbox
    /// envelope. That made the resulting state invisible to the projector and
    /// allowed reconciliation to certify data that could not be replayed. The
    /// event boundary is intentionally mandatory now.
    pub async fn save_aggregate_with_source_refs<A: OperationsAggregate>(
        &self,
        aggregate: &A,
        expected_version: Option<u64>,
        source_refs: &[SourceTransactionRef],
    ) -> Result<u64, PersistenceError> {
        validate_aggregate(aggregate, &self.holding_ref)?;
        ensure_declared_source_references(aggregate, source_refs)?;
        let _ = expected_version;
        Err(PersistenceError::InvalidData(
            "snapshot-only Operations persistence is disabled; apply an authoritative event with outbox evidence".into(),
        ))
    }

    /// Apply an event and its aggregate state atomically. A duplicate event ID
    /// or idempotency key is a successful replay and does not mutate state.
    pub async fn apply_event<A: OperationsAggregate>(
        &self,
        event: &OperationsEvent,
        aggregate: &A,
        expected_version: Option<u64>,
        source_refs: &[SourceTransactionRef],
    ) -> Result<OperationsEventApplication, PersistenceError> {
        validate_event(event, aggregate, &self.holding_ref)?;
        ensure_declared_source_references(aggregate, source_refs)?;
        let metadata = aggregate.metadata();
        let mut tx = self.begin_write().await?;

        if let Some(row) = sqlx::query(
            "SELECT event_id, contract_version, event_type, event_version, occurred_at,
                    aggregate_type, aggregate_id, aggregate_version, tenant_scope, actor,
                    correlation_id, causation_id, idempotency_key, payload
             FROM erpx_tenant.operations_events
             WHERE event_id = $1",
        )
        .bind(event.envelope.event_id)
        .fetch_optional(&mut *tx)
        .await?
        {
            if !event_row_matches(&row, event)? {
                return Err(PersistenceError::InvalidData(
                    "event ID replay did not match the persisted event".into(),
                ));
            }
            let aggregate_version = positive_u64(
                row.try_get("aggregate_version")?,
                "operations event aggregate version",
            )?;
            append_operations_outbox_event(&mut tx, &event.envelope, &self.holding_ref).await?;
            tx.commit().await?;
            return Ok(OperationsEventApplication {
                event_id: event.envelope.event_id,
                aggregate_type: A::AGGREGATE_TYPE.into(),
                aggregate_id: metadata.id,
                aggregate_version,
                applied: false,
            });
        }

        if let Some(idempotency_key) = event.envelope.idempotency_key.as_deref()
            && let Some(row) = sqlx::query(
                "SELECT event_id FROM erpx_tenant.operations_events
                 WHERE idempotency_key = $1",
            )
            .bind(idempotency_key)
            .fetch_optional(&mut *tx)
            .await?
        {
            let stored_event_id: Uuid = row.try_get("event_id")?;
            if stored_event_id != event.envelope.event_id {
                return Err(PersistenceError::IdempotencyConflict {
                    key: idempotency_key.into(),
                });
            }
        }

        let current_version =
            current_snapshot_version(&mut tx, A::AGGREGATE_TYPE, metadata.id).await?;
        ensure_expected_version(
            A::AGGREGATE_TYPE,
            metadata.id,
            expected_version,
            current_version,
        )?;
        let expected_event_version = next_version(expected_version)?;
        if event.envelope.aggregate_version != expected_event_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: A::AGGREGATE_TYPE.into(),
                aggregate_id: metadata.id,
                expected: Some(expected_event_version),
                actual: Some(event.envelope.aggregate_version),
            });
        }

        if let Some(row) = sqlx::query(
            "SELECT event_id FROM erpx_tenant.operations_events
             WHERE aggregate_type = $1 AND aggregate_id = $2
               AND aggregate_version = $3",
        )
        .bind(A::AGGREGATE_TYPE)
        .bind(metadata.id)
        .bind(as_i64(
            expected_event_version,
            "operations event aggregate version",
        )?)
        .fetch_optional(&mut *tx)
        .await?
        {
            let stored_event_id: Uuid = row.try_get("event_id")?;
            if stored_event_id != event.envelope.event_id {
                return Err(PersistenceError::InvalidData(
                    "aggregate version already has another event".into(),
                ));
            }
        }

        let state = normalized_state(aggregate, event.envelope.aggregate_version)?;
        sqlx::query(
            "INSERT INTO erpx_tenant.operations_events
                (event_id, contract_version, event_type, event_version, occurred_at,
                 aggregate_type, aggregate_id, aggregate_version, holding_ref,
                 tenant_scope, actor, correlation_id, causation_id, idempotency_key,
                 payload)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15)",
        )
        .bind(event.envelope.event_id)
        .bind(i32::from(event.envelope.contract_version))
        .bind(&event.envelope.event_type)
        .bind(i32::from(event.envelope.event_version))
        .bind(event.envelope.occurred_at)
        .bind(A::AGGREGATE_TYPE)
        .bind(metadata.id)
        .bind(as_i64(
            event.envelope.aggregate_version,
            "operations event aggregate version",
        )?)
        .bind(&self.holding_ref)
        .bind(serde_json::to_value(&event.envelope.tenant_scope)?)
        .bind(serde_json::to_value(&event.envelope.actor)?)
        .bind(event.envelope.correlation_id)
        .bind(event.envelope.causation_id)
        .bind(&event.envelope.idempotency_key)
        .bind(&event.envelope.payload)
        .execute(&mut *tx)
        .await
        .map_err(map_event_insert_error)?;
        append_operations_outbox_event(&mut tx, &event.envelope, &self.holding_ref).await?;

        write_snapshot(
            &mut tx,
            metadata,
            A::AGGREGATE_TYPE,
            event.envelope.aggregate_version,
            serde_json::to_value(&metadata.scope)?,
            aggregate.lifecycle_status(),
            state,
        )
        .await?;
        write_source_references(
            &mut tx,
            A::AGGREGATE_TYPE,
            metadata.id,
            event.envelope.aggregate_version,
            source_refs,
            Some(event.envelope.event_id),
            &self.holding_ref,
        )
        .await?;
        tx.commit().await?;
        Ok(OperationsEventApplication {
            event_id: event.envelope.event_id,
            aggregate_type: A::AGGREGATE_TYPE.into(),
            aggregate_id: metadata.id,
            aggregate_version: event.envelope.aggregate_version,
            applied: true,
        })
    }

    pub async fn get_aggregate<A: OperationsAggregate>(
        &self,
        aggregate_id: Uuid,
    ) -> Result<Option<A>, PersistenceError> {
        let Some(snapshot) = self.get_snapshot::<A>(aggregate_id).await? else {
            return Ok(None);
        };
        if snapshot.aggregate_type != A::AGGREGATE_TYPE {
            return Err(PersistenceError::InvalidData(
                "operations snapshot aggregate type does not match its typed reader".into(),
            ));
        }
        let value: A = serde_json::from_value(snapshot.state)?;
        let metadata = validate_persisted_aggregate(
            &value,
            snapshot.aggregate_id,
            snapshot.aggregate_version,
            &snapshot.holding_ref,
            &snapshot.lifecycle_status,
        )?;
        if serde_json::to_value(&metadata.scope)? != snapshot.scope {
            return Err(PersistenceError::InvalidData(
                "operations snapshot scope does not match its typed aggregate".into(),
            ));
        }
        Ok(Some(value))
    }

    pub async fn get_snapshot<A: OperationsAggregate>(
        &self,
        aggregate_id: Uuid,
    ) -> Result<Option<OperationsSnapshotRecord>, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let row = sqlx::query(
            "SELECT aggregate_type, aggregate_id, holding_ref, scope,
                    aggregate_version, lifecycle_status, state, created_at, updated_at
             FROM erpx_tenant.operations_aggregate_snapshots
             WHERE aggregate_type = $1 AND aggregate_id = $2",
        )
        .bind(A::AGGREGATE_TYPE)
        .bind(aggregate_id)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let record = row.map(snapshot_from_row).transpose()?;
        if let Some(record) = &record
            && record.holding_ref != self.holding_ref
        {
            return Err(PersistenceError::InvalidData(
                "operations snapshot holding reference does not match the routed tenant".into(),
            ));
        }
        Ok(record)
    }

    pub async fn get_source_references(
        &self,
        aggregate_type: &str,
        aggregate_id: Uuid,
    ) -> Result<Vec<SourceTransactionRef>, PersistenceError> {
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "aggregate_id")?;
        let mut tx = self.begin_write().await?;
        let rows = sqlx::query(
            "SELECT source_domain, source_type, source_id, source_event_id
             FROM erpx_tenant.operations_source_references
             WHERE aggregate_type = $1 AND aggregate_id = $2
             ORDER BY source_event_id",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .fetch_all(&mut *tx)
        .await?;
        tx.commit().await?;
        rows.into_iter()
            .map(|row| {
                SourceTransactionRef::new(
                    row.try_get::<String, _>("source_domain")?,
                    row.try_get::<String, _>("source_type")?,
                    row.try_get("source_id")?,
                    row.try_get("source_event_id")?,
                )
                .map_err(|error| invalid_domain("source reference", error))
            })
            .collect()
    }

    /// Read a bounded, holding-scoped page from the immutable Operations event
    /// stream. The cursor is exclusive and follows the same ordering as the
    /// query: aggregate ID, aggregate version, then event ID.
    pub async fn read_event_page(
        &self,
        cursor: Option<OperationsEventCursor>,
        page_size: u32,
    ) -> Result<OperationsEventPage, PersistenceError> {
        let page_size = bounded_page_size(page_size)?;
        if let Some(cursor) = cursor {
            validate_uuid(cursor.aggregate_id, "event cursor aggregate_id")?;
        }
        let query_limit = i64::from(page_size) + 1;
        let mut tx = self.begin_write().await?;
        let rows = if let Some(cursor) = cursor {
            sqlx::query(
                "SELECT event_id, contract_version, event_type, event_version,
                        occurred_at, aggregate_type, aggregate_id, aggregate_version,
                        holding_ref, tenant_scope, actor, correlation_id, causation_id,
                        idempotency_key, payload, applied_at
                 FROM erpx_tenant.operations_events
                 WHERE holding_ref = $1
                   AND (aggregate_id > $2
                        OR (aggregate_id = $2 AND aggregate_version > $3))
                 ORDER BY aggregate_id, aggregate_version, event_id
                 LIMIT $4",
            )
            .bind(&self.holding_ref)
            .bind(cursor.aggregate_id)
            .bind(as_i64(
                cursor.aggregate_version,
                "event cursor aggregate version",
            )?)
            .bind(query_limit)
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "SELECT event_id, contract_version, event_type, event_version,
                        occurred_at, aggregate_type, aggregate_id, aggregate_version,
                        holding_ref, tenant_scope, actor, correlation_id, causation_id,
                        idempotency_key, payload, applied_at
                 FROM erpx_tenant.operations_events
                 WHERE holding_ref = $1
                 ORDER BY aggregate_id, aggregate_version, event_id
                 LIMIT $2",
            )
            .bind(&self.holding_ref)
            .bind(query_limit)
            .fetch_all(&mut *tx)
            .await?
        };
        tx.commit().await?;

        let has_more = rows.len()
            > usize::try_from(page_size).map_err(|_| {
                PersistenceError::InvalidData("event page size is outside platform range".into())
            })?;
        let mut events = rows;
        if has_more {
            events.pop();
        }
        let events = events
            .into_iter()
            .map(|row| event_record_from_row(row, &self.holding_ref))
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more.then(|| events.last().map(OperationsEventRecord::cursor));
        Ok(OperationsEventPage {
            events,
            next_cursor: next_cursor.flatten(),
            has_more,
        })
    }

    /// Return the immutable holding-scoped source-event count used as the
    /// candidate rebuild high-watermark. Operations events are append-only;
    /// the candidate projector verifies this value both before and after
    /// replay so a moving source cannot be activated as complete.
    pub async fn source_event_high_watermark(&self) -> Result<u64, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM erpx_tenant.operations_events
             WHERE holding_ref = $1",
        )
        .bind(&self.holding_ref)
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        u64::try_from(count).map_err(|_| {
            PersistenceError::InvalidData("source event high-watermark is invalid".into())
        })
    }

    /// Return the latest immutable source-owned outbox envelope for this
    /// holding. Reconciliation uses this as a feed fence because the
    /// Operations event count cannot detect a source event published while
    /// the source-owned outbox pages are being read.
    pub async fn source_outbox_high_watermark(
        &self,
    ) -> Result<Option<OperationsSourceOutboxCursor>, PersistenceError> {
        let (_, latest) = self.source_outbox_fence().await?;
        Ok(latest)
    }

    /// Return a stable fence for the source-owned outbox feed. The count is
    /// part of the fence because a source event may be inserted with an older
    /// `occurred_at` than the current latest row; comparing only the latest
    /// timestamp/event ID would then miss the new source fact.
    pub async fn source_outbox_fence(
        &self,
    ) -> Result<(u64, Option<OperationsSourceOutboxCursor>), PersistenceError> {
        let mut tx = self.begin_write().await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*)::BIGINT
             FROM erpx_tenant.outbox_events
             WHERE holding_ref = $1
               AND event_type NOT LIKE 'operations.%'
               AND event_type NOT LIKE 'organization.%'",
        )
        .bind(&self.holding_ref)
        .fetch_one(&mut *tx)
        .await?;
        let row = sqlx::query(
            "SELECT occurred_at, event_id
             FROM erpx_tenant.outbox_events
             WHERE holding_ref = $1
               AND event_type NOT LIKE 'operations.%'
               AND event_type NOT LIKE 'organization.%'
             ORDER BY occurred_at DESC, event_id DESC
             LIMIT 1",
        )
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;

        let latest = row
            .map(|row| {
                OperationsSourceOutboxCursor::new(
                    row.try_get("occurred_at")?,
                    row.try_get("event_id")?,
                )
            })
            .transpose()?;
        Ok((nonnegative_u64(count, "source outbox event count")?, latest))
    }

    /// Read a bounded, tenant-scoped page of source provenance for migration,
    /// reconciliation, and diagnostics. This returns identity and linkage
    /// only; source payloads stay in Sales, Procurement, Inventory, Finance,
    /// Maintenance, Support, or Integrations.
    pub async fn read_source_event_page(
        &self,
        source_domain: Option<OperationsSourceDomain>,
        cursor: Option<OperationsSourceEventCursor>,
        page_size: u32,
    ) -> Result<OperationsSourceEventPage, PersistenceError> {
        let page_size = bounded_source_event_page_size(page_size)?;
        if let Some(cursor) = cursor {
            validate_uuid(cursor.source_event_id, "source event cursor event_id")?;
        }
        let query_limit = i64::from(page_size) + 1;
        let mut tx = self.begin_write().await?;
        let rows = match (source_domain, cursor) {
            (Some(domain), Some(cursor)) => {
                sqlx::query(
                    "SELECT aggregate_type, aggregate_id, aggregate_version, holding_ref,
                        source_domain, source_type, source_id, source_event_id,
                        operations_event_id, recorded_at
                 FROM erpx_tenant.operations_source_references
                 WHERE holding_ref = $1 AND source_domain = $2
                   AND (recorded_at > $3
                        OR (recorded_at = $3 AND source_event_id > $4))
                 ORDER BY recorded_at, source_event_id
                 LIMIT $5",
                )
                .bind(&self.holding_ref)
                .bind(domain.as_str())
                .bind(cursor.recorded_at)
                .bind(cursor.source_event_id)
                .bind(query_limit)
                .fetch_all(&mut *tx)
                .await?
            }
            (Some(domain), None) => {
                sqlx::query(
                    "SELECT aggregate_type, aggregate_id, aggregate_version, holding_ref,
                        source_domain, source_type, source_id, source_event_id,
                        operations_event_id, recorded_at
                 FROM erpx_tenant.operations_source_references
                 WHERE holding_ref = $1 AND source_domain = $2
                 ORDER BY recorded_at, source_event_id
                 LIMIT $3",
                )
                .bind(&self.holding_ref)
                .bind(domain.as_str())
                .bind(query_limit)
                .fetch_all(&mut *tx)
                .await?
            }
            (None, Some(cursor)) => {
                sqlx::query(
                    "SELECT aggregate_type, aggregate_id, aggregate_version, holding_ref,
                        source_domain, source_type, source_id, source_event_id,
                        operations_event_id, recorded_at
                 FROM erpx_tenant.operations_source_references
                 WHERE holding_ref = $1
                   AND (recorded_at > $2
                        OR (recorded_at = $2 AND source_event_id > $3))
                 ORDER BY recorded_at, source_event_id
                 LIMIT $4",
                )
                .bind(&self.holding_ref)
                .bind(cursor.recorded_at)
                .bind(cursor.source_event_id)
                .bind(query_limit)
                .fetch_all(&mut *tx)
                .await?
            }
            (None, None) => {
                sqlx::query(
                    "SELECT aggregate_type, aggregate_id, aggregate_version, holding_ref,
                        source_domain, source_type, source_id, source_event_id,
                        operations_event_id, recorded_at
                 FROM erpx_tenant.operations_source_references
                 WHERE holding_ref = $1
                 ORDER BY recorded_at, source_event_id
                 LIMIT $2",
                )
                .bind(&self.holding_ref)
                .bind(query_limit)
                .fetch_all(&mut *tx)
                .await?
            }
        };
        tx.commit().await?;

        let has_more = rows.len()
            > usize::try_from(page_size).map_err(|_| {
                PersistenceError::InvalidData(
                    "source event page size is outside platform range".into(),
                )
            })?;
        let mut rows = rows;
        if has_more {
            rows.pop();
        }
        let references = rows
            .into_iter()
            .map(|row| source_reference_from_row(row, &self.holding_ref))
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more.then(|| {
            references
                .last()
                .map(|reference| OperationsSourceEventCursor {
                    recorded_at: reference.recorded_at,
                    source_event_id: reference.source.source_event_id,
                })
        });
        Ok(OperationsSourceEventPage {
            references,
            next_cursor: next_cursor.flatten(),
            has_more,
        })
    }

    /// Read a bounded page of source-context outbox envelopes for the
    /// Operations detection scheduler. Only rows with a complete shared
    /// envelope are accepted; legacy rows with missing actor, scope, event
    /// version, or correlation metadata fail closed instead of inventing
    /// authorization or source attribution.
    pub async fn read_source_outbox_event_page(
        &self,
        cursor: Option<OperationsSourceOutboxCursor>,
        page_size: u32,
    ) -> Result<OperationsSourceOutboxEventPage, PersistenceError> {
        let page_size = bounded_source_event_page_size(page_size)?;
        if let Some(cursor) = cursor {
            validate_uuid(cursor.source_event_id, "source outbox cursor event_id")?;
        }
        let query_limit = i64::from(page_size) + 1;
        let mut tx = self.begin_write().await?;
        let rows = if let Some(cursor) = cursor {
            sqlx::query(
                "SELECT event_id, aggregate_type, aggregate_id, holding_ref,
                        aggregate_version, event_type, contract_version,
                        event_version, tenant_scope, actor, correlation_id,
                        causation_id, payload, occurred_at
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref = $1
                   AND event_type NOT LIKE 'operations.%'
                   AND event_type NOT LIKE 'organization.%'
                   AND (occurred_at > $2
                        OR (occurred_at = $2 AND event_id > $3))
                 ORDER BY occurred_at, event_id
                 LIMIT $4",
            )
            .bind(&self.holding_ref)
            .bind(cursor.occurred_at)
            .bind(cursor.source_event_id)
            .bind(query_limit)
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "SELECT event_id, aggregate_type, aggregate_id, holding_ref,
                        aggregate_version, event_type, contract_version,
                        event_version, tenant_scope, actor, correlation_id,
                        causation_id, payload, occurred_at
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref = $1
                   AND event_type NOT LIKE 'operations.%'
                   AND event_type NOT LIKE 'organization.%'
                 ORDER BY occurred_at, event_id
                 LIMIT $2",
            )
            .bind(&self.holding_ref)
            .bind(query_limit)
            .fetch_all(&mut *tx)
            .await?
        };
        tx.commit().await?;

        let has_more = rows.len()
            > usize::try_from(page_size).map_err(|_| {
                PersistenceError::InvalidData(
                    "source outbox page size is outside platform range".into(),
                )
            })?;
        let mut rows = rows;
        if has_more {
            rows.pop();
        }
        let events = rows
            .into_iter()
            .map(|row| source_outbox_event_from_row(row, &self.holding_ref))
            .collect::<Result<Vec<_>, _>>()?;
        let next_cursor = has_more.then(|| {
            events.last().map(|event| OperationsSourceOutboxCursor {
                occurred_at: event.source_event.occurred_at,
                source_event_id: event.source_event.source_event_id,
            })
        });
        Ok(OperationsSourceOutboxEventPage {
            events,
            next_cursor: next_cursor.flatten(),
            has_more,
        })
    }

    /// Clear only one read projection in this routed holding's read database
    /// and return preparation metadata. This deliberately does not alter
    /// routing, ownership, write-side events, or any other projection.
    pub async fn prepare_projection_rebuild(
        &self,
        projection_name: &str,
    ) -> Result<OperationsProjectionRebuildPreparation, PersistenceError> {
        validate_projection_name(projection_name)?;
        let mut tx = self.begin_read().await?;
        // Rebuild preparation is a rollback boundary. Serialize it with all
        // projectors for this holding and projection so no projector can
        // repopulate a row between the read-row and checkpoint deletes.
        lock_projection_maintenance(&mut tx, &self.holding_ref, projection_name).await?;
        let read_rows_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.operations_read_rows
             WHERE projection_name = $1 AND holding_ref = $2",
        )
        .bind(projection_name)
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let checkpoints_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.operations_projection_checkpoints
             WHERE projection_name = $1 AND holding_ref = $2",
        )
        .bind(projection_name)
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        let detection_cursor_cleared = if projection_name == OPERATIONS_READ_PROJECTION {
            sqlx::query(
                "DELETE FROM erpx_tenant.operations_projection_checkpoints
                 WHERE projection_name = $1 AND holding_ref = $2",
            )
            .bind(OPERATIONS_DETECTION_CURSOR_PROJECTION)
            .bind(&self.holding_ref)
            .execute(&mut *tx)
            .await?
            .rows_affected()
        } else {
            0
        };
        tx.commit().await?;
        Ok(OperationsProjectionRebuildPreparation {
            projection_name: projection_name.into(),
            holding_ref: self.holding_ref.clone(),
            read_rows_cleared,
            checkpoints_cleared: checkpoints_cleared.saturating_add(detection_cursor_cleared),
        })
    }

    /// Apply a typed event to the rebuildable tenant read model. Events older
    /// than the checkpoint, including exact replays, are no-ops.
    pub async fn project_event<A: OperationsAggregate>(
        &self,
        event: &OperationsEvent,
        aggregate: &A,
    ) -> Result<OperationsProjectionApplication, PersistenceError> {
        validate_event(event, aggregate, &self.holding_ref)?;
        let metadata = aggregate.metadata();
        let projection = &event.projection;
        let mut tx = self.begin_read().await?;
        // Use the same transaction-scoped lock as projection rebuilds. This
        // keeps a writer from racing a rollback that clears rows/checkpoints.
        lock_projection_maintenance(&mut tx, &self.holding_ref, &projection.projection_name)
            .await?;
        sqlx::query(
            "SELECT pg_advisory_xact_lock(
                        hashtextextended(
                            $1 || ':' || $2 || ':' || $3::text,
                            0
                        )
                    )",
        )
        .bind(&projection.projection_name)
        .bind(A::AGGREGATE_TYPE)
        .bind(metadata.id)
        .execute(&mut *tx)
        .await?;
        let current = sqlx::query(
            "SELECT last_event_id, last_aggregate_version
             FROM erpx_tenant.operations_projection_checkpoints
             WHERE projection_name = $1 AND aggregate_type = $2 AND aggregate_id = $3
             FOR UPDATE",
        )
        .bind(&projection.projection_name)
        .bind(A::AGGREGATE_TYPE)
        .bind(metadata.id)
        .fetch_optional(&mut *tx)
        .await?;
        let current_version = current
            .as_ref()
            .map(|row| row.try_get::<i64, _>("last_aggregate_version"))
            .transpose()?
            .map(|value| positive_u64(value, "projection checkpoint version"))
            .transpose()?
            .unwrap_or(0);
        let current_event_id = current
            .as_ref()
            .map(|row| row.try_get::<Option<Uuid>, _>("last_event_id"))
            .transpose()?
            .flatten();
        if current_event_id == Some(projection.event_id)
            || projection.aggregate_version < current_version
        {
            tx.commit().await?;
            return Ok(projection_result(event, false));
        }
        if projection.aggregate_version == current_version {
            return Err(PersistenceError::InvalidData(
                "projection checkpoint has a different event at the same aggregate version".into(),
            ));
        }
        let expected_version = current_version
            .checked_add(1)
            .ok_or_else(|| PersistenceError::InvalidData("projection version overflow".into()))?;
        if projection.aggregate_version != expected_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: A::AGGREGATE_TYPE.into(),
                aggregate_id: metadata.id,
                expected: Some(expected_version),
                actual: Some(projection.aggregate_version),
            });
        }

        let state = normalized_state(aggregate, projection.aggregate_version)?;
        sqlx::query(
            "INSERT INTO erpx_tenant.operations_read_rows
                (projection_name, aggregate_type, aggregate_id, holding_ref,
                 aggregate_version, lifecycle_status, state, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
             ON CONFLICT (projection_name, aggregate_type, aggregate_id) DO UPDATE SET
                holding_ref = EXCLUDED.holding_ref,
                aggregate_version = EXCLUDED.aggregate_version,
                lifecycle_status = EXCLUDED.lifecycle_status,
                state = EXCLUDED.state,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(&projection.projection_name)
        .bind(A::AGGREGATE_TYPE)
        .bind(metadata.id)
        .bind(&self.holding_ref)
        .bind(as_i64(
            projection.aggregate_version,
            "projection aggregate version",
        )?)
        .bind(aggregate.lifecycle_status())
        .bind(state)
        .bind(projection.projected_at)
        .execute(&mut *tx)
        .await?;
        sqlx::query(
            "INSERT INTO erpx_tenant.operations_projection_checkpoints
                (projection_name, aggregate_type, aggregate_id, holding_ref,
                 last_event_id, last_aggregate_version, projected_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7)
             ON CONFLICT (projection_name, aggregate_type, aggregate_id) DO UPDATE SET
                holding_ref = EXCLUDED.holding_ref,
                last_event_id = EXCLUDED.last_event_id,
                last_aggregate_version = EXCLUDED.last_aggregate_version,
                projected_at = EXCLUDED.projected_at",
        )
        .bind(&projection.projection_name)
        .bind(A::AGGREGATE_TYPE)
        .bind(metadata.id)
        .bind(&self.holding_ref)
        .bind(projection.event_id)
        .bind(as_i64(
            projection.aggregate_version,
            "projection aggregate version",
        )?)
        .bind(projection.projected_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(projection_result(event, true))
    }

    pub async fn get_read_aggregate<A: OperationsAggregate>(
        &self,
        aggregate_id: Uuid,
    ) -> Result<Option<OperationsReadRow<A>>, PersistenceError> {
        self.get_read_aggregate_for_projection::<A>(OPERATIONS_READ_PROJECTION, aggregate_id)
            .await
    }

    /// Read a bounded page of typed aggregate JSON from the rebuildable
    /// Operations projection. The prefix is allow-listed by the caller's
    /// domain adapter and the tenant RLS setting remains transaction-local.
    pub async fn read_json_page(
        &self,
        aggregate_type_prefix: &str,
        cursor: Option<OperationsJsonReadCursor>,
        page_size: u32,
    ) -> Result<OperationsJsonReadPage, PersistenceError> {
        validate_aggregate_type(aggregate_type_prefix)?;
        if page_size == 0 || page_size > 100 {
            return Err(PersistenceError::InvalidData(
                "JSON read page size is out of bounds".into(),
            ));
        }
        let limit = i64::from(page_size) + 1;
        let mut tx = self.begin_read().await?;
        let rows = if let Some(cursor) = cursor {
            sqlx::query(
                "SELECT aggregate_type, aggregate_id, holding_ref,
                        aggregate_version, lifecycle_status, state, updated_at
                 FROM erpx_tenant.operations_read_rows
                 WHERE projection_name = $1
                   AND holding_ref = $3
                   AND aggregate_type LIKE $2
                   AND (updated_at, aggregate_id) < ($4, $5)
                 ORDER BY updated_at DESC, aggregate_id DESC
                 LIMIT $6",
            )
            .bind(OPERATIONS_READ_PROJECTION)
            .bind(format!("{aggregate_type_prefix}%"))
            .bind(&self.holding_ref)
            .bind(cursor.updated_at)
            .bind(cursor.aggregate_id)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "SELECT aggregate_type, aggregate_id, holding_ref,
                        aggregate_version, lifecycle_status, state, updated_at
                 FROM erpx_tenant.operations_read_rows
                 WHERE projection_name = $1
                   AND holding_ref = $3
                   AND aggregate_type LIKE $2
                 ORDER BY updated_at DESC, aggregate_id DESC
                 LIMIT $4",
            )
            .bind(OPERATIONS_READ_PROJECTION)
            .bind(format!("{aggregate_type_prefix}%"))
            .bind(&self.holding_ref)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        };
        tx.commit().await?;
        let page_size = usize::try_from(page_size).map_err(|_| {
            PersistenceError::InvalidData("JSON read page size is outside platform range".into())
        })?;
        let has_more = rows.len() > page_size;
        let next_cursor = if has_more {
            let row = rows.last().ok_or_else(|| {
                PersistenceError::InvalidData(
                    "JSON read page reported more rows without a final row".into(),
                )
            })?;
            Some(OperationsJsonReadCursor {
                updated_at: row.try_get("updated_at")?,
                aggregate_id: row.try_get("aggregate_id")?,
            })
        } else {
            None
        };
        let rows = rows.into_iter().take(page_size).collect::<Vec<_>>();
        let rows = rows
            .into_iter()
            .map(|row| {
                let holding_ref: String = row.try_get("holding_ref")?;
                if holding_ref != self.holding_ref {
                    return Err(PersistenceError::InvalidData(
                        "JSON read holding reference does not match the routed tenant".into(),
                    ));
                }
                Ok(OperationsJsonReadRow {
                    aggregate_type: row.try_get("aggregate_type")?,
                    aggregate_id: row.try_get("aggregate_id")?,
                    holding_ref,
                    aggregate_version: positive_u64(
                        row.try_get("aggregate_version")?,
                        "JSON read aggregate version",
                    )?,
                    lifecycle_status: row.try_get("lifecycle_status")?,
                    state: row.try_get("state")?,
                    updated_at: row.try_get("updated_at")?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        Ok(OperationsJsonReadPage {
            rows,
            has_more,
            next_cursor,
        })
    }

    pub async fn get_read_aggregate_for_projection<A: OperationsAggregate>(
        &self,
        projection_name: &str,
        aggregate_id: Uuid,
    ) -> Result<Option<OperationsReadRow<A>>, PersistenceError> {
        validate_projection_name(projection_name)?;
        validate_uuid(aggregate_id, "aggregate_id")?;
        let mut tx = self.begin_read().await?;
        let row = sqlx::query(
            "SELECT projection_name, aggregate_type, aggregate_id, holding_ref,
                    aggregate_version, lifecycle_status, state, updated_at
             FROM erpx_tenant.operations_read_rows
             WHERE projection_name = $1 AND aggregate_type = $2 AND aggregate_id = $3
               AND holding_ref = $4",
        )
        .bind(projection_name)
        .bind(A::AGGREGATE_TYPE)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let Some(row) = row else {
            return Ok(None);
        };
        let aggregate_version =
            positive_u64(row.try_get("aggregate_version")?, "read aggregate version")?;
        let state: Value = row.try_get("state")?;
        let value: A = serde_json::from_value(state)?;
        let holding_ref: String = row.try_get("holding_ref")?;
        let lifecycle_status: String = row.try_get("lifecycle_status")?;
        validate_persisted_aggregate(
            &value,
            aggregate_id,
            aggregate_version,
            &holding_ref,
            &lifecycle_status,
        )?;
        Ok(Some(OperationsReadRow {
            projection_name: row.try_get("projection_name")?,
            aggregate_type: row.try_get("aggregate_type")?,
            aggregate_id,
            holding_ref,
            aggregate_version,
            lifecycle_status,
            value,
            updated_at: row.try_get("updated_at")?,
        }))
    }

    /// Read a bounded, holding-scoped queue of non-terminal exception cases.
    /// The read model is disposable; callers must rebuild it from the
    /// write-side event page rather than treating this query as a source
    /// transaction.
    pub async fn read_active_exception_page(
        &self,
        projection_name: &str,
        cursor: Option<OperationsExceptionCursor>,
        page_size: u32,
    ) -> Result<OperationsExceptionPage, PersistenceError> {
        validate_projection_name(projection_name)?;
        let page_size = bounded_exception_page_size(page_size)?;
        if let Some(cursor) = cursor {
            validate_uuid(cursor.aggregate_id, "exception cursor aggregate_id")?;
        }
        let query_limit = i64::from(page_size) + 1;
        let mut tx = self.begin_read().await?;
        let rows = if let Some(cursor) = cursor {
            sqlx::query(
                "SELECT projection_name, aggregate_type, aggregate_id, holding_ref,
                        aggregate_version, lifecycle_status, state, updated_at
                 FROM erpx_tenant.operations_read_rows
                 WHERE projection_name = $1
                   AND aggregate_type = $2
                   AND holding_ref = $3
                   AND lifecycle_status IN ('detected', 'assigned', 'acknowledged', 'escalated')
                   AND (updated_at < $4
                        OR (updated_at = $4 AND aggregate_id > $5))
                 ORDER BY updated_at DESC, aggregate_id, aggregate_version
                 LIMIT $6",
            )
            .bind(projection_name)
            .bind(ExceptionCase::AGGREGATE_TYPE)
            .bind(&self.holding_ref)
            .bind(cursor.updated_at)
            .bind(cursor.aggregate_id)
            .bind(query_limit)
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "SELECT projection_name, aggregate_type, aggregate_id, holding_ref,
                        aggregate_version, lifecycle_status, state, updated_at
                 FROM erpx_tenant.operations_read_rows
                 WHERE projection_name = $1
                   AND aggregate_type = $2
                   AND holding_ref = $3
                   AND lifecycle_status IN ('detected', 'assigned', 'acknowledged', 'escalated')
                 ORDER BY updated_at DESC, aggregate_id, aggregate_version
                 LIMIT $4",
            )
            .bind(projection_name)
            .bind(ExceptionCase::AGGREGATE_TYPE)
            .bind(&self.holding_ref)
            .bind(query_limit)
            .fetch_all(&mut *tx)
            .await?
        };
        tx.commit().await?;

        let has_more = rows.len()
            > usize::try_from(page_size).map_err(|_| {
                PersistenceError::InvalidData(
                    "exception page size is outside platform range".into(),
                )
            })?;
        let mut rows = rows;
        if has_more {
            rows.pop();
        }
        let cases = rows
            .into_iter()
            .map(|row| {
                let updated_at: DateTime<Utc> = row.try_get("updated_at")?;
                let aggregate_id: Uuid = row.try_get("aggregate_id")?;
                let aggregate_version =
                    positive_u64(row.try_get("aggregate_version")?, "exception version")?;
                let state: Value = row.try_get("state")?;
                let value: ExceptionCase = serde_json::from_value(state)?;
                if value.meta.id != aggregate_id
                    || value.meta.version != aggregate_version
                    || value.meta.holding_ref != self.holding_ref
                    || row.try_get::<String, _>("holding_ref")? != self.holding_ref
                {
                    return Err(PersistenceError::InvalidData(
                        "exception read row metadata does not match its routed key".into(),
                    ));
                }
                Ok((
                    OperationsReadRow {
                        projection_name: row.try_get("projection_name")?,
                        aggregate_type: row.try_get("aggregate_type")?,
                        aggregate_id,
                        holding_ref: row.try_get("holding_ref")?,
                        aggregate_version,
                        lifecycle_status: row.try_get("lifecycle_status")?,
                        value,
                        updated_at,
                    },
                    OperationsExceptionCursor {
                        updated_at,
                        aggregate_id,
                    },
                ))
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        let next_cursor = has_more.then(|| cases.last().map(|(_, cursor)| *cursor));
        Ok(OperationsExceptionPage {
            cases: cases.into_iter().map(|(case, _)| case).collect(),
            next_cursor: next_cursor.flatten(),
            has_more,
        })
    }

    pub async fn get_projection_checkpoint(
        &self,
        projection_name: &str,
        aggregate_type: &str,
        aggregate_id: Uuid,
    ) -> Result<Option<OperationsProjectionCheckpoint>, PersistenceError> {
        validate_projection_name(projection_name)?;
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "aggregate_id")?;
        let mut tx = self.begin_read().await?;
        let row = sqlx::query(
            "SELECT projection_name, aggregate_type, aggregate_id, holding_ref,
                    last_event_id, last_aggregate_version, projected_at
             FROM erpx_tenant.operations_projection_checkpoints
             WHERE projection_name = $1 AND aggregate_type = $2 AND aggregate_id = $3
               AND holding_ref = $4",
        )
        .bind(projection_name)
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        let allow_zero_version = projection_name == OPERATIONS_DETECTION_CURSOR_PROJECTION
            && aggregate_type == OPERATIONS_DETECTION_CURSOR_AGGREGATE_TYPE
            && aggregate_id == operations_detection_cursor_id(&self.holding_ref);
        row.map(|row| checkpoint_from_row(row, allow_zero_version))
            .transpose()
    }

    /// Read the durable source-feed high-water mark for this holding. The
    /// cursor is represented as a checkpoint because it is rebuildable control
    /// state, while the source outbox and detected exception remain immutable
    /// authority.
    pub async fn source_detection_cursor(
        &self,
    ) -> Result<Option<OperationsSourceOutboxCursor>, PersistenceError> {
        let checkpoint = self
            .get_projection_checkpoint(
                OPERATIONS_DETECTION_CURSOR_PROJECTION,
                OPERATIONS_DETECTION_CURSOR_AGGREGATE_TYPE,
                operations_detection_cursor_id(&self.holding_ref),
            )
            .await?;
        let Some(checkpoint) = checkpoint else {
            return Ok(None);
        };
        if checkpoint.last_aggregate_version != 0 {
            return Err(PersistenceError::InvalidData(
                "Operations source detection cursor checkpoint version is invalid".into(),
            ));
        }
        let source_event_id = checkpoint.last_event_id.ok_or_else(|| {
            PersistenceError::InvalidData(
                "Operations source detection cursor is missing its source event ID".into(),
            )
        })?;
        OperationsSourceOutboxCursor::new(checkpoint.projected_at, source_event_id).map(Some)
    }

    /// Persist a source-feed high-water mark without allowing a concurrent or
    /// stale worker to move it backwards. Detection pages call this only after
    /// every matched request in the page has completed its authoritative,
    /// idempotent write transaction.
    pub async fn save_source_detection_cursor(
        &self,
        cursor: OperationsSourceOutboxCursor,
    ) -> Result<(), PersistenceError> {
        let cursor = OperationsSourceOutboxCursor::new(cursor.occurred_at, cursor.source_event_id)?;
        let mut source_tx = self.begin_write().await?;
        let source_row = sqlx::query(
            "SELECT event_id, aggregate_type, aggregate_id, holding_ref,
                    aggregate_version, event_type, contract_version,
                    event_version, tenant_scope, actor, correlation_id,
                    causation_id, payload, occurred_at
             FROM erpx_tenant.outbox_events
             WHERE event_id = $1 AND holding_ref = $2
               AND event_type NOT LIKE 'operations.%'
               AND event_type NOT LIKE 'organization.%'
             FOR SHARE",
        )
        .bind(cursor.source_event_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *source_tx)
        .await?
        .ok_or_else(|| {
            PersistenceError::InvalidData(
                "Operations source detection cursor does not reference an authoritative source outbox event".into(),
            )
        })?;
        let source_event = source_outbox_event_from_row(source_row, &self.holding_ref)?;
        validate_authoritative_source_detection_cursor(
            &source_event.source_event,
            &cursor,
            &self.holding_ref,
        )?;
        source_tx.commit().await?;

        let aggregate_id = operations_detection_cursor_id(&self.holding_ref);
        let mut tx = self.begin_read().await?;
        let current = sqlx::query(
            "SELECT last_event_id, last_aggregate_version, projected_at
             FROM erpx_tenant.operations_projection_checkpoints
             WHERE projection_name = $1 AND aggregate_type = $2
               AND aggregate_id = $3 AND holding_ref = $4
             FOR UPDATE",
        )
        .bind(OPERATIONS_DETECTION_CURSOR_PROJECTION)
        .bind(OPERATIONS_DETECTION_CURSOR_AGGREGATE_TYPE)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(row) = &current {
            let current_version = positive_u64(
                row.try_get("last_aggregate_version")?,
                "Operations source detection cursor version",
            )?;
            if current_version != 0 {
                return Err(PersistenceError::InvalidData(
                    "Operations source detection cursor checkpoint version is invalid".into(),
                ));
            }
            let current_event_id: Uuid = row
                .try_get::<Option<Uuid>, _>("last_event_id")?
                .ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "Operations source detection cursor is missing its source event ID".into(),
                    )
                })?;
            let current_cursor =
                OperationsSourceOutboxCursor::new(row.try_get("projected_at")?, current_event_id)?;
            if (current_cursor.occurred_at, current_cursor.source_event_id)
                > (cursor.occurred_at, cursor.source_event_id)
            {
                return Err(PersistenceError::InvalidData(
                    "Operations source detection cursor cannot move backwards".into(),
                ));
            }
        }
        sqlx::query(
            "INSERT INTO erpx_tenant.operations_projection_checkpoints
                (projection_name, aggregate_type, aggregate_id, holding_ref,
                 last_event_id, last_aggregate_version, projected_at)
             VALUES ($1, $2, $3, $4, $5, 0, $6)
             ON CONFLICT (projection_name, aggregate_type, aggregate_id) DO UPDATE SET
                holding_ref = EXCLUDED.holding_ref,
                last_event_id = EXCLUDED.last_event_id,
                last_aggregate_version = EXCLUDED.last_aggregate_version,
                projected_at = EXCLUDED.projected_at",
        )
        .bind(OPERATIONS_DETECTION_CURSOR_PROJECTION)
        .bind(OPERATIONS_DETECTION_CURSOR_AGGREGATE_TYPE)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .bind(cursor.source_event_id)
        .bind(cursor.occurred_at)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Clear only the rebuildable source-feed cursor for this holding. Normal
    /// rollback uses `prepare_projection_rebuild`, which clears it atomically
    /// with the Operations read projection.
    pub async fn clear_source_detection_cursor(&self) -> Result<bool, PersistenceError> {
        let mut tx = self.begin_read().await?;
        let cleared = sqlx::query(
            "DELETE FROM erpx_tenant.operations_projection_checkpoints
             WHERE projection_name = $1 AND aggregate_type = $2
               AND aggregate_id = $3 AND holding_ref = $4",
        )
        .bind(OPERATIONS_DETECTION_CURSOR_PROJECTION)
        .bind(OPERATIONS_DETECTION_CURSOR_AGGREGATE_TYPE)
        .bind(operations_detection_cursor_id(&self.holding_ref))
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?
        .rows_affected()
            != 0;
        tx.commit().await?;
        Ok(cleared)
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.write_pool, &self.holding_ref, true).await
    }

    async fn begin_read(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.read_pool, &self.holding_ref, false).await
    }
}

fn operations_notification_kind(
    value: String,
) -> Result<OperationsSlaNotificationKind, PersistenceError> {
    match value.as_str() {
        "sla_escalated" => Ok(OperationsSlaNotificationKind::SlaEscalated),
        _ => Err(PersistenceError::InvalidData(
            "Operations SLA notification kind is invalid".into(),
        )),
    }
}

fn validate_operations_notification(
    notification: &OperationsSlaNotification,
) -> Result<(), PersistenceError> {
    if notification.notification_key.trim().is_empty()
        || notification.notification_key.len() > 512
        || notification.notification_key.chars().any(char::is_control)
        || notification.holding_ref.trim().is_empty()
        || notification.holding_ref.len() > 255
        || notification.holding_ref.chars().any(char::is_control)
        || notification.exception_id.is_nil()
        || notification.exception_version == 0
        || notification.recipient_ref.trim().is_empty()
        || notification.recipient_ref.len() > 255
        || notification.recipient_ref.chars().any(char::is_control)
        || notification.not_before >= notification.expires_at
    {
        return Err(PersistenceError::InvalidData(
            "Operations SLA notification is invalid".into(),
        ));
    }
    let payload = serde_json::to_vec(&notification.payload)?;
    if payload.len() > 8 * 1024 {
        return Err(PersistenceError::InvalidData(
            "Operations SLA notification payload is too large".into(),
        ));
    }
    Ok(())
}

fn validate_operations_notification_claim(
    claim: &ClaimedOperationsSlaNotification,
) -> Result<(), PersistenceError> {
    validate_operations_notification(&claim.notification)?;
    if claim.claimed_by.is_nil()
        || claim.claim_token.is_nil()
        || claim.claim_expires_at <= claim.notification.not_before
        || claim.claim_expires_at > claim.notification.expires_at
    {
        return Err(PersistenceError::InvalidData(
            "Operations SLA notification claim is invalid".into(),
        ));
    }
    Ok(())
}

async fn enqueue_operations_sla_notification_tx(
    tx: &mut Transaction<'_, Postgres>,
    notification: &OperationsSlaNotification,
) -> Result<(), PersistenceError> {
    validate_operations_notification(notification)?;
    sqlx::query(
        "INSERT INTO erpx_tenant.operations_sla_notifications
            (notification_key, holding_ref, exception_id, exception_version,
             recipient_ref, kind, not_before, expires_at, status, payload)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, 'pending', $9)
         ON CONFLICT (notification_key) DO NOTHING",
    )
    .bind(&notification.notification_key)
    .bind(&notification.holding_ref)
    .bind(notification.exception_id)
    .bind(as_i64(
        notification.exception_version,
        "notification version",
    )?)
    .bind(&notification.recipient_ref)
    .bind(notification.kind.as_str())
    .bind(notification.not_before)
    .bind(notification.expires_at)
    .bind(&notification.payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn begin_scoped<'a>(
    pool: &'a PgPool,
    holding_ref: &str,
    lock_source_writes: bool,
) -> Result<Transaction<'a, Postgres>, PersistenceError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await?;
    if lock_source_writes {
        let lock_name = format!("{TENANT_SOURCE_REBUILD_LOCK_PREFIX}{holding_ref}");
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(lock_name)
            .execute(&mut *tx)
            .await?;
    }
    Ok(tx)
}

fn validate_aggregate<'a, A: OperationsAggregate>(
    aggregate: &'a A,
    holding_ref: &str,
) -> Result<&'a AggregateMetadata, PersistenceError> {
    aggregate
        .validate()
        .map_err(|error| invalid_domain("aggregate", error))?;
    let metadata = aggregate.metadata();
    validate_uuid(metadata.id, "aggregate_id")?;
    validate_aggregate_type(A::AGGREGATE_TYPE)?;
    if metadata.holding_ref != holding_ref {
        return Err(PersistenceError::InvalidData(
            "aggregate holding reference does not match the routed tenant".into(),
        ));
    }
    Ok(metadata)
}

fn validate_persisted_aggregate<'a, A: OperationsAggregate>(
    aggregate: &'a A,
    aggregate_id: Uuid,
    aggregate_version: u64,
    holding_ref: &str,
    lifecycle_status: &str,
) -> Result<&'a AggregateMetadata, PersistenceError> {
    let metadata = validate_aggregate(aggregate, holding_ref)?;
    if metadata.id != aggregate_id
        || metadata.version != aggregate_version
        || aggregate.lifecycle_status() != lifecycle_status
    {
        return Err(PersistenceError::InvalidData(
            "persisted aggregate metadata does not match its storage key".into(),
        ));
    }
    Ok(metadata)
}

fn ensure_declared_source_references<A: OperationsAggregate>(
    aggregate: &A,
    source_refs: &[SourceTransactionRef],
) -> Result<(), PersistenceError> {
    let mut declared = aggregate.source_references().to_vec();
    let mut supplied = source_refs.to_vec();
    let source_key = |source: &SourceTransactionRef| {
        (
            source.source_domain.clone(),
            source.source_type.clone(),
            source.source_id,
            source.source_event_id,
        )
    };
    declared.sort_by_key(&source_key);
    supplied.sort_by_key(source_key);
    if declared != supplied {
        return Err(PersistenceError::InvalidData(
            "persisted source references must exactly match the aggregate provenance".into(),
        ));
    }
    Ok(())
}

fn validate_event<A: OperationsAggregate>(
    event: &OperationsEvent,
    aggregate: &A,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    let metadata = validate_aggregate(aggregate, holding_ref)?;
    if event.envelope.tenant_scope != tenant_scope_for_operations_scope(&metadata.scope) {
        return Err(PersistenceError::InvalidData(
            "operations event tenant scope does not match its aggregate scope".into(),
        ));
    }
    if event.envelope.contract_version != OPERATIONS_CONTRACT_VERSION
        || event.envelope.event_id.is_nil()
        || event.envelope.event_type.trim().is_empty()
        || event.envelope.event_version == 0
        || event.envelope.aggregate_version == 0
        || event.envelope.aggregate_type != A::AGGREGATE_TYPE
        || event.envelope.aggregate_id != metadata.id
        || metadata.version != event.envelope.aggregate_version
        || event.envelope.payload
            != aggregate
                .canonical_json()
                .map_err(|error| invalid_domain("event payload", error))?
    {
        return Err(PersistenceError::InvalidData(
            "operations event does not match its typed aggregate".into(),
        ));
    }
    validate_uuid(event.envelope.correlation_id, "correlation_id")?;
    if let Some(key) = event.envelope.idempotency_key.as_deref() {
        validate_non_empty(key, "idempotency_key")?;
    }
    validate_projection_metadata(&event.projection, event)?;
    Ok(())
}

fn tenant_scope_for_operations_scope(scope: &erpx_domain::access::Scope) -> TenantScope {
    match scope {
        erpx_domain::access::Scope::Platform => TenantScope::Platform,
        erpx_domain::access::Scope::Holding { holding_id } => TenantScope::Holding {
            holding_id: *holding_id,
        },
        erpx_domain::access::Scope::Company {
            holding_id,
            company_id,
        } => TenantScope::Company {
            holding_id: *holding_id,
            company_id: *company_id,
        },
        erpx_domain::access::Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => TenantScope::Branch {
            holding_id: *holding_id,
            company_id: *company_id,
            branch_id: *branch_id,
        },
        erpx_domain::access::Scope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => TenantScope::Subtree {
            holding_id: *holding_id,
            company_ids: company_ids.clone(),
            branch_ids: branch_ids.clone(),
        },
    }
}

fn source_event_for_aggregate<A: OperationsAggregate>(
    source_event: &OperationsSourceEvent,
    aggregate: &A,
    holding_ref: &str,
) -> Result<OperationsEvent, PersistenceError> {
    source_event
        .validate()
        .map_err(|error| invalid_domain("source event", error))?;
    let metadata = validate_aggregate(aggregate, holding_ref)?;
    source_event
        .validate_for_aggregate_scope(&metadata.scope)
        .map_err(|error| invalid_domain("source event scope", error))?;
    if source_event.holding_ref != holding_ref {
        return Err(PersistenceError::InvalidData(
            "source event holding reference does not match the routed tenant".into(),
        ));
    }
    let source_reference = source_event
        .source_reference()
        .map_err(|error| invalid_domain("source event reference", error))?;
    if !aggregate.source_references().contains(&source_reference) {
        return Err(PersistenceError::InvalidData(
            "source event reference is not declared by the Operations aggregate".into(),
        ));
    }
    let idempotency_key = source_event
        .idempotency_key()
        .map_err(|error| invalid_domain("source event idempotency", error))?;
    let event_type = source_event
        .operations_event_type()
        .map_err(|error| invalid_domain("source event type", error))?;
    let mut event = event_for(
        aggregate,
        event_type,
        source_event.event_version,
        source_event.actor.clone(),
        Some(idempotency_key),
        source_event.correlation_id,
        Some(source_event.source_event_id),
        source_event.occurred_at,
    )
    .map_err(|error| invalid_domain("source event application", error))?;
    // Keep source provenance and the Operations application event in distinct
    // identity spaces. A source-owned outbox row may already use the source
    // event ID as its own envelope identity; reusing it here would make a
    // scheduled detection collide with the source row instead of appending an
    // Operations event. The derived value is deterministic across retries and
    // bound to the source, aggregate, and version.
    let operations_event_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!(
            "erpx:operations-source-application:{}:{}:{}:{}",
            source_event.source_event_id,
            A::AGGREGATE_TYPE,
            metadata.id,
            metadata.version
        )
        .as_bytes(),
    );
    event.envelope.event_id = operations_event_id;
    event.projection.event_id = operations_event_id;
    Ok(event)
}

fn validate_projection_metadata(
    projection: &ProjectionEventMetadata,
    event: &OperationsEvent,
) -> Result<(), PersistenceError> {
    projection
        .validate()
        .map_err(|error| invalid_domain("projection metadata", error))?;
    if projection.event_id != event.envelope.event_id
        || projection.aggregate_type != event.envelope.aggregate_type
        || projection.aggregate_id != event.envelope.aggregate_id
        || projection.aggregate_version != event.envelope.aggregate_version
    {
        return Err(PersistenceError::InvalidData(
            "projection metadata does not match the event envelope".into(),
        ));
    }
    validate_projection_name(&projection.projection_name)
}

fn normalized_state<A: OperationsAggregate>(
    aggregate: &A,
    version: u64,
) -> Result<Value, PersistenceError> {
    let mut state = aggregate
        .canonical_json()
        .map_err(|error| invalid_domain("aggregate state", error))?;
    let metadata = state
        .get_mut("meta")
        .and_then(Value::as_object_mut)
        .ok_or_else(|| PersistenceError::InvalidData("aggregate state has no metadata".into()))?;
    metadata.insert("version".into(), json!(version));
    Ok(state)
}

async fn current_snapshot_version(
    tx: &mut Transaction<'_, Postgres>,
    aggregate_type: &str,
    aggregate_id: Uuid,
) -> Result<Option<u64>, PersistenceError> {
    sqlx::query(
        "SELECT pg_advisory_xact_lock(
                    hashtextextended($1 || ':' || $2::text, 0)
                )",
    )
    .bind(aggregate_type)
    .bind(aggregate_id)
    .execute(&mut **tx)
    .await?;
    let row = sqlx::query(
        "SELECT aggregate_version
         FROM erpx_tenant.operations_aggregate_snapshots
         WHERE aggregate_type = $1 AND aggregate_id = $2
         FOR UPDATE",
    )
    .bind(aggregate_type)
    .bind(aggregate_id)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|row| positive_u64(row.try_get("aggregate_version")?, "aggregate version"))
        .transpose()
}

async fn append_operations_outbox_event(
    tx: &mut Transaction<'_, Postgres>,
    envelope: &EventEnvelope,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, tenant_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8,
                 $9, $10, $11, $12, $13, $14, $15)
         ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(envelope.event_id)
    .bind(&envelope.aggregate_type)
    .bind(envelope.aggregate_id)
    .bind(holding_ref)
    .bind(as_i64(
        envelope.aggregate_version,
        "Operations outbox aggregate version",
    )?)
    .bind(&envelope.event_type)
    .bind(&envelope.payload)
    .bind(envelope.occurred_at)
    .bind(i32::from(envelope.contract_version))
    .bind(i32::from(envelope.event_version))
    .bind(serde_json::to_value(&envelope.tenant_scope)?)
    .bind(serde_json::to_value(&envelope.actor)?)
    .bind(envelope.correlation_id)
    .bind(envelope.causation_id)
    .bind(&envelope.idempotency_key)
    .execute(&mut **tx)
    .await?;
    ensure_operations_outbox_matches_envelope(tx, envelope, holding_ref).await?;
    Ok(())
}

async fn append_existing_operations_event_outbox(
    tx: &mut Transaction<'_, Postgres>,
    event_id: Uuid,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, tenant_scope, actor,
             correlation_id, causation_id, idempotency_key)
         SELECT event_id, aggregate_type, aggregate_id, holding_ref,
                aggregate_version, event_type, payload, occurred_at,
                contract_version, event_version, tenant_scope, actor,
                correlation_id, causation_id, idempotency_key
         FROM erpx_tenant.operations_events
         WHERE event_id = $1 AND holding_ref = $2
         ON CONFLICT (event_id) DO NOTHING",
    )
    .bind(event_id)
    .bind(holding_ref)
    .execute(&mut **tx)
    .await?;
    let mismatch: bool = sqlx::query_scalar(
        "SELECT EXISTS (
             SELECT 1
             FROM erpx_tenant.operations_events e
             JOIN erpx_tenant.outbox_events o ON o.event_id = e.event_id
             WHERE e.event_id = $1
               AND e.holding_ref = $2
               AND (
                    o.aggregate_type IS DISTINCT FROM e.aggregate_type
                 OR o.aggregate_id IS DISTINCT FROM e.aggregate_id
                 OR o.holding_ref IS DISTINCT FROM e.holding_ref
                 OR o.aggregate_version IS DISTINCT FROM e.aggregate_version
                 OR o.event_type IS DISTINCT FROM e.event_type
                 OR o.payload IS DISTINCT FROM e.payload
                 OR o.occurred_at IS DISTINCT FROM e.occurred_at
                 OR o.contract_version IS DISTINCT FROM e.contract_version
                 OR o.event_version IS DISTINCT FROM e.event_version
                 OR o.tenant_scope IS DISTINCT FROM e.tenant_scope
                 OR o.actor IS DISTINCT FROM e.actor
                 OR o.correlation_id IS DISTINCT FROM e.correlation_id
                 OR o.causation_id IS DISTINCT FROM e.causation_id
                 OR o.idempotency_key IS DISTINCT FROM e.idempotency_key
               )
         )",
    )
    .bind(event_id)
    .bind(holding_ref)
    .fetch_one(&mut **tx)
    .await?;
    if mismatch {
        return Err(PersistenceError::InvalidData(
            "Operations outbox event ID replay did not match".into(),
        ));
    }
    Ok(())
}

async fn ensure_operations_outbox_matches_envelope(
    tx: &mut Transaction<'_, Postgres>,
    envelope: &EventEnvelope,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT aggregate_type, aggregate_id, holding_ref, aggregate_version,
                event_type, payload, occurred_at, contract_version, event_version,
                tenant_scope, actor, correlation_id, causation_id, idempotency_key
         FROM erpx_tenant.outbox_events WHERE event_id = $1",
    )
    .bind(envelope.event_id)
    .fetch_one(&mut **tx)
    .await?;
    let matches = row.try_get::<String, _>("aggregate_type")? == envelope.aggregate_type
        && row.try_get::<Uuid, _>("aggregate_id")? == envelope.aggregate_id
        && row.try_get::<String, _>("holding_ref")? == holding_ref
        && positive_u64(
            row.try_get("aggregate_version")?,
            "Operations outbox version",
        )? == envelope.aggregate_version
        && row.try_get::<String, _>("event_type")? == envelope.event_type
        && row.try_get::<Value, _>("payload")? == envelope.payload
        && row.try_get::<DateTime<Utc>, _>("occurred_at")? == envelope.occurred_at
        && row.try_get::<Option<i32>, _>("contract_version")?
            == Some(i32::from(envelope.contract_version))
        && row.try_get::<Option<i32>, _>("event_version")?
            == Some(i32::from(envelope.event_version))
        && row.try_get::<Option<Value>, _>("tenant_scope")?
            == Some(serde_json::to_value(&envelope.tenant_scope)?)
        && row.try_get::<Option<Value>, _>("actor")?
            == Some(serde_json::to_value(&envelope.actor)?)
        && row.try_get::<Option<Uuid>, _>("correlation_id")? == Some(envelope.correlation_id)
        && row.try_get::<Option<Uuid>, _>("causation_id")? == envelope.causation_id
        && row.try_get::<Option<String>, _>("idempotency_key")? == envelope.idempotency_key;
    if !matches {
        return Err(PersistenceError::InvalidData(
            "Operations outbox event ID replay did not match".into(),
        ));
    }
    Ok(())
}

async fn write_snapshot(
    tx: &mut Transaction<'_, Postgres>,
    metadata: &AggregateMetadata,
    aggregate_type: &str,
    aggregate_version: u64,
    scope: Value,
    lifecycle_status: &str,
    state: Value,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.operations_aggregate_snapshots
            (aggregate_type, aggregate_id, holding_ref, scope, aggregate_version,
             lifecycle_status, state, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
         ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
            holding_ref = EXCLUDED.holding_ref,
            scope = EXCLUDED.scope,
            aggregate_version = EXCLUDED.aggregate_version,
            lifecycle_status = EXCLUDED.lifecycle_status,
            state = EXCLUDED.state,
            updated_at = EXCLUDED.updated_at",
    )
    .bind(aggregate_type)
    .bind(metadata.id)
    .bind(&metadata.holding_ref)
    .bind(scope)
    .bind(as_i64(aggregate_version, "aggregate version")?)
    .bind(lifecycle_status)
    .bind(state)
    .bind(metadata.created_at)
    .bind(metadata.updated_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn write_source_references(
    tx: &mut Transaction<'_, Postgres>,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    source_refs: &[SourceTransactionRef],
    operations_event_id: Option<Uuid>,
    holding_ref: &str,
) -> Result<(), PersistenceError> {
    for source in source_refs {
        validate_source_reference(source)?;
        sqlx::query(
            "INSERT INTO erpx_tenant.operations_source_references
                (aggregate_type, aggregate_id, aggregate_version, holding_ref,
                 source_domain, source_type, source_id, source_event_id,
                 operations_event_id)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
             ON CONFLICT (aggregate_type, aggregate_id, source_event_id) DO NOTHING",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(as_i64(
            aggregate_version,
            "source reference aggregate version",
        )?)
        .bind(holding_ref)
        .bind(&source.source_domain)
        .bind(&source.source_type)
        .bind(source.source_id)
        .bind(source.source_event_id)
        .bind(operations_event_id)
        .execute(&mut **tx)
        .await?;

        let row = sqlx::query(
            "SELECT source_domain, source_type, source_id
             FROM erpx_tenant.operations_source_references
             WHERE aggregate_type = $1 AND aggregate_id = $2 AND source_event_id = $3",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(source.source_event_id)
        .fetch_one(&mut **tx)
        .await?;
        if row.try_get::<String, _>("source_domain")? != source.source_domain
            || row.try_get::<String, _>("source_type")? != source.source_type
            || row.try_get::<Uuid, _>("source_id")? != source.source_id
        {
            return Err(PersistenceError::InvalidData(
                "source event reference replay did not match the persisted reference".into(),
            ));
        }
    }
    Ok(())
}

fn validate_source_reference(source: &SourceTransactionRef) -> Result<(), PersistenceError> {
    SourceTransactionRef::new(
        source.source_domain.clone(),
        source.source_type.clone(),
        source.source_id,
        source.source_event_id,
    )
    .map(|_| ())
    .map_err(|error| invalid_domain("source reference", error))
}

fn ensure_expected_version(
    aggregate_type: &str,
    aggregate_id: Uuid,
    expected: Option<u64>,
    actual: Option<u64>,
) -> Result<(), PersistenceError> {
    if expected != actual {
        return Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected,
            actual,
        });
    }
    Ok(())
}

fn event_row_matches(
    row: &sqlx::postgres::PgRow,
    event: &OperationsEvent,
) -> Result<bool, PersistenceError> {
    Ok(
        row.try_get::<Uuid, _>("event_id")? == event.envelope.event_id
            && row.try_get::<i32, _>("contract_version")?
                == i32::from(event.envelope.contract_version)
            && row.try_get::<i32, _>("event_version")? == i32::from(event.envelope.event_version)
            && row.try_get::<DateTime<Utc>, _>("occurred_at")? == event.envelope.occurred_at
            && row.try_get::<String, _>("aggregate_type")? == event.envelope.aggregate_type
            && row.try_get::<Uuid, _>("aggregate_id")? == event.envelope.aggregate_id
            && positive_u64(
                row.try_get("aggregate_version")?,
                "operations event aggregate version",
            )? == event.envelope.aggregate_version
            && row.try_get::<String, _>("event_type")? == event.envelope.event_type
            && row.try_get::<Value, _>("tenant_scope")?
                == serde_json::to_value(&event.envelope.tenant_scope)?
            && row.try_get::<Value, _>("actor")? == serde_json::to_value(&event.envelope.actor)?
            && row.try_get::<Uuid, _>("correlation_id")? == event.envelope.correlation_id
            && row.try_get::<Option<Uuid>, _>("causation_id")? == event.envelope.causation_id
            && row.try_get::<Option<String>, _>("idempotency_key")?
                == event.envelope.idempotency_key
            && row.try_get::<Value, _>("payload")? == event.envelope.payload,
    )
}

fn projection_result(event: &OperationsEvent, applied: bool) -> OperationsProjectionApplication {
    OperationsProjectionApplication {
        projection_name: event.projection.projection_name.clone(),
        event_id: event.projection.event_id,
        aggregate_type: event.projection.aggregate_type.clone(),
        aggregate_id: event.projection.aggregate_id,
        aggregate_version: event.projection.aggregate_version,
        applied,
    }
}

fn event_record_from_row(
    row: sqlx::postgres::PgRow,
    expected_holding_ref: &str,
) -> Result<OperationsEventRecord, PersistenceError> {
    let holding_ref: String = row.try_get("holding_ref")?;
    let record = OperationsEventRecord {
        envelope: EventEnvelope {
            contract_version: positive_u16(row.try_get("contract_version")?, "contract version")?,
            event_id: row.try_get("event_id")?,
            event_type: row.try_get("event_type")?,
            event_version: positive_u16(row.try_get("event_version")?, "event version")?,
            occurred_at: row.try_get("occurred_at")?,
            aggregate_type: row.try_get("aggregate_type")?,
            aggregate_id: row.try_get("aggregate_id")?,
            aggregate_version: positive_u64(
                row.try_get("aggregate_version")?,
                "event aggregate version",
            )?,
            tenant_scope: serde_json::from_value(row.try_get("tenant_scope")?)?,
            actor: serde_json::from_value(row.try_get("actor")?)?,
            correlation_id: row.try_get("correlation_id")?,
            causation_id: row.try_get("causation_id")?,
            idempotency_key: row.try_get("idempotency_key")?,
            payload: row.try_get("payload")?,
        },
        holding_ref,
        applied_at: row.try_get("applied_at")?,
    };
    validate_event_record(&record, expected_holding_ref)?;
    Ok(record)
}

fn validate_event_record(
    record: &OperationsEventRecord,
    expected_holding_ref: &str,
) -> Result<(), PersistenceError> {
    record.validate_for_holding(expected_holding_ref)?;
    if record.envelope.contract_version == 0
        || record.envelope.event_id.is_nil()
        || record.envelope.event_type.trim().is_empty()
        || record.envelope.event_version == 0
        || record.envelope.aggregate_id.is_nil()
        || record.envelope.aggregate_type.trim().is_empty()
        || record.envelope.correlation_id.is_nil()
    {
        return Err(PersistenceError::InvalidData(
            "operations event record contains invalid envelope metadata".into(),
        ));
    }
    if let Some(key) = record.envelope.idempotency_key.as_deref() {
        validate_non_empty(key, "idempotency_key")?;
    }
    Ok(())
}

fn snapshot_from_row(
    row: sqlx::postgres::PgRow,
) -> Result<OperationsSnapshotRecord, PersistenceError> {
    Ok(OperationsSnapshotRecord {
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id: row.try_get("aggregate_id")?,
        holding_ref: row.try_get("holding_ref")?,
        scope: row.try_get("scope")?,
        aggregate_version: positive_u64(row.try_get("aggregate_version")?, "aggregate version")?,
        lifecycle_status: row.try_get("lifecycle_status")?,
        state: row.try_get("state")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn checkpoint_from_row(
    row: sqlx::postgres::PgRow,
    allow_zero_version: bool,
) -> Result<OperationsProjectionCheckpoint, PersistenceError> {
    let raw_version: i64 = row.try_get("last_aggregate_version")?;
    let last_aggregate_version = if allow_zero_version {
        nonnegative_u64(raw_version, "projection checkpoint version")?
    } else {
        positive_u64(raw_version, "projection checkpoint version")?
    };
    Ok(OperationsProjectionCheckpoint {
        projection_name: row.try_get("projection_name")?,
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id: row.try_get("aggregate_id")?,
        holding_ref: row.try_get("holding_ref")?,
        last_event_id: row.try_get("last_event_id")?,
        last_aggregate_version,
        projected_at: row.try_get("projected_at")?,
    })
}

fn next_version(expected_version: Option<u64>) -> Result<u64, PersistenceError> {
    expected_version
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| PersistenceError::InvalidData("aggregate version overflow".into()))
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds database range")))
}

fn positive_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is invalid")))
}

fn nonnegative_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is invalid")))
}

fn positive_count(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is invalid")))
}

fn signed_count_delta(erpx_count: u64, legacy_count: u64) -> i64 {
    if erpx_count >= legacy_count {
        i64::try_from(erpx_count - legacy_count).unwrap_or(i64::MAX)
    } else {
        i64::try_from(legacy_count - erpx_count)
            .map(|delta| -delta)
            .unwrap_or(i64::MIN)
    }
}

fn positive_u16(value: i32, field: &'static str) -> Result<u16, PersistenceError> {
    u16::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is invalid")))
}

fn bounded_page_size(page_size: u32) -> Result<u32, PersistenceError> {
    if page_size == 0 {
        return Err(PersistenceError::InvalidData(
            "event page size must be positive".into(),
        ));
    }
    Ok(page_size.min(MAX_OPERATIONS_EVENT_PAGE_SIZE))
}

fn bounded_exception_page_size(page_size: u32) -> Result<u32, PersistenceError> {
    if page_size == 0 {
        return Err(PersistenceError::InvalidData(
            "exception page size must be positive".into(),
        ));
    }
    Ok(page_size.min(MAX_OPERATIONS_EXCEPTION_PAGE_SIZE))
}

fn bounded_source_event_page_size(page_size: u32) -> Result<u32, PersistenceError> {
    if page_size == 0 {
        return Err(PersistenceError::InvalidData(
            "source event page size must be positive".into(),
        ));
    }
    Ok(page_size.min(MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE))
}

fn source_reference_from_row(
    row: sqlx::postgres::PgRow,
    expected_holding_ref: &str,
) -> Result<OperationsSourceReferenceRecord, PersistenceError> {
    let holding_ref: String = row.try_get("holding_ref")?;
    if holding_ref != expected_holding_ref {
        return Err(PersistenceError::InvalidData(
            "source reference holding reference does not match the routed tenant".into(),
        ));
    }
    let source_domain = row.try_get::<String, _>("source_domain")?;
    let source_domain = OperationsSourceDomain::parse(&source_domain)
        .map_err(|error| invalid_domain("source reference domain", error))?;
    let source = SourceTransactionRef::new(
        source_domain.as_str(),
        row.try_get::<String, _>("source_type")?,
        row.try_get("source_id")?,
        row.try_get("source_event_id")?,
    )
    .map_err(|error| invalid_domain("source reference", error))?;
    Ok(OperationsSourceReferenceRecord {
        aggregate_type: row.try_get("aggregate_type")?,
        aggregate_id: row.try_get("aggregate_id")?,
        aggregate_version: positive_u64(
            row.try_get("aggregate_version")?,
            "source reference aggregate version",
        )?,
        holding_ref,
        source,
        operations_event_id: row.try_get("operations_event_id")?,
        recorded_at: row.try_get("recorded_at")?,
    })
}

async fn ensure_sla_notification_delivery_outbox_matches(
    tx: &mut Transaction<'_, Postgres>,
    event_id: Uuid,
    notification: &OperationsSlaNotification,
    payload: &Value,
    tenant_scope: &Value,
    actor: &Value,
    correlation_id: Uuid,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT event_id, aggregate_type, aggregate_id, holding_ref,
                aggregate_version, event_type, payload, occurred_at,
                contract_version, event_version, tenant_scope, actor,
                correlation_id, causation_id, idempotency_key
         FROM erpx_tenant.outbox_events
         WHERE event_id = $1",
    )
    .bind(event_id)
    .fetch_one(&mut **tx)
    .await?;
    let matches = row.try_get::<Uuid, _>("event_id")? == event_id
        && row.try_get::<String, _>("aggregate_type")? == OPERATIONS_NOTIFICATION_AGGREGATE_TYPE
        && row.try_get::<Uuid, _>("aggregate_id")? == notification.exception_id
        && row.try_get::<String, _>("holding_ref")? == notification.holding_ref
        && positive_u64(
            row.try_get("aggregate_version")?,
            "notification handoff version",
        )? == notification.exception_version
        && row.try_get::<String, _>("event_type")? == OPERATIONS_NOTIFICATION_DELIVERY_EVENT_TYPE
        && row.try_get::<Value, _>("payload")? == payload.clone()
        && row.try_get::<DateTime<Utc>, _>("occurred_at")? == notification.not_before
        && row.try_get::<Option<i32>, _>("contract_version")?
            == Some(i32::from(OPERATIONS_CONTRACT_VERSION))
        && row.try_get::<Option<i32>, _>("event_version")? == Some(1)
        && row.try_get::<Option<Value>, _>("tenant_scope")? == Some(tenant_scope.clone())
        && row.try_get::<Option<Value>, _>("actor")? == Some(actor.clone())
        && row.try_get::<Option<Uuid>, _>("correlation_id")? == Some(correlation_id)
        && row.try_get::<Option<Uuid>, _>("causation_id")?.is_none()
        && row.try_get::<Option<String>, _>("idempotency_key")?
            == Some(notification.notification_key.clone());
    if !matches {
        return Err(PersistenceError::InvalidData(
            "Operations notification outbox envelope does not match authoritative state".into(),
        ));
    }
    Ok(())
}

/// Notification delivery is a durable Operations-owned handoff, but it is not
/// an Operations aggregate event and therefore has no row in
/// `operations_events`. Reconciliation may accept it only when its complete
/// deterministic envelope still matches the durable notification inbox. A
/// forged or tampered handoff remains an outbox mismatch.
fn valid_sla_notification_delivery_handoff(
    row: &sqlx::postgres::PgRow,
) -> Result<bool, PersistenceError> {
    let notification_key: Option<String> = row.try_get("notification_key")?;
    let Some(notification_key) = notification_key else {
        return Ok(false);
    };
    let notification_holding_ref: Option<String> = row.try_get("notification_holding_ref")?;
    let notification_exception_id: Option<Uuid> = row.try_get("notification_exception_id")?;
    let notification_exception_version: Option<i64> =
        row.try_get("notification_exception_version")?;
    let notification_recipient_ref: Option<String> = row.try_get("notification_recipient_ref")?;
    let notification_kind: Option<String> = row.try_get("notification_kind")?;
    let notification_not_before: Option<DateTime<Utc>> = row.try_get("notification_not_before")?;
    let notification_status: Option<String> = row.try_get("notification_status")?;
    let (
        Some(notification_exception_id),
        Some(notification_exception_version),
        Some(notification_recipient_ref),
        Some(notification_kind),
        Some(notification_not_before),
        Some(notification_holding_ref),
    ) = (
        notification_exception_id,
        notification_exception_version,
        notification_recipient_ref,
        notification_kind,
        notification_not_before,
        notification_holding_ref,
    )
    else {
        return Ok(false);
    };
    if !valid_notification_handoff_status(notification_status.as_deref()) {
        return Ok(false);
    }
    let notification_exception_version = match u64::try_from(notification_exception_version) {
        Ok(version) if version > 0 => version,
        _ => return Ok(false),
    };
    let expected_event_id = Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("operations-sla-notification-delivered:{notification_key}").as_bytes(),
    );
    let expected_correlation_id = Uuid::new_v5(&Uuid::NAMESPACE_URL, notification_key.as_bytes());
    let expected_payload = json!({
        "notification_key": &notification_key,
        "exception_id": notification_exception_id,
        "exception_version": notification_exception_version,
        "recipient_ref": notification_recipient_ref,
        "kind": notification_kind,
    });
    let actor: Option<ActorAttribution> = row
        .try_get::<Option<Value>, _>("actor")?
        .and_then(|value| serde_json::from_value(value).ok());
    let actor_is_valid = actor.as_ref().is_some_and(|actor| {
        actor.kind == ActorKind::SystemWorker
            && !actor.principal_ref.trim().is_empty()
            && actor.principal_ref.len() <= 255
            && !actor.principal_ref.chars().any(char::is_control)
    });
    Ok(row.try_get::<Uuid, _>("event_id")? == expected_event_id
        && row.try_get::<String, _>("aggregate_type")? == OPERATIONS_NOTIFICATION_AGGREGATE_TYPE
        && row.try_get::<Uuid, _>("aggregate_id")? == notification_exception_id
        && row.try_get::<String, _>("holding_ref")? == notification_holding_ref
        && positive_u64(
            row.try_get("aggregate_version")?,
            "notification handoff version",
        )? == notification_exception_version
        && row.try_get::<String, _>("event_type")? == OPERATIONS_NOTIFICATION_DELIVERY_EVENT_TYPE
        && row.try_get::<Value, _>("payload")? == expected_payload
        && row.try_get::<DateTime<Utc>, _>("occurred_at")? == notification_not_before
        && row.try_get::<Option<i32>, _>("contract_version")?
            == Some(i32::from(OPERATIONS_CONTRACT_VERSION))
        && row.try_get::<Option<i32>, _>("event_version")? == Some(1)
        && row.try_get::<Option<Value>, _>("tenant_scope")?
            == Some(serde_json::to_value(TenantScope::Platform)?)
        && actor_is_valid
        && row.try_get::<Option<Uuid>, _>("correlation_id")? == Some(expected_correlation_id)
        && row.try_get::<Option<Uuid>, _>("causation_id")?.is_none()
        && row.try_get::<Option<String>, _>("idempotency_key")? == Some(notification_key))
}

fn valid_notification_handoff_status(status: Option<&str>) -> bool {
    matches!(status, Some("claimed") | Some("delivered"))
}

fn source_outbox_event_from_row(
    row: sqlx::postgres::PgRow,
    expected_holding_ref: &str,
) -> Result<OperationsSourceOutboxEvent, PersistenceError> {
    let holding_ref: String = row.try_get("holding_ref")?;
    if holding_ref != expected_holding_ref {
        return Err(PersistenceError::InvalidData(
            "source outbox holding reference does not match the routed tenant".into(),
        ));
    }
    let event_type: String = row.try_get("event_type")?;
    let aggregate_type: String = row.try_get("aggregate_type")?;
    let source_domain = source_domain_for_outbox_event(&event_type, &aggregate_type)?;
    let contract_version = row
        .try_get::<Option<i32>, _>("contract_version")?
        .ok_or_else(|| {
            PersistenceError::InvalidData(
                "source outbox event is missing its authoritative contract version".into(),
            )
        })?;
    if contract_version != i32::from(CONTRACT_VERSION) {
        return Err(PersistenceError::InvalidData(
            "source outbox event contract version is unsupported".into(),
        ));
    }
    let source_event_type = event_type
        .strip_prefix(source_domain.as_str())
        .and_then(|value| value.strip_prefix('.'))
        .filter(|value| !value.is_empty())
        .unwrap_or(&event_type)
        .to_string();
    let tenant_scope: Option<Value> = row.try_get("tenant_scope")?;
    let tenant_scope = tenant_scope.ok_or_else(|| {
        PersistenceError::InvalidData(
            "source outbox event is missing its authoritative tenant scope".into(),
        )
    })?;
    let scope = operations_scope_from_tenant_scope(tenant_scope.clone())?;
    let actor: Option<Value> = row.try_get("actor")?;
    let actor: ActorAttribution = actor
        .ok_or_else(|| {
            PersistenceError::InvalidData(
                "source outbox event is missing its authoritative actor".into(),
            )
        })
        .and_then(|value| serde_json::from_value(value).map_err(PersistenceError::from))?;
    let event_version = positive_u16(
        row.try_get::<Option<i32>, _>("event_version")?
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "source outbox event is missing its authoritative event version".into(),
                )
            })?,
        "source outbox event version",
    )?;
    let correlation_id: Option<Uuid> = row.try_get("correlation_id")?;
    let correlation_id = correlation_id.ok_or_else(|| {
        PersistenceError::InvalidData(
            "source outbox event is missing its authoritative correlation ID".into(),
        )
    })?;
    let source_event = OperationsSourceEvent {
        source_domain,
        source_type: aggregate_type,
        source_id: row.try_get("aggregate_id")?,
        source_event_id: row.try_get("event_id")?,
        source_version: positive_u64(row.try_get("aggregate_version")?, "source outbox version")?,
        event_type: source_event_type,
        event_version,
        holding_ref,
        scope,
        actor,
        correlation_id,
        causation_id: row.try_get("causation_id")?,
        occurred_at: row.try_get("occurred_at")?,
        payload: row.try_get("payload")?,
    };
    source_event
        .validate()
        .map_err(|error| invalid_domain("source outbox event", error))?;
    Ok(OperationsSourceOutboxEvent { source_event })
}

fn source_events_match_authority(
    presented: &OperationsSourceEvent,
    authoritative: &OperationsSourceEvent,
) -> bool {
    presented.source_domain == authoritative.source_domain
        && presented.source_type == authoritative.source_type
        && presented.source_id == authoritative.source_id
        && presented.source_event_id == authoritative.source_event_id
        && presented.source_version == authoritative.source_version
        && (presented.event_type == authoritative.event_type
            || authoritative
                .event_type
                .split_once('.')
                .is_some_and(|(_, suffix)| suffix == presented.event_type))
        && presented.event_version == authoritative.event_version
        && presented.holding_ref == authoritative.holding_ref
        && presented.scope == authoritative.scope
        && presented.actor == authoritative.actor
        && presented.correlation_id == authoritative.correlation_id
        && presented.causation_id == authoritative.causation_id
        && presented.occurred_at == authoritative.occurred_at
        && presented.payload == authoritative.payload
}

fn validate_authoritative_source_detection_cursor(
    source_event: &OperationsSourceEvent,
    cursor: &OperationsSourceOutboxCursor,
    expected_holding_ref: &str,
) -> Result<(), PersistenceError> {
    if source_event.holding_ref != expected_holding_ref
        || source_event.source_event_id != cursor.source_event_id
        || source_event.occurred_at != cursor.occurred_at
    {
        return Err(PersistenceError::InvalidData(
            "Operations source detection cursor does not match its authoritative source event"
                .into(),
        ));
    }
    Ok(())
}

fn source_domain_for_outbox_event(
    event_type: &str,
    aggregate_type: &str,
) -> Result<OperationsSourceDomain, PersistenceError> {
    let prefix = event_type.split('.').next().unwrap_or_default();
    match prefix {
        "sales" => Ok(OperationsSourceDomain::Sales),
        "scm"
            if aggregate_type.starts_with("purchase")
                || aggregate_type == "goods_receipt"
                || aggregate_type == "purchase_request" =>
        {
            Ok(OperationsSourceDomain::Procurement)
        }
        "scm" => Ok(OperationsSourceDomain::Inventory),
        "finance" => Ok(OperationsSourceDomain::Finance),
        "asset" | "maintenance" => Ok(OperationsSourceDomain::Maintenance),
        "support" => Ok(OperationsSourceDomain::Support),
        "integration" | "integrations" | "marketing" | "crm" => {
            Ok(OperationsSourceDomain::Integrations)
        }
        _ => Err(PersistenceError::InvalidData(format!(
            "source outbox event domain is not allowlisted: {prefix}"
        ))),
    }
}

fn operations_scope_from_tenant_scope(value: Value) -> Result<Scope, PersistenceError> {
    let scope: TenantScope = serde_json::from_value(value)?;
    Ok(match scope {
        TenantScope::Platform => Scope::Platform,
        TenantScope::Holding { holding_id } => Scope::Holding { holding_id },
        TenantScope::Company {
            holding_id,
            company_id,
        } => Scope::Company {
            holding_id,
            company_id,
        },
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => Scope::Branch {
            holding_id,
            company_id,
            branch_id,
        },
        TenantScope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        } => Scope::Subtree {
            holding_id,
            company_ids,
            branch_ids,
        },
    })
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    validate_opaque_reference(value, "holding_ref")
}

fn require_holding_scope(
    scope: &erpx_domain::access::Scope,
    operation: &'static str,
) -> Result<(), PersistenceError> {
    if matches!(scope, erpx_domain::access::Scope::Holding { .. }) {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(format!(
            "{operation} requires an explicit holding scope"
        )))
    }
}

fn validate_projection_name(value: &str) -> Result<(), PersistenceError> {
    validate_opaque_reference(value, "projection_name")
}

/// Validate policy source domains before any database read. The transport
/// boundary is allowed to evolve independently, so the tenant persistence
/// boundary must enforce the same closed source-domain contract even when it
/// is called directly by a worker or another in-process adapter.
fn validate_work_queue_policy_domains(domains: &[String]) -> Result<(), PersistenceError> {
    if domains.is_empty() || domains.len() > MAX_OPERATIONS_POLICY_DOMAINS {
        return Err(PersistenceError::InvalidData(
            "work queue policy source domains are outside their bounded range".into(),
        ));
    }
    let mut seen = std::collections::BTreeSet::new();
    for domain in domains {
        let parsed = OperationsSourceDomain::parse(domain)
            .map_err(|error| invalid_domain("work queue policy source domain", error))?;
        if !seen.insert(parsed) {
            return Err(PersistenceError::InvalidData(
                "work queue policy source domains contain a duplicate".into(),
            ));
        }
    }
    Ok(())
}

async fn lock_projection_maintenance(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    projection_name: &str,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "SELECT pg_advisory_xact_lock(
                    hashtextextended($1 || ':' || $2, 0)
                )",
    )
    .bind(holding_ref)
    .bind(projection_name)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn validate_aggregate_type(value: &str) -> Result<(), PersistenceError> {
    validate_opaque_reference(value, "aggregate_type")
}

fn validate_opaque_reference(value: &str, field: &'static str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > 255
        || value
            .chars()
            .any(|character| character.is_control() || matches!(character, '/' | '\\' | ':' | ' '))
    {
        return Err(PersistenceError::InvalidData(format!(
            "{field} must be opaque, path-safe, and non-empty"
        )));
    }
    Ok(())
}

fn validate_non_empty(value: &str, field: &'static str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() {
        return Err(PersistenceError::InvalidData(format!(
            "{field} cannot be empty"
        )));
    }
    Ok(())
}

fn validate_uuid(value: Uuid, field: &'static str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(PersistenceError::InvalidData(format!(
            "{field} cannot be nil"
        )));
    }
    Ok(())
}

fn invalid_domain(field: &'static str, error: impl std::fmt::Display) -> PersistenceError {
    PersistenceError::InvalidData(format!("invalid Operations {field}: {error}"))
}

fn map_exception_command_error(
    error: erpx_domain::operations::OperationsError,
    command: &ExceptionLifecycleCommand,
    actual_version: u64,
) -> PersistenceError {
    match error {
        erpx_domain::operations::OperationsError::ExceptionVersionMismatch { expected, .. } => {
            PersistenceError::OptimisticConcurrency {
                aggregate_type: ExceptionCase::AGGREGATE_TYPE.into(),
                aggregate_id: command.exception_id,
                expected: Some(expected),
                actual: Some(actual_version),
            }
        }
        error => invalid_domain("exception lifecycle command", error),
    }
}

fn map_event_insert_error(error: sqlx::Error) -> PersistenceError {
    if let sqlx::Error::Database(database_error) = &error
        && database_error.constraint() == Some("operations_events_idempotency_key_key")
    {
        return PersistenceError::IdempotencyConflict {
            key: "persisted event idempotency key".into(),
        };
    }
    PersistenceError::Database(error)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::{
        access::Scope,
        contracts::{ActorAttribution, ActorKind, TenantScope},
        operations::{AggregateMetadata, OperationalPlan, SourceTransactionRef, event_for},
    };

    #[test]
    fn exception_queue_page_size_is_positive_and_bounded() {
        assert!(bounded_exception_page_size(0).is_err());
        assert_eq!(
            bounded_exception_page_size(1).expect("minimum page size"),
            1
        );
        assert_eq!(
            bounded_exception_page_size(u32::MAX).expect("large page is capped"),
            MAX_OPERATIONS_EXCEPTION_PAGE_SIZE
        );
    }

    #[test]
    fn notification_claim_is_fenced_to_the_notification_expiry_window() {
        let not_before = Utc::now();
        let notification = OperationsSlaNotification {
            notification_key: "notification-claim-window".into(),
            holding_ref: "holding-notification-window".into(),
            exception_id: Uuid::from_u128(0x7501),
            exception_version: 1,
            recipient_ref: "principal:manager".into(),
            kind: OperationsSlaNotificationKind::SlaEscalated,
            not_before,
            expires_at: not_before + Duration::minutes(5),
            payload: json!({"severity": "high"}),
        };
        let valid = ClaimedOperationsSlaNotification {
            notification: notification.clone(),
            claimed_by: Uuid::from_u128(0x7502),
            claim_token: Uuid::from_u128(0x7503),
            claim_expires_at: not_before + Duration::minutes(1),
        };
        assert!(validate_operations_notification_claim(&valid).is_ok());

        let mut outside_window = valid;
        outside_window.claim_expires_at = notification.expires_at + Duration::seconds(1);
        assert!(validate_operations_notification_claim(&outside_window).is_err());
    }

    #[test]
    fn notification_handoff_requires_authoritative_claim_or_delivery_state() {
        assert!(valid_notification_handoff_status(Some("claimed")));
        assert!(valid_notification_handoff_status(Some("delivered")));
        assert!(!valid_notification_handoff_status(Some("pending")));
        assert!(!valid_notification_handoff_status(None));
    }

    #[test]
    fn source_outbox_domain_mapping_is_closed_and_context_aware() {
        assert_eq!(
            source_domain_for_outbox_event("sales.order.confirmed.v1", "sales_order")
                .expect("sales source"),
            OperationsSourceDomain::Sales
        );
        assert_eq!(
            source_domain_for_outbox_event("scm.purchase_order.issued.v1", "purchase_order")
                .expect("procurement source"),
            OperationsSourceDomain::Procurement
        );
        assert_eq!(
            source_domain_for_outbox_event("scm.stock_movement.posted.v1", "stock_movement")
                .expect("inventory source"),
            OperationsSourceDomain::Inventory
        );
        assert_eq!(
            source_domain_for_outbox_event("asset.work_order.closed.v1", "work_order")
                .expect("maintenance source"),
            OperationsSourceDomain::Maintenance
        );
        assert!(source_domain_for_outbox_event("unknown.event.v1", "unknown").is_err());
    }

    #[test]
    fn source_outbox_scope_conversion_preserves_hierarchy_and_rejects_nothing_implicitly() {
        let holding = Uuid::from_u128(0x7400);
        let branch = Uuid::from_u128(0x7401);
        let value = serde_json::to_value(TenantScope::Branch {
            holding_id: holding,
            company_id: Uuid::from_u128(0x7402),
            branch_id: branch,
        })
        .expect("scope JSON");
        assert_eq!(
            operations_scope_from_tenant_scope(value).expect("branch scope"),
            Scope::Branch {
                holding_id: holding,
                company_id: Uuid::from_u128(0x7402),
                branch_id: branch,
            }
        );
    }

    fn source_event_for_test() -> OperationsSourceEvent {
        OperationsSourceEvent {
            source_domain: OperationsSourceDomain::Sales,
            source_type: "sales_order".into(),
            source_id: Uuid::from_u128(0x7301),
            source_event_id: Uuid::from_u128(0x7302),
            source_version: 2,
            event_type: "sales_order.confirmed".into(),
            event_version: 1,
            holding_ref: "holding-operations-contract-test".into(),
            scope: Scope::Holding {
                holding_id: Uuid::from_u128(0x7400),
            },
            actor: ActorAttribution {
                principal_ref: "sales-worker".into(),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            correlation_id: Uuid::from_u128(0x7303),
            causation_id: None,
            occurred_at: Utc::now(),
            payload: json!({"status": "confirmed"}),
        }
    }

    fn operations_plan(at: DateTime<Utc>, id: Uuid) -> OperationalPlan {
        let holding_id = Uuid::from_u128(0x7400);
        let metadata = AggregateMetadata::new(
            id,
            "holding-operations-contract-test",
            Scope::Holding { holding_id },
            at,
        )
        .expect("valid Operations metadata");
        let source = SourceTransactionRef::new(
            "sales",
            "sales_order",
            Uuid::from_u128(0x7301),
            Uuid::from_u128(0x7302),
        )
        .expect("valid source reference");
        OperationalPlan::new(
            metadata,
            "contract test plan",
            at,
            at + chrono::Duration::hours(1),
            vec![Scope::Branch {
                holding_id,
                company_id: Uuid::from_u128(0x7403),
                branch_id: Uuid::from_u128(0x7404),
            }],
            vec![source],
        )
        .expect("valid Operations plan")
    }

    #[test]
    fn persisted_aggregate_metadata_is_bound_to_its_storage_key() {
        let aggregate = operations_plan(Utc::now(), Uuid::from_u128(0x7410));
        assert!(
            validate_persisted_aggregate(
                &aggregate,
                aggregate.meta.id,
                aggregate.meta.version,
                "holding-operations-contract-test",
                aggregate.lifecycle_status(),
            )
            .is_ok()
        );
        assert!(matches!(
            validate_persisted_aggregate(
                &aggregate,
                Uuid::from_u128(0x7411),
                aggregate.meta.version,
                "holding-operations-contract-test",
                aggregate.lifecycle_status(),
            ),
            Err(PersistenceError::InvalidData(message))
                if message.contains("does not match its storage key")
        ));
    }

    #[test]
    fn source_event_application_binds_identity_scope_and_aggregate_provenance() {
        let source_event = source_event_for_test();
        let aggregate = operations_plan(Utc::now(), Uuid::from_u128(0x7401));
        let event = source_event_for_aggregate(
            &source_event,
            &aggregate,
            "holding-operations-contract-test",
        )
        .expect("source event is valid for the Operations aggregate");
        assert_ne!(event.envelope.event_id, source_event.source_event_id);
        assert_eq!(event.projection.event_id, event.envelope.event_id);
        assert_eq!(
            event.envelope.idempotency_key.as_deref(),
            Some("operations-source-event:00000000-0000-0000-0000-000000007302")
        );
        assert_eq!(
            event.envelope.causation_id,
            Some(source_event.source_event_id)
        );
        assert_eq!(
            event.envelope.event_type,
            "operations.source.sales.sales_order.confirmed"
        );
    }

    #[test]
    fn event_application_rejects_a_forged_tenant_scope() {
        let at = Utc::now();
        let aggregate = operations_plan(at, Uuid::from_u128(0x7405));
        let mut event = event_for(
            &aggregate,
            "operations.operational_plan.created",
            1,
            ActorAttribution {
                principal_ref: "operations-worker".into(),
                kind: ActorKind::SystemWorker,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            Some("operations-forged-scope-test".into()),
            Uuid::from_u128(0x7406),
            None,
            at,
        )
        .expect("valid Operations event");
        event.envelope.tenant_scope = TenantScope::Holding {
            holding_id: Uuid::from_u128(0xdead),
        };

        assert!(matches!(
            validate_event(
                &event,
                &aggregate,
                "holding-operations-contract-test"
            ),
            Err(PersistenceError::InvalidData(message))
                if message.contains("tenant scope does not match")
        ));
    }

    #[test]
    fn source_event_application_rejects_scope_or_provenance_mismatch() {
        let mut source_event = source_event_for_test();
        let aggregate = operations_plan(Utc::now(), Uuid::from_u128(0x7402));
        source_event.scope = Scope::Branch {
            holding_id: Uuid::from_u128(0x7400),
            company_id: Uuid::from_u128(0x7403),
            branch_id: Uuid::from_u128(0x7404),
        };
        assert!(matches!(
            source_event_for_aggregate(
                &source_event,
                &aggregate,
                "holding-operations-contract-test"
            ),
            Err(PersistenceError::InvalidData(message))
                if message.contains("source event scope")
        ));

        let mut source_event = source_event_for_test();
        source_event.source_event_id = Uuid::from_u128(0x7304);
        assert!(matches!(
            source_event_for_aggregate(
                &source_event,
                &aggregate,
                "holding-operations-contract-test"
            ),
            Err(PersistenceError::InvalidData(message))
                if message.contains("not declared")
        ));
    }

    #[test]
    fn source_event_page_bound_is_positive_and_capped() {
        assert!(bounded_source_event_page_size(0).is_err());
        assert_eq!(bounded_source_event_page_size(1).unwrap(), 1);
        assert_eq!(
            bounded_source_event_page_size(u32::MAX).unwrap(),
            MAX_OPERATIONS_SOURCE_EVENT_PAGE_SIZE
        );
        assert!(OperationsSourceEventCursor::new(Utc::now(), Uuid::nil()).is_err());
    }

    #[test]
    fn source_detection_cursor_requires_authoritative_event_identity_and_time() {
        let source_event = source_event_for_test();
        let cursor = OperationsSourceOutboxCursor::new(
            source_event.occurred_at,
            source_event.source_event_id,
        )
        .expect("valid source cursor");
        validate_authoritative_source_detection_cursor(
            &source_event,
            &cursor,
            &source_event.holding_ref,
        )
        .expect("cursor must match its source event");

        let forged_time = OperationsSourceOutboxCursor::new(
            source_event.occurred_at + Duration::seconds(1),
            source_event.source_event_id,
        )
        .expect("valid cursor identity");
        assert!(matches!(
            validate_authoritative_source_detection_cursor(
                &source_event,
                &forged_time,
                &source_event.holding_ref,
            ),
            Err(PersistenceError::InvalidData(message))
                if message.contains("does not match its authoritative source event")
        ));

        assert!(matches!(
            validate_authoritative_source_detection_cursor(
                &source_event,
                &cursor,
                "different-holding",
            ),
            Err(PersistenceError::InvalidData(message))
                if message.contains("does not match its authoritative source event")
        ));
    }
}
