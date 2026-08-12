//! Tenant-write persistence for the typed POS sale aggregate.
//!
//! POS source state is normalized into sale, line, and payment rows.  The
//! shared tenant outbox is the only serialized transport boundary; no POS
//! aggregate JSON is stored in source tables and payment-instrument secrets
//! are not represented by this adapter.

use chrono::{DateTime, Utc};
use erpx_domain::contracts::TenantScope;
use erpx_domain::pos::{
    Payment, PaymentMethod, PosEventMetadata, PosEventType, PosScopedReference, Sale, SaleLine,
    SaleStatus,
};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

use crate::PersistenceError;
use crate::finance_persistence::begin_scoped;

const POS_AGGREGATE: &str = "pos_sale";
const MAX_HOLDING_REF_BYTES: usize = 255;
const MAX_IDEMPOTENCY_KEY_BYTES: usize = 512;

/// The typed result of an accepted or replayed POS source mutation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PosWriteReceipt {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub version: u64,
    pub idempotency_key: String,
    pub event_id: Uuid,
    pub replayed: bool,
}

#[derive(Debug, Clone)]
pub struct SqlxPosPersistence {
    write_pool: PgPool,
    holding_ref: String,
}

impl SqlxPosPersistence {
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

    /// Persist the complete validated POS sale state and its source event in
    /// one tenant-scoped transaction.
    pub async fn save_sale(
        &self,
        sale: &Sale,
        expected_version: Option<u64>,
    ) -> Result<PosWriteReceipt, PersistenceError> {
        validate_sale_for_persistence(sale)?;
        let command = sale
            .command_history
            .last()
            .ok_or_else(|| invalid("POS sale command history is empty"))?;
        let idempotency_key = command.idempotency_key.as_str();
        validate_idempotency_key(idempotency_key)?;
        let request_hash = request_hash(sale, expected_version)?;
        let mut tx = self.begin_write().await?;
        lock_pos_transaction(
            &mut tx,
            pos_idempotency_lock_name(&self.holding_ref, idempotency_key),
        )
        .await?;
        lock_pos_transaction(&mut tx, pos_aggregate_lock_name(&self.holding_ref, sale.id)).await?;

        let event = &command.event;
        if let Some(receipt) = find_replay(
            &mut tx,
            &self.holding_ref,
            idempotency_key,
            &request_hash,
            sale,
            event,
        )
        .await?
        {
            tx.commit().await?;
            return Ok(receipt);
        }

        let current = sqlx::query(
            "SELECT version, status, holding_id, company_id, branch_id
             FROM erpx_tenant.pos_sales
             WHERE holding_ref=$1 AND sale_id=$2
             FOR UPDATE",
        )
        .bind(&self.holding_ref)
        .bind(sale.id)
        .fetch_optional(&mut *tx)
        .await?
        .map(|row| {
            Ok::<_, PersistenceError>((
                as_u64(row.try_get("version")?, "POS sale version")?,
                row.try_get::<String, _>("status")?,
                row.try_get::<Uuid, _>("holding_id")?,
                row.try_get::<Uuid, _>("company_id")?,
                row.try_get::<Option<Uuid>, _>("branch_id")?,
            ))
        })
        .transpose()?;

        if let Some((current_version, current_status, ..)) = &current {
            if matches!(current_status.as_str(), "voided" | "refunded") {
                return Err(invalid("terminal POS sale is immutable"));
            }
            if expected_version != Some(*current_version) {
                return Err(PersistenceError::OptimisticConcurrency {
                    aggregate_type: POS_AGGREGATE.into(),
                    aggregate_id: sale.id,
                    expected: expected_version,
                    actual: Some(*current_version),
                });
            }
        } else if expected_version.is_some() {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: POS_AGGREGATE.into(),
                aggregate_id: sale.id,
                expected: expected_version,
                actual: None,
            });
        }

        ensure_source_event_continuity(
            &mut tx,
            &self.holding_ref,
            sale.id,
            current.as_ref().map(|(version, ..)| *version),
        )
        .await?;
        ensure_persisted_payments_are_preserved(
            &mut tx,
            &self.holding_ref,
            sale.id,
            current.as_ref().map(|(version, ..)| *version),
            &sale.payments,
            event.event_type,
        )
        .await?;

        let expected_next = current
            .as_ref()
            .map_or(1, |(version, ..)| version.saturating_add(1));
        if sale.version != expected_next {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: POS_AGGREGATE.into(),
                aggregate_id: sale.id,
                expected: Some(expected_next),
                actual: Some(sale.version),
            });
        }

        let sale_scope = scope_columns(&sale.scope)?;
        if let Some((_, _, holding_id, company_id, branch_id)) = current.as_ref()
            && (*holding_id != sale_scope.holding_id
                || sale_scope.company_id != Some(*company_id)
                || *branch_id != sale_scope.branch_id)
        {
            return Err(invalid("POS sale scope is immutable"));
        }
        let customer = optional_reference_columns(sale.customer.as_ref(), "customer")?;
        let register = optional_reference_columns(sale.register.as_ref(), "register")?;

        if let Some((current_version, ..)) = current.as_ref() {
            sqlx::query(
                "UPDATE erpx_tenant.pos_sales
                 SET holding_id=$3, company_id=$4, branch_id=$5, reference=$6,
                     customer_id=$7, customer_scope_kind=$8, customer_scope_holding_id=$9,
                     customer_scope_company_id=$10, customer_scope_branch_id=$11,
                     register_id=$12, register_scope_kind=$13, register_scope_holding_id=$14,
                     register_scope_company_id=$15, register_scope_branch_id=$16,
                     currency=$17, subtotal_minor=$18, tax_minor=$19, total_minor=$20,
                     paid_minor=$21, status=$22, version=$23, command_id=$24,
                     idempotency_key=$25, created_at=$26, updated_at=$27
                 WHERE holding_ref=$1 AND sale_id=$2 AND version=$28",
            )
            .bind(&self.holding_ref)
            .bind(sale.id)
            .bind(sale_scope.holding_id)
            .bind(sale_scope.company_id)
            .bind(sale_scope.branch_id)
            .bind(&sale.reference)
            .bind(customer.id)
            .bind(customer.kind)
            .bind(customer.holding_id)
            .bind(customer.company_id)
            .bind(customer.branch_id)
            .bind(register.id)
            .bind(register.kind)
            .bind(register.holding_id)
            .bind(register.company_id)
            .bind(register.branch_id)
            .bind(&sale.currency)
            .bind(sale.subtotal_minor)
            .bind(sale.tax_minor)
            .bind(sale.total_minor)
            .bind(sale.paid_minor)
            .bind(sale_status(sale.status))
            .bind(as_i64(sale.version, "POS sale version")?)
            .bind(command.command_id)
            .bind(idempotency_key)
            .bind(sale.created_at)
            .bind(sale.updated_at)
            .bind(as_i64(*current_version, "POS sale version")?)
            .execute(&mut *tx)
            .await?
            .rows_affected();
        } else {
            sqlx::query(
                "INSERT INTO erpx_tenant.pos_sales
                    (holding_ref, sale_id, holding_id, company_id, branch_id, reference,
                     customer_id, customer_scope_kind, customer_scope_holding_id,
                     customer_scope_company_id, customer_scope_branch_id,
                     register_id, register_scope_kind, register_scope_holding_id,
                     register_scope_company_id, register_scope_branch_id, currency,
                     subtotal_minor, tax_minor, total_minor, paid_minor, status, version,
                     command_id, idempotency_key, created_at, updated_at)
                 VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16,$17,
                         $18,$19,$20,$21,$22,$23,$24,$25,$26,$27)",
            )
            .bind(&self.holding_ref)
            .bind(sale.id)
            .bind(sale_scope.holding_id)
            .bind(sale_scope.company_id)
            .bind(sale_scope.branch_id)
            .bind(&sale.reference)
            .bind(customer.id)
            .bind(customer.kind)
            .bind(customer.holding_id)
            .bind(customer.company_id)
            .bind(customer.branch_id)
            .bind(register.id)
            .bind(register.kind)
            .bind(register.holding_id)
            .bind(register.company_id)
            .bind(register.branch_id)
            .bind(&sale.currency)
            .bind(sale.subtotal_minor)
            .bind(sale.tax_minor)
            .bind(sale.total_minor)
            .bind(sale.paid_minor)
            .bind(sale_status(sale.status))
            .bind(as_i64(sale.version, "POS sale version")?)
            .bind(command.command_id)
            .bind(idempotency_key)
            .bind(sale.created_at)
            .bind(sale.updated_at)
            .execute(&mut *tx)
            .await?;
        }

        sqlx::query(
            "DELETE FROM erpx_tenant.pos_sale_lines
             WHERE holding_ref=$1 AND sale_id=$2",
        )
        .bind(&self.holding_ref)
        .bind(sale.id)
        .execute(&mut *tx)
        .await?;
        for (line_no, line) in sale.lines.iter().enumerate() {
            insert_line(&mut tx, &self.holding_ref, sale.id, line_no, line).await?;
        }

        sqlx::query(
            "DELETE FROM erpx_tenant.pos_sale_payments
             WHERE holding_ref=$1 AND sale_id=$2",
        )
        .bind(&self.holding_ref)
        .bind(sale.id)
        .execute(&mut *tx)
        .await?;
        for payment in &sale.payments {
            insert_payment(&mut tx, &self.holding_ref, sale.id, payment).await?;
        }

        append_outbox_event(&mut tx, &self.holding_ref, sale, event).await?;
        let receipt = PosWriteReceipt {
            aggregate_type: POS_AGGREGATE.into(),
            aggregate_id: sale.id,
            version: sale.version,
            idempotency_key: idempotency_key.into(),
            event_id: event.event_id,
            replayed: false,
        };
        sqlx::query(
            "INSERT INTO erpx_tenant.pos_command_idempotency
                (holding_ref, idempotency_key, request_hash, command_id,
                 aggregate_type, aggregate_id, aggregate_version, event_id)
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8)",
        )
        .bind(&self.holding_ref)
        .bind(idempotency_key)
        .bind(&request_hash)
        .bind(command.command_id)
        .bind(POS_AGGREGATE)
        .bind(sale.id)
        .bind(as_i64(sale.version, "POS sale version")?)
        .bind(event.event_id)
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.write_pool, &self.holding_ref).await
    }
}

fn pos_idempotency_lock_name(holding_ref: &str, idempotency_key: &str) -> String {
    format!("pos:idempotency:{holding_ref}:{idempotency_key}")
}

fn pos_aggregate_lock_name(holding_ref: &str, aggregate_id: Uuid) -> String {
    format!("pos:aggregate:{holding_ref}:{aggregate_id}")
}

async fn lock_pos_transaction(
    tx: &mut Transaction<'_, Postgres>,
    lock_name: String,
) -> Result<(), PersistenceError> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(lock_name)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

fn validate_persisted_source_event(row: &PgRow) -> Result<(Uuid, u64), PersistenceError> {
    let event_id: Uuid = row.try_get("event_id")?;
    let aggregate_type: String = row.try_get("aggregate_type")?;
    let aggregate_id: Uuid = row.try_get("aggregate_id")?;
    let aggregate_version = as_u64(
        row.try_get("aggregate_version")?,
        "POS source event version",
    )?;
    let event_type_name: String = row.try_get("event_type")?;
    let occurred_at: DateTime<Utc> = row.try_get("occurred_at")?;
    let payload: serde_json::Value = row.try_get("payload")?;
    let stored: StoredPosOutboxPayload = serde_json::from_value(payload)?;
    stored
        .aggregate
        .validate()
        .map_err(|error| invalid(format!("invalid persisted POS source aggregate: {error}")))?;
    stored
        .event
        .validate()
        .map_err(|error| invalid(format!("invalid persisted POS source event: {error}")))?;
    let latest_event = stored
        .aggregate
        .command_history
        .last()
        .map(|command| &command.event);
    let expected_tenant_scope = serde_json::to_value(&stored.event.tenant_scope)?;
    let expected_actor = serde_json::to_value(&stored.event.actor)?;
    let envelope_matches = aggregate_type == POS_AGGREGATE
        && event_type_name == event_type(stored.event.event_type)
        && row.try_get::<Option<i32>, _>("contract_version")?
            == Some(i32::from(stored.event.contract_version))
        && row.try_get::<Option<i32>, _>("event_version")?
            == Some(i32::from(stored.event.event_version))
        && row
            .try_get::<Option<serde_json::Value>, _>("tenant_scope")?
            .is_some_and(|scope| scope == expected_tenant_scope)
        && row
            .try_get::<Option<serde_json::Value>, _>("actor")?
            .is_some_and(|actor| actor == expected_actor)
        && row
            .try_get::<Option<Uuid>, _>("correlation_id")?
            .is_some_and(|correlation_id| correlation_id == stored.event.correlation_id)
        && row.try_get::<Option<Uuid>, _>("causation_id")? == stored.event.causation_id
        && row
            .try_get::<Option<String>, _>("idempotency_key")?
            .is_some_and(|key| key == stored.event.idempotency_key)
        && occurred_at == stored.event.occurred_at
        && stored.aggregate_type == POS_AGGREGATE
        && stored.version == aggregate_version
        && stored.aggregate.id == aggregate_id
        && stored.aggregate.version == aggregate_version
        && stored.event.event_id == event_id
        && stored.event.aggregate_type == POS_AGGREGATE
        && stored.event.aggregate_id == aggregate_id
        && stored.event.aggregate_version == aggregate_version
        && stored.event.tenant_scope == stored.aggregate.scope
        && latest_event == Some(&stored.event);
    if !envelope_matches {
        return Err(invalid(
            "POS source outbox event does not match its authenticated envelope",
        ));
    }
    Ok((aggregate_id, aggregate_version))
}

/// The normalized sale row and its outbox stream advance together.  Before a
/// later source version is accepted, require the already-persisted stream to
/// contain exactly versions 1..=current_version.  Without this fence, deleting
/// or otherwise losing an earlier outbox row would allow an apparently valid
/// source update that can never be replayed by the projection worker.
async fn ensure_source_event_continuity(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    aggregate_id: Uuid,
    current_version: Option<u64>,
) -> Result<(), PersistenceError> {
    let row = sqlx::query(
        "SELECT COUNT(*) AS event_count,
                COUNT(DISTINCT aggregate_version) AS distinct_version_count,
                COALESCE(MIN(aggregate_version), 0) AS first_version,
                COALESCE(MAX(aggregate_version), 0) AS last_version
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type=$2 AND aggregate_id=$3",
    )
    .bind(holding_ref)
    .bind(POS_AGGREGATE)
    .bind(aggregate_id)
    .fetch_one(&mut **tx)
    .await?;

    let event_count = as_u64(row.try_get("event_count")?, "POS source event count")?;
    let distinct_version_count = as_u64(
        row.try_get("distinct_version_count")?,
        "POS source distinct version count",
    )?;
    let first_version = as_u64(row.try_get("first_version")?, "POS source first version")?;
    let last_version = as_u64(row.try_get("last_version")?, "POS source last version")?;
    let expected_count = current_version.unwrap_or(0);
    let source_rows = sqlx::query(
        "SELECT event_id, aggregate_type, aggregate_id, aggregate_version, event_type,
        payload, contract_version, event_version, tenant_scope, actor,
                correlation_id, causation_id, idempotency_key, occurred_at
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type=$2 AND aggregate_id=$3
         ORDER BY aggregate_version, event_id",
    )
    .bind(holding_ref)
    .bind(POS_AGGREGATE)
    .bind(aggregate_id)
    .fetch_all(&mut **tx)
    .await?;
    for source_row in &source_rows {
        validate_persisted_source_event(source_row)?;
    }
    let contiguous = event_count == expected_count
        && distinct_version_count == expected_count
        && (expected_count == 0 || (first_version == 1 && last_version == expected_count));
    if !contiguous {
        return Err(invalid(
            "POS source outbox history is not contiguous for the current sale version",
        ));
    }
    Ok(())
}

/// Payment facts are append-only within a sale.  A later aggregate snapshot
/// may add a payment, but it must not remove or rewrite one that was already
/// committed: the source outbox may already have carried that fact to finance
/// and downstream reconciliation.  Enforce this before replacing normalized
/// child rows so a malformed snapshot cannot erase authoritative history.
async fn ensure_persisted_payments_are_preserved(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    sale_id: Uuid,
    current_version: Option<u64>,
    payments: &[Payment],
    latest_event_type: PosEventType,
) -> Result<(), PersistenceError> {
    let rows = if current_version.is_some() {
        sqlx::query(
            "SELECT payment_id, amount_minor, method, received_at
             FROM erpx_tenant.pos_sale_payments
             WHERE holding_ref=$1 AND sale_id=$2
             ORDER BY payment_id
             FOR UPDATE",
        )
        .bind(holding_ref)
        .bind(sale_id)
        .fetch_all(&mut **tx)
        .await?
    } else {
        Vec::new()
    };
    let mut persisted_payment_ids = std::collections::BTreeSet::new();

    for row in rows {
        let payment_id: Uuid = row.try_get("payment_id")?;
        persisted_payment_ids.insert(payment_id);
        let amount_minor: i64 = row.try_get("amount_minor")?;
        let method: String = row.try_get("method")?;
        let received_at: DateTime<Utc> = row.try_get("received_at")?;
        let preserved = payments.iter().any(|payment| {
            payment.payment_id == payment_id
                && payment.amount_minor == amount_minor
                && payment_method(payment.method) == method
                && payment.received_at == received_at
        });
        if !preserved {
            return Err(invalid(
                "POS persisted payment facts cannot be removed or changed",
            ));
        }
    }

    validate_payment_event_delta(latest_event_type, &persisted_payment_ids, payments)
}

fn validate_payment_event_delta(
    latest_event_type: PosEventType,
    persisted_payment_ids: &std::collections::BTreeSet<Uuid>,
    payments: &[Payment],
) -> Result<(), PersistenceError> {
    let new_payment_count = payments
        .iter()
        .filter(|payment| !persisted_payment_ids.contains(&payment.payment_id))
        .count();
    match (latest_event_type, new_payment_count) {
        (PosEventType::PaymentRecorded, 1) => Ok(()),
        (PosEventType::PaymentRecorded, _) => Err(invalid(
            "POS payment_recorded events must append exactly one new payment fact",
        )),
        (_, 0) => Ok(()),
        (_, _) => Err(invalid(
            "new POS payment facts require a payment_recorded event",
        )),
    }
}

#[derive(Debug, Clone, Copy)]
struct ScopeColumns {
    kind: &'static str,
    holding_id: Uuid,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
}

#[derive(Debug, Clone, Copy)]
struct OptionalReferenceColumns {
    id: Option<Uuid>,
    kind: Option<&'static str>,
    holding_id: Option<Uuid>,
    company_id: Option<Uuid>,
    branch_id: Option<Uuid>,
}

async fn find_replay(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    idempotency_key: &str,
    request_hash: &str,
    sale: &Sale,
    event: &PosEventMetadata,
) -> Result<Option<PosWriteReceipt>, PersistenceError> {
    let row = sqlx::query(
        "SELECT i.request_hash, i.aggregate_type, i.aggregate_id,
                i.aggregate_version, i.command_id, i.event_id,
                s.version AS persisted_sale_version,
                s.command_id AS persisted_sale_command_id,
                s.idempotency_key AS persisted_sale_idempotency_key,
                o.event_id AS persisted_outbox_event_id,
                o.aggregate_type AS persisted_outbox_aggregate_type,
                o.aggregate_id AS persisted_outbox_aggregate_id,
                o.aggregate_version AS persisted_outbox_version,
                o.event_type AS persisted_outbox_event_type,
                o.payload AS persisted_outbox_payload,
                o.occurred_at AS persisted_outbox_occurred_at,
                o.contract_version AS persisted_outbox_contract_version,
                o.event_version AS persisted_outbox_event_version,
                o.tenant_scope AS persisted_outbox_tenant_scope,
                o.actor AS persisted_outbox_actor,
                o.correlation_id AS persisted_outbox_correlation_id,
                o.causation_id AS persisted_outbox_causation_id,
                o.idempotency_key AS persisted_outbox_idempotency_key
         FROM erpx_tenant.pos_command_idempotency i
         LEFT JOIN erpx_tenant.pos_sales s
           ON s.holding_ref=i.holding_ref AND s.sale_id=i.aggregate_id
         LEFT JOIN erpx_tenant.outbox_events o
           ON o.holding_ref=i.holding_ref AND o.event_id=i.event_id
         WHERE i.holding_ref=$1 AND i.idempotency_key=$2
         FOR UPDATE OF i",
    )
    .bind(holding_ref)
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await?;
    let Some(row) = row else { return Ok(None) };
    let stored_hash: String = row.try_get("request_hash")?;
    if stored_hash != request_hash {
        return Err(PersistenceError::IdempotencyConflict {
            key: idempotency_key.into(),
        });
    }
    let aggregate_type: String = row.try_get("aggregate_type")?;
    let aggregate_id: Uuid = row.try_get("aggregate_id")?;
    let aggregate_version: i64 = row.try_get("aggregate_version")?;
    let command_id: Uuid = row.try_get("command_id")?;
    let event_id: Uuid = row.try_get("event_id")?;
    let expected_payload = serde_json::to_value(PosOutboxPayload {
        aggregate_type: POS_AGGREGATE,
        aggregate: sale,
        version: sale.version,
        event,
    })?;
    let expected_tenant_scope = serde_json::to_value(&event.tenant_scope)?;
    let expected_actor = serde_json::to_value(&event.actor)?;
    let expected_aggregate_version = as_i64(sale.version, "POS sale version")?;
    let idempotency_record_is_bound = aggregate_type == POS_AGGREGATE
        && aggregate_id == sale.id
        && aggregate_version == expected_aggregate_version;
    let persisted_sale_version = row.try_get::<Option<i64>, _>("persisted_sale_version")?;
    let sale_is_backed = match persisted_sale_version {
        Some(version) if version > aggregate_version => true,
        Some(version) if version == aggregate_version => {
            row.try_get::<Option<Uuid>, _>("persisted_sale_command_id")?
                .is_some_and(|persisted| persisted == command_id)
                && row
                    .try_get::<Option<String>, _>("persisted_sale_idempotency_key")?
                    .is_some_and(|persisted| persisted == idempotency_key)
        }
        _ => false,
    };
    let outbox_is_backed = row
        .try_get::<Option<Uuid>, _>("persisted_outbox_event_id")?
        .is_some_and(|persisted| persisted == event_id)
        && row
            .try_get::<Option<String>, _>("persisted_outbox_aggregate_type")?
            .is_some_and(|persisted| persisted == POS_AGGREGATE)
        && row
            .try_get::<Option<Uuid>, _>("persisted_outbox_aggregate_id")?
            .is_some_and(|persisted| persisted == aggregate_id)
        && row
            .try_get::<Option<i64>, _>("persisted_outbox_version")?
            .is_some_and(|version| version == aggregate_version)
        && row
            .try_get::<Option<String>, _>("persisted_outbox_event_type")?
            .is_some_and(|stored_event_type| stored_event_type == event_type(event.event_type))
        && row
            .try_get::<Option<serde_json::Value>, _>("persisted_outbox_payload")?
            .is_some_and(|payload| payload == expected_payload)
        && row
            .try_get::<Option<DateTime<Utc>>, _>("persisted_outbox_occurred_at")?
            .is_some_and(|occurred_at| occurred_at == event.occurred_at)
        && row
            .try_get::<Option<i32>, _>("persisted_outbox_contract_version")?
            .is_some_and(|version| version == i32::from(event.contract_version))
        && row
            .try_get::<Option<i32>, _>("persisted_outbox_event_version")?
            .is_some_and(|version| version == i32::from(event.event_version))
        && row
            .try_get::<Option<serde_json::Value>, _>("persisted_outbox_tenant_scope")?
            .is_some_and(|scope| scope == expected_tenant_scope)
        && row
            .try_get::<Option<serde_json::Value>, _>("persisted_outbox_actor")?
            .is_some_and(|actor| actor == expected_actor)
        && row
            .try_get::<Option<Uuid>, _>("persisted_outbox_correlation_id")?
            .is_some_and(|correlation_id| correlation_id == event.correlation_id)
        && row.try_get::<Option<Uuid>, _>("persisted_outbox_causation_id")? == event.causation_id
        && row
            .try_get::<Option<String>, _>("persisted_outbox_idempotency_key")?
            .is_some_and(|persisted| persisted == idempotency_key);
    if !idempotency_record_is_bound || !sale_is_backed || !outbox_is_backed {
        return Err(invalid(
            "POS idempotency record is not backed by a matching sale and outbox event",
        ));
    }
    Ok(Some(PosWriteReceipt {
        aggregate_type,
        aggregate_id,
        version: as_u64(aggregate_version, "POS idempotency version")?,
        idempotency_key: idempotency_key.into(),
        event_id,
        replayed: true,
    }))
}

async fn insert_line(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    sale_id: Uuid,
    line_no: usize,
    line: &SaleLine,
) -> Result<(), PersistenceError> {
    let product = optional_reference_columns(line.product.as_ref(), "product")?;
    let line_no = i32::try_from(line_no).map_err(|_| invalid("POS line number exceeds INTEGER"))?;
    let subtotal_minor = line
        .subtotal_minor()
        .map_err(|error| invalid(error.to_string()))?;
    let total_minor = line
        .total_minor()
        .map_err(|error| invalid(error.to_string()))?;
    sqlx::query(
        "INSERT INTO erpx_tenant.pos_sale_lines
            (holding_ref, sale_id, line_id, line_no, item_reference, description,
             quantity, unit_price_minor, tax_minor, subtotal_minor, total_minor,
             product_id, product_scope_kind, product_scope_holding_id,
             product_scope_company_id, product_scope_branch_id)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15,$16)",
    )
    .bind(holding_ref)
    .bind(sale_id)
    .bind(line.line_id)
    .bind(line_no)
    .bind(&line.item_reference)
    .bind(&line.description)
    .bind(i64::try_from(line.quantity).map_err(|_| invalid("POS quantity exceeds BIGINT"))?)
    .bind(line.unit_price_minor)
    .bind(line.tax_minor)
    .bind(subtotal_minor)
    .bind(total_minor)
    .bind(product.id)
    .bind(product.kind)
    .bind(product.holding_id)
    .bind(product.company_id)
    .bind(product.branch_id)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn insert_payment(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    sale_id: Uuid,
    payment: &Payment,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.pos_sale_payments
            (holding_ref, sale_id, payment_id, amount_minor, method, received_at)
         VALUES ($1,$2,$3,$4,$5,$6)",
    )
    .bind(holding_ref)
    .bind(sale_id)
    .bind(payment.payment_id)
    .bind(payment.amount_minor)
    .bind(payment_method(payment.method))
    .bind(payment.received_at)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

#[derive(Serialize)]
struct PosOutboxPayload<'a> {
    aggregate_type: &'static str,
    aggregate: &'a Sale,
    version: u64,
    event: &'a PosEventMetadata,
}

#[derive(Debug, Deserialize)]
struct StoredPosOutboxPayload {
    aggregate_type: String,
    aggregate: Sale,
    version: u64,
    event: PosEventMetadata,
}

async fn append_outbox_event(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    sale: &Sale,
    event: &PosEventMetadata,
) -> Result<(), PersistenceError> {
    let payload = serde_json::to_value(PosOutboxPayload {
        aggregate_type: POS_AGGREGATE,
        aggregate: sale,
        version: sale.version,
        event,
    })?;
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at,
             contract_version, event_version, tenant_scope, actor,
             correlation_id, causation_id, idempotency_key)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,$14,$15)",
    )
    .bind(event.event_id)
    .bind(POS_AGGREGATE)
    .bind(sale.id)
    .bind(holding_ref)
    .bind(as_i64(sale.version, "POS sale version")?)
    .bind(event_type(event.event_type))
    .bind(payload)
    .bind(event.occurred_at)
    .bind(i32::from(event.contract_version))
    .bind(i32::from(event.event_version))
    .bind(serde_json::to_value(&event.tenant_scope)?)
    .bind(serde_json::to_value(&event.actor)?)
    .bind(event.correlation_id)
    .bind(event.causation_id)
    .bind(&event.idempotency_key)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn validate_sale_for_persistence(sale: &Sale) -> Result<(), PersistenceError> {
    sale.validate()
        .map_err(|error| invalid(format!("invalid POS sale: {error}")))?;
    if sale.command_history.len() > 1_000 {
        return Err(invalid("POS command history exceeds persistence bound"));
    }
    let last = sale
        .command_history
        .last()
        .ok_or_else(|| invalid("POS sale command history is empty"))?;
    if last.event.aggregate_type != POS_AGGREGATE
        || last.event.aggregate_id != sale.id
        || last.event.aggregate_version != sale.version
        || last.event.tenant_scope != sale.scope
        || last.event.idempotency_key != last.idempotency_key
    {
        return Err(invalid("POS sale latest event identity is invalid"));
    }
    validate_uuid(last.command_id, "POS command_id")?;
    validate_uuid(last.event.event_id, "POS event_id")?;
    validate_idempotency_key(&last.idempotency_key)
}

fn scope_columns(scope: &TenantScope) -> Result<ScopeColumns, PersistenceError> {
    match scope {
        TenantScope::Company {
            holding_id,
            company_id,
        } => {
            validate_uuid(*holding_id, "POS holding_id")?;
            validate_uuid(*company_id, "POS company_id")?;
            Ok(ScopeColumns {
                kind: "company",
                holding_id: *holding_id,
                company_id: Some(*company_id),
                branch_id: None,
            })
        }
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => {
            validate_uuid(*holding_id, "POS holding_id")?;
            validate_uuid(*company_id, "POS company_id")?;
            validate_uuid(*branch_id, "POS branch_id")?;
            Ok(ScopeColumns {
                kind: "branch",
                holding_id: *holding_id,
                company_id: Some(*company_id),
                branch_id: Some(*branch_id),
            })
        }
        _ => Err(invalid("POS sale scope must be company or branch")),
    }
}

fn optional_reference_columns(
    reference: Option<&PosScopedReference>,
    label: &'static str,
) -> Result<OptionalReferenceColumns, PersistenceError> {
    let Some(reference) = reference else {
        return Ok(OptionalReferenceColumns {
            id: None,
            kind: None,
            holding_id: None,
            company_id: None,
            branch_id: None,
        });
    };
    reference
        .validate()
        .map_err(|error| invalid(format!("invalid POS {label} reference: {error}")))?;
    let scope = scope_columns_for_reference(&reference.scope)?;
    Ok(OptionalReferenceColumns {
        id: Some(reference.id),
        kind: Some(scope.kind),
        holding_id: Some(scope.holding_id),
        company_id: scope.company_id,
        branch_id: scope.branch_id,
    })
}

fn scope_columns_for_reference(scope: &TenantScope) -> Result<ScopeColumns, PersistenceError> {
    match scope {
        TenantScope::Holding { holding_id } => {
            validate_uuid(*holding_id, "POS reference holding_id")?;
            Ok(ScopeColumns {
                kind: "holding",
                holding_id: *holding_id,
                company_id: None,
                branch_id: None,
            })
        }
        TenantScope::Company {
            holding_id,
            company_id,
        } => {
            validate_uuid(*holding_id, "POS reference holding_id")?;
            validate_uuid(*company_id, "POS reference company_id")?;
            Ok(ScopeColumns {
                kind: "company",
                holding_id: *holding_id,
                company_id: Some(*company_id),
                branch_id: None,
            })
        }
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => {
            validate_uuid(*holding_id, "POS reference holding_id")?;
            validate_uuid(*company_id, "POS reference company_id")?;
            validate_uuid(*branch_id, "POS reference branch_id")?;
            Ok(ScopeColumns {
                kind: "branch",
                holding_id: *holding_id,
                company_id: Some(*company_id),
                branch_id: Some(*branch_id),
            })
        }
        _ => Err(invalid("POS reference scope is not persistable")),
    }
}

fn request_hash(sale: &Sale, expected_version: Option<u64>) -> Result<String, PersistenceError> {
    let bytes = serde_json::to_vec(&(POS_AGGREGATE, sale, expected_version))?;
    Ok(hex::encode(Sha256::digest(bytes)))
}

fn event_type(event: PosEventType) -> &'static str {
    match event {
        PosEventType::SaleCreated => "pos.sale_created.v1",
        PosEventType::SaleOpened => "pos.sale_opened.v1",
        PosEventType::SaleHeld => "pos.sale_held.v1",
        PosEventType::SaleResumed => "pos.sale_resumed.v1",
        PosEventType::PaymentRecorded => "pos.payment_recorded.v1",
        PosEventType::SaleCompleted => "pos.sale_completed.v1",
        PosEventType::SaleVoided => "pos.sale_voided.v1",
        PosEventType::SaleRefunded => "pos.sale_refunded.v1",
    }
}

fn payment_method(method: PaymentMethod) -> &'static str {
    match method {
        PaymentMethod::Cash => "cash",
        PaymentMethod::Card => "card",
        PaymentMethod::BankTransfer => "bank_transfer",
        PaymentMethod::MobileWallet => "mobile_wallet",
        PaymentMethod::Voucher => "voucher",
        PaymentMethod::Other => "other",
    }
}

fn sale_status(status: SaleStatus) -> &'static str {
    match status {
        SaleStatus::Draft => "draft",
        SaleStatus::Open => "open",
        SaleStatus::Held => "held",
        SaleStatus::Completed => "completed",
        SaleStatus::Voided => "voided",
        SaleStatus::Refunded => "refunded",
    }
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > MAX_HOLDING_REF_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ));
    }
    Ok(())
}

fn validate_idempotency_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty()
        || value.len() > MAX_IDEMPOTENCY_KEY_BYTES
        || value.chars().any(char::is_control)
    {
        return Err(invalid("POS idempotency key is invalid"));
    }
    Ok(())
}

fn validate_uuid(value: Uuid, field: &'static str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        return Err(invalid(format!("{field} cannot be nil")));
    }
    Ok(())
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

    #[tokio::test]
    async fn constructor_rejects_unscoped_holding_references() {
        let pool = PgPool::connect_lazy("postgres://unused").expect("lazy pool");
        assert!(SqlxPosPersistence::new(pool.clone(), " ").is_err());
        assert!(SqlxPosPersistence::new(pool.clone(), "tenant\nref").is_err());
        assert!(SqlxPosPersistence::new(pool, "x".repeat(MAX_HOLDING_REF_BYTES + 1)).is_err());
    }

    #[test]
    fn payment_method_and_event_allow_lists_are_explicit() {
        assert_eq!(payment_method(PaymentMethod::Cash), "cash");
        assert_eq!(payment_method(PaymentMethod::BankTransfer), "bank_transfer");
        assert_eq!(event_type(PosEventType::SaleCreated), "pos.sale_created.v1");
        assert_eq!(
            event_type(PosEventType::SaleRefunded),
            "pos.sale_refunded.v1"
        );
    }

    #[test]
    fn payment_additions_are_bound_to_one_payment_recorded_event() {
        let existing = Payment::new(
            Uuid::from_u128(1),
            100,
            PaymentMethod::Cash,
            chrono::Utc::now(),
        )
        .expect("existing payment");
        let added = Payment::new(
            Uuid::from_u128(2),
            50,
            PaymentMethod::Card,
            chrono::Utc::now(),
        )
        .expect("added payment");
        let extra = Payment::new(
            Uuid::from_u128(3),
            25,
            PaymentMethod::Voucher,
            chrono::Utc::now(),
        )
        .expect("extra payment");
        let persisted = std::collections::BTreeSet::from([existing.payment_id]);

        assert!(
            validate_payment_event_delta(
                PosEventType::PaymentRecorded,
                &persisted,
                &[existing.clone(), added.clone()],
            )
            .is_ok()
        );
        assert!(
            validate_payment_event_delta(
                PosEventType::SaleHeld,
                &persisted,
                &[existing.clone(), added.clone()],
            )
            .is_err()
        );
        assert!(
            validate_payment_event_delta(
                PosEventType::PaymentRecorded,
                &persisted,
                std::slice::from_ref(&existing),
            )
            .is_err()
        );
        assert!(
            validate_payment_event_delta(
                PosEventType::PaymentRecorded,
                &persisted,
                &[existing, added, extra],
            )
            .is_err()
        );
    }

    #[test]
    fn invalid_idempotency_and_reference_values_are_rejected_without_echoing_payloads() {
        let error = validate_idempotency_key("secret\nvalue").expect_err("control character");
        assert!(!error.to_string().contains("secret"));
        let error = optional_reference_columns(None, "customer").expect("empty reference");
        assert!(error.id.is_none());
        assert!(scope_columns(&TenantScope::Platform).is_err());
    }

    #[test]
    fn transaction_fences_are_tenant_and_operation_bound() {
        let sale_id = Uuid::from_u128(7);
        let idempotency = pos_idempotency_lock_name("holding-a", "command-1");
        assert_eq!(idempotency, "pos:idempotency:holding-a:command-1");
        assert_ne!(
            idempotency,
            pos_idempotency_lock_name("holding-b", "command-1")
        );
        assert_ne!(idempotency, pos_aggregate_lock_name("holding-a", sale_id));
    }
}
