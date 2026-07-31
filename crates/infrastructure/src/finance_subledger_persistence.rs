//! Tenant-routed SQLx persistence for Finance AP/AR and cash control.
//!
//! The write database is authoritative.  Every successful mutation commits
//! the Finance aggregate, the shared snapshot, one outbox event, and a
//! request-bound idempotency receipt in the same transaction.  The read
//! database is disposable and is populated only through the projection API.
//! Allocation batches additionally lock and validate every affected payment
//! and invoice before changing any row, preserving the domain's all-or-
//! nothing contract at the repository boundary.

use std::collections::BTreeMap;

use chrono::{DateTime, NaiveDate, Utc};
use erpx_domain::contracts::TenantScope;
use erpx_domain::finance::{JournalEntry, PostedJournalEntry};
use erpx_domain::finance_subledgers::{
    Allocation, AllocationBatch, CashDisbursement, CashReceipt, CashReconciliation,
    FinanceJournalHandoff, FinanceRecordMetadata, FinanceSubledgerReconciliationIssue,
    FinanceSubledgerReconciliationIssueKind, FinanceSubledgerReconciliationReport, Invoice,
    InvoiceKind, InvoiceStatus, MAX_FINANCE_SUBLEDGER_RECONCILIATION_ISSUES, Payment, PaymentKind,
    PaymentStatus, SubledgerEntry,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, QueryBuilder, Row, Transaction, postgres::PgRow};
use uuid::Uuid;

use crate::PersistenceError;

type ReplayOutboxRecord = (String, Value);

pub const MAX_FINANCE_SUBLEDGER_READ_PAGE_SIZE: u32 = 500;
pub const MAX_FINANCE_SUBLEDGER_SOURCE_EVENT_PAGE_SIZE: u32 = 500;
pub const FINANCE_SUBLEDGER_READ_PROJECTION: &str = "finance_subledger";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceSubledgerWriteReceipt {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub version: u64,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FinanceAllocationBatchReceipt {
    pub payment: FinanceSubledgerWriteReceipt,
    pub invoices: Vec<FinanceSubledgerWriteReceipt>,
    pub allocations: Vec<FinanceSubledgerWriteReceipt>,
    pub replayed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceSubledgerProjectionApplication {
    pub event_id: Uuid,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub applied: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceSubledgerProjectionRebuildPreparation {
    pub holding_ref: String,
    pub rows_cleared: u64,
    pub checkpoints_cleared: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceSubledgerSourceEventCursor {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceSubledgerSourceEvent {
    pub event_id: Uuid,
    pub event_type: String,
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub aggregate_version: u64,
    pub payload: Value,
    pub occurred_at: DateTime<Utc>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FinanceSubledgerSourceEventPage {
    pub events: Vec<FinanceSubledgerSourceEvent>,
    pub next_cursor: Option<FinanceSubledgerSourceEventCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceSubledgerReadRow {
    pub aggregate_type: String,
    pub aggregate_id: Uuid,
    pub company_id: Uuid,
    pub branch_id: Option<Uuid>,
    pub aggregate_version: u64,
    pub aggregate: FinanceSubledgerAggregate,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceSubledgerReadCursor {
    aggregate_type: String,
    aggregate_id: Uuid,
}

impl FinanceSubledgerReadCursor {
    pub fn new(
        aggregate_type: impl Into<String>,
        aggregate_id: Uuid,
    ) -> Result<Self, PersistenceError> {
        let aggregate_type = aggregate_type.into();
        validate_aggregate_type(&aggregate_type)?;
        validate_uuid(aggregate_id, "Finance subledger read cursor aggregate ID")?;
        Ok(Self {
            aggregate_type,
            aggregate_id,
        })
    }

    pub fn aggregate_type(&self) -> &str {
        &self.aggregate_type
    }

    pub fn aggregate_id(&self) -> Uuid {
        self.aggregate_id
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FinanceSubledgerReadPage {
    pub rows: Vec<FinanceSubledgerReadRow>,
    pub next_cursor: Option<FinanceSubledgerReadCursor>,
    pub has_more: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(
    tag = "aggregate_type",
    content = "aggregate",
    rename_all = "snake_case"
)]
pub enum FinanceSubledgerAggregate {
    Invoice(Invoice),
    Payment(Payment),
    Allocation(Allocation),
    CashReceipt(CashReceipt),
    CashDisbursement(CashDisbursement),
    CashReconciliation(CashReconciliation),
    SubledgerEntry(SubledgerEntry),
}

impl FinanceSubledgerAggregate {
    pub fn validate(&self) -> Result<(), PersistenceError> {
        let result = match self {
            Self::Invoice(value) => value.validate(),
            Self::Payment(value) => value.validate(),
            Self::Allocation(value) => value.validate(),
            Self::CashReceipt(value) => value.validate(),
            Self::CashDisbursement(value) => value.validate(),
            Self::CashReconciliation(value) => value.validate(),
            Self::SubledgerEntry(value) => value.validate(),
        };
        result.map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        validate_handoff_state(self)
    }

    pub fn aggregate_type(&self) -> &'static str {
        match self {
            Self::Invoice(value) => match value.kind {
                InvoiceKind::Payable => "payable_invoice",
                InvoiceKind::Receivable => "receivable_invoice",
            },
            Self::Payment(value) => match value.kind {
                PaymentKind::VendorPayment => "vendor_payment",
                PaymentKind::CustomerPayment => "customer_payment",
            },
            Self::Allocation(_) => "allocation",
            Self::CashReceipt(_) => "cash_receipt",
            Self::CashDisbursement(_) => "cash_disbursement",
            Self::CashReconciliation(_) => "cash_reconciliation",
            Self::SubledgerEntry(value) => match value.party.kind {
                erpx_domain::finance_subledgers::PartyKind::Vendor => "vendor_subledger_entry",
                erpx_domain::finance_subledgers::PartyKind::Customer => "customer_subledger_entry",
            },
        }
    }

    pub fn aggregate_id(&self) -> Uuid {
        self.metadata().id
    }

    pub fn metadata(&self) -> &FinanceRecordMetadata {
        match self {
            Self::Invoice(value) => &value.metadata,
            Self::Payment(value) => &value.metadata,
            Self::Allocation(value) => &value.metadata,
            Self::CashReceipt(value) => &value.metadata,
            Self::CashDisbursement(value) => &value.metadata,
            Self::CashReconciliation(value) => &value.metadata,
            Self::SubledgerEntry(value) => &value.metadata,
        }
    }

    pub fn scope(&self) -> &TenantScope {
        &self.metadata().scope
    }

    fn journal_handoff(&self) -> Option<&FinanceJournalHandoff> {
        match self {
            Self::Invoice(value) => value.journal_handoff.as_ref(),
            Self::Payment(value) => value.journal_handoff.as_ref(),
            Self::Allocation(_) => None,
            Self::CashReceipt(value) => value.journal_handoff.as_ref(),
            Self::CashDisbursement(value) => value.journal_handoff.as_ref(),
            Self::CashReconciliation(_) => None,
            Self::SubledgerEntry(value) => value.journal_handoff.as_ref(),
        }
    }

    fn fields(&self) -> Result<SubledgerFields, PersistenceError> {
        let metadata = self.metadata();
        let payload = match self {
            Self::Invoice(value) => serde_json::to_value(value)?,
            Self::Payment(value) => serde_json::to_value(value)?,
            Self::Allocation(value) => serde_json::to_value(value)?,
            Self::CashReceipt(value) => serde_json::to_value(value)?,
            Self::CashDisbursement(value) => serde_json::to_value(value)?,
            Self::CashReconciliation(value) => serde_json::to_value(value)?,
            Self::SubledgerEntry(value) => serde_json::to_value(value)?,
        };
        Ok(SubledgerFields {
            aggregate_type: self.aggregate_type(),
            aggregate_id: metadata.id,
            company_id: metadata.company_id,
            branch_id: branch_from_scope(&metadata.scope),
            period_id: metadata.period_id,
            accounting_date: metadata.accounting_date,
            reference: metadata.reference.clone(),
            payload,
        })
    }
}

#[derive(Debug)]
struct SubledgerFields {
    aggregate_type: &'static str,
    aggregate_id: Uuid,
    company_id: Uuid,
    branch_id: Option<Uuid>,
    period_id: Uuid,
    accounting_date: NaiveDate,
    reference: String,
    payload: Value,
}

#[derive(Debug, Clone)]
pub struct SqlxFinanceSubledgerStore {
    write_pool: PgPool,
    read_pool: PgPool,
    holding_ref: String,
}

impl SqlxFinanceSubledgerStore {
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

    /// Save one Finance aggregate.  The aggregate metadata's idempotency key
    /// is the default request key; callers with a new command context may use
    /// `save_with_request_key` to bind the write to that request explicitly.
    pub async fn save(
        &self,
        aggregate: &FinanceSubledgerAggregate,
        expected_version: Option<u64>,
    ) -> Result<FinanceSubledgerWriteReceipt, PersistenceError> {
        let request_key = aggregate.metadata().idempotency_key.clone();
        self.save_with_request_key(aggregate, expected_version, &request_key)
            .await
    }

    pub async fn save_with_request_key(
        &self,
        aggregate: &FinanceSubledgerAggregate,
        expected_version: Option<u64>,
        request_key: &str,
    ) -> Result<FinanceSubledgerWriteReceipt, PersistenceError> {
        aggregate.validate()?;
        validate_request_key(request_key)?;
        let fields = aggregate.fields()?;
        let request_hash = hash_request(
            "save",
            fields.aggregate_type,
            &fields.payload,
            expected_version,
            request_key,
        )?;
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = find_receipt::<FinanceSubledgerWriteReceipt>(
            &mut tx,
            &self.holding_ref,
            request_key,
            &request_hash,
            "save",
            fields.aggregate_type,
            fields.aggregate_id,
        )
        .await?
        {
            validate_save_replay_evidence(
                &mut tx,
                &self.holding_ref,
                &receipt,
                expected_version,
                fields.aggregate_type,
                &fields.payload,
            )
            .await?;
            tx.commit().await?;
            return Ok(receipt.replayed());
        }
        validate_authoritative_handoff(&mut tx, &self.holding_ref, aggregate).await?;
        let current = current_version(
            &mut tx,
            fields.aggregate_type,
            fields.aggregate_id,
            &self.holding_ref,
        )
        .await?;
        ensure_expected_version(
            fields.aggregate_type,
            fields.aggregate_id,
            expected_version,
            current,
        )?;
        let version = next_version(current, fields.aggregate_type)?;
        persist_aggregate(&mut tx, &self.holding_ref, &fields, version, request_key).await?;
        let receipt = FinanceSubledgerWriteReceipt {
            aggregate_type: fields.aggregate_type.into(),
            aggregate_id: fields.aggregate_id,
            version,
            replayed: false,
        };
        insert_receipt(
            &mut tx,
            &self.holding_ref,
            "save",
            fields.aggregate_type,
            fields.aggregate_id,
            request_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Apply one balanced allocation batch atomically.  The supplied state is
    /// validated first, then the current write rows are locked and decoded;
    /// the same domain validation is repeated against those locked rows before
    /// any payment, invoice, or allocation row is changed.
    pub async fn save_allocation_batch(
        &self,
        batch: &AllocationBatch,
        payment: &Payment,
        invoices: &[Invoice],
        expected_payment_version: u64,
    ) -> Result<FinanceAllocationBatchReceipt, PersistenceError> {
        batch
            .validate_against(payment, invoices)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let request_key = batch
            .allocations
            .first()
            .map(|allocation| allocation.metadata.idempotency_key.as_str())
            .ok_or_else(|| PersistenceError::InvalidData("allocation batch is empty".into()))?;
        for allocation in &batch.allocations {
            if allocation.metadata.idempotency_key != request_key {
                return Err(PersistenceError::InvalidData(
                    "all allocations in a batch must share one request idempotency key".into(),
                ));
            }
        }
        validate_request_key(request_key)?;
        let request_hash = hash_request(
            "allocation_batch",
            "allocation_batch",
            &serde_json::json!({
                "batch": batch,
                "payment": payment,
                "invoices": invoices,
            }),
            Some(expected_payment_version),
            request_key,
        )?;
        let mut tx = self.begin_write().await?;
        if let Some(receipt) = find_receipt::<FinanceAllocationBatchReceipt>(
            &mut tx,
            &self.holding_ref,
            request_key,
            &request_hash,
            "allocation_batch",
            "allocation_batch",
            payment.metadata.id,
        )
        .await?
        {
            validate_allocation_batch_replay_evidence(
                &mut tx,
                &self.holding_ref,
                &receipt,
                batch,
                payment,
                invoices,
                expected_payment_version,
            )
            .await?;
            tx.commit().await?;
            return Ok(receipt.replayed());
        }

        let current_payment = load_write_aggregate(
            &mut tx,
            &self.holding_ref,
            payment.metadata.id,
            payment_type(payment),
        )
        .await?;
        let locked_payment = match current_payment {
            FinanceSubledgerAggregate::Payment(value) => value,
            _ => {
                return Err(PersistenceError::InvalidData(
                    "stored payment type mismatch".into(),
                ));
            }
        };
        let stored_payment_version = current_version(
            &mut tx,
            payment_type(payment),
            payment.metadata.id,
            &self.holding_ref,
        )
        .await?
        .ok_or_else(|| PersistenceError::InvalidData("stored payment version is missing".into()))?;
        if locked_payment != *payment {
            return Err(PersistenceError::InvalidData(
                "allocation payment does not match authoritative write state".into(),
            ));
        }
        if stored_payment_version != expected_payment_version {
            return Err(PersistenceError::OptimisticConcurrency {
                aggregate_type: payment_type(payment).into(),
                aggregate_id: payment.metadata.id,
                expected: Some(expected_payment_version),
                actual: Some(stored_payment_version),
            });
        }

        let mut locked_invoices = Vec::with_capacity(invoices.len());
        for invoice in invoices {
            let aggregate = load_write_aggregate(
                &mut tx,
                &self.holding_ref,
                invoice.metadata.id,
                invoice_type(invoice),
            )
            .await?;
            let locked = match aggregate {
                FinanceSubledgerAggregate::Invoice(value) => value,
                _ => {
                    return Err(PersistenceError::InvalidData(
                        "stored invoice type mismatch".into(),
                    ));
                }
            };
            if locked != *invoice {
                return Err(PersistenceError::InvalidData(
                    "allocation invoice does not match authoritative write state".into(),
                ));
            }
            locked_invoices.push(locked);
        }

        batch
            .validate_against(&locked_payment, &locked_invoices)
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
        let mut next_payment = locked_payment;
        let mut next_invoices = locked_invoices;
        let expected_payment_metadata_version = next_payment.metadata.version;
        batch
            .apply(
                &mut next_payment,
                &mut next_invoices,
                expected_payment_metadata_version,
            )
            .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;

        let payment_version = next_version(Some(stored_payment_version), payment_type(payment))?;
        let next_payment = FinanceSubledgerAggregate::Payment(next_payment);
        validate_authoritative_handoff(&mut tx, &self.holding_ref, &next_payment).await?;
        let payment_fields = next_payment.fields()?;
        persist_aggregate(
            &mut tx,
            &self.holding_ref,
            &payment_fields,
            payment_version,
            request_key,
        )
        .await?;
        let payment_receipt = FinanceSubledgerWriteReceipt {
            aggregate_type: payment_fields.aggregate_type.into(),
            aggregate_id: payment_fields.aggregate_id,
            version: payment_version,
            replayed: false,
        };

        let mut invoice_receipts = Vec::with_capacity(next_invoices.len());
        for invoice in next_invoices {
            let stored_version = current_version(
                &mut tx,
                invoice_type(&invoice),
                invoice.metadata.id,
                &self.holding_ref,
            )
            .await?
            .ok_or_else(|| {
                PersistenceError::InvalidData("stored invoice version is missing".into())
            })?;
            let version = next_version(Some(stored_version), invoice_type(&invoice))?;
            let invoice = FinanceSubledgerAggregate::Invoice(invoice);
            validate_authoritative_handoff(&mut tx, &self.holding_ref, &invoice).await?;
            let fields = invoice.fields()?;
            persist_aggregate(&mut tx, &self.holding_ref, &fields, version, request_key).await?;
            invoice_receipts.push(FinanceSubledgerWriteReceipt {
                aggregate_type: fields.aggregate_type.into(),
                aggregate_id: fields.aggregate_id,
                version,
                replayed: false,
            });
        }
        let mut allocation_receipts = Vec::with_capacity(batch.allocations.len());
        for allocation in &batch.allocations {
            if current_version(
                &mut tx,
                "allocation",
                allocation.metadata.id,
                &self.holding_ref,
            )
            .await?
            .is_some()
            {
                return Err(PersistenceError::IdempotencyConflict {
                    key: request_key.into(),
                });
            }
            let fields = FinanceSubledgerAggregate::Allocation(allocation.clone()).fields()?;
            persist_aggregate(&mut tx, &self.holding_ref, &fields, 1, request_key).await?;
            allocation_receipts.push(FinanceSubledgerWriteReceipt {
                aggregate_type: fields.aggregate_type.into(),
                aggregate_id: fields.aggregate_id,
                version: 1,
                replayed: false,
            });
        }
        let receipt = FinanceAllocationBatchReceipt {
            payment: payment_receipt,
            invoices: invoice_receipts,
            allocations: allocation_receipts,
            replayed: false,
        };
        insert_receipt(
            &mut tx,
            &self.holding_ref,
            "allocation_batch",
            "allocation_batch",
            payment.metadata.id,
            request_key,
            &request_hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn get(
        &self,
        aggregate_type: &str,
        aggregate_id: Uuid,
    ) -> Result<Option<FinanceSubledgerReadRow>, PersistenceError> {
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "Finance subledger aggregate ID")?;
        let mut tx = self.begin_read().await?;
        let row = sqlx::query(
            "SELECT aggregate_type, aggregate_id, company_id, branch_id,
                    payload, source_version
             FROM erpx_tenant.finance_subledger_read_rows
             WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$3",
        )
        .bind(aggregate_type)
        .bind(aggregate_id)
        .bind(&self.holding_ref)
        .fetch_optional(&mut *tx)
        .await?;
        tx.commit().await?;
        row.map(decode_read_row).transpose()
    }

    /// Return a bounded keyset page from the disposable tenant-read database.
    /// The write database is never consulted. Scope predicates are applied in
    /// SQL before pagination so inaccessible rows cannot affect cursors.
    pub async fn list_read_rows(
        &self,
        scope: &TenantScope,
        aggregate_type: Option<&str>,
        company_id: Option<Uuid>,
        after: Option<&FinanceSubledgerReadCursor>,
        page_size: u32,
    ) -> Result<FinanceSubledgerReadPage, PersistenceError> {
        if page_size == 0 || page_size > MAX_FINANCE_SUBLEDGER_READ_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "Finance subledger read page size is outside its bound".into(),
            ));
        }
        validate_read_scope(scope)?;
        if let Some(aggregate_type) = aggregate_type {
            validate_aggregate_type(aggregate_type)?;
        }
        if let Some(company_id) = company_id {
            validate_uuid(company_id, "Finance subledger read company ID")?;
        }
        if let (Some(aggregate_type), Some(after)) = (aggregate_type, after)
            && aggregate_type != after.aggregate_type()
        {
            return Err(PersistenceError::InvalidData(
                "Finance subledger read cursor does not match its aggregate filter".into(),
            ));
        }
        if let Some(after) = after {
            validate_aggregate_type(after.aggregate_type())?;
            validate_uuid(
                after.aggregate_id(),
                "Finance subledger read cursor aggregate ID",
            )?;
        }
        validate_company_filter(scope, company_id)?;

        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT aggregate_type, aggregate_id, company_id, branch_id,
                    source_version, payload
             FROM erpx_tenant.finance_subledger_read_rows
             WHERE holding_ref = ",
        );
        query.push_bind(&self.holding_ref);
        append_read_scope_filter(&mut query, scope)?;
        if let Some(aggregate_type) = aggregate_type {
            query
                .push(" AND aggregate_type = ")
                .push_bind(aggregate_type);
        }
        if let Some(company_id) = company_id {
            query.push(" AND company_id = ").push_bind(company_id);
        }
        if let Some(after) = after {
            query
                .push(" AND (aggregate_type, aggregate_id) > (")
                .push_bind(after.aggregate_type())
                .push(", ")
                .push_bind(after.aggregate_id())
                .push(")");
        }
        let mut tx = self.begin_read().await?;
        let rows = query
            .push(" ORDER BY aggregate_type ASC, aggregate_id ASC LIMIT ")
            .push_bind(i64::from(page_size) + 1)
            .build()
            .fetch_all(&mut *tx)
            .await?
            .into_iter()
            .map(decode_read_row)
            .collect::<Result<Vec<_>, _>>()?;
        tx.commit().await?;
        let has_more = rows.len() > page_size as usize;
        let rows = rows
            .into_iter()
            .take(page_size as usize)
            .collect::<Vec<_>>();
        let next_cursor = if has_more {
            rows.last().map(|row| FinanceSubledgerReadCursor {
                aggregate_type: row.aggregate_type.clone(),
                aggregate_id: row.aggregate_id,
            })
        } else {
            None
        };
        Ok(FinanceSubledgerReadPage {
            rows,
            next_cursor,
            has_more,
        })
    }

    pub async fn read_source_event_page(
        &self,
        cursor: Option<&FinanceSubledgerSourceEventCursor>,
        page_size: u32,
    ) -> Result<FinanceSubledgerSourceEventPage, PersistenceError> {
        if page_size == 0 || page_size > MAX_FINANCE_SUBLEDGER_SOURCE_EVENT_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(format!(
                "Finance subledger source event page size must be 1..={MAX_FINANCE_SUBLEDGER_SOURCE_EVENT_PAGE_SIZE}"
            )));
        }
        let limit = i64::from(page_size) + 1;
        let mut tx = self.begin_write().await?;
        let rows = if let Some(cursor) = cursor {
            validate_aggregate_type(&cursor.aggregate_type)?;
            validate_uuid(cursor.aggregate_id, "Finance source cursor aggregate ID")?;
            sqlx::query(
                "SELECT event_id, event_type, aggregate_type, aggregate_id,
                        aggregate_version, payload, occurred_at
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_type = ANY($2)
                   AND (aggregate_type > $3
                        OR (aggregate_type = $3 AND aggregate_id > $4)
                        OR (aggregate_type = $3 AND aggregate_id = $4
                            AND aggregate_version > $5))
                 ORDER BY aggregate_type, aggregate_id, aggregate_version
                 LIMIT $6",
            )
            .bind(&self.holding_ref)
            .bind(allowed_types())
            .bind(&cursor.aggregate_type)
            .bind(cursor.aggregate_id)
            .bind(as_i64(
                cursor.aggregate_version,
                "Finance source cursor version",
            )?)
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        } else {
            sqlx::query(
                "SELECT event_id, event_type, aggregate_type, aggregate_id,
                        aggregate_version, payload, occurred_at
                 FROM erpx_tenant.outbox_events
                 WHERE holding_ref=$1 AND aggregate_type = ANY($2)
                 ORDER BY aggregate_type, aggregate_id, aggregate_version
                 LIMIT $3",
            )
            .bind(&self.holding_ref)
            .bind(allowed_types())
            .bind(limit)
            .fetch_all(&mut *tx)
            .await?
        };
        tx.commit().await?;
        let requested = usize::try_from(page_size).map_err(|_| {
            PersistenceError::InvalidData("Finance source page size is not representable".into())
        })?;
        let has_more = rows.len() > requested;
        let events = rows
            .into_iter()
            .take(requested)
            .map(|row| {
                let event_id: Uuid = row.try_get("event_id")?;
                let aggregate_id: Uuid = row.try_get("aggregate_id")?;
                let aggregate_type: String = row.try_get("aggregate_type")?;
                validate_uuid(event_id, "Finance source event ID")?;
                validate_uuid(aggregate_id, "Finance source aggregate ID")?;
                validate_aggregate_type(&aggregate_type)?;
                Ok(FinanceSubledgerSourceEvent {
                    event_id,
                    event_type: row.try_get("event_type")?,
                    aggregate_type,
                    aggregate_id,
                    aggregate_version: as_u64(
                        row.try_get("aggregate_version")?,
                        "Finance source event version",
                    )?,
                    payload: row.try_get("payload")?,
                    occurred_at: row.try_get("occurred_at")?,
                })
            })
            .collect::<Result<Vec<_>, PersistenceError>>()?;
        let next_cursor = has_more
            .then(|| {
                events
                    .last()
                    .map(|event| FinanceSubledgerSourceEventCursor {
                        aggregate_type: event.aggregate_type.clone(),
                        aggregate_id: event.aggregate_id,
                        aggregate_version: event.aggregate_version,
                    })
            })
            .flatten();
        Ok(FinanceSubledgerSourceEventPage {
            events,
            next_cursor,
            has_more,
        })
    }

    /// Return the holding-scoped count of source events consumed by the
    /// Finance subledger projection. The source stream is the tenant-write
    /// outbox, and the closed aggregate allow-list must stay identical to the
    /// one used by [`read_source_event_page`]. Replacement-read rebuilds use
    /// this count before and after replay to detect an append-only source that
    /// moved while the candidate was being built.
    pub async fn source_event_high_watermark(&self) -> Result<u64, PersistenceError> {
        let mut tx = self.begin_write().await?;
        let count: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM erpx_tenant.outbox_events
             WHERE holding_ref = $1 AND aggregate_type = ANY($2)",
        )
        .bind(&self.holding_ref)
        .bind(allowed_types())
        .fetch_one(&mut *tx)
        .await?;
        tx.commit().await?;
        u64::try_from(count).map_err(|_| {
            PersistenceError::InvalidData("source event high-watermark is invalid".into())
        })
    }

    /// Reconcile the disposable Finance subledger read model against the
    /// authoritative tenant-write aggregates. This is a read-only operation
    /// on both physical databases: it never repairs, deletes, or mutates
    /// source state. A worker can use a report with `is_reconciled()` as the
    /// prerequisite for exposing a fresh read-side result or activating a
    /// replacement database.
    pub async fn reconcile_read_model(
        &self,
        max_rows: u32,
    ) -> Result<FinanceSubledgerReconciliationReport, PersistenceError> {
        if max_rows == 0 || max_rows as usize > MAX_FINANCE_SUBLEDGER_RECONCILIATION_ISSUES {
            return Err(PersistenceError::InvalidData(
                "Finance subledger reconciliation bound is invalid".into(),
            ));
        }
        let limit = i64::from(max_rows) + 1;
        let mut write_tx = self.begin_write().await?;
        let source_event_high_watermark: i64 = sqlx::query_scalar(
            "SELECT COUNT(*)
             FROM erpx_tenant.outbox_events
             WHERE holding_ref = $1 AND aggregate_type = ANY($2)",
        )
        .bind(&self.holding_ref)
        .bind(allowed_types())
        .fetch_one(&mut *write_tx)
        .await?;
        let source_rows = sqlx::query(
            "SELECT aggregate_type, aggregate_id, company_id, branch_id,
                    version, payload
             FROM erpx_tenant.finance_subledger_aggregates
             WHERE holding_ref=$1
             ORDER BY aggregate_type, aggregate_id
             LIMIT $2",
        )
        .bind(&self.holding_ref)
        .bind(limit)
        .fetch_all(&mut *write_tx)
        .await?;
        let source_outbox_cardinality_violations = sqlx::query(
            "SELECT source.aggregate_type, source.aggregate_id, source.version,
                    COUNT(event.event_id) AS event_count,
                    COALESCE(MIN(event.aggregate_version), 0) AS first_event_version,
                    COALESCE(MAX(event.aggregate_version), 0) AS last_event_version,
                    COUNT(DISTINCT event.aggregate_version) AS distinct_event_versions
             FROM erpx_tenant.finance_subledger_aggregates AS source
             LEFT JOIN erpx_tenant.outbox_events AS event
               ON event.holding_ref = source.holding_ref
              AND event.aggregate_type = source.aggregate_type
              AND event.aggregate_id = source.aggregate_id
             WHERE source.holding_ref=$1
             GROUP BY source.aggregate_type, source.aggregate_id, source.version
             HAVING COUNT(event.event_id) <> source.version
                 OR COALESCE(MIN(event.aggregate_version), 0) <> 1
                 OR COALESCE(MAX(event.aggregate_version), 0) <> source.version
                 OR COUNT(DISTINCT event.aggregate_version) <> source.version
             LIMIT $2",
        )
        .bind(&self.holding_ref)
        .bind(limit)
        .fetch_all(&mut *write_tx)
        .await?;
        for row in source_outbox_cardinality_violations {
            validate_source_outbox_cardinality(
                row.try_get("aggregate_type")?,
                row.try_get("aggregate_id")?,
                row.try_get("version")?,
                row.try_get("event_count")?,
                row.try_get("first_event_version")?,
                row.try_get("last_event_version")?,
                row.try_get("distinct_event_versions")?,
            )?;
        }
        write_tx.commit().await?;

        let source_event_high_watermark =
            u64::try_from(source_event_high_watermark).map_err(|_| {
                PersistenceError::InvalidData("source event watermark is invalid".into())
            })?;
        let source_truncated = source_rows.len() > max_rows as usize;
        let source_rows = source_rows
            .into_iter()
            .take(max_rows as usize)
            .map(|row| {
                let aggregate_type: String = row.try_get("aggregate_type")?;
                let aggregate_id: Uuid = row.try_get("aggregate_id")?;
                validate_aggregate_type(&aggregate_type)?;
                validate_uuid(aggregate_id, "Finance source aggregate ID")?;
                let aggregate = decode_aggregate(&aggregate_type, row.try_get("payload")?)?;
                aggregate.validate()?;
                if aggregate.aggregate_id() != aggregate_id
                    || aggregate.metadata().company_id != row.try_get::<Uuid, _>("company_id")?
                    || branch_from_scope(&aggregate.metadata().scope) != row.try_get("branch_id")?
                {
                    return Err(PersistenceError::InvalidData(
                        "Finance source aggregate identity is inconsistent".into(),
                    ));
                }
                Ok::<_, PersistenceError>((
                    (aggregate_type, aggregate_id),
                    (
                        as_u64(row.try_get("version")?, "Finance source version")?,
                        aggregate,
                    ),
                ))
            })
            .collect::<Result<BTreeMap<_, _>, _>>()?;

        let mut read_tx = self.begin_read().await?;
        let read_rows = sqlx::query(
            "SELECT aggregate_type, aggregate_id, company_id, branch_id,
                    source_version, payload
             FROM erpx_tenant.finance_subledger_read_rows
             WHERE holding_ref=$1
             ORDER BY aggregate_type, aggregate_id
             LIMIT $2",
        )
        .bind(&self.holding_ref)
        .bind(limit)
        .fetch_all(&mut *read_tx)
        .await?;
        read_tx.commit().await?;
        let read_truncated = read_rows.len() > max_rows as usize;
        let read_rows = read_rows
            .into_iter()
            .take(max_rows as usize)
            .map(decode_read_row)
            .collect::<Result<Vec<_>, _>>()?;
        let read_rows = read_rows
            .into_iter()
            .map(|row| ((row.aggregate_type.clone(), row.aggregate_id), row))
            .collect::<BTreeMap<_, _>>();

        let mut issues = Vec::new();
        let mut push_issue = |issue: FinanceSubledgerReconciliationIssue| {
            if issues.len() < MAX_FINANCE_SUBLEDGER_RECONCILIATION_ISSUES {
                issues.push(issue);
            }
        };
        for ((aggregate_type, aggregate_id), (source_version, source)) in &source_rows {
            match read_rows.get(&(aggregate_type.clone(), *aggregate_id)) {
                None => push_issue(FinanceSubledgerReconciliationIssue {
                    aggregate_type: aggregate_type.clone(),
                    aggregate_id: *aggregate_id,
                    source_version: Some(*source_version),
                    read_version: None,
                    kind: FinanceSubledgerReconciliationIssueKind::MissingReadRow,
                }),
                Some(read) => {
                    if read.aggregate_version != *source_version {
                        push_issue(FinanceSubledgerReconciliationIssue {
                            aggregate_type: aggregate_type.clone(),
                            aggregate_id: *aggregate_id,
                            source_version: Some(*source_version),
                            read_version: Some(read.aggregate_version),
                            kind: FinanceSubledgerReconciliationIssueKind::VersionMismatch,
                        });
                    }
                    if read.company_id != source.metadata().company_id
                        || read.branch_id != branch_from_scope(&source.metadata().scope)
                    {
                        push_issue(FinanceSubledgerReconciliationIssue {
                            aggregate_type: aggregate_type.clone(),
                            aggregate_id: *aggregate_id,
                            source_version: Some(*source_version),
                            read_version: Some(read.aggregate_version),
                            kind: FinanceSubledgerReconciliationIssueKind::ScopeMismatch,
                        });
                    }
                    if &read.aggregate != source {
                        push_issue(FinanceSubledgerReconciliationIssue {
                            aggregate_type: aggregate_type.clone(),
                            aggregate_id: *aggregate_id,
                            source_version: Some(*source_version),
                            read_version: Some(read.aggregate_version),
                            kind: FinanceSubledgerReconciliationIssueKind::PayloadMismatch,
                        });
                    }
                }
            }
        }
        for (aggregate_type, aggregate_id) in read_rows.keys() {
            if !source_rows.contains_key(&(aggregate_type.clone(), *aggregate_id)) {
                let read = &read_rows[&(aggregate_type.clone(), *aggregate_id)];
                push_issue(FinanceSubledgerReconciliationIssue {
                    aggregate_type: aggregate_type.clone(),
                    aggregate_id: *aggregate_id,
                    source_version: None,
                    read_version: Some(read.aggregate_version),
                    kind: FinanceSubledgerReconciliationIssueKind::UnexpectedReadRow,
                });
            }
        }
        let source_after = self.source_event_high_watermark().await?;
        let source_stable = source_after == source_event_high_watermark;
        let issues_truncated = issues.len() >= MAX_FINANCE_SUBLEDGER_RECONCILIATION_ISSUES
            || source_truncated
            || read_truncated;
        Ok(FinanceSubledgerReconciliationReport {
            contract_version:
                erpx_domain::finance_subledgers::FINANCE_SUBLEDGER_RECONCILIATION_CONTRACT_VERSION,
            holding_ref: self.holding_ref.clone(),
            source_aggregate_count: source_rows.len() as u64,
            read_row_count: read_rows.len() as u64,
            source_event_high_watermark,
            source_stable,
            complete: !source_truncated && !read_truncated,
            issues_truncated,
            issues,
        })
    }

    pub async fn project_event(
        &self,
        event_id: Uuid,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<FinanceSubledgerProjectionApplication, PersistenceError> {
        validate_uuid(event_id, "Finance projection event ID")?;
        validate_aggregate_type(aggregate_type)?;
        validate_uuid(aggregate_id, "Finance projection aggregate ID")?;
        if aggregate_version == 0 {
            return Err(PersistenceError::InvalidData(
                "Finance projection version must be positive".into(),
            ));
        }
        let event: SubledgerEventPayload = serde_json::from_value(payload.clone())?;
        if event.aggregate_type != aggregate_type || event.version != aggregate_version {
            return Err(PersistenceError::InvalidData(
                "Finance projection event identity or version does not match payload".into(),
            ));
        }
        let aggregate = decode_aggregate(aggregate_type, event.aggregate)?;
        aggregate.validate()?;
        if aggregate.aggregate_id() != aggregate_id {
            return Err(PersistenceError::InvalidData(
                "Finance projection aggregate ID does not match event key".into(),
            ));
        }
        crate::finance_persistence::verify_authoritative_source_event(
            &self.write_pool,
            &self.holding_ref,
            event_id,
            &format!("finance.subledger.{aggregate_type}.v1"),
            aggregate_type,
            aggregate_id,
            aggregate_version,
            payload,
        )
        .await?;
        let fields = aggregate.fields()?;
        let mut tx = self.begin_read().await?;
        let result = project_subledger_event_in_transaction(
            &mut tx,
            &self.holding_ref,
            event_id,
            aggregate_type,
            aggregate_id,
            aggregate_version,
            fields,
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn replay_event(
        &self,
        event_id: Uuid,
        aggregate_type: &str,
        aggregate_id: Uuid,
        aggregate_version: u64,
        payload: &Value,
    ) -> Result<FinanceSubledgerProjectionApplication, PersistenceError> {
        self.project_event(
            event_id,
            aggregate_type,
            aggregate_id,
            aggregate_version,
            payload,
        )
        .await
    }

    /// Atomically replace the subledger read model from a fenced outbox
    /// snapshot. The write-side outbox lock prevents appends while the
    /// candidate read model is built, and the read transaction keeps the
    /// previous projection intact until every source event has replayed.
    pub async fn rebuild_projection_with_source_fence(
        &self,
        expected_source_version: Option<u64>,
        page_size: u32,
    ) -> Result<(FinanceSubledgerProjectionRebuildPreparation, u64), PersistenceError> {
        if page_size == 0 || page_size > MAX_FINANCE_SUBLEDGER_SOURCE_EVENT_PAGE_SIZE {
            return Err(PersistenceError::InvalidData(
                "Finance subledger source event page size is outside its bound".into(),
            ));
        }
        let mut source_tx = self.begin_write().await?;
        sqlx::query("LOCK TABLE erpx_tenant.outbox_events IN SHARE MODE")
            .execute(&mut *source_tx)
            .await?;
        let actual_source_version =
            subledger_source_event_count_in_transaction(&mut source_tx, &self.holding_ref).await?;
        if expected_source_version.is_some_and(|expected| expected != actual_source_version) {
            return Err(PersistenceError::InvalidData(
                "Finance subledger source fence changed before replacement rebuild".into(),
            ));
        }

        let mut read_tx = self.begin_read().await?;
        let preparation =
            clear_subledger_projection_in_transaction(&mut read_tx, &self.holding_ref).await?;
        let mut cursor = None;
        let mut events_replayed = 0_u64;
        loop {
            let page = read_subledger_source_event_page_in_transaction(
                &mut source_tx,
                &self.holding_ref,
                cursor.as_ref(),
                page_size,
            )
            .await?;
            for event in &page.events {
                let expected_event_type = format!("finance.subledger.{}.v1", event.aggregate_type);
                if event.event_type != expected_event_type {
                    return Err(PersistenceError::InvalidData(
                        "Finance subledger source event type is invalid".into(),
                    ));
                }
                let event_payload: SubledgerEventPayload =
                    serde_json::from_value(event.payload.clone())?;
                if event_payload.aggregate_type != event.aggregate_type
                    || event_payload.version != event.aggregate_version
                {
                    return Err(PersistenceError::InvalidData(
                        "Finance subledger source event identity is invalid".into(),
                    ));
                }
                let aggregate = decode_aggregate(&event.aggregate_type, event_payload.aggregate)?;
                aggregate.validate()?;
                if aggregate.aggregate_id() != event.aggregate_id {
                    return Err(PersistenceError::InvalidData(
                        "Finance subledger source aggregate identity is invalid".into(),
                    ));
                }
                let fields = aggregate.fields()?;
                if project_subledger_event_in_transaction(
                    &mut read_tx,
                    &self.holding_ref,
                    event.event_id,
                    &event.aggregate_type,
                    event.aggregate_id,
                    event.aggregate_version,
                    fields,
                )
                .await?
                .applied
                {
                    events_replayed = events_replayed.saturating_add(1);
                }
            }
            if !page.has_more {
                break;
            }
            cursor = page.next_cursor;
            if cursor.is_none() {
                return Err(PersistenceError::InvalidData(
                    "Finance subledger source cursor is missing".into(),
                ));
            }
        }
        let final_source_version =
            subledger_source_event_count_in_transaction(&mut source_tx, &self.holding_ref).await?;
        if final_source_version != actual_source_version {
            return Err(PersistenceError::InvalidData(
                "Finance subledger source fence changed during replacement rebuild".into(),
            ));
        }
        read_tx.commit().await?;
        source_tx.commit().await?;
        Ok((preparation, events_replayed))
    }

    pub async fn clear_projection(
        &self,
    ) -> Result<FinanceSubledgerProjectionRebuildPreparation, PersistenceError> {
        let mut tx = self.begin_read().await?;
        let rows_cleared =
            sqlx::query("DELETE FROM erpx_tenant.finance_subledger_read_rows WHERE holding_ref=$1")
                .bind(&self.holding_ref)
                .execute(&mut *tx)
                .await?
                .rows_affected();
        let checkpoints_cleared = sqlx::query(
            "DELETE FROM erpx_tenant.finance_subledger_projection_checkpoints WHERE holding_ref=$1",
        )
        .bind(&self.holding_ref)
        .execute(&mut *tx)
        .await?
        .rows_affected();
        tx.commit().await?;
        Ok(FinanceSubledgerProjectionRebuildPreparation {
            holding_ref: self.holding_ref.clone(),
            rows_cleared,
            checkpoints_cleared,
        })
    }

    async fn begin_write(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.write_pool, &self.holding_ref).await
    }

    async fn begin_read(&self) -> Result<Transaction<'_, Postgres>, PersistenceError> {
        begin_scoped(&self.read_pool, &self.holding_ref).await
    }
}

#[derive(Debug, Clone, Deserialize)]
struct SubledgerEventPayload {
    aggregate_type: String,
    aggregate: Value,
    version: u64,
}

impl FinanceSubledgerWriteReceipt {
    fn replayed(mut self) -> Self {
        self.replayed = true;
        self
    }
}

impl FinanceAllocationBatchReceipt {
    fn replayed(mut self) -> Self {
        self.replayed = true;
        self.payment.replayed = true;
        self.invoices
            .iter_mut()
            .for_each(|receipt| receipt.replayed = true);
        self.allocations
            .iter_mut()
            .for_each(|receipt| receipt.replayed = true);
        self
    }
}

async fn project_subledger_event_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    event_id: Uuid,
    aggregate_type: &str,
    aggregate_id: Uuid,
    aggregate_version: u64,
    fields: SubledgerFields,
) -> Result<FinanceSubledgerProjectionApplication, PersistenceError> {
    validate_uuid(event_id, "Finance projection event ID")?;
    validate_aggregate_type(aggregate_type)?;
    validate_uuid(aggregate_id, "Finance projection aggregate ID")?;
    if aggregate_version == 0 {
        return Err(PersistenceError::InvalidData(
            "Finance projection version must be positive".into(),
        ));
    }
    if fields.aggregate_type != aggregate_type || fields.aggregate_id != aggregate_id {
        return Err(PersistenceError::InvalidData(
            "Finance projection fields do not match the event identity".into(),
        ));
    }

    let checkpoint = sqlx::query(
        "SELECT last_event_id, source_version
         FROM erpx_tenant.finance_subledger_projection_checkpoints
         WHERE holding_ref=$1 AND aggregate_type=$2 AND aggregate_id=$3
         FOR UPDATE",
    )
    .bind(holding_ref)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .fetch_optional(&mut **tx)
    .await?;
    if let Some(checkpoint) = checkpoint {
        let last_event_id: Uuid = checkpoint.try_get("last_event_id")?;
        let current_version = as_u64(
            checkpoint.try_get("source_version")?,
            "Finance subledger projection version",
        )?;
        if aggregate_version < current_version
            || (aggregate_version == current_version && event_id == last_event_id)
        {
            return Ok(FinanceSubledgerProjectionApplication {
                event_id,
                aggregate_type: aggregate_type.into(),
                aggregate_id,
                aggregate_version,
                applied: false,
            });
        }
        if aggregate_version == current_version || aggregate_version != current_version + 1 {
            return Err(PersistenceError::InvalidData(
                "Finance subledger projection version is not contiguous".into(),
            ));
        }
    } else if aggregate_version != 1 {
        return Err(PersistenceError::InvalidData(
            "Finance subledger projection must start at version one".into(),
        ));
    }

    sqlx::query(
        "INSERT INTO erpx_tenant.finance_subledger_read_rows
            (aggregate_id, aggregate_type, holding_ref, company_id, branch_id,
             period_id, accounting_date, reference, source_version, payload)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10)
         ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
             holding_ref=EXCLUDED.holding_ref, company_id=EXCLUDED.company_id,
             branch_id=EXCLUDED.branch_id, period_id=EXCLUDED.period_id,
             accounting_date=EXCLUDED.accounting_date, reference=EXCLUDED.reference,
             source_version=EXCLUDED.source_version, payload=EXCLUDED.payload,
             projected_at=now()",
    )
    .bind(fields.aggregate_id)
    .bind(fields.aggregate_type)
    .bind(holding_ref)
    .bind(fields.company_id)
    .bind(fields.branch_id)
    .bind(fields.period_id)
    .bind(fields.accounting_date)
    .bind(&fields.reference)
    .bind(as_i64(
        aggregate_version,
        "Finance subledger projection version",
    )?)
    .bind(&fields.payload)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_subledger_projection_checkpoints
            (aggregate_type, aggregate_id, holding_ref, last_event_id, source_version)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
             holding_ref=EXCLUDED.holding_ref, last_event_id=EXCLUDED.last_event_id,
             source_version=EXCLUDED.source_version, projected_at=now()",
    )
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(holding_ref)
    .bind(event_id)
    .bind(as_i64(
        aggregate_version,
        "Finance subledger projection version",
    )?)
    .execute(&mut **tx)
    .await?;
    Ok(FinanceSubledgerProjectionApplication {
        event_id,
        aggregate_type: aggregate_type.into(),
        aggregate_id,
        aggregate_version,
        applied: true,
    })
}

async fn read_subledger_source_event_page_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    cursor: Option<&FinanceSubledgerSourceEventCursor>,
    page_size: u32,
) -> Result<FinanceSubledgerSourceEventPage, PersistenceError> {
    if page_size == 0 || page_size > MAX_FINANCE_SUBLEDGER_SOURCE_EVENT_PAGE_SIZE {
        return Err(PersistenceError::InvalidData(
            "Finance subledger source event page size is outside its bound".into(),
        ));
    }
    let limit = i64::from(page_size) + 1;
    let rows = if let Some(cursor) = cursor {
        validate_aggregate_type(&cursor.aggregate_type)?;
        validate_uuid(cursor.aggregate_id, "Finance source cursor aggregate ID")?;
        if cursor.aggregate_version == 0 {
            return Err(PersistenceError::InvalidData(
                "Finance source cursor version is invalid".into(),
            ));
        }
        sqlx::query(
            "SELECT event_id,event_type,aggregate_type,aggregate_id,
                    aggregate_version,payload,occurred_at
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type=ANY($2)
               AND (aggregate_type>$3
                    OR (aggregate_type=$3 AND aggregate_id>$4)
                    OR (aggregate_type=$3 AND aggregate_id=$4
                        AND aggregate_version>$5))
             ORDER BY aggregate_type,aggregate_id,aggregate_version
             LIMIT $6",
        )
        .bind(holding_ref)
        .bind(allowed_types())
        .bind(&cursor.aggregate_type)
        .bind(cursor.aggregate_id)
        .bind(as_i64(
            cursor.aggregate_version,
            "Finance source cursor version",
        )?)
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    } else {
        sqlx::query(
            "SELECT event_id,event_type,aggregate_type,aggregate_id,
                    aggregate_version,payload,occurred_at
             FROM erpx_tenant.outbox_events
             WHERE holding_ref=$1 AND aggregate_type=ANY($2)
             ORDER BY aggregate_type,aggregate_id,aggregate_version
             LIMIT $3",
        )
        .bind(holding_ref)
        .bind(allowed_types())
        .bind(limit)
        .fetch_all(&mut **tx)
        .await?
    };
    let requested = usize::try_from(page_size).map_err(|_| {
        PersistenceError::InvalidData("Finance source page size is not representable".into())
    })?;
    let has_more = rows.len() > requested;
    let events = rows
        .into_iter()
        .take(requested)
        .map(|row| {
            let event_id: Uuid = row.try_get("event_id")?;
            let aggregate_id: Uuid = row.try_get("aggregate_id")?;
            let aggregate_type: String = row.try_get("aggregate_type")?;
            validate_uuid(event_id, "Finance source event ID")?;
            validate_uuid(aggregate_id, "Finance source aggregate ID")?;
            validate_aggregate_type(&aggregate_type)?;
            Ok(FinanceSubledgerSourceEvent {
                event_id,
                event_type: row.try_get("event_type")?,
                aggregate_type,
                aggregate_id,
                aggregate_version: as_u64(
                    row.try_get("aggregate_version")?,
                    "Finance source event version",
                )?,
                payload: row.try_get("payload")?,
                occurred_at: row.try_get("occurred_at")?,
            })
        })
        .collect::<Result<Vec<_>, PersistenceError>>()?;
    let next_cursor = has_more.then(|| {
        events
            .last()
            .map(|event| FinanceSubledgerSourceEventCursor {
                aggregate_type: event.aggregate_type.clone(),
                aggregate_id: event.aggregate_id,
                aggregate_version: event.aggregate_version,
            })
    });
    Ok(FinanceSubledgerSourceEventPage {
        events,
        next_cursor: next_cursor.flatten(),
        has_more,
    })
}

async fn subledger_source_event_count_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<u64, PersistenceError> {
    let count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*)
         FROM erpx_tenant.outbox_events
         WHERE holding_ref=$1 AND aggregate_type=ANY($2)",
    )
    .bind(holding_ref)
    .bind(allowed_types())
    .fetch_one(&mut **tx)
    .await?;
    as_u64(count, "Finance subledger source event count")
}

async fn clear_subledger_projection_in_transaction(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
) -> Result<FinanceSubledgerProjectionRebuildPreparation, PersistenceError> {
    let rows_cleared =
        sqlx::query("DELETE FROM erpx_tenant.finance_subledger_read_rows WHERE holding_ref=$1")
            .bind(holding_ref)
            .execute(&mut **tx)
            .await?
            .rows_affected();
    let checkpoints_cleared = sqlx::query(
        "DELETE FROM erpx_tenant.finance_subledger_projection_checkpoints WHERE holding_ref=$1",
    )
    .bind(holding_ref)
    .execute(&mut **tx)
    .await?
    .rows_affected();
    Ok(FinanceSubledgerProjectionRebuildPreparation {
        holding_ref: holding_ref.into(),
        rows_cleared,
        checkpoints_cleared,
    })
}

async fn persist_aggregate(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    fields: &SubledgerFields,
    version: u64,
    request_key: &str,
) -> Result<(), PersistenceError> {
    let aggregate_storage_key = format!(
        "{request_key}/{}/{}",
        fields.aggregate_type,
        fields.aggregate_id.simple()
    );
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_subledger_aggregates
            (aggregate_id, aggregate_type, holding_ref, company_id, branch_id,
             period_id, accounting_date, reference, version, idempotency_key, payload)
         VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)
         ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
             holding_ref=$3, company_id=$4, branch_id=$5, period_id=$6,
             accounting_date=$7, reference=$8, version=$9, idempotency_key=$10,
             payload=$11, updated_at=now()",
    )
    .bind(fields.aggregate_id)
    .bind(fields.aggregate_type)
    .bind(holding_ref)
    .bind(fields.company_id)
    .bind(fields.branch_id)
    .bind(fields.period_id)
    .bind(fields.accounting_date)
    .bind(&fields.reference)
    .bind(as_i64(version, "Finance subledger aggregate version")?)
    .bind(aggregate_storage_key)
    .bind(&fields.payload)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.aggregate_snapshots
            (aggregate_type, aggregate_id, holding_ref, version, state)
         VALUES ($1,$2,$3,$4,$5)
         ON CONFLICT (aggregate_type, aggregate_id) DO UPDATE SET
             holding_ref=$3, version=$4, state=$5, updated_at=now()",
    )
    .bind(fields.aggregate_type)
    .bind(fields.aggregate_id)
    .bind(holding_ref)
    .bind(as_i64(version, "Finance subledger aggregate version")?)
    .bind(&fields.payload)
    .execute(&mut **tx)
    .await?;
    sqlx::query(
        "INSERT INTO erpx_tenant.outbox_events
            (event_id, aggregate_type, aggregate_id, holding_ref,
             aggregate_version, event_type, payload, occurred_at)
         VALUES ($1,$2,$3,$4,$5,$6,$7,now())",
    )
    .bind(Uuid::new_v4())
    .bind(fields.aggregate_type)
    .bind(fields.aggregate_id)
    .bind(holding_ref)
    .bind(as_i64(version, "Finance subledger aggregate version")?)
    .bind(format!("finance.subledger.{}.v1", fields.aggregate_type))
    .bind(serde_json::json!({
        "aggregate_type": fields.aggregate_type,
        "aggregate": &fields.payload,
        "version": version,
    }))
    .execute(&mut **tx)
    .await?;
    Ok(())
}

async fn load_write_aggregate(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    aggregate_id: Uuid,
    aggregate_type: &str,
) -> Result<FinanceSubledgerAggregate, PersistenceError> {
    let row = sqlx::query(
        "SELECT payload FROM erpx_tenant.finance_subledger_aggregates
         WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$3 FOR UPDATE",
    )
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?
    .ok_or_else(|| PersistenceError::InvalidData("Finance aggregate not found".into()))?;
    let aggregate = decode_aggregate(aggregate_type, row.try_get("payload")?)?;
    aggregate.validate()?;
    validate_authoritative_handoff(tx, holding_ref, &aggregate).await?;
    Ok(aggregate)
}

#[derive(Debug)]
struct AuthoritativePostedJournalRow {
    journal_id: Uuid,
    holding_id: Uuid,
    company_id: Uuid,
    period_id: Uuid,
    payload: Value,
}

async fn validate_authoritative_handoff(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    aggregate: &FinanceSubledgerAggregate,
) -> Result<(), PersistenceError> {
    let Some(handoff) = aggregate.journal_handoff() else {
        return Ok(());
    };
    let row = sqlx::query(
        "SELECT journal_id, holding_id, company_id, period_id, payload
         FROM erpx_tenant.finance_posted_journals
         WHERE journal_id=$1 AND holding_ref=$2
         FOR SHARE",
    )
    .bind(handoff.journal_id)
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?;
    let row = match row {
        Some(row) => Some(AuthoritativePostedJournalRow {
            journal_id: row.try_get("journal_id")?,
            holding_id: row.try_get("holding_id")?,
            company_id: row.try_get("company_id")?,
            period_id: row.try_get("period_id")?,
            payload: row.try_get("payload")?,
        }),
        None => None,
    };

    validate_posted_journal_row(handoff, row)?;
    Ok(())
}

fn validate_posted_journal_row(
    handoff: &FinanceJournalHandoff,
    row: Option<AuthoritativePostedJournalRow>,
) -> Result<(), PersistenceError> {
    let row = row.ok_or(PersistenceError::NotFound {
        entity: "finance posted journal",
        id: handoff.journal_id,
    })?;
    let posted: PostedJournalEntry = serde_json::from_value(row.payload.clone()).map_err(|_| {
        PersistenceError::InvalidData("authoritative Finance journal payload is invalid".into())
    })?;
    if !posted.is_immutable()
        || posted.id() != row.journal_id
        || posted.holding_id() != row.holding_id
        || posted.company_id() != row.company_id
        || posted.period_id() != row.period_id
    {
        return Err(PersistenceError::InvalidData(
            "authoritative Finance journal identity or scope is inconsistent".into(),
        ));
    }
    let entry_payload = row.payload.get("entry").cloned().ok_or_else(|| {
        PersistenceError::InvalidData("authoritative Finance journal payload is invalid".into())
    })?;
    let entry: JournalEntry = serde_json::from_value(entry_payload).map_err(|_| {
        PersistenceError::InvalidData("authoritative Finance journal payload is invalid".into())
    })?;
    handoff.validate_against_journal(&entry).map_err(|_| {
        PersistenceError::InvalidData(
            "Finance journal handoff does not match the authoritative posted journal".into(),
        )
    })?;
    Ok(())
}

async fn current_version(
    tx: &mut Transaction<'_, Postgres>,
    aggregate_type: &str,
    aggregate_id: Uuid,
    holding_ref: &str,
) -> Result<Option<u64>, PersistenceError> {
    sqlx::query_scalar::<_, i64>(
        "SELECT version FROM erpx_tenant.finance_subledger_aggregates
         WHERE aggregate_type=$1 AND aggregate_id=$2 AND holding_ref=$3 FOR UPDATE",
    )
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?
    .map(|value| as_u64(value, "Finance subledger aggregate version"))
    .transpose()
}

async fn validate_save_replay_evidence(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    receipt: &FinanceSubledgerWriteReceipt,
    expected_version: Option<u64>,
    aggregate_type: &str,
    aggregate_payload: &Value,
) -> Result<(), PersistenceError> {
    let expected_version = expected_version
        .map(|version| next_version(Some(version), aggregate_type))
        .transpose()?
        .unwrap_or(1);
    if receipt.aggregate_type != aggregate_type || receipt.version != expected_version {
        return Err(PersistenceError::InvalidData(
            "Finance subledger save replay receipt version or identity is inconsistent".into(),
        ));
    }
    let expected_payload = serde_json::json!({
        "aggregate_type": aggregate_type,
        "aggregate": aggregate_payload,
        "version": receipt.version,
    });
    validate_replay_outbox_evidence(tx, holding_ref, receipt, Some(&expected_payload)).await
}

async fn validate_allocation_batch_replay_evidence(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    receipt: &FinanceAllocationBatchReceipt,
    batch: &AllocationBatch,
    payment: &Payment,
    invoices: &[Invoice],
    expected_payment_version: u64,
) -> Result<(), PersistenceError> {
    let expected_payment_version =
        next_version(Some(expected_payment_version), payment_type(payment))?;
    if receipt.payment.aggregate_type != payment_type(payment)
        || receipt.payment.aggregate_id != payment.metadata.id
        || receipt.payment.version != expected_payment_version
        || receipt.invoices.len() != invoices.len()
        || receipt.allocations.len() != batch.allocations.len()
    {
        return Err(PersistenceError::InvalidData(
            "Finance allocation batch replay receipt is inconsistent".into(),
        ));
    }
    let mut expected_payment = payment.clone();
    let mut expected_invoices = invoices.to_vec();
    batch
        .apply(
            &mut expected_payment,
            &mut expected_invoices,
            expected_payment_version - 1,
        )
        .map_err(|error| PersistenceError::InvalidData(error.to_string()))?;
    let expected_payment_payload = serde_json::to_value(&expected_payment)?;
    validate_replay_outbox_evidence(
        tx,
        holding_ref,
        &receipt.payment,
        Some(&serde_json::json!({
            "aggregate_type": receipt.payment.aggregate_type,
            "aggregate": expected_payment_payload,
            "version": receipt.payment.version,
        })),
    )
    .await?;

    for ((receipt, invoice), expected_invoice) in receipt
        .invoices
        .iter()
        .zip(invoices)
        .zip(&expected_invoices)
    {
        let expected_version = next_version(Some(invoice.metadata.version), invoice_type(invoice))?;
        if receipt.aggregate_type != invoice_type(invoice)
            || receipt.aggregate_id != invoice.metadata.id
            || receipt.version != expected_version
        {
            return Err(PersistenceError::InvalidData(
                "Finance allocation invoice replay receipt is inconsistent".into(),
            ));
        }
        let expected_invoice_payload = serde_json::to_value(expected_invoice)?;
        validate_replay_outbox_evidence(
            tx,
            holding_ref,
            receipt,
            Some(&serde_json::json!({
                "aggregate_type": receipt.aggregate_type,
                "aggregate": expected_invoice_payload,
                "version": receipt.version,
            })),
        )
        .await?;
    }
    for (receipt, allocation) in receipt.allocations.iter().zip(&batch.allocations) {
        if receipt.aggregate_type != "allocation"
            || receipt.aggregate_id != allocation.metadata.id
            || receipt.version != 1
        {
            return Err(PersistenceError::InvalidData(
                "Finance allocation replay receipt is inconsistent".into(),
            ));
        }
        let expected_allocation_payload = serde_json::to_value(allocation)?;
        validate_replay_outbox_evidence(
            tx,
            holding_ref,
            receipt,
            Some(&serde_json::json!({
                "aggregate_type": "allocation",
                "aggregate": expected_allocation_payload,
                "version": 1,
            })),
        )
        .await?;
    }
    Ok(())
}

async fn validate_replay_outbox_evidence(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    receipt: &FinanceSubledgerWriteReceipt,
    expected_payload: Option<&Value>,
) -> Result<(), PersistenceError> {
    let rows: Vec<ReplayOutboxRecord> = sqlx::query_as(
        "SELECT event_type, payload
           FROM erpx_tenant.outbox_events
          WHERE holding_ref=$1
            AND aggregate_type=$2
            AND aggregate_id=$3
            AND aggregate_version=$4
          FOR SHARE",
    )
    .bind(holding_ref)
    .bind(&receipt.aggregate_type)
    .bind(receipt.aggregate_id)
    .bind(as_i64(
        receipt.version,
        "Finance subledger replay source version",
    )?)
    .fetch_all(&mut **tx)
    .await?;
    validate_replay_outbox_rows(receipt, expected_payload, &rows)
}

fn validate_replay_outbox_rows(
    receipt: &FinanceSubledgerWriteReceipt,
    expected_payload: Option<&Value>,
    rows: &[ReplayOutboxRecord],
) -> Result<(), PersistenceError> {
    validate_uuid(
        receipt.aggregate_id,
        "Finance subledger replay aggregate ID",
    )?;
    validate_aggregate_type(&receipt.aggregate_type)?;
    if receipt.version == 0 || rows.len() != 1 {
        return Err(PersistenceError::InvalidData(format!(
            "Finance subledger replay requires exactly one source outbox event, found {}",
            rows.len()
        )));
    }
    let expected_event_type = format!("finance.subledger.{}.v1", receipt.aggregate_type);
    let (event_type, payload) = &rows[0];
    if event_type != &expected_event_type {
        return Err(PersistenceError::InvalidData(
            "Finance subledger replay source event type is inconsistent".into(),
        ));
    }
    if let Some(expected_payload) = expected_payload
        && payload != expected_payload
    {
        return Err(PersistenceError::InvalidData(
            "Finance subledger replay source payload is inconsistent".into(),
        ));
    }
    let source: SubledgerEventPayload = serde_json::from_value(payload.clone())?;
    if source.aggregate_type != receipt.aggregate_type || source.version != receipt.version {
        return Err(PersistenceError::InvalidData(
            "Finance subledger replay source event identity is inconsistent".into(),
        ));
    }
    let aggregate = decode_aggregate(&source.aggregate_type, source.aggregate.clone())?;
    if aggregate.aggregate_id() != receipt.aggregate_id {
        return Err(PersistenceError::InvalidData(
            "Finance subledger replay source aggregate ID is inconsistent".into(),
        ));
    }
    aggregate.validate()
}

async fn find_receipt<T: for<'de> Deserialize<'de>>(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    request_key: &str,
    request_hash: &str,
    expected_operation: &str,
    expected_aggregate_type: &str,
    expected_aggregate_id: Uuid,
) -> Result<Option<T>, PersistenceError> {
    let row = sqlx::query(
        "SELECT operation, aggregate_type, aggregate_id, request_hash, response
         FROM erpx_tenant.finance_subledger_command_idempotency
         WHERE idempotency_key=$1 AND holding_ref=$2 FOR UPDATE",
    )
    .bind(request_key)
    .bind(holding_ref)
    .fetch_optional(&mut **tx)
    .await?;
    row.map(|row| {
        let operation: String = row.try_get("operation")?;
        let aggregate_type: String = row.try_get("aggregate_type")?;
        let aggregate_id: Uuid = row.try_get("aggregate_id")?;
        validate_receipt_binding(
            &operation,
            &aggregate_type,
            aggregate_id,
            expected_operation,
            expected_aggregate_type,
            expected_aggregate_id,
        )?;
        let stored_hash: String = row.try_get("request_hash")?;
        if stored_hash != request_hash {
            return Err(PersistenceError::IdempotencyConflict {
                key: request_key.into(),
            });
        }
        Ok(serde_json::from_value(row.try_get("response")?)?)
    })
    .transpose()
}

fn validate_receipt_binding(
    operation: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    expected_operation: &str,
    expected_aggregate_type: &str,
    expected_aggregate_id: Uuid,
) -> Result<(), PersistenceError> {
    if operation != expected_operation
        || aggregate_type != expected_aggregate_type
        || aggregate_id != expected_aggregate_id
    {
        return Err(PersistenceError::InvalidData(
            "Finance subledger idempotency receipt binding is inconsistent".into(),
        ));
    }
    Ok(())
}

#[allow(clippy::too_many_arguments)]
async fn insert_receipt<T: Serialize>(
    tx: &mut Transaction<'_, Postgres>,
    holding_ref: &str,
    operation: &str,
    aggregate_type: &str,
    aggregate_id: Uuid,
    request_key: &str,
    request_hash: &str,
    receipt: &T,
) -> Result<(), PersistenceError> {
    sqlx::query(
        "INSERT INTO erpx_tenant.finance_subledger_command_idempotency
            (idempotency_key, holding_ref, operation, aggregate_type,
             aggregate_id, request_hash, response)
         VALUES ($1,$2,$3,$4,$5,$6,$7)",
    )
    .bind(request_key)
    .bind(holding_ref)
    .bind(operation)
    .bind(aggregate_type)
    .bind(aggregate_id)
    .bind(request_hash)
    .bind(serde_json::to_value(receipt)?)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

fn decode_aggregate(
    aggregate_type: &str,
    payload: Value,
) -> Result<FinanceSubledgerAggregate, PersistenceError> {
    validate_aggregate_type(aggregate_type)?;
    let aggregate = match aggregate_type {
        "payable_invoice" | "receivable_invoice" => {
            FinanceSubledgerAggregate::Invoice(serde_json::from_value(payload)?)
        }
        "vendor_payment" | "customer_payment" => {
            FinanceSubledgerAggregate::Payment(serde_json::from_value(payload)?)
        }
        "allocation" => FinanceSubledgerAggregate::Allocation(serde_json::from_value(payload)?),
        "cash_receipt" => FinanceSubledgerAggregate::CashReceipt(serde_json::from_value(payload)?),
        "cash_disbursement" => {
            FinanceSubledgerAggregate::CashDisbursement(serde_json::from_value(payload)?)
        }
        "cash_reconciliation" => {
            FinanceSubledgerAggregate::CashReconciliation(serde_json::from_value(payload)?)
        }
        "vendor_subledger_entry" | "customer_subledger_entry" => {
            FinanceSubledgerAggregate::SubledgerEntry(serde_json::from_value(payload)?)
        }
        _ => unreachable!("aggregate type was validated above"),
    };
    if aggregate.aggregate_type() != aggregate_type {
        return Err(PersistenceError::InvalidData(
            "Finance aggregate type does not match its storage key".into(),
        ));
    }
    Ok(aggregate)
}

fn decode_read_row(row: PgRow) -> Result<FinanceSubledgerReadRow, PersistenceError> {
    let aggregate_type: String = row.try_get("aggregate_type")?;
    let aggregate_id: Uuid = row.try_get("aggregate_id")?;
    validate_aggregate_type(&aggregate_type)?;
    validate_uuid(aggregate_id, "Finance subledger read aggregate ID")?;
    let aggregate = decode_aggregate(&aggregate_type, row.try_get("payload")?)?;
    if aggregate.aggregate_id() != aggregate_id {
        return Err(PersistenceError::InvalidData(
            "Finance subledger projection identity does not match its key".into(),
        ));
    }
    let company_id: Uuid = row.try_get("company_id")?;
    let branch_id: Option<Uuid> = row.try_get("branch_id")?;
    validate_uuid(company_id, "Finance subledger read company ID")?;
    if branch_from_scope(aggregate.scope()) != branch_id
        || aggregate.metadata().company_id != company_id
    {
        return Err(PersistenceError::InvalidData(
            "Finance subledger read scope does not match its row".into(),
        ));
    }
    Ok(FinanceSubledgerReadRow {
        aggregate_type,
        aggregate_id,
        company_id,
        branch_id,
        aggregate_version: as_u64(row.try_get("source_version")?, "Finance source version")?,
        aggregate,
    })
}

fn validate_handoff_state(aggregate: &FinanceSubledgerAggregate) -> Result<(), PersistenceError> {
    let missing = match aggregate {
        FinanceSubledgerAggregate::Invoice(value) => {
            matches!(
                value.status,
                InvoiceStatus::Posted | InvoiceStatus::PartiallyPaid | InvoiceStatus::Paid
            ) && value.journal_handoff.is_none()
        }
        FinanceSubledgerAggregate::Payment(value) => {
            matches!(
                value.status,
                PaymentStatus::Settled | PaymentStatus::Reconciled
            ) && value.journal_handoff.is_none()
        }
        FinanceSubledgerAggregate::CashReceipt(value) => {
            matches!(
                value.status,
                erpx_domain::finance_subledgers::CashStatus::Posted
                    | erpx_domain::finance_subledgers::CashStatus::Reconciled
            ) && value.journal_handoff.is_none()
        }
        FinanceSubledgerAggregate::CashDisbursement(value) => {
            matches!(
                value.status,
                erpx_domain::finance_subledgers::CashStatus::Posted
                    | erpx_domain::finance_subledgers::CashStatus::Reconciled
            ) && value.journal_handoff.is_none()
        }
        FinanceSubledgerAggregate::SubledgerEntry(value) => {
            value.applied_minor > 0 && value.journal_handoff.is_none()
        }
        FinanceSubledgerAggregate::Allocation(_)
        | FinanceSubledgerAggregate::CashReconciliation(_) => false,
    };
    if missing {
        Err(PersistenceError::InvalidData(
            "posted Finance subledger state requires an explicit FinanceJournalHandoff".into(),
        ))
    } else {
        Ok(())
    }
}

fn aggregate_type_for_invoice(value: &Invoice) -> &'static str {
    match value.kind {
        InvoiceKind::Payable => "payable_invoice",
        InvoiceKind::Receivable => "receivable_invoice",
    }
}

fn invoice_type(value: &Invoice) -> &'static str {
    aggregate_type_for_invoice(value)
}

fn payment_type(value: &Payment) -> &'static str {
    match value.kind {
        PaymentKind::VendorPayment => "vendor_payment",
        PaymentKind::CustomerPayment => "customer_payment",
    }
}

fn allowed_types() -> Vec<&'static str> {
    vec![
        "payable_invoice",
        "receivable_invoice",
        "vendor_payment",
        "customer_payment",
        "allocation",
        "cash_receipt",
        "cash_disbursement",
        "cash_reconciliation",
        "vendor_subledger_entry",
        "customer_subledger_entry",
    ]
}

fn validate_aggregate_type(value: &str) -> Result<(), PersistenceError> {
    if allowed_types().contains(&value) {
        Ok(())
    } else {
        Err(PersistenceError::InvalidData(
            "unknown Finance subledger aggregate type".into(),
        ))
    }
}

fn validate_holding_ref(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 255 || value.chars().any(char::is_control) {
        Err(PersistenceError::Configuration(
            "holding reference must be bounded and non-empty",
        ))
    } else {
        Ok(())
    }
}

fn validate_request_key(value: &str) -> Result<(), PersistenceError> {
    if value.trim().is_empty() || value.len() > 512 || value.chars().any(char::is_control) {
        Err(PersistenceError::InvalidData(
            "Finance request idempotency key must be bounded and non-empty".into(),
        ))
    } else {
        Ok(())
    }
}

fn validate_uuid(value: Uuid, field: &'static str) -> Result<(), PersistenceError> {
    if value.is_nil() {
        Err(PersistenceError::InvalidData(format!(
            "{field} cannot be nil"
        )))
    } else {
        Ok(())
    }
}

fn validate_read_scope(scope: &TenantScope) -> Result<(), PersistenceError> {
    match scope {
        TenantScope::Holding { holding_id } => {
            validate_uuid(*holding_id, "Finance subledger read holding ID")?
        }
        TenantScope::Company {
            holding_id,
            company_id,
        } => {
            validate_uuid(*holding_id, "Finance subledger read holding ID")?;
            validate_uuid(*company_id, "Finance subledger read company ID")?;
        }
        TenantScope::Branch {
            holding_id,
            company_id,
            branch_id,
        } => {
            validate_uuid(*holding_id, "Finance subledger read holding ID")?;
            validate_uuid(*company_id, "Finance subledger read company ID")?;
            validate_uuid(*branch_id, "Finance subledger read branch ID")?;
        }
        TenantScope::Subtree { .. } | TenantScope::Platform => {
            return Err(PersistenceError::InvalidData(
                "Finance subledger reads require a holding, company, or branch scope".into(),
            ));
        }
    }
    Ok(())
}

fn validate_company_filter(
    scope: &TenantScope,
    company_id: Option<Uuid>,
) -> Result<(), PersistenceError> {
    let Some(company_id) = company_id else {
        return Ok(());
    };
    match scope {
        TenantScope::Holding { .. } => Ok(()),
        TenantScope::Company {
            company_id: scope_company_id,
            ..
        }
        | TenantScope::Branch {
            company_id: scope_company_id,
            ..
        } if *scope_company_id == company_id => Ok(()),
        _ => Err(PersistenceError::InvalidData(
            "Finance subledger read company filter is outside the requested scope".into(),
        )),
    }
}

fn append_read_scope_filter(
    query: &mut QueryBuilder<'_, Postgres>,
    scope: &TenantScope,
) -> Result<(), PersistenceError> {
    match scope {
        TenantScope::Holding { .. } => {}
        TenantScope::Company { company_id, .. } => {
            query.push(" AND company_id = ").push_bind(*company_id);
        }
        TenantScope::Branch {
            company_id,
            branch_id,
            ..
        } => {
            query
                .push(" AND company_id = ")
                .push_bind(*company_id)
                .push(" AND branch_id = ")
                .push_bind(*branch_id);
        }
        TenantScope::Subtree { .. } | TenantScope::Platform => {
            return Err(PersistenceError::InvalidData(
                "Finance subledger reads require a holding, company, or branch scope".into(),
            ));
        }
    }
    Ok(())
}

fn branch_from_scope(scope: &TenantScope) -> Option<Uuid> {
    match scope {
        TenantScope::Branch { branch_id, .. } => Some(*branch_id),
        TenantScope::Subtree { branch_ids, .. } if branch_ids.len() == 1 => {
            branch_ids.first().copied()
        }
        _ => None,
    }
}

fn hash_request(
    operation: &str,
    aggregate_type: &str,
    payload: &Value,
    expected_version: Option<u64>,
    request_key: &str,
) -> Result<String, PersistenceError> {
    Ok(hex::encode(Sha256::digest(serde_json::to_vec(&(
        operation,
        aggregate_type,
        payload,
        expected_version,
        request_key,
    ))?)))
}

fn ensure_expected_version(
    aggregate_type: &str,
    aggregate_id: Uuid,
    expected: Option<u64>,
    actual: Option<u64>,
) -> Result<(), PersistenceError> {
    if expected == actual {
        Ok(())
    } else {
        Err(PersistenceError::OptimisticConcurrency {
            aggregate_type: aggregate_type.into(),
            aggregate_id,
            expected,
            actual,
        })
    }
}

fn next_version(current: Option<u64>, aggregate_type: &str) -> Result<u64, PersistenceError> {
    current
        .unwrap_or(0)
        .checked_add(1)
        .ok_or_else(|| PersistenceError::InvalidData(format!("{aggregate_type} version overflow")))
}

fn validate_source_outbox_cardinality(
    aggregate_type: String,
    aggregate_id: Uuid,
    source_version: i64,
    event_count: i64,
    first_event_version: i64,
    last_event_version: i64,
    distinct_event_versions: i64,
) -> Result<(), PersistenceError> {
    validate_aggregate_type(&aggregate_type)?;
    validate_uuid(aggregate_id, "Finance source aggregate ID")?;
    if source_version <= 0
        || event_count != source_version
        || first_event_version != 1
        || last_event_version != source_version
        || distinct_event_versions != source_version
    {
        return Err(PersistenceError::InvalidData(format!(
            "Finance subledger source/outbox cardinality is invalid for {aggregate_type}/{aggregate_id}"
        )));
    }
    Ok(())
}

fn as_i64(value: u64, field: &'static str) -> Result<i64, PersistenceError> {
    i64::try_from(value)
        .map_err(|_| PersistenceError::InvalidData(format!("{field} exceeds BIGINT")))
}

fn as_u64(value: i64, field: &'static str) -> Result<u64, PersistenceError> {
    u64::try_from(value).map_err(|_| PersistenceError::InvalidData(format!("{field} is negative")))
}

async fn begin_scoped<'a>(
    pool: &'a PgPool,
    holding_ref: &str,
) -> Result<Transaction<'a, Postgres>, PersistenceError> {
    let mut tx = pool.begin().await?;
    sqlx::query("SELECT set_config('erpx.holding_ref', $1, true)")
        .bind(holding_ref)
        .execute(&mut *tx)
        .await?;
    Ok(tx)
}

#[cfg(test)]
mod tests {
    use super::*;
    use erpx_domain::contracts::{ActorAttribution, ActorKind, CommandContext};

    #[test]
    fn aggregate_type_allow_list_is_closed() {
        assert!(validate_aggregate_type("payable_invoice").is_ok());
        assert!(validate_aggregate_type("journal_entry").is_err());
    }

    #[test]
    fn unsafe_request_keys_are_rejected_without_database_access() {
        assert!(validate_request_key(" ").is_err());
        assert!(validate_request_key("a\nkey").is_err());
    }

    #[test]
    fn branch_scope_is_the_only_single_branch_projection_hint() {
        let branch = Uuid::new_v4();
        let holding = Uuid::new_v4();
        let company = Uuid::new_v4();
        assert_eq!(
            branch_from_scope(&TenantScope::Branch {
                holding_id: holding,
                company_id: company,
                branch_id: branch,
            }),
            Some(branch)
        );
        assert_eq!(
            branch_from_scope(&TenantScope::Subtree {
                holding_id: holding,
                company_ids: vec![company],
                branch_ids: vec![branch],
            }),
            Some(branch)
        );
    }

    #[test]
    fn request_hash_changes_when_expected_version_changes() {
        let payload = serde_json::json!({"amount_minor": 100});
        let first = hash_request("save", "customer_payment", &payload, Some(1), "req").unwrap();
        let second = hash_request("save", "customer_payment", &payload, Some(2), "req").unwrap();
        assert_ne!(first, second);
    }

    #[test]
    fn source_outbox_cardinality_rejects_a_missing_version_event() {
        let error = validate_source_outbox_cardinality(
            "payable_invoice".into(),
            Uuid::new_v4(),
            1,
            0,
            0,
            0,
            0,
        )
        .expect_err("a missing source event must fail closed");
        assert!(matches!(
            error,
            PersistenceError::InvalidData(message)
                if message.starts_with("Finance subledger source/outbox cardinality is invalid")
        ));
    }

    #[test]
    fn replay_outbox_matching_requires_one_exact_authoritative_event() {
        let holding_id = Uuid::new_v4();
        let company_id = Uuid::new_v4();
        let scope = TenantScope::Company {
            holding_id,
            company_id,
        };
        let context = CommandContext::try_new(
            "subledger-replay-evidence-test",
            ActorAttribution {
                principal_ref: "subledger-test".into(),
                kind: ActorKind::TenantUser,
                session_id: None,
                represented_principal_ref: None,
                support_capability_id: None,
            },
            scope.clone(),
            "subledger replay evidence",
        )
        .expect("context");
        let reconciliation = erpx_domain::finance_subledgers::CashReconciliation::new(
            Uuid::new_v4(),
            scope,
            holding_id,
            company_id,
            Uuid::new_v4(),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 1).expect("date"),
            "bank-1",
            100,
            100,
            &context,
        )
        .expect("reconciliation");
        let aggregate = serde_json::to_value(&reconciliation).expect("aggregate payload");
        let receipt = FinanceSubledgerWriteReceipt {
            aggregate_type: "cash_reconciliation".into(),
            aggregate_id: reconciliation.metadata.id,
            version: 1,
            replayed: false,
        };
        let event_payload = serde_json::json!({
            "aggregate_type": "cash_reconciliation",
            "aggregate": aggregate,
            "version": 1,
        });
        let valid = vec![(
            "finance.subledger.cash_reconciliation.v1".into(),
            event_payload.clone(),
        )];
        validate_replay_outbox_rows(&receipt, Some(&event_payload), &valid)
            .expect("exact replay source evidence");

        assert!(validate_replay_outbox_rows(&receipt, Some(&event_payload), &[]).is_err());
        assert!(
            validate_replay_outbox_rows(
                &receipt,
                Some(&event_payload),
                &[valid[0].clone(), valid[0].clone()]
            )
            .is_err()
        );
        let wrong_type = vec![(
            "finance.subledger.cash_receipt.v1".into(),
            event_payload.clone(),
        )];
        assert!(validate_replay_outbox_rows(&receipt, Some(&event_payload), &wrong_type).is_err());
        let wrong_payload = serde_json::json!({
            "aggregate_type": "cash_reconciliation",
            "aggregate": {"tampered": true},
            "version": 1,
        });
        assert!(
            validate_replay_outbox_rows(
                &receipt,
                Some(&event_payload),
                &[(
                    "finance.subledger.cash_reconciliation.v1".into(),
                    wrong_payload,
                )]
            )
            .is_err()
        );
    }

    #[test]
    fn idempotency_receipt_binding_rejects_operation_or_aggregate_drift() {
        let aggregate_id = Uuid::new_v4();
        validate_receipt_binding(
            "save",
            "cash_receipt",
            aggregate_id,
            "save",
            "cash_receipt",
            aggregate_id,
        )
        .expect("matching receipt binding");
        assert!(
            validate_receipt_binding(
                "allocation_batch",
                "cash_receipt",
                aggregate_id,
                "save",
                "cash_receipt",
                aggregate_id,
            )
            .is_err()
        );
        assert!(
            validate_receipt_binding(
                "save",
                "cash_receipt",
                aggregate_id,
                "save",
                "cash_receipt",
                Uuid::new_v4(),
            )
            .is_err()
        );
    }
}
