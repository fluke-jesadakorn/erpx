//! Transport-neutral support diagnostics.
//!
//! This module is deliberately an allow-list boundary.  Producers may submit
//! untrusted strings through [`DiagnosticsSnapshotInput`], but the resulting
//! [`DiagnosticsSnapshot`] can only contain bounded operational fields.  In
//! particular, this module never stores or serializes raw configuration,
//! error bodies, credentials, or tenant business data.

use std::cmp::Ordering;

use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Row};
use uuid::Uuid;

use crate::{BackupPitrObservation, PersistenceError, ReadinessObservationState};

pub const DIAGNOSTICS_SCHEMA_VERSION: &str = "erpx.support.diagnostics.v1";
pub const DEFAULT_MAX_DIAGNOSTICS_BYTES: usize = 64 * 1024;
const MAX_FIELD_BYTES: usize = 160;
const MAX_COMPONENT_BYTES: usize = 96;
const MIN_MAX_DIAGNOSTICS_BYTES: usize = 1024;
const MAX_VERSIONS: usize = 64;
const MAX_FAILED_JOBS: usize = 128;
const MAX_INTEGRATION_ERRORS: usize = 128;
const MAX_CORRELATION_IDS: usize = 128;
const MAX_CONFIGURATION_ENTRIES: usize = 64;
const SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE: &str = "support_diagnostics_audit";
const SUPPORT_DIAGNOSTICS_AUDIT_EVENT: &str = "support.diagnostics.snapshot_recorded.v1";
const SUPPORT_DIAGNOSTICS_AUDIT_ACTION: &str = "support.diagnostics.read";
const MAX_SUPPORT_DIAGNOSTICS_REFERENCE_BYTES: usize = 255;
const MAX_SUPPORT_DIAGNOSTICS_AGE: Duration = Duration::minutes(5);
const SUPPORT_DIAGNOSTICS_CLOCK_SKEW: Duration = Duration::seconds(30);

/// Classification used by the redaction boundary.  Any class other than
/// [`RedactionClass::Safe`] is excluded from the output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RedactionClass {
    Safe,
    Credential,
    Secret,
    PrivateKey,
    PaymentInstrument,
    Token,
    UnrelatedBusinessData,
}

/// Classify an untrusted key/value pair before it crosses the diagnostics
/// boundary.  The classifier is conservative by design: callers must still
/// pass the key through the explicit configuration allow-list.
pub fn classify_diagnostic_field(key: &str, value: &str) -> RedactionClass {
    let key = key.trim().to_ascii_lowercase();
    let value = value.trim().to_ascii_lowercase();

    if contains_any(&key, &["private_key", "private-key", "pem", "certificate"]) {
        return RedactionClass::PrivateKey;
    }
    if contains_any(
        &key,
        &[
            "card", "pan", "cvv", "cvc", "payment", "bank", "iban", "routing", "stripe",
        ],
    ) {
        return RedactionClass::PaymentInstrument;
    }
    if contains_any(
        &key,
        &[
            "token",
            "bearer",
            "authorization",
            "cookie",
            "jwt",
            "session",
            "refresh",
        ],
    ) || contains_any(&value, &["bearer ", "sk_live_", "rk_live_", "whsec_"])
    {
        return RedactionClass::Token;
    }
    if contains_any(
        &key,
        &[
            "secret",
            "password",
            "passwd",
            "credential",
            "api_key",
            "access_key",
            "private",
        ],
    ) || contains_any(&value, &["password=", "client_secret", "-----begin "])
    {
        return RedactionClass::Secret;
    }
    if value.contains('@') {
        return RedactionClass::UnrelatedBusinessData;
    }
    if contains_any(
        &key,
        &[
            "customer", "tenant", "company", "branch", "invoice", "order", "account", "user",
            "email", "phone", "address", "contract", "document",
        ],
    ) {
        return RedactionClass::UnrelatedBusinessData;
    }
    if key.is_empty() || value.is_empty() {
        return RedactionClass::UnrelatedBusinessData;
    }
    RedactionClass::Safe
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DiagnosticsLimits {
    pub max_output_bytes: usize,
    pub max_versions: usize,
    pub max_failed_jobs: usize,
    pub max_integration_errors: usize,
    pub max_correlation_ids: usize,
    pub max_configuration_entries: usize,
}

impl Default for DiagnosticsLimits {
    fn default() -> Self {
        Self {
            max_output_bytes: DEFAULT_MAX_DIAGNOSTICS_BYTES,
            max_versions: MAX_VERSIONS,
            max_failed_jobs: MAX_FAILED_JOBS,
            max_integration_errors: MAX_INTEGRATION_ERRORS,
            max_correlation_ids: MAX_CORRELATION_IDS,
            max_configuration_entries: MAX_CONFIGURATION_ENTRIES,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticsError {
    OutputLimitTooSmall,
}

impl std::fmt::Display for DiagnosticsError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::OutputLimitTooSmall => {
                formatter.write_str("diagnostics output limit is too small")
            }
        }
    }
}

impl std::error::Error for DiagnosticsError {}

/// Untrusted diagnostics input.  Fields remain private so raw values cannot
/// be copied from this staging object into transport responses accidentally.
#[derive(Debug, Clone)]
pub struct DiagnosticsSnapshotInput {
    captured_at: DateTime<Utc>,
    versions: Vec<VersionInput>,
    projection_lag_seconds: Option<u64>,
    failed_jobs: Vec<FailedJobInput>,
    integration_errors: Vec<IntegrationErrorInput>,
    worker_health: Option<HealthInput>,
    backup_health: Option<BackupHealthInput>,
    correlation_ids: Vec<String>,
    configuration: Vec<ConfigurationInput>,
}

#[derive(Debug, Clone)]
struct VersionInput {
    component: String,
    version: String,
}

#[derive(Debug, Clone)]
struct FailedJobInput {
    queue: String,
    failure_code: String,
    attempts: u32,
    last_failed_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct IntegrationErrorInput {
    integration: String,
    error_code: String,
    retryable: bool,
    occurred_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Clone)]
struct HealthInput {
    state: ReadinessObservationState,
    checked_at: Option<DateTime<Utc>>,
    age_seconds: Option<u64>,
    reason_code: Option<String>,
}

#[derive(Debug, Clone)]
struct BackupHealthInput {
    health: HealthInput,
    verified: bool,
}

#[derive(Debug, Clone)]
struct ConfigurationInput {
    key: String,
    value: String,
}

impl DiagnosticsSnapshotInput {
    pub fn new(captured_at: DateTime<Utc>) -> Self {
        Self {
            captured_at,
            versions: Vec::new(),
            projection_lag_seconds: None,
            failed_jobs: Vec::new(),
            integration_errors: Vec::new(),
            worker_health: None,
            backup_health: None,
            correlation_ids: Vec::new(),
            configuration: Vec::new(),
        }
    }

    pub fn add_version(&mut self, component: impl Into<String>, version: impl Into<String>) {
        self.versions.push(VersionInput {
            component: component.into(),
            version: version.into(),
        });
    }

    pub fn set_projection_lag_seconds(&mut self, lag_seconds: u64) {
        self.projection_lag_seconds = Some(lag_seconds);
    }

    pub fn add_failed_job(
        &mut self,
        queue: impl Into<String>,
        failure_code: impl Into<String>,
        attempts: u32,
        last_failed_at: Option<DateTime<Utc>>,
    ) {
        self.failed_jobs.push(FailedJobInput {
            queue: queue.into(),
            failure_code: failure_code.into(),
            attempts,
            last_failed_at,
        });
    }

    pub fn add_integration_error(
        &mut self,
        integration: impl Into<String>,
        error_code: impl Into<String>,
        retryable: bool,
        occurred_at: Option<DateTime<Utc>>,
    ) {
        self.integration_errors.push(IntegrationErrorInput {
            integration: integration.into(),
            error_code: error_code.into(),
            retryable,
            occurred_at,
        });
    }

    pub fn set_worker_health(
        &mut self,
        state: ReadinessObservationState,
        checked_at: Option<DateTime<Utc>>,
        age_seconds: Option<u64>,
        reason_code: Option<impl Into<String>>,
    ) {
        self.worker_health = Some(HealthInput {
            state,
            checked_at,
            age_seconds,
            reason_code: reason_code.map(Into::into),
        });
    }

    pub fn set_backup_health(
        &mut self,
        state: ReadinessObservationState,
        checked_at: Option<DateTime<Utc>>,
        age_seconds: Option<u64>,
        verified: bool,
        reason_code: Option<impl Into<String>>,
    ) {
        self.backup_health = Some(BackupHealthInput {
            health: HealthInput {
                state,
                checked_at,
                age_seconds,
                reason_code: reason_code.map(Into::into),
            },
            verified,
        });
    }

    pub fn set_backup_health_from_readiness(
        &mut self,
        observation: &BackupPitrObservation,
        now: DateTime<Utc>,
    ) {
        let checked_at = observation.completed_at.or(observation.started_at);
        self.set_backup_health(
            observation.state,
            checked_at,
            checked_at.map(|at| age_seconds(at, now)),
            observation.verified,
            observation.reason_code.clone(),
        );
    }

    pub fn add_correlation_id(&mut self, correlation_id: impl Into<String>) {
        self.correlation_ids.push(correlation_id.into());
    }

    /// Add a configuration observation.  Only the small safe key allow-list is
    /// retained during [`Self::build`]; all other keys or sensitive values are
    /// counted as redacted and omitted.
    pub fn add_configuration(&mut self, key: impl Into<String>, value: impl Into<String>) {
        self.configuration.push(ConfigurationInput {
            key: key.into(),
            value: value.into(),
        });
    }

    pub fn build(self, limits: DiagnosticsLimits) -> Result<DiagnosticsSnapshot, DiagnosticsError> {
        if limits.max_output_bytes < MIN_MAX_DIAGNOSTICS_BYTES {
            return Err(DiagnosticsError::OutputLimitTooSmall);
        }

        let mut truncated = DiagnosticsTruncation::default();
        let mut redacted_configuration_count = 0usize;

        let mut versions = self
            .versions
            .into_iter()
            .filter_map(|entry| {
                Some(DiagnosticVersion {
                    component: safe_text(&entry.component, MAX_COMPONENT_BYTES)?,
                    version: safe_text(&entry.version, MAX_FIELD_BYTES)?,
                })
            })
            .collect::<Vec<_>>();
        sort_dedup(
            &mut versions,
            |left, right| (&left.component, &left.version).cmp(&(&right.component, &right.version)),
            |left, right| left.component == right.component && left.version == right.version,
        );
        truncate(&mut versions, limits.max_versions, &mut truncated.versions);

        let mut failed_jobs = self
            .failed_jobs
            .into_iter()
            .filter_map(|entry| {
                Some(FailedJobSummary {
                    queue: safe_identifier(&entry.queue)?,
                    failure_code: safe_identifier(&entry.failure_code)?,
                    attempts: entry.attempts,
                    last_failed_at: entry.last_failed_at,
                })
            })
            .collect::<Vec<_>>();
        sort_dedup(
            &mut failed_jobs,
            |left, right| {
                (&left.queue, &left.failure_code, left.last_failed_at).cmp(&(
                    &right.queue,
                    &right.failure_code,
                    right.last_failed_at,
                ))
            },
            |left, right| {
                left.queue == right.queue
                    && left.failure_code == right.failure_code
                    && left.last_failed_at == right.last_failed_at
            },
        );
        truncate(
            &mut failed_jobs,
            limits.max_failed_jobs,
            &mut truncated.failed_jobs,
        );

        let mut integration_errors = self
            .integration_errors
            .into_iter()
            .filter_map(|entry| {
                Some(IntegrationErrorSummary {
                    integration: safe_identifier(&entry.integration)?,
                    error_code: safe_identifier(&entry.error_code)?,
                    retryable: entry.retryable,
                    occurred_at: entry.occurred_at,
                })
            })
            .collect::<Vec<_>>();
        sort_dedup(
            &mut integration_errors,
            |left, right| {
                (
                    &left.integration,
                    &left.error_code,
                    left.retryable,
                    left.occurred_at,
                )
                    .cmp(&(
                        &right.integration,
                        &right.error_code,
                        right.retryable,
                        right.occurred_at,
                    ))
            },
            |left, right| {
                left.integration == right.integration
                    && left.error_code == right.error_code
                    && left.retryable == right.retryable
                    && left.occurred_at == right.occurred_at
            },
        );
        truncate(
            &mut integration_errors,
            limits.max_integration_errors,
            &mut truncated.integration_errors,
        );

        let mut correlation_ids = self
            .correlation_ids
            .into_iter()
            .filter_map(|value| safe_identifier(&value))
            .collect::<Vec<_>>();
        correlation_ids.sort();
        correlation_ids.dedup();
        truncate(
            &mut correlation_ids,
            limits.max_correlation_ids,
            &mut truncated.correlation_ids,
        );

        let mut configuration = Vec::new();
        for entry in self.configuration {
            let key = entry.key.trim().to_ascii_lowercase();
            if !SAFE_CONFIGURATION_KEYS.contains(&key.as_str())
                || classify_diagnostic_field(&key, &entry.value) != RedactionClass::Safe
            {
                redacted_configuration_count += 1;
                continue;
            }
            let Some(value) = safe_text(&entry.value, MAX_FIELD_BYTES) else {
                redacted_configuration_count += 1;
                continue;
            };
            configuration.push(RedactedConfiguration { key, value });
        }
        sort_dedup(
            &mut configuration,
            |left, right| (&left.key, &left.value).cmp(&(&right.key, &right.value)),
            |left, right| left.key == right.key && left.value == right.value,
        );
        truncate(
            &mut configuration,
            limits.max_configuration_entries,
            &mut truncated.configuration,
        );

        let snapshot = DiagnosticsSnapshot {
            schema_version: DIAGNOSTICS_SCHEMA_VERSION,
            captured_at: self.captured_at,
            versions,
            projection_lag_seconds: self.projection_lag_seconds,
            failed_jobs,
            integration_errors,
            worker_health: self.worker_health.and_then(DiagnosticHealth::from_input),
            backup_health: self
                .backup_health
                .and_then(DiagnosticBackupHealth::from_input),
            correlation_ids,
            configuration,
            redacted_configuration_count,
            truncated,
        };
        enforce_output_budget(snapshot, limits.max_output_bytes)
    }
}

const SAFE_CONFIGURATION_KEYS: &[&str] = &[
    "api_version",
    "build",
    "deployment",
    "environment",
    "log_level",
    "region",
    "release",
    "schema_version",
    "worker_concurrency",
    "worker_mode",
];

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticsSnapshot {
    schema_version: &'static str,
    captured_at: DateTime<Utc>,
    versions: Vec<DiagnosticVersion>,
    projection_lag_seconds: Option<u64>,
    failed_jobs: Vec<FailedJobSummary>,
    integration_errors: Vec<IntegrationErrorSummary>,
    worker_health: Option<DiagnosticHealth>,
    backup_health: Option<DiagnosticBackupHealth>,
    correlation_ids: Vec<String>,
    configuration: Vec<RedactedConfiguration>,
    redacted_configuration_count: usize,
    truncated: DiagnosticsTruncation,
}

impl DiagnosticsSnapshot {
    pub fn schema_version(&self) -> &'static str {
        self.schema_version
    }

    pub fn captured_at(&self) -> DateTime<Utc> {
        self.captured_at
    }

    pub fn versions(&self) -> &[DiagnosticVersion] {
        &self.versions
    }

    pub fn projection_lag_seconds(&self) -> Option<u64> {
        self.projection_lag_seconds
    }

    pub fn failed_jobs(&self) -> &[FailedJobSummary] {
        &self.failed_jobs
    }

    pub fn integration_errors(&self) -> &[IntegrationErrorSummary] {
        &self.integration_errors
    }

    pub fn worker_health(&self) -> Option<&DiagnosticHealth> {
        self.worker_health.as_ref()
    }

    pub fn backup_health(&self) -> Option<&DiagnosticBackupHealth> {
        self.backup_health.as_ref()
    }

    pub fn correlation_ids(&self) -> &[String] {
        &self.correlation_ids
    }

    pub fn configuration(&self) -> &[RedactedConfiguration] {
        &self.configuration
    }

    pub fn redacted_configuration_count(&self) -> usize {
        self.redacted_configuration_count
    }

    pub fn truncated(&self) -> &DiagnosticsTruncation {
        &self.truncated
    }

    pub fn serialized_len(&self) -> usize {
        serde_json::to_vec(self).map_or(usize::MAX, |bytes| bytes.len())
    }

    /// Persist a redacted diagnostics snapshot and its audit/outbox envelope
    /// atomically on the control write database. The scope is retained as an
    /// authorization reference only; the snapshot itself remains the
    /// allow-listed value produced by [`DiagnosticsSnapshotInput::build`].
    ///
    /// This method is intentionally separate from transport authorization:
    /// callers must pass the already-authorized canonical scope, actor, and
    /// purpose. The database transaction still binds those values to the
    /// immutable aggregate snapshot and deterministic outbox event.
    pub async fn persist_audit_snapshot(
        &self,
        write: &PgPool,
        request: &SupportDiagnosticsAuditRequest<'_>,
    ) -> Result<SupportDiagnosticsAuditReceipt, PersistenceError> {
        validate_persistence_reference(request.scope, "support diagnostics scope")?;
        validate_persistence_reference(request.actor_ref, "support diagnostics actor")?;
        validate_persistence_reference(request.purpose, "support diagnostics purpose")?;
        validate_persistence_reference(
            request.idempotency_key,
            "support diagnostics idempotency key",
        )?;
        if request.correlation_id.is_nil() {
            return Err(PersistenceError::InvalidData(
                "support diagnostics correlation id is required".into(),
            ));
        }
        validate_snapshot_freshness(self.captured_at, request.observed_at)?;

        let snapshot = serde_json::to_value(self)?;
        let snapshot_bytes = serde_json::to_vec(&snapshot)?;
        let content_hash = sha256_hex(&snapshot_bytes);
        let aggregate_id = support_diagnostics_aggregate_id(request.scope);
        let stored_idempotency_key = format!("support-diagnostics:{}", request.idempotency_key);
        validate_persistence_reference(
            &stored_idempotency_key,
            "support diagnostics stored idempotency key",
        )?;
        let event_id = support_diagnostics_event_id(&stored_idempotency_key);
        let request_hash = sha256_hex(
            serde_json::to_vec(&serde_json::json!({
                "scope": request.scope,
                "actor_ref": request.actor_ref,
                "purpose": request.purpose,
                "correlation_id": request.correlation_id,
                "captured_at": self.captured_at,
                "content_hash": content_hash,
            }))?
            .as_slice(),
        );
        let state = serde_json::json!({
            "schema_version": DIAGNOSTICS_SCHEMA_VERSION,
            "scope": request.scope,
            "actor_ref": request.actor_ref,
            "purpose": request.purpose,
            "correlation_id": request.correlation_id,
            "captured_at": self.captured_at,
            "content_hash": content_hash,
            "snapshot": snapshot,
        });

        let mut tx = write.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(format!(
                "{SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE}:{}",
                request.scope
            ))
            .execute(&mut *tx)
            .await?;

        let inserted = sqlx::query(
            "INSERT INTO erpx_control.idempotency_keys
                (idempotency_key, request_hash, command_id, aggregate_type,
                 aggregate_id, event_id, status)
             VALUES ($1, $2, $3, $4, $5, $6, 'in_progress')
             ON CONFLICT (idempotency_key) DO NOTHING",
        )
        .bind(&stored_idempotency_key)
        .bind(&request_hash)
        .bind(event_id)
        .bind(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE)
        .bind(aggregate_id)
        .bind(event_id)
        .execute(&mut *tx)
        .await?;

        if inserted.rows_affected() == 0 {
            let row = sqlx::query(
                "SELECT request_hash, command_id, aggregate_type, aggregate_id,
                        aggregate_version, event_id, status, response
                 FROM erpx_control.idempotency_keys
                 WHERE idempotency_key = $1 FOR UPDATE",
            )
            .bind(&stored_idempotency_key)
            .fetch_one(&mut *tx)
            .await?;
            if row.try_get::<String, _>("request_hash")? != request_hash {
                return Err(PersistenceError::IdempotencyConflict {
                    key: stored_idempotency_key,
                });
            }
            let stored_command_id: Option<Uuid> = row.try_get("command_id")?;
            let stored_aggregate_type: Option<String> = row.try_get("aggregate_type")?;
            let stored_aggregate_id: Option<Uuid> = row.try_get("aggregate_id")?;
            let stored_event_id: Option<Uuid> = row.try_get("event_id")?;
            validate_diagnostics_idempotency_binding(
                &stored_idempotency_key,
                stored_command_id,
                stored_aggregate_type.as_deref(),
                stored_aggregate_id,
                stored_event_id,
                event_id,
                aggregate_id,
            )?;
            match row.try_get::<String, _>("status")?.as_str() {
                "completed" => {
                    let response =
                        row.try_get::<Option<Value>, _>("response")?
                            .ok_or_else(|| {
                                PersistenceError::InvalidData(
                                    "completed support diagnostics receipt has no response".into(),
                                )
                            })?;
                    let mut receipt: SupportDiagnosticsAuditReceipt =
                        serde_json::from_value(response)?;
                    let stored_version = row
                        .try_get::<Option<i64>, _>("aggregate_version")?
                        .and_then(|value| u64::try_from(value).ok());
                    if stored_version != Some(receipt.aggregate_version)
                        || receipt.aggregate_id != aggregate_id
                        || receipt.event_id != event_id
                        || receipt.scope != request.scope
                        || receipt.captured_at != self.captured_at
                        || receipt.content_hash != content_hash
                    {
                        return Err(PersistenceError::IdempotencyConflict {
                            key: stored_idempotency_key,
                        });
                    }
                    receipt.reused_idempotency = true;
                    tx.commit().await?;
                    return Ok(receipt);
                }
                "in_progress" => {
                    return Err(PersistenceError::InvalidData(
                        "support diagnostics idempotency request is already in progress".into(),
                    ));
                }
                status => {
                    return Err(PersistenceError::InvalidData(format!(
                        "unsupported support diagnostics idempotency status {status}"
                    )));
                }
            }
        }

        let current_version = sqlx::query(
            "SELECT aggregate_version
             FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = $1 AND aggregate_id = $2
             FOR UPDATE",
        )
        .bind(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE)
        .bind(aggregate_id)
        .fetch_optional(&mut *tx)
        .await?
        .map(|row| row.try_get::<i64, _>("aggregate_version"))
        .transpose()?
        .map(|version| {
            u64::try_from(version).map_err(|_| {
                PersistenceError::InvalidData(
                    "support diagnostics aggregate version is negative".into(),
                )
            })
        })
        .transpose()?
        .unwrap_or(0);
        let next_version = current_version.checked_add(1).ok_or_else(|| {
            PersistenceError::InvalidData("support diagnostics aggregate version overflow".into())
        })?;
        let next_version_i64 = i64::try_from(next_version).map_err(|_| {
            PersistenceError::InvalidData(
                "support diagnostics aggregate version is too large".into(),
            )
        })?;

        sqlx::query(
            "INSERT INTO erpx_control.aggregate_snapshots
                (aggregate_type, aggregate_id, aggregate_version, state, updated_at)
             VALUES ($1, $2, $3, $4, $5)
             ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
                aggregate_version = EXCLUDED.aggregate_version,
                state = EXCLUDED.state,
                updated_at = EXCLUDED.updated_at",
        )
        .bind(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE)
        .bind(aggregate_id)
        .bind(next_version_i64)
        .bind(&state)
        .bind(request.observed_at)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO erpx_control.audit_events
                (id, actor_ref, customer_account_id, action, purpose,
                 correlation_id, recorded_at, payload)
             VALUES ($1, $2, NULL, $3, $4, $5, $6, $7)",
        )
        .bind(event_id)
        .bind(request.actor_ref)
        .bind(SUPPORT_DIAGNOSTICS_AUDIT_ACTION)
        .bind(request.purpose)
        .bind(request.correlation_id)
        .bind(request.observed_at)
        .bind(&state)
        .execute(&mut *tx)
        .await?;

        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, tenant_scope, actor, correlation_id,
                 idempotency_key, payload, occurred_at, available_at)
             VALUES ($1, $2, 1, $3, $4, $5, $6, $7, $8, $9, $10, $11, $11)",
        )
        .bind(event_id)
        .bind(SUPPORT_DIAGNOSTICS_AUDIT_EVENT)
        .bind(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE)
        .bind(aggregate_id)
        .bind(next_version_i64)
        .bind(serde_json::json!({ "support_scope": request.scope }))
        .bind(serde_json::json!({ "actor_ref": request.actor_ref }))
        .bind(request.correlation_id)
        .bind(&stored_idempotency_key)
        .bind(&state)
        .bind(request.observed_at)
        .execute(&mut *tx)
        .await?;

        let receipt = SupportDiagnosticsAuditReceipt {
            aggregate_id,
            aggregate_version: next_version,
            event_id,
            scope: request.scope.into(),
            captured_at: self.captured_at,
            content_hash,
            reused_idempotency: false,
        };
        sqlx::query(
            "UPDATE erpx_control.idempotency_keys
             SET status = 'completed', response = $2, completed_at = $3,
                 aggregate_version = $4
             WHERE idempotency_key = $1 AND status = 'in_progress'",
        )
        .bind(&stored_idempotency_key)
        .bind(serde_json::to_value(&receipt)?)
        .bind(request.observed_at)
        .bind(next_version_i64)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Reconcile one scope's authoritative aggregate snapshot against every
    /// durable diagnostics outbox event. A mismatched scope, stale snapshot,
    /// content hash, event payload, or contiguous version history fails
    /// closed before any read can treat the evidence as current.
    pub async fn reconcile_audit_snapshot(
        write: &PgPool,
        scope: &str,
        observed_at: DateTime<Utc>,
    ) -> Result<SupportDiagnosticsReconciliation, PersistenceError> {
        validate_persistence_reference(scope, "support diagnostics scope")?;
        let aggregate_id = support_diagnostics_aggregate_id(scope);
        let row = sqlx::query(
            "SELECT aggregate_version, state
             FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = $1 AND aggregate_id = $2",
        )
        .bind(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE)
        .bind(aggregate_id)
        .fetch_optional(write)
        .await?
        .ok_or(PersistenceError::NotFound {
            entity: "support diagnostics audit snapshot",
            id: aggregate_id,
        })?;
        let version = u64::try_from(row.try_get::<i64, _>("aggregate_version")?).map_err(|_| {
            PersistenceError::InvalidData(
                "support diagnostics aggregate version is negative".into(),
            )
        })?;
        let state: Value = row.try_get("state")?;
        let state_scope = state.get("scope").and_then(Value::as_str).ok_or_else(|| {
            PersistenceError::InvalidData("support diagnostics scope is missing".into())
        })?;
        if state_scope != scope {
            return Err(PersistenceError::InvalidData(
                "support diagnostics scope binding mismatch".into(),
            ));
        }
        let captured_at: DateTime<Utc> =
            serde_json::from_value(state.get("captured_at").cloned().ok_or_else(|| {
                PersistenceError::InvalidData("support diagnostics captured_at is missing".into())
            })?)?;
        validate_snapshot_freshness(captured_at, observed_at)?;
        let snapshot = state.get("snapshot").ok_or_else(|| {
            PersistenceError::InvalidData("support diagnostics snapshot is missing".into())
        })?;
        let actual_hash = sha256_hex(&serde_json::to_vec(snapshot)?);
        let stored_hash = state
            .get("content_hash")
            .and_then(Value::as_str)
            .ok_or_else(|| {
                PersistenceError::InvalidData("support diagnostics content hash is missing".into())
            })?;
        if stored_hash != actual_hash {
            return Err(PersistenceError::InvalidData(
                "support diagnostics snapshot content hash mismatch".into(),
            ));
        }

        let events = sqlx::query(
            "SELECT id, aggregate_version, event_version, aggregate_type,
                    event_type, tenant_scope, actor, correlation_id,
                    idempotency_key, occurred_at, payload
             FROM erpx_control.outbox_events
             WHERE aggregate_type = $1 AND aggregate_id = $2
             ORDER BY aggregate_version, id",
        )
        .bind(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE)
        .bind(aggregate_id)
        .fetch_all(write)
        .await?;
        if events.len() != usize::try_from(version).unwrap_or(usize::MAX) {
            return Err(PersistenceError::InvalidData(
                "support diagnostics outbox history is not contiguous".into(),
            ));
        }
        let expected_scope = serde_json::json!({ "support_scope": scope });
        for (index, event) in events.iter().enumerate() {
            let expected_version = u64::try_from(index + 1).map_err(|_| {
                PersistenceError::InvalidData("support diagnostics event index overflow".into())
            })?;
            let event_schema_version = event.try_get::<i32, _>("event_version")?;
            let event_version = u64::try_from(event.try_get::<i64, _>("aggregate_version")?)
                .map_err(|_| {
                    PersistenceError::InvalidData(
                        "support diagnostics event version is negative".into(),
                    )
                })?;
            let event_id = event.try_get::<Uuid, _>("id")?;
            let event_occurred_at = event.try_get::<DateTime<Utc>, _>("occurred_at")?;
            let event_payload = event.try_get::<Value, _>("payload")?;
            let event_scope = event_payload
                .get("scope")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "support diagnostics event scope is missing".into(),
                    )
                })?;
            let audit = sqlx::query(
                "SELECT actor_ref, action, purpose, correlation_id, recorded_at, payload
                 FROM erpx_control.audit_events WHERE id = $1",
            )
            .bind(event_id)
            .fetch_optional(write)
            .await?
            .ok_or_else(|| {
                PersistenceError::InvalidData(
                    "support diagnostics audit event is missing during reconciliation".into(),
                )
            })?;
            let event_actor = event_payload
                .get("actor_ref")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "support diagnostics event actor is missing".into(),
                    )
                })?;
            let event_purpose = event_payload
                .get("purpose")
                .and_then(Value::as_str)
                .ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "support diagnostics event purpose is missing".into(),
                    )
                })?;
            let expected_actor = serde_json::json!({ "actor_ref": event_actor });
            let event_idempotency_key = event
                .try_get::<Option<String>, _>("idempotency_key")?
                .ok_or_else(|| {
                    PersistenceError::InvalidData(
                        "support diagnostics event idempotency key is missing".into(),
                    )
                })?;
            if event_version != expected_version
                || event_schema_version != 1
                || event.try_get::<String, _>("aggregate_type")?
                    != SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE
                || event.try_get::<String, _>("event_type")? != SUPPORT_DIAGNOSTICS_AUDIT_EVENT
                || event.try_get::<Option<Value>, _>("tenant_scope")?
                    != Some(expected_scope.clone())
                || event.try_get::<Option<Value>, _>("actor")? != Some(expected_actor)
                || support_diagnostics_event_id(&event_idempotency_key) != event_id
                || event_scope != scope
                || audit.try_get::<String, _>("actor_ref")? != event_actor
                || audit.try_get::<String, _>("action")? != SUPPORT_DIAGNOSTICS_AUDIT_ACTION
                || audit.try_get::<String, _>("purpose")? != event_purpose
                || audit.try_get::<DateTime<Utc>, _>("recorded_at")? != event_occurred_at
                || audit.try_get::<Uuid, _>("correlation_id")?
                    != event.try_get::<Uuid, _>("correlation_id")?
                || audit.try_get::<Value, _>("payload")? != event_payload
            {
                return Err(PersistenceError::InvalidData(
                    "support diagnostics outbox reconciliation mismatch".into(),
                ));
            }
        }
        let latest_payload = events
            .last()
            .map(|event| event.try_get::<Value, _>("payload"))
            .transpose()?
            .ok_or_else(|| {
                PersistenceError::InvalidData("support diagnostics latest event is missing".into())
            })?;
        if latest_payload != state {
            return Err(PersistenceError::InvalidData(
                "support diagnostics latest snapshot reconciliation mismatch".into(),
            ));
        }
        let event_id = events
            .last()
            .map(|event| event.try_get::<Uuid, _>("id"))
            .transpose()?
            .ok_or_else(|| {
                PersistenceError::InvalidData("support diagnostics outbox event is missing".into())
            })?;
        Ok(SupportDiagnosticsReconciliation {
            aggregate_id,
            aggregate_version: version,
            event_id,
            scope: scope.into(),
            captured_at,
            content_hash: stored_hash.into(),
        })
    }
}

#[derive(Debug, Clone, Copy)]
pub struct SupportDiagnosticsAuditRequest<'a> {
    scope: &'a str,
    actor_ref: &'a str,
    purpose: &'a str,
    correlation_id: Uuid,
    idempotency_key: &'a str,
    observed_at: DateTime<Utc>,
}

impl<'a> SupportDiagnosticsAuditRequest<'a> {
    pub fn new(
        scope: &'a str,
        actor_ref: &'a str,
        purpose: &'a str,
        correlation_id: Uuid,
        idempotency_key: &'a str,
        observed_at: DateTime<Utc>,
    ) -> Self {
        Self {
            scope,
            actor_ref,
            purpose,
            correlation_id,
            idempotency_key,
            observed_at,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupportDiagnosticsAuditReceipt {
    aggregate_id: Uuid,
    aggregate_version: u64,
    event_id: Uuid,
    scope: String,
    captured_at: DateTime<Utc>,
    content_hash: String,
    reused_idempotency: bool,
}

impl SupportDiagnosticsAuditReceipt {
    pub fn aggregate_id(&self) -> Uuid {
        self.aggregate_id
    }

    pub fn aggregate_version(&self) -> u64 {
        self.aggregate_version
    }

    pub fn event_id(&self) -> Uuid {
        self.event_id
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn captured_at(&self) -> DateTime<Utc> {
        self.captured_at
    }

    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }

    pub fn reused_idempotency(&self) -> bool {
        self.reused_idempotency
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupportDiagnosticsReconciliation {
    aggregate_id: Uuid,
    aggregate_version: u64,
    event_id: Uuid,
    scope: String,
    captured_at: DateTime<Utc>,
    content_hash: String,
}

impl SupportDiagnosticsReconciliation {
    pub fn aggregate_id(&self) -> Uuid {
        self.aggregate_id
    }

    pub fn aggregate_version(&self) -> u64 {
        self.aggregate_version
    }

    pub fn event_id(&self) -> Uuid {
        self.event_id
    }

    pub fn scope(&self) -> &str {
        &self.scope
    }

    pub fn captured_at(&self) -> DateTime<Utc> {
        self.captured_at
    }

    pub fn content_hash(&self) -> &str {
        &self.content_hash
    }
}

fn validate_persistence_reference(
    value: &str,
    field: &'static str,
) -> Result<(), PersistenceError> {
    if value.len() > MAX_SUPPORT_DIAGNOSTICS_REFERENCE_BYTES || safe_identifier(value).is_none() {
        return Err(PersistenceError::InvalidData(format!(
            "{field} is outside its bounded safe reference format"
        )));
    }
    Ok(())
}

fn validate_diagnostics_idempotency_binding(
    key: &str,
    command_id: Option<Uuid>,
    aggregate_type: Option<&str>,
    aggregate_id: Option<Uuid>,
    event_id: Option<Uuid>,
    expected_event_id: Uuid,
    expected_aggregate_id: Uuid,
) -> Result<(), PersistenceError> {
    if command_id != Some(expected_event_id)
        || aggregate_type != Some(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE)
        || aggregate_id != Some(expected_aggregate_id)
        || event_id != Some(expected_event_id)
    {
        return Err(PersistenceError::IdempotencyConflict { key: key.into() });
    }
    Ok(())
}

fn validate_snapshot_freshness(
    captured_at: DateTime<Utc>,
    observed_at: DateTime<Utc>,
) -> Result<(), PersistenceError> {
    if captured_at > observed_at + SUPPORT_DIAGNOSTICS_CLOCK_SKEW {
        return Err(PersistenceError::InvalidData(
            "support diagnostics snapshot is from the future".into(),
        ));
    }
    if captured_at < observed_at - MAX_SUPPORT_DIAGNOSTICS_AGE {
        return Err(PersistenceError::InvalidData(
            "support diagnostics snapshot is stale".into(),
        ));
    }
    Ok(())
}

fn support_diagnostics_aggregate_id(scope: &str) -> Uuid {
    Uuid::new_v5(
        &Uuid::NAMESPACE_URL,
        format!("{SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE}:{scope}").as_bytes(),
    )
}

fn support_diagnostics_event_id(idempotency_key: &str) -> Uuid {
    Uuid::new_v5(&Uuid::NAMESPACE_URL, idempotency_key.as_bytes())
}

fn sha256_hex(bytes: &[u8]) -> String {
    let mut digest = Sha256::new();
    digest.update(bytes);
    hex::encode(digest.finalize())
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticVersion {
    component: String,
    version: String,
}

impl DiagnosticVersion {
    pub fn component(&self) -> &str {
        &self.component
    }

    pub fn version(&self) -> &str {
        &self.version
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct FailedJobSummary {
    queue: String,
    failure_code: String,
    attempts: u32,
    last_failed_at: Option<DateTime<Utc>>,
}

impl FailedJobSummary {
    pub fn queue(&self) -> &str {
        &self.queue
    }

    pub fn failure_code(&self) -> &str {
        &self.failure_code
    }

    pub fn attempts(&self) -> u32 {
        self.attempts
    }

    pub fn last_failed_at(&self) -> Option<DateTime<Utc>> {
        self.last_failed_at
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct IntegrationErrorSummary {
    integration: String,
    error_code: String,
    retryable: bool,
    occurred_at: Option<DateTime<Utc>>,
}

impl IntegrationErrorSummary {
    pub fn integration(&self) -> &str {
        &self.integration
    }

    pub fn error_code(&self) -> &str {
        &self.error_code
    }

    pub fn retryable(&self) -> bool {
        self.retryable
    }

    pub fn occurred_at(&self) -> Option<DateTime<Utc>> {
        self.occurred_at
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticHealth {
    state: &'static str,
    checked_at: Option<DateTime<Utc>>,
    age_seconds: Option<u64>,
    reason_code: Option<String>,
}

impl DiagnosticHealth {
    pub fn state(&self) -> &'static str {
        self.state
    }

    pub fn checked_at(&self) -> Option<DateTime<Utc>> {
        self.checked_at
    }

    pub fn age_seconds(&self) -> Option<u64> {
        self.age_seconds
    }

    pub fn reason_code(&self) -> Option<&str> {
        self.reason_code.as_deref()
    }

    fn from_input(input: HealthInput) -> Option<Self> {
        Some(Self {
            state: input.state.as_str(),
            checked_at: input.checked_at,
            age_seconds: input.age_seconds,
            reason_code: input
                .reason_code
                .and_then(|reason| safe_identifier(&reason)),
        })
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct DiagnosticBackupHealth {
    health: DiagnosticHealth,
    verified: bool,
}

impl DiagnosticBackupHealth {
    pub fn health(&self) -> &DiagnosticHealth {
        &self.health
    }

    pub fn verified(&self) -> bool {
        self.verified
    }

    fn from_input(input: BackupHealthInput) -> Option<Self> {
        Some(Self {
            health: DiagnosticHealth::from_input(input.health)?,
            verified: input.verified,
        })
    }
}

#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct RedactedConfiguration {
    key: String,
    value: String,
}

impl RedactedConfiguration {
    pub fn key(&self) -> &str {
        &self.key
    }

    pub fn value(&self) -> &str {
        &self.value
    }
}

#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct DiagnosticsTruncation {
    versions: usize,
    failed_jobs: usize,
    integration_errors: usize,
    correlation_ids: usize,
    configuration: usize,
}

impl DiagnosticsTruncation {
    pub fn versions(&self) -> usize {
        self.versions
    }

    pub fn failed_jobs(&self) -> usize {
        self.failed_jobs
    }

    pub fn integration_errors(&self) -> usize {
        self.integration_errors
    }

    pub fn correlation_ids(&self) -> usize {
        self.correlation_ids
    }

    pub fn configuration(&self) -> usize {
        self.configuration
    }
}

fn contains_any(value: &str, markers: &[&str]) -> bool {
    markers.iter().any(|marker| value.contains(marker))
}

fn safe_text(value: &str, max_bytes: usize) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > max_bytes
        || value.chars().any(|character| character.is_control())
        || classify_diagnostic_field("safe", value) != RedactionClass::Safe
    {
        return None;
    }
    Some(value.into())
}

fn safe_identifier(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > MAX_FIELD_BYTES
        || !value
            .chars()
            .all(|character| character.is_ascii_alphanumeric() || "._:/-".contains(character))
        || classify_diagnostic_field("identifier", value) != RedactionClass::Safe
    {
        return None;
    }
    Some(value.into())
}

fn age_seconds(at: DateTime<Utc>, now: DateTime<Utc>) -> u64 {
    now.signed_duration_since(at)
        .num_seconds()
        .max(0)
        .try_into()
        .unwrap_or(u64::MAX)
}

fn sort_dedup<T>(
    values: &mut Vec<T>,
    compare: impl Fn(&T, &T) -> Ordering,
    equal: impl Fn(&T, &T) -> bool,
) {
    values.sort_by(compare);
    values.dedup_by(|left, right| equal(left, right));
}

fn truncate<T>(values: &mut Vec<T>, max: usize, count: &mut usize) {
    if values.len() > max {
        *count += values.len() - max;
        values.truncate(max);
    }
}

fn enforce_output_budget(
    mut snapshot: DiagnosticsSnapshot,
    max_bytes: usize,
) -> Result<DiagnosticsSnapshot, DiagnosticsError> {
    // Optional, high-cardinality sections are removed from the end in a fixed
    // order until the serialized response fits.  Health, lag, and schema
    // metadata are retained whenever the configured minimum permits it.
    while snapshot.serialized_len() > max_bytes {
        if snapshot.configuration.pop().is_some() {
            snapshot.truncated.configuration += 1;
        } else if snapshot.correlation_ids.pop().is_some() {
            snapshot.truncated.correlation_ids += 1;
        } else if snapshot.integration_errors.pop().is_some() {
            snapshot.truncated.integration_errors += 1;
        } else if snapshot.failed_jobs.pop().is_some() {
            snapshot.truncated.failed_jobs += 1;
        } else if snapshot.versions.pop().is_some() {
            snapshot.truncated.versions += 1;
        } else {
            return Err(DiagnosticsError::OutputLimitTooSmall);
        }
    }
    Ok(snapshot)
}

#[cfg(test)]
mod tests {
    use chrono::{Duration, TimeZone};
    use serde_json::Value;

    use super::*;

    fn now() -> DateTime<Utc> {
        Utc.with_ymd_and_hms(2026, 7, 24, 12, 0, 0)
            .single()
            .expect("fixed timestamp")
    }

    #[test]
    fn classifier_is_conservative_for_forbidden_material() {
        assert_eq!(
            classify_diagnostic_field("DATABASE_PASSWORD", "not returned"),
            RedactionClass::Secret
        );
        assert_eq!(
            classify_diagnostic_field("private_key_pem", "-----BEGIN PRIVATE KEY-----"),
            RedactionClass::PrivateKey
        );
        assert_eq!(
            classify_diagnostic_field("card_number", "4242424242424242"),
            RedactionClass::PaymentInstrument
        );
        assert_eq!(
            classify_diagnostic_field("refresh_token", "opaque"),
            RedactionClass::Token
        );
        assert_eq!(
            classify_diagnostic_field("customer_email", "person@example.test"),
            RedactionClass::UnrelatedBusinessData
        );
        assert_eq!(
            classify_diagnostic_field("environment", "production"),
            RedactionClass::Safe
        );
        assert_eq!(
            classify_diagnostic_field("version", "person@example.test"),
            RedactionClass::UnrelatedBusinessData
        );
    }

    #[test]
    fn snapshot_contains_required_safe_fields_and_redacts_configuration() {
        let mut input = DiagnosticsSnapshotInput::new(now());
        input.add_version("server", "1.2.3");
        input.set_projection_lag_seconds(17);
        input.add_failed_job("outbox", "timeout", 3, Some(now() - Duration::minutes(2)));
        input.add_integration_error("stripe", "rate_limited", true, Some(now()));
        input.set_worker_health(
            ReadinessObservationState::Ready,
            Some(now()),
            Some(2),
            Some("heartbeat_current"),
        );
        input.set_backup_health(
            ReadinessObservationState::Ready,
            Some(now()),
            Some(60),
            true,
            Some("verified"),
        );
        input.add_correlation_id("corr-123");
        input.add_configuration("environment", "production");
        input.add_configuration("DATABASE_PASSWORD", "password=must-not-appear");
        input.add_configuration("customer_name", "must-not-appear");
        input.add_configuration("unknown_runtime_value", "must-not-appear");

        let snapshot = input.build(DiagnosticsLimits::default()).expect("snapshot");
        assert_eq!(snapshot.schema_version(), DIAGNOSTICS_SCHEMA_VERSION);
        assert_eq!(snapshot.projection_lag_seconds(), Some(17));
        assert_eq!(snapshot.versions().len(), 1);
        assert_eq!(snapshot.failed_jobs().len(), 1);
        assert_eq!(snapshot.integration_errors().len(), 1);
        assert_eq!(snapshot.worker_health().expect("worker").state(), "ready");
        assert!(snapshot.backup_health().expect("backup").verified());
        assert_eq!(snapshot.configuration().len(), 1);
        assert_eq!(snapshot.redacted_configuration_count(), 3);

        let json = serde_json::to_string(&snapshot).expect("safe JSON");
        for forbidden in [
            "DATABASE_PASSWORD",
            "password=must-not-appear",
            "customer_name",
            "must-not-appear",
        ] {
            assert!(!json.contains(forbidden), "forbidden material: {forbidden}");
        }
    }

    #[test]
    fn output_is_deterministic_and_bounded() {
        let mut first = DiagnosticsSnapshotInput::new(now());
        let mut second = DiagnosticsSnapshotInput::new(now());
        for (input, reverse) in [(&mut first, false), (&mut second, true)] {
            let values = ["zeta", "alpha", "middle"];
            for value in values.iter().copied().rev().filter(|_| !reverse) {
                input.add_version(value, "1");
            }
            for value in values.iter().copied().filter(|_| reverse) {
                input.add_version(value, "1");
            }
            for index in 0..100 {
                input.add_correlation_id(format!("corr-{index:03}"));
            }
        }
        let limits = DiagnosticsLimits {
            max_output_bytes: 1_024,
            max_correlation_ids: 100,
            ..DiagnosticsLimits::default()
        };
        let left = first.build(limits).expect("bounded snapshot");
        let right = second.build(limits).expect("bounded snapshot");
        assert_eq!(
            serde_json::to_vec(&left).expect("left JSON"),
            serde_json::to_vec(&right).expect("right JSON")
        );
        assert!(left.serialized_len() <= limits.max_output_bytes);
        assert!(left.truncated().correlation_ids() > 0);
    }

    #[test]
    fn per_section_caps_and_readiness_mapping_are_fail_closed() {
        let mut input = DiagnosticsSnapshotInput::new(now());
        for index in 0..10 {
            input.add_failed_job(format!("queue-{index}"), "retry", 1, None);
        }
        input.set_backup_health_from_readiness(
            &BackupPitrObservation {
                state: ReadinessObservationState::Unavailable,
                run_id: None,
                kind: None,
                status: None,
                started_at: Some(now() - Duration::hours(1)),
                completed_at: None,
                verified: false,
                has_error: true,
                restore_target_present: false,
                reason_code: Some("backup_unavailable".into()),
            },
            now(),
        );
        input.set_worker_health(
            ReadinessObservationState::Unavailable,
            None,
            None,
            Some("secret=should-drop"),
        );
        let snapshot = input
            .build(DiagnosticsLimits {
                max_failed_jobs: 3,
                ..DiagnosticsLimits::default()
            })
            .expect("bounded snapshot");
        assert_eq!(snapshot.failed_jobs().len(), 3);
        assert_eq!(snapshot.truncated().failed_jobs(), 7);
        assert_eq!(
            snapshot.backup_health().expect("backup").health().state(),
            "unavailable"
        );
        assert_eq!(
            snapshot.worker_health().expect("worker").reason_code(),
            None
        );

        let value: Value = serde_json::to_value(snapshot).expect("JSON value");
        assert!(value.get("raw_error").is_none());
        assert!(!value.to_string().contains("secret=should-drop"));
    }

    #[test]
    fn too_small_limit_fails_before_serialization() {
        let result = DiagnosticsSnapshotInput::new(now()).build(DiagnosticsLimits {
            max_output_bytes: MIN_MAX_DIAGNOSTICS_BYTES - 1,
            ..DiagnosticsLimits::default()
        });
        assert_eq!(result, Err(DiagnosticsError::OutputLimitTooSmall));
    }

    #[test]
    fn deduplication_is_stable() {
        let mut input = DiagnosticsSnapshotInput::new(now());
        input.add_integration_error("api", "timeout", true, None);
        input.add_integration_error("api", "timeout", true, None);
        input.add_correlation_id("corr");
        input.add_correlation_id("corr");
        let snapshot = input.build(DiagnosticsLimits::default()).expect("snapshot");
        assert_eq!(snapshot.integration_errors().len(), 1);
        assert_eq!(snapshot.correlation_ids(), &["corr"]);
    }

    #[test]
    fn persistence_boundary_rejects_cross_scope_and_stale_references() {
        assert!(validate_persistence_reference("holding/abc", "scope").is_ok());
        assert!(validate_persistence_reference("customer_email", "scope").is_ok());
        assert!(validate_persistence_reference("secret=value", "scope").is_err());
        assert!(validate_persistence_reference("scope with spaces", "scope").is_err());
        assert!(validate_snapshot_freshness(now(), now() + Duration::seconds(30)).is_ok());
        assert!(validate_snapshot_freshness(now(), now() + Duration::minutes(5)).is_ok());
        assert!(
            validate_snapshot_freshness(now(), now() + Duration::minutes(5) + Duration::seconds(1))
                .is_err()
        );
        assert!(validate_snapshot_freshness(now() + Duration::seconds(31), now()).is_err());
    }

    #[test]
    fn idempotency_replay_requires_immutable_event_binding() {
        let key = "support-diagnostics:test";
        let event_id = Uuid::from_u128(1);
        let aggregate_id = Uuid::from_u128(2);
        assert!(
            validate_diagnostics_idempotency_binding(
                key,
                Some(event_id),
                Some(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE),
                Some(aggregate_id),
                Some(event_id),
                event_id,
                aggregate_id,
            )
            .is_ok()
        );
        assert!(matches!(
            validate_diagnostics_idempotency_binding(
                key,
                Some(Uuid::from_u128(3)),
                Some(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE),
                Some(aggregate_id),
                Some(event_id),
                event_id,
                aggregate_id,
            ),
            Err(PersistenceError::IdempotencyConflict { .. })
        ));
        assert!(matches!(
            validate_diagnostics_idempotency_binding(
                key,
                Some(event_id),
                Some(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE),
                Some(Uuid::from_u128(4)),
                Some(event_id),
                event_id,
                aggregate_id,
            ),
            Err(PersistenceError::IdempotencyConflict { .. })
        ));
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL via ERPX_TEST_CONTROL_WRITE_DATABASE_URL"]
    async fn sqlx_support_diagnostics_audit_is_redacted_scoped_fresh_idempotent_and_reconciled() {
        let url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required for this ignored test");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .expect("control write database");
        erpx_migration::migrate_control_plane(&pool)
            .await
            .expect("control migrations");

        let captured_at = Utc::now();
        let observed_at = captured_at + Duration::seconds(1);
        let scope = format!("holding/{}", Uuid::new_v4());
        let correlation_id = Uuid::new_v4();
        let idempotency_key = format!("audit-{}", Uuid::new_v4());
        let mut input = DiagnosticsSnapshotInput::new(captured_at);
        input.add_version("server", "1.0.0");
        input.add_configuration("environment", "test");
        input.add_configuration("DATABASE_PASSWORD", "password=must-not-persist");
        input.add_configuration("customer_email", "person@example.test");
        let snapshot = input
            .build(DiagnosticsLimits::default())
            .expect("redacted snapshot");
        let request = SupportDiagnosticsAuditRequest::new(
            &scope,
            "support-agent",
            "diagnostics_read",
            correlation_id,
            &idempotency_key,
            observed_at,
        );

        let receipt = snapshot
            .persist_audit_snapshot(&pool, &request)
            .await
            .expect("persist diagnostics audit");
        assert_eq!(receipt.aggregate_version(), 1);
        assert!(!receipt.reused_idempotency());
        assert_eq!(receipt.scope(), scope);

        let replay = snapshot
            .persist_audit_snapshot(&pool, &request)
            .await
            .expect("replay diagnostics audit");
        assert!(replay.reused_idempotency());
        assert_eq!(replay.event_id(), receipt.event_id());

        let reconciled = DiagnosticsSnapshot::reconcile_audit_snapshot(&pool, &scope, observed_at)
            .await
            .expect("reconcile diagnostics audit");
        assert_eq!(reconciled.aggregate_id(), receipt.aggregate_id());
        assert_eq!(reconciled.aggregate_version(), 1);
        assert_eq!(reconciled.event_id(), receipt.event_id());
        assert_eq!(reconciled.content_hash(), receipt.content_hash());

        let second_captured_at = captured_at + Duration::seconds(2);
        let second_observed_at = second_captured_at + Duration::seconds(1);
        let second_idempotency_key = format!("audit-{}", Uuid::new_v4());
        let mut second_input = DiagnosticsSnapshotInput::new(second_captured_at);
        second_input.add_version("server", "1.0.1");
        second_input.add_configuration("environment", "test");
        let second_snapshot = second_input
            .build(DiagnosticsLimits::default())
            .expect("second redacted snapshot");
        let second_request = SupportDiagnosticsAuditRequest::new(
            &scope,
            "support-agent",
            "diagnostics_read",
            Uuid::new_v4(),
            &second_idempotency_key,
            second_observed_at,
        );
        let second_receipt = second_snapshot
            .persist_audit_snapshot(&pool, &second_request)
            .await
            .expect("persist second diagnostics audit");
        assert_eq!(second_receipt.aggregate_id(), receipt.aggregate_id());
        assert_eq!(second_receipt.aggregate_version(), 2);
        let latest_reconciled =
            DiagnosticsSnapshot::reconcile_audit_snapshot(&pool, &scope, second_observed_at)
                .await
                .expect("reconcile two-version diagnostics audit");
        assert_eq!(latest_reconciled.aggregate_version(), 2);
        assert_eq!(latest_reconciled.event_id(), second_receipt.event_id());
        assert_eq!(
            latest_reconciled.content_hash(),
            second_receipt.content_hash()
        );

        let state: Value = sqlx::query_scalar(
            "SELECT state FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = $1 AND aggregate_id = $2",
        )
        .bind(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE)
        .bind(second_receipt.aggregate_id())
        .fetch_one(&pool)
        .await
        .expect("persisted diagnostics state");
        let serialized = state.to_string();
        assert!(serialized.contains(&scope));
        assert!(serialized.contains("environment"));
        assert!(!serialized.contains("must-not-persist"));
        assert!(!serialized.contains("customer_email"));

        let stale = DiagnosticsSnapshotInput::new(captured_at - Duration::minutes(6))
            .build(DiagnosticsLimits::default())
            .expect("stale snapshot still builds before persistence freshness gate");
        let stale_scope = format!("holding/{}", Uuid::new_v4());
        let stale_key = format!("audit-{}", Uuid::new_v4());
        let stale_request = SupportDiagnosticsAuditRequest::new(
            &stale_scope,
            "support-agent",
            "diagnostics_read",
            Uuid::new_v4(),
            &stale_key,
            observed_at,
        );
        assert!(
            stale
                .persist_audit_snapshot(&pool, &stale_request)
                .await
                .expect_err("stale diagnostics must not persist")
                .to_string()
                .contains("stale")
        );

        sqlx::query(
            "UPDATE erpx_control.outbox_events
             SET payload = jsonb_build_object('scope', 'tampered')
             WHERE id = $1",
        )
        .bind(second_receipt.event_id())
        .execute(&pool)
        .await
        .expect("tamper test update");
        assert!(
            DiagnosticsSnapshot::reconcile_audit_snapshot(&pool, &scope, second_observed_at)
                .await
                .is_err(),
            "tampered outbox must fail reconciliation"
        );

        for event_id in [receipt.event_id(), second_receipt.event_id()] {
            sqlx::query("DELETE FROM erpx_control.outbox_events WHERE id = $1")
                .bind(event_id)
                .execute(&pool)
                .await
                .expect("diagnostics outbox cleanup");
            sqlx::query("DELETE FROM erpx_control.audit_events WHERE id = $1")
                .bind(event_id)
                .execute(&pool)
                .await
                .expect("diagnostics audit cleanup");
        }
        sqlx::query(
            "DELETE FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = $1 AND aggregate_id = $2",
        )
        .bind(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE)
        .bind(receipt.aggregate_id())
        .execute(&pool)
        .await
        .expect("diagnostics aggregate cleanup");
        sqlx::query("DELETE FROM erpx_control.idempotency_keys WHERE idempotency_key = $1")
            .bind(format!("support-diagnostics:{idempotency_key}"))
            .execute(&pool)
            .await
            .expect("diagnostics idempotency cleanup");
        sqlx::query("DELETE FROM erpx_control.idempotency_keys WHERE idempotency_key = $1")
            .bind(format!("support-diagnostics:{second_idempotency_key}"))
            .execute(&pool)
            .await
            .expect("second diagnostics idempotency cleanup");
        pool.close().await;
    }

    #[tokio::test]
    #[ignore = "requires disposable PostgreSQL via ERPX_TEST_CONTROL_WRITE_DATABASE_URL"]
    async fn sqlx_support_diagnostics_audit_rolls_back_snapshot_on_outbox_conflict() {
        let url = std::env::var("ERPX_TEST_CONTROL_WRITE_DATABASE_URL")
            .expect("ERPX_TEST_CONTROL_WRITE_DATABASE_URL is required for this ignored test");
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(4)
            .connect(&url)
            .await
            .expect("control write database");
        erpx_migration::migrate_control_plane(&pool)
            .await
            .expect("control migrations");

        let captured_at = Utc::now();
        let scope = format!("holding/{}", Uuid::new_v4());
        let idempotency_key = format!("conflict-{}", Uuid::new_v4());
        let stored_idempotency_key = format!("support-diagnostics:{idempotency_key}");
        let aggregate_id = support_diagnostics_aggregate_id(&scope);
        let event_id = support_diagnostics_event_id(&stored_idempotency_key);
        sqlx::query(
            "INSERT INTO erpx_control.outbox_events
                (id, event_type, event_version, aggregate_type, aggregate_id,
                 aggregate_version, correlation_id, payload, occurred_at)
             VALUES ($1, 'foreign.event', 1, $2, $3, 1, $4, '{}'::jsonb, $5)",
        )
        .bind(event_id)
        .bind(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE)
        .bind(aggregate_id)
        .bind(Uuid::new_v4())
        .bind(captured_at)
        .execute(&pool)
        .await
        .expect("seed conflicting event");

        let snapshot = DiagnosticsSnapshotInput::new(captured_at)
            .build(DiagnosticsLimits::default())
            .expect("snapshot");
        let request = SupportDiagnosticsAuditRequest::new(
            &scope,
            "support-agent",
            "diagnostics_read",
            Uuid::new_v4(),
            &idempotency_key,
            captured_at,
        );
        assert!(
            snapshot
                .persist_audit_snapshot(&pool, &request)
                .await
                .is_err()
        );

        let aggregate_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_control.aggregate_snapshots
             WHERE aggregate_type = $1 AND aggregate_id = $2",
        )
        .bind(SUPPORT_DIAGNOSTICS_AUDIT_AGGREGATE)
        .bind(aggregate_id)
        .fetch_one(&pool)
        .await
        .expect("aggregate rollback count");
        let idempotency_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM erpx_control.idempotency_keys WHERE idempotency_key = $1",
        )
        .bind(&stored_idempotency_key)
        .fetch_one(&pool)
        .await
        .expect("idempotency rollback count");
        assert_eq!(aggregate_count, 0);
        assert_eq!(idempotency_count, 0);

        sqlx::query("DELETE FROM erpx_control.outbox_events WHERE id = $1")
            .bind(event_id)
            .execute(&pool)
            .await
            .expect("conflicting event cleanup");
        pool.close().await;
    }
}
