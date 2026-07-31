//! SQLx write boundary for authoritative Finance close-control evidence.
//!
//! This adapter is intentionally separate from close-review persistence.  A
//! source Finance context owns one control row and may publish a newer result
//! until the accounting period or its close review is closed. Source versions
//! are monotonic per control, so stale events cannot overwrite newer results.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sqlx::{PgPool, Postgres, Transaction};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::begin_scoped;

pub const MAX_FINANCE_CLOSE_CONTROL_CODE_BYTES: usize = 128;
pub const MAX_FINANCE_CLOSE_CONTROL_SOURCE_TYPE_BYTES: usize = 128;
pub const MAX_FINANCE_CLOSE_CONTROL_EVIDENCE_REF_BYTES: usize = 255;
pub const MAX_FINANCE_CLOSE_CONTROL_PAYLOAD_BYTES: usize = 64 * 1024;
pub(crate) const FINANCE_CLOSE_CONTROL_AGGREGATE_TYPE: &str = "finance_close_control_evidence";
pub(crate) const FINANCE_CLOSE_CONTROL_EVENT_TYPE: &str =
    "finance.close_control_evidence_published.v1";

type ExistingEvidenceRow = (
    Uuid,
    Uuid,
    String,
    String,
    Uuid,
    i64,
    String,
    bool,
    DateTime<Utc>,
    Value,
);

/// A source-owned control result.  The identity fields are repeated in the
/// row so the adapter can bind a result to one holding, company, and period
/// before writing it; they are not inferred from caller-selected SQL values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FinanceCloseControlEvidenceSourceRecord {
    pub holding_id: Uuid,
    pub company_id: Uuid,
    pub period_id: Uuid,
    pub control_code: String,
    pub source_type: String,
    pub source_aggregate_id: Uuid,
    pub source_version: u64,
    pub evidence_ref: String,
    pub resolved: bool,
    pub observed_at: DateTime<Utc>,
    pub payload: Value,
}

impl FinanceCloseControlEvidenceSourceRecord {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        holding_id: Uuid,
        company_id: Uuid,
        period_id: Uuid,
        control_code: impl Into<String>,
        source_type: impl Into<String>,
        source_aggregate_id: Uuid,
        source_version: u64,
        evidence_ref: impl Into<String>,
        resolved: bool,
        observed_at: DateTime<Utc>,
        payload: Value,
    ) -> Result<Self, PersistenceError> {
        let record = Self {
            holding_id,
            company_id,
            period_id,
            control_code: control_code.into(),
            source_type: source_type.into(),
            source_aggregate_id,
            source_version,
            evidence_ref: evidence_ref.into(),
            resolved,
            observed_at,
            payload,
        };
        validate_record(&record)?;
        Ok(record)
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceCloseControlEvidenceWriteReceipt {
    pub control_code: String,
    pub source_aggregate_id: Uuid,
    pub source_version: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct SqlxFinanceCloseControlEvidenceStore {
    write_pool: PgPool,
    holding_ref: String,
}

impl SqlxFinanceCloseControlEvidenceStore {
    pub fn new(
        write_pool: PgPool,
        holding_ref: impl Into<String>,
    ) -> Result<Self, PersistenceError> {
        let holding_ref = holding_ref.into();
        validate_holding_ref(&holding_ref)?;
        Ok(Self {
            write_pool,
            holding_ref,
        })
    }

    pub fn holding_ref(&self) -> &str {
        &self.holding_ref
    }

    /// Publish the current result for one source control.
    ///
    /// The period and close-review rows are checked inside the same scoped
    /// transaction as the write.  A matching row is returned as a replay;
    /// otherwise only a strictly newer source version may replace the
    /// evidence fields. Database triggers remain the final race defense for a
    /// close that commits concurrently.
    pub async fn publish(
        &self,
        record: &FinanceCloseControlEvidenceSourceRecord,
    ) -> Result<FinanceCloseControlEvidenceWriteReceipt, PersistenceError> {
        validate_record(record)?;
        let mut tx = begin_scoped(&self.write_pool, &self.holding_ref).await?;
        lock_control(&mut tx, &self.holding_ref, record).await?;
        validate_period_identity(&mut tx, &self.holding_ref, record).await?;
        reject_closed_period(&mut tx, &self.holding_ref, record).await?;

        let existing: Option<ExistingEvidenceRow> = sqlx::query_as(
            "SELECT holding_id, company_id, control_code, source_type,
                    source_aggregate_id, source_version, evidence_ref,
                    resolved, observed_at, payload
             FROM erpx_tenant.finance_close_control_evidence
             WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3
               AND period_id=$4 AND control_code=$5
             FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(record.holding_id)
        .bind(record.company_id)
        .bind(record.period_id)
        .bind(&record.control_code)
        .fetch_optional(&mut *tx)
        .await?;

        let receipt = if let Some(row) = existing {
            validate_existing_identity(&row, record)?;
            if row.3 != record.source_type || row.4 != record.source_aggregate_id {
                return Err(invalid(
                    "Finance close control source aggregate identity cannot change",
                ));
            }
            let existing_version = u64::try_from(row.5).map_err(|_| {
                invalid("persisted Finance close control source version is invalid")
            })?;
            if record.source_version < existing_version {
                return Err(invalid("Finance close control source version is stale"));
            }
            if record.source_version == existing_version {
                if row.6 != record.evidence_ref
                    || row.7 != record.resolved
                    || row.8 != record.observed_at
                    || row.9 != record.payload
                {
                    return Err(invalid(
                        "Finance close control source version is already bound to different evidence",
                    ));
                }
                let outbox_event_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*)
                     FROM erpx_tenant.outbox_events
                     WHERE holding_ref=$1
                       AND aggregate_type=$2
                       AND aggregate_id=$3
                       AND aggregate_version=$4
                       AND event_type=$5",
                )
                .bind(&self.holding_ref)
                .bind(FINANCE_CLOSE_CONTROL_AGGREGATE_TYPE)
                .bind(deterministic_control_aggregate_id(
                    &self.holding_ref,
                    record,
                ))
                .bind(as_i64(record.source_version)?)
                .bind(FINANCE_CLOSE_CONTROL_EVENT_TYPE)
                .fetch_one(&mut *tx)
                .await?;
                validate_source_outbox_cardinality(record.source_version, outbox_event_count)?;
                FinanceCloseControlEvidenceWriteReceipt {
                    control_code: record.control_code.clone(),
                    source_aggregate_id: record.source_aggregate_id,
                    source_version: existing_version,
                    replayed: true,
                }
            } else {
                let target_outbox_event_count: i64 = sqlx::query_scalar(
                    "SELECT COUNT(*)
                     FROM erpx_tenant.outbox_events
                     WHERE holding_ref=$1
                       AND aggregate_type=$2
                       AND aggregate_id=$3
                       AND aggregate_version=$4
                       AND event_type=$5",
                )
                .bind(&self.holding_ref)
                .bind(FINANCE_CLOSE_CONTROL_AGGREGATE_TYPE)
                .bind(deterministic_control_aggregate_id(
                    &self.holding_ref,
                    record,
                ))
                .bind(as_i64(record.source_version)?)
                .bind(FINANCE_CLOSE_CONTROL_EVENT_TYPE)
                .fetch_one(&mut *tx)
                .await?;
                validate_new_source_outbox_slot(record.source_version, target_outbox_event_count)?;
                sqlx::query(
                    "UPDATE erpx_tenant.finance_close_control_evidence
                     SET source_version=$6, evidence_ref=$7, resolved=$8,
                         observed_at=$9, payload=$10, updated_at=now()
                     WHERE holding_ref=$1 AND holding_id=$2 AND company_id=$3
                       AND period_id=$4 AND control_code=$5",
                )
                .bind(&self.holding_ref)
                .bind(record.holding_id)
                .bind(record.company_id)
                .bind(record.period_id)
                .bind(&record.control_code)
                .bind(as_i64(record.source_version)?)
                .bind(&record.evidence_ref)
                .bind(record.resolved)
                .bind(record.observed_at)
                .bind(&record.payload)
                .execute(&mut *tx)
                .await?;
                FinanceCloseControlEvidenceWriteReceipt {
                    control_code: record.control_code.clone(),
                    source_aggregate_id: record.source_aggregate_id,
                    source_version: record.source_version,
                    replayed: false,
                }
            }
        } else {
            sqlx::query(
                "INSERT INTO erpx_tenant.finance_close_control_evidence
                     (holding_ref, holding_id, company_id, period_id,
                     control_code, source_type, source_aggregate_id,
                     source_version, evidence_ref, resolved, observed_at, payload)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)",
            )
            .bind(&self.holding_ref)
            .bind(record.holding_id)
            .bind(record.company_id)
            .bind(record.period_id)
            .bind(&record.control_code)
            .bind(&record.source_type)
            .bind(record.source_aggregate_id)
            .bind(as_i64(record.source_version)?)
            .bind(&record.evidence_ref)
            .bind(record.resolved)
            .bind(record.observed_at)
            .bind(&record.payload)
            .execute(&mut *tx)
            .await?;
            FinanceCloseControlEvidenceWriteReceipt {
                control_code: record.control_code.clone(),
                source_aggregate_id: record.source_aggregate_id,
                source_version: record.source_version,
                replayed: false,
            }
        };

        if !receipt.replayed {
            append_source_outbox_event(&mut tx, &self.holding_ref, record).await?;
        }
        tx.commit().await?;
        Ok(receipt)
    }
}

async fn append_source_outbox_event(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    record: &FinanceCloseControlEvidenceSourceRecord,
) -> Result<(), PersistenceError> {
    let event_payload = build_source_outbox_payload(holding_ref, record)?;
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,now())",
    )
    .bind(Uuid::new_v4())
    .bind(FINANCE_CLOSE_CONTROL_AGGREGATE_TYPE)
    .bind(deterministic_control_aggregate_id(holding_ref, record))
    .bind(holding_ref)
    .bind(as_i64(record.source_version)?)
    .bind(FINANCE_CLOSE_CONTROL_EVENT_TYPE)
    .bind(event_payload)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn build_source_outbox_payload(
    holding_ref: &str,
    record: &FinanceCloseControlEvidenceSourceRecord,
) -> Result<Value, PersistenceError> {
    let source_event_key = format!(
        "{holding_ref}:{FINANCE_CLOSE_CONTROL_AGGREGATE_TYPE}:{}:{}:{}:{}:{}:{}",
        record.holding_id,
        record.company_id,
        record.period_id,
        record.control_code,
        record.source_aggregate_id,
        record.source_version,
    );
    let aggregate = serde_json::json!({
        "event_key": source_event_key,
        "source": {
            "type": record.source_type,
            "aggregate_id": record.source_aggregate_id,
            "version": record.source_version,
        },
        "control": {
            "holding_id": record.holding_id,
            "company_id": record.company_id,
            "period_id": record.period_id,
            "code": record.control_code,
        },
        "evidence": {
            "reference": record.evidence_ref,
            "resolved": record.resolved,
            "observed_at": record.observed_at,
            "payload": record.payload,
        },
    });
    let event_payload = serde_json::json!({
        "aggregate_type": FINANCE_CLOSE_CONTROL_AGGREGATE_TYPE,
        "aggregate": aggregate,
        "version": record.source_version,
    });
    let event_payload_bytes = serde_json::to_vec(&event_payload)?.len();
    if event_payload_bytes > MAX_FINANCE_CLOSE_CONTROL_PAYLOAD_BYTES + 4096 {
        return Err(invalid(
            "Finance close control outbox payload exceeds bounded size",
        ));
    }
    Ok(event_payload)
}

fn deterministic_control_aggregate_id(
    holding_ref: &str,
    record: &FinanceCloseControlEvidenceSourceRecord,
) -> Uuid {
    deterministic_control_aggregate_id_for_scope(
        holding_ref,
        record.holding_id,
        record.company_id,
        record.period_id,
        &record.control_code,
    )
}

pub(crate) fn deterministic_control_aggregate_id_for_scope(
    holding_ref: &str,
    holding_id: Uuid,
    company_id: Uuid,
    period_id: Uuid,
    control_code: &str,
) -> Uuid {
    use sha2::{Digest, Sha256};

    let digest = Sha256::digest(
        format!(
            "erpx/finance/close-control/v1/{holding_ref}/{}/{}/{}/{}",
            holding_id, company_id, period_id, control_code,
        )
        .as_bytes(),
    );
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x50;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

async fn lock_control(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    record: &FinanceCloseControlEvidenceSourceRecord,
) -> Result<(), PersistenceError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!(
            "{holding_ref}:finance-close-control:{}:{}:{}",
            record.company_id, record.period_id, record.control_code
        ))
        .execute(&mut **tx)
        .await?;
    Ok(())
}

async fn validate_period_identity(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    record: &FinanceCloseControlEvidenceSourceRecord,
) -> Result<(), PersistenceError> {
    let period: Option<(Uuid, Uuid, String)> = sqlx::query_as(
        "SELECT holding_id, company_id, status
         FROM erpx_tenant.finance_periods
         WHERE holding_ref=$1 AND period_id=$2
         FOR UPDATE",
    )
    .bind(holding_ref)
    .bind(record.period_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((holding_id, company_id, status)) = period else {
        return Err(PersistenceError::NotFound {
            entity: "Finance accounting period",
            id: record.period_id,
        });
    };
    if holding_id != record.holding_id || company_id != record.company_id {
        return Err(invalid(
            "Finance close control holding/company does not match accounting period",
        ));
    }
    if status == "closed" {
        return Err(invalid(
            "Finance close control evidence cannot mutate a closed accounting period",
        ));
    }
    Ok(())
}

async fn reject_closed_period(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    record: &FinanceCloseControlEvidenceSourceRecord,
) -> Result<(), PersistenceError> {
    let closed_review: bool = sqlx::query_scalar(
        "SELECT EXISTS(
             SELECT 1
             FROM erpx_tenant.finance_close_reviews
             WHERE holding_ref=$1 AND company_id=$2 AND period_id=$3
               AND status='closed'
         )",
    )
    .bind(holding_ref)
    .bind(record.company_id)
    .bind(record.period_id)
    .fetch_one(&mut **tx)
    .await?;
    if closed_review {
        return Err(invalid(
            "Finance close control evidence cannot mutate after close review closure",
        ));
    }
    Ok(())
}

fn validate_existing_identity(
    row: &ExistingEvidenceRow,
    record: &FinanceCloseControlEvidenceSourceRecord,
) -> Result<(), PersistenceError> {
    if row.0 != record.holding_id || row.1 != record.company_id || row.2 != record.control_code {
        return Err(invalid(
            "persisted Finance close control identity does not match its key",
        ));
    }
    if row.3.trim().is_empty() || row.4.is_nil() {
        return Err(invalid(
            "persisted Finance close control source identity is invalid",
        ));
    }
    Ok(())
}

fn validate_record(
    record: &FinanceCloseControlEvidenceSourceRecord,
) -> Result<(), PersistenceError> {
    for (value, field, max) in [
        (
            record.control_code.as_str(),
            "Finance close control code",
            MAX_FINANCE_CLOSE_CONTROL_CODE_BYTES,
        ),
        (
            record.source_type.as_str(),
            "Finance close control source type",
            MAX_FINANCE_CLOSE_CONTROL_SOURCE_TYPE_BYTES,
        ),
        (
            record.evidence_ref.as_str(),
            "Finance close control evidence reference",
            MAX_FINANCE_CLOSE_CONTROL_EVIDENCE_REF_BYTES,
        ),
    ] {
        if value.trim().is_empty() || value.len() > max || value.chars().any(char::is_control) {
            return Err(invalid(format!("{field} is invalid")));
        }
    }
    for (value, field) in [
        (record.holding_id, "holding_id"),
        (record.company_id, "company_id"),
        (record.period_id, "period_id"),
        (record.source_aggregate_id, "source_aggregate_id"),
    ] {
        if value.is_nil() {
            return Err(invalid(format!(
                "Finance close control {field} cannot be nil"
            )));
        }
    }
    let payload_bytes = serde_json::to_vec(&record.payload)?.len();
    if payload_bytes > MAX_FINANCE_CLOSE_CONTROL_PAYLOAD_BYTES {
        return Err(invalid(format!(
            "Finance close control payload exceeds {MAX_FINANCE_CLOSE_CONTROL_PAYLOAD_BYTES} bytes"
        )));
    }
    if record.source_version == 0 {
        return Err(invalid(
            "Finance close control source version must be positive",
        ));
    }
    if !record.payload.is_object() {
        return Err(invalid(
            "Finance close control payload must be a JSON object",
        ));
    }
    validate_optional_source_identity(record)
}

fn validate_optional_source_identity(
    record: &FinanceCloseControlEvidenceSourceRecord,
) -> Result<(), PersistenceError> {
    for key in ["aggregate_id", "source_aggregate_id"] {
        if let Some(value) = record.payload.get(key) {
            let Some(value) = value.as_str() else {
                return Err(invalid(format!(
                    "Finance close control payload field {key} must be a UUID string"
                )));
            };
            let parsed = Uuid::parse_str(value).map_err(|_| {
                invalid(format!(
                    "Finance close control payload field {key} is not a UUID"
                ))
            })?;
            if parsed != record.source_aggregate_id {
                return Err(invalid(format!(
                    "Finance close control payload field {key} does not match source aggregate"
                )));
            }
        }
    }
    if let Some(value) = record.payload.get("source_version") {
        let Some(value) = value.as_u64() else {
            return Err(invalid(
                "Finance close control payload source_version must be an unsigned integer",
            ));
        };
        if value != record.source_version {
            return Err(invalid(
                "Finance close control payload source_version does not match source identity",
            ));
        }
    }
    match record.payload.get("source_type").and_then(Value::as_str) {
        Some(value) if value != record.source_type => {
            return Err(invalid(
                "Finance close control payload source_type does not match source identity",
            ));
        }
        _ => {}
    }
    Ok(())
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn invalid(message: impl Into<String>) -> PersistenceError {
    PersistenceError::InvalidData(message.into())
}

fn as_i64(value: u64) -> Result<i64, PersistenceError> {
    i64::try_from(value).map_err(|_| invalid("Finance close control source version exceeds BIGINT"))
}

fn validate_source_outbox_cardinality(
    source_version: u64,
    outbox_event_count: i64,
) -> Result<(), PersistenceError> {
    if outbox_event_count != 1 {
        return Err(invalid(format!(
            "Finance close control source version {source_version} requires exactly one authoritative outbox event, found {outbox_event_count}"
        )));
    }
    Ok(())
}

fn validate_new_source_outbox_slot(
    source_version: u64,
    outbox_event_count: i64,
) -> Result<(), PersistenceError> {
    if outbox_event_count != 0 {
        return Err(invalid(format!(
            "Finance close control source version {source_version} already has authoritative outbox evidence"
        )));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_record() -> FinanceCloseControlEvidenceSourceRecord {
        let source_aggregate_id = Uuid::new_v4();
        FinanceCloseControlEvidenceSourceRecord::new(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            "bank_reconciliation",
            "finance_bank_reconciliation",
            source_aggregate_id,
            1,
            "bank-reconciliation-evidence-1",
            true,
            Utc::now(),
            serde_json::json!({
                "source_type": "finance_bank_reconciliation",
                "aggregate_id": source_aggregate_id,
            }),
        )
        .expect("valid evidence")
    }

    #[test]
    fn rejects_unbounded_or_unsafe_source_evidence() {
        let mut record = sample_record();
        record.control_code = "\n".into();
        assert!(validate_record(&record).is_err());

        let mut record = sample_record();
        record.payload = serde_json::json!(["not", "an", "object"]);
        assert!(validate_record(&record).is_err());

        let mut record = sample_record();
        record.payload = serde_json::json!({
            "aggregate_id": Uuid::new_v4().to_string(),
        });
        assert!(validate_record(&record).is_err());
    }

    #[test]
    fn permits_same_source_identity_to_publish_new_evidence() {
        let record = sample_record();
        let mut updated = record.clone();
        updated.evidence_ref = "bank-reconciliation-evidence-2".into();
        updated.resolved = false;
        assert!(validate_record(&record).is_ok());
        assert!(validate_record(&updated).is_ok());
        assert_eq!(record.source_aggregate_id, updated.source_aggregate_id);
    }

    #[test]
    fn source_outbox_payload_is_bounded_and_keyed_by_source_version() {
        let record = sample_record();
        let holding_ref = "holding-test";
        let payload = build_source_outbox_payload(holding_ref, &record).expect("event payload");
        assert_eq!(
            payload["aggregate_type"],
            FINANCE_CLOSE_CONTROL_AGGREGATE_TYPE
        );
        assert_eq!(payload["version"], record.source_version);
        assert_eq!(payload["aggregate"]["source"]["type"], record.source_type);
        assert_eq!(
            payload["aggregate"]["source"]["aggregate_id"],
            record.source_aggregate_id.to_string()
        );
        assert_eq!(
            payload["aggregate"]["source"]["version"],
            record.source_version
        );
        assert_eq!(
            payload["aggregate"]["control"]["period_id"],
            record.period_id.to_string()
        );
        assert_eq!(payload["aggregate"]["evidence"]["payload"], record.payload);
        assert_eq!(
            payload["aggregate"]["event_key"],
            format!(
                "{holding_ref}:{FINANCE_CLOSE_CONTROL_AGGREGATE_TYPE}:{}:{}:{}:{}:{}:{}",
                record.holding_id,
                record.company_id,
                record.period_id,
                record.control_code,
                record.source_aggregate_id,
                record.source_version,
            )
        );
        assert!(
            serde_json::to_vec(&payload)
                .expect("serialize event payload")
                .len()
                <= 68 * 1024
        );
    }

    #[test]
    fn source_outbox_aggregate_identity_is_control_scoped() {
        let record = sample_record();
        let mut other_control = record.clone();
        other_control.control_code = "invoice_reconciliation".into();
        assert_ne!(
            deterministic_control_aggregate_id("holding-test", &record),
            deterministic_control_aggregate_id("holding-test", &other_control)
        );
    }

    #[test]
    fn source_replay_rejects_missing_or_duplicate_outbox_evidence() {
        assert!(validate_source_outbox_cardinality(3, 0).is_err());
        assert!(validate_source_outbox_cardinality(3, 2).is_err());
        validate_source_outbox_cardinality(3, 1)
            .expect("one source-version outbox event is the only valid cardinality");
    }

    #[test]
    fn newer_source_version_requires_an_empty_outbox_slot() {
        assert!(validate_new_source_outbox_slot(4, 1).is_err());
        assert!(validate_new_source_outbox_slot(4, 2).is_err());
        validate_new_source_outbox_slot(4, 0)
            .expect("a newer source version must append its event exactly once");
    }
}
