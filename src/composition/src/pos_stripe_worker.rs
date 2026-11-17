//! Native Stripe `complete_sale` drain worker.
//!
//! One bounded cycle per exact Holding database. The worker claims only
//! `complete_sale` jobs from the retained `pos_jobs` queue, runs the native
//! POS `SaleComplete` command through the same owner ports the HTTP route uses,
//! and commits the exact queued Finance/Inventory effects on the same
//! serializable Holding unit. The retained TypeScript `drainPos` must invoke
//! this worker and fails closed without it; there is no TS completion fallback.
use erpx_server_finance::public::FinanceActor;
use erpx_server_finance::public::pos_journal::{
    PosInvoiceRecord, PosJournalLine as FinancePosJournalLine, PosJournalPosting, PosReceiptRecord,
    insert_pos_invoice, insert_pos_receipt, insert_pos_refund_payment, post_pos_journal,
};
use erpx_server_finance::public::pos_sources::{read_open_period_for_date, PosBankTransaction as PosBankSnapshot};
use erpx_server_inventory::public::availability_read::read_availability;
use erpx_server_inventory::public::pos_consumption::{PosConsumptionPart as InventoryConsumptionPart, consume_pos_parts};
use erpx_server_organization::public::pos_issuer::read_company_display_name;
use erpx_server_pos::public::commands::{
    PgPosCommandRepository, PosActor, PosBankTransaction as PosDomainBankTransaction,
    PosCommand, PosCommandInput, PosCommandRepository, PosConsumptionPart, PosFinancePort,
    PosFuture, PosHospitalityPort, PosInventoryPort, PosInvoiceRequest, PosJournalRequest,
    PosOperation, PosOrganizationPort, PosReceiptRequest, PosRefundPaymentRequest, PosResult,
    PosReturnPart, PosStockRow, execute_pos_command,
};
use erpx_server_runtime::public::postgres::{DatabaseConfig, DatabasePool, PoolLimits, connect_pool};
use erpx_server_shared::public::{
    BusinessError, Clock, CommandKey, CompanyId, FailureKind, HoldingId, TimestampMillis,
};
use serde::Deserialize;
use sqlx::Acquire;
use std::collections::BTreeMap;
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

const DRAIN_LOCK: i64 = 905_317_441;
const MAX_CYCLE: usize = 8;

#[derive(Deserialize)]
struct DatabaseManifest {
    version: u8,
    databases: BTreeMap<String, String>,
    slots: Vec<DatabaseManifestSlot>,
}

#[derive(Deserialize)]
struct DatabaseManifestSlot {
    id: String,
    write: String,
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

fn holding_url<'a>(manifest: &'a DatabaseManifest, holding: &str) -> Result<&'a str, &'static str> {
    let slot = manifest
        .slots
        .iter()
        .find(|slot| slot.id == holding)
        .ok_or("POS_STRIPE_HOLDING_NOT_FOUND")?;
    manifest
        .databases
        .get(&slot.write)
        .map(String::as_str)
        .ok_or("POS_STRIPE_DATABASE_NOT_CONFIGURED")
}

async fn open(url: &str) -> Result<DatabasePool, &'static str> {
    let config = DatabaseConfig::from_url(url).map_err(|_| "POS_STRIPE_DATABASE_INVALID")?;
    connect_pool(
        &config,
        PoolLimits {
            max_connections: 2,
            acquire_timeout: std::time::Duration::from_secs(10),
        },
    )
    .await
    .map_err(|_| "POS_STRIPE_DATABASE_UNAVAILABLE")
}

struct FixedClock(TimestampMillis);
impl Clock for FixedClock {
    fn now(&self) -> TimestampMillis {
        self.0
    }
}

struct DrainFinance {
    business_date: String,
    period_open: bool,
    party_exists: bool,
    journals: Vec<PosJournalRequest>,
    receipts: Vec<PosReceiptRequest>,
    invoices: Vec<PosInvoiceRequest>,
    refund_payments: Vec<PosRefundPaymentRequest>,
    bank: Option<PosBankSnapshot>,
    bank_matches: Vec<(String, String)>,
}

#[derive(Default)]
struct DrainInventory {
    parts: Vec<PosConsumptionPart>,
    returns: Vec<PosReturnPart>,
}

impl PosInventoryPort for DrainInventory {
    fn queue_consumption<'a>(&'a mut self, parts: &'a [PosConsumptionPart]) -> PosFuture<'a, i64> {
        Box::pin(async move {
            self.parts.extend_from_slice(parts);
            Ok(parts.iter().map(|p| p.expected_value_minor).sum())
        })
    }
    fn queue_return<'a>(&'a mut self, parts: &'a [PosReturnPart]) -> PosFuture<'a, ()> {
        Box::pin(async move {
            self.returns.extend_from_slice(parts);
            Ok(())
        })
    }
}

impl PosFinancePort for DrainFinance {
    fn open_period<'a>(&'a mut self, _: &'a CompanyId, date: &'a str) -> PosFuture<'a, ()> {
        Box::pin(async move {
            if date == self.business_date && self.period_open {
                Ok(())
            } else {
                Err(BusinessError::new(
                    FailureKind::Conflict,
                    "POS_PERIOD_CLOSED",
                    "Open the Finance period for this business date.",
                ))
            }
        })
    }
    fn post_journal<'a>(&'a mut self, request: &'a PosJournalRequest) -> PosFuture<'a, Option<String>> {
        Box::pin(async move {
            let journal_id = format!("journal-{}", request.source_key);
            self.journals.push(request.clone());
            Ok(Some(journal_id))
        })
    }
    fn party_exists<'a>(&'a mut self, _: &'a CompanyId, _: &'a str) -> PosFuture<'a, bool> {
        Box::pin(async move { Ok(self.party_exists) })
    }
    fn record_receipt<'a>(&'a mut self, request: &'a PosReceiptRequest) -> PosFuture<'a, ()> {
        Box::pin(async move {
            self.receipts.push(request.clone());
            Ok(())
        })
    }
    fn record_invoice<'a>(&'a mut self, request: &'a PosInvoiceRequest) -> PosFuture<'a, ()> {
        Box::pin(async move {
            self.invoices.push(request.clone());
            Ok(())
        })
    }
    fn bank_transaction<'a>(
        &'a mut self,
        _: &'a CompanyId,
        id: &'a str,
    ) -> PosFuture<'a, Option<PosDomainBankTransaction>> {
        Box::pin(async move {
            Ok(self.bank.as_ref().filter(|b| b.id == id).map(|b| PosDomainBankTransaction {
                id: b.id.clone(),
                version: b.version,
                match_status: b.match_status.clone(),
                bank_account: b.bank_account.clone(),
                currency: b.currency.clone(),
                amount_minor: b.amount_minor,
                external_id: b.external_id.clone(),
                transaction_date: b.transaction_date.clone(),
            }))
        })
    }
    fn attach_bank_payment<'a>(
        &'a mut self,
        bank_transaction_id: &'a str,
        finance_payment_id: &'a str,
    ) -> PosFuture<'a, ()> {
        Box::pin(async move {
            self.bank_matches
                .push((bank_transaction_id.to_owned(), finance_payment_id.to_owned()));
            Ok(())
        })
    }
    fn record_refund_payment<'a>(&'a mut self, request: &'a PosRefundPaymentRequest) -> PosFuture<'a, ()> {
        Box::pin(async move {
            self.refund_payments.push(request.clone());
            Ok(())
        })
    }
}

struct DrainHospitality;
impl PosHospitalityPort for DrainHospitality {
    fn hotel_day_open<'a>(
        &'a mut self,
        _: &'a CompanyId,
        _: &'a str,
        _: &'a str,
        _: &'a str,
    ) -> PosFuture<'a, bool> {
        Box::pin(async move { Ok(true) })
    }
    fn room_charge_exists<'a>(&'a mut self, _: &'a str) -> PosFuture<'a, bool> {
        Box::pin(async move { Ok(false) })
    }
}

struct DrainOrganization(String);
impl PosOrganizationPort for DrainOrganization {
    fn issuer_name<'a>(&'a mut self, _: &'a CompanyId) -> PosFuture<'a, String> {
        Box::pin(async move { Ok(self.0.clone()) })
    }
}

struct ClaimedJob {
    id: String,
    company_id: String,
    record_id: String,
    payload: Option<String>,
}

async fn process_one(
    connection: &mut sqlx::PgConnection,
    holding: &str,
    job: ClaimedJob,
    token: &str,
) -> Result<bool, &'static str> {
    let now_ms = now();
    let company = CompanyId::new(&job.company_id).map_err(|_| "POS_STRIPE_COMPANY_INVALID")?;
    let holding_id = HoldingId::new(holding).map_err(|_| "POS_STRIPE_HOLDING_INVALID")?;
    let actor = PosActor::system(holding_id, company.clone());
    let mut repository = PgPosCommandRepository::new(connection);
    let sale = repository
        .sale(&company, &job.record_id)
        .await
        .map_err(|_| "POS_STRIPE_SALE_UNAVAILABLE")?
        .ok_or("POS_STRIPE_SALE_MISSING")?;
    if sale.state == "completed" {
        return Ok(false);
    }
    let business_date = repository
        .business_date(TimestampMillis::new(now_ms))
        .await
        .map_err(|_| "POS_STRIPE_BUSINESS_DATE_UNAVAILABLE")?;
    drop(repository);
    let period_open = read_open_period_for_date(connection, company.as_str(), &business_date)
        .await
        .map_err(|_| "POS_STRIPE_PERIOD_UNAVAILABLE")?
        .is_some();
    let issuer = read_company_display_name(connection, company.as_str())
        .await
        .map_err(|_| "POS_STRIPE_ISSUER_UNAVAILABLE")?;
    let stock_rows = read_availability(connection, company.as_str(), None)
        .await
        .map_err(|_| "POS_STRIPE_STOCK_UNAVAILABLE")?;
    let stock: Vec<PosStockRow> = stock_rows
        .into_iter()
        .map(|row| PosStockRow {
            warehouse_id: row.warehouse_id,
            product_id: row.product_id,
            quantity_milli: row.quantity_milli,
            reserved_milli: row.reserved_milli,
            value_minor: row.value_minor,
        })
        .collect();
    let hotel = PgPosCommandRepository::new(connection)
        .company_hotel_registers(&company)
        .await
        .map_err(|_| "POS_STRIPE_HOSPITALITY_UNAVAILABLE")?;
    if hotel {
        return Err("POS_STRIPE_HOSPITALITY_PORT_REQUIRED");
    }
    let value = serde_json::json!({
        "id": sale.id,
        "expectedVersion": sale.version,
    });
    let key = CommandKey::new(format!("pos-stripe:{}:{}", job.id, sale.id))
        .map_err(|_| "POS_STRIPE_COMMAND_KEY_INVALID")?;
    let command = PosCommand::parse(PosOperation::SaleComplete, key, &value)
        .map_err(|_| "POS_STRIPE_COMMAND_INVALID")?;
    let mut repository = PgPosCommandRepository::new(connection);
    let mut finance = DrainFinance {
        business_date,
        period_open,
        party_exists: true,
        journals: Vec::new(),
        receipts: Vec::new(),
        invoices: Vec::new(),
        refund_payments: Vec::new(),
        bank: None,
        bank_matches: Vec::new(),
    };
    let mut inventory = DrainInventory::default();
    let mut hospitality = DrainHospitality;
    let mut organization = DrainOrganization(issuer);
    let result: PosResult = execute_pos_command(
        &actor,
        &command,
        &mut repository,
        None,
        Some(&stock),
        &mut finance,
        &mut inventory,
        &mut hospitality,
        &mut organization,
        TimestampMillis::new(now_ms),
    )
    .await
    .map_err(|_| "POS_STRIPE_COMMAND_FAILED")?;
    drop(repository);
    if !inventory.parts.is_empty() {
        consume_pos_parts(
            connection,
            &inventory
                .parts
                .iter()
                .map(|part| InventoryConsumptionPart {
                    fulfillment_id: part.fulfillment_id.clone(),
                    warehouse_id: part.warehouse_id.clone(),
                    product_id: part.product_id.clone(),
                    quantity_milli: part.quantity_milli,
                    expected_value_minor: part.expected_value_minor,
                })
                .collect::<Vec<_>>(),
        )
        .await
        .map_err(|_| "POS_STRIPE_CONSUMPTION_FAILED")?;
    }
    if !inventory.returns.is_empty() {
        return Err("POS_STRIPE_RETURNS_UNSUPPORTED");
    }
    let finance_actor = FinanceActor {
        user_id: actor.user_id.clone(),
        holding_id: actor.holding_id.clone(),
        company_id: actor.company_id.clone(),
        assignment_id: actor.assignment_id.clone(),
    };
    for request in finance.journals {
        let posting = PosJournalPosting {
            journal_id: format!("journal-{}", request.source_key),
            actor: finance_actor.clone(),
            chart_id: request.chart_id,
            currency: request.currency,
            business_date: request.business_date,
            memo: format!("POS {}", request.source_key),
            source_key: request.source_key,
            source_id: request.source_id,
            lines: request
                .lines
                .into_iter()
                .map(|line| FinancePosJournalLine {
                    account_id: line.account_id,
                    debit_minor: line.debit,
                    credit_minor: line.credit,
                })
                .collect(),
        };
        post_pos_journal(connection, &FixedClock(TimestampMillis::new(now_ms)), &posting)
            .await
            .map_err(|_| "POS_STRIPE_JOURNAL_FAILED")?;
    }
    for receipt in finance.receipts {
        let pos_payment_id = receipt.pos_payment_id.clone();
        let finance_payment_id = receipt.finance_payment_id.clone();
        insert_pos_receipt(
            connection,
            &PosReceiptRecord {
                finance_payment_id: receipt.finance_payment_id,
                company_id: receipt.company_id.as_str().to_owned(),
                party_id: receipt.party_id,
                payment_number: receipt.number,
                payment_date: receipt.payment_date,
                currency: receipt.currency,
                amount_minor: receipt.amount_minor,
                account_id: receipt.account_id,
                journal_id: receipt.journal_id,
                actor_id: receipt.actor_id.as_str().to_owned(),
                now: receipt.now.get(),
            },
        )
        .await
        .map_err(|_| "POS_STRIPE_RECEIPT_FAILED")?;
        PgPosCommandRepository::new(connection)
            .attach_finance_payment(&pos_payment_id, &finance_payment_id)
            .await
            .map_err(|_| "POS_STRIPE_RECEIPT_ATTACH_FAILED")?;
    }
    for invoice in finance.invoices {
        insert_pos_invoice(
            connection,
            &PosInvoiceRecord {
                invoice_id: invoice.invoice_id.clone(),
                company_id: invoice.company_id.as_str().to_owned(),
                party_id: invoice.party_id,
                number: invoice.number,
                invoice_date: invoice.invoice_date,
                currency: invoice.currency,
                subtotal_minor: invoice.subtotal_minor,
                tax_minor: invoice.tax_minor,
                total_minor: invoice.total_minor,
                receivable_account_id: invoice.receivable_account_id,
                revenue_account_id: invoice.revenue_account_id,
                tax_account_id: invoice.tax_account_id,
                journal_id: invoice.journal_id,
                source_id: invoice.sale_id.clone(),
                actor_id: invoice.actor_id.as_str().to_owned(),
                now: invoice.now.get(),
            },
        )
        .await
        .map_err(|_| "POS_STRIPE_INVOICE_FAILED")?;
        PgPosCommandRepository::new(connection)
            .attach_invoice(&invoice.sale_id, &invoice.invoice_id)
            .await
            .map_err(|_| "POS_STRIPE_INVOICE_ATTACH_FAILED")?;
    }
    for refund in finance.refund_payments {
        insert_pos_refund_payment(
            connection,
            &PosReceiptRecord {
                finance_payment_id: refund.finance_payment_id,
                company_id: refund.company_id.as_str().to_owned(),
                party_id: refund.party_id,
                payment_number: refund.number,
                payment_date: refund.payment_date,
                currency: refund.currency,
                amount_minor: refund.amount_minor,
                account_id: refund.account_id,
                journal_id: refund.journal_id,
                actor_id: refund.actor_id.as_str().to_owned(),
                now: refund.now.get(),
            },
        )
        .await
        .map_err(|_| "POS_STRIPE_REFUND_FAILED")?;
    }
    for (bank_transaction_id, finance_payment_id) in finance.bank_matches {
        erpx_server_finance::public::pos_sources::attach_pos_bank_payment(
            connection,
            &bank_transaction_id,
            &finance_payment_id,
        )
        .await
        .map_err(|_| "POS_STRIPE_BANK_MATCH_FAILED")?;
    }
    let _ = result;
    let _ = job.payload.as_deref();
    let _ = token;
    Ok(false)
}

/// CLI entry: `pos-stripe-worker --once --database-manifest <path> --holding <id>`.
pub async fn run_once_file(path: &Path, holding: &str) -> Result<String, &'static str> {
    let document = std::fs::read_to_string(path).map_err(|_| "POS_STRIPE_MANIFEST_UNREADABLE")?;
    let manifest: DatabaseManifest =
        serde_json::from_str(&document).map_err(|_| "POS_STRIPE_MANIFEST_INVALID")?;
    if manifest.version != 1 {
        return Err("POS_STRIPE_MANIFEST_VERSION");
    }
    let url = holding_url(&manifest, holding)?;
    let pool = open(url).await?;
    let result = run_once(&pool, holding).await;
    pool.close().await;
    result
}

async fn run_once(pool: &DatabasePool, holding: &str) -> Result<String, &'static str> {
    let mut connection = pool
        .raw()
        .acquire()
        .await
        .map_err(|_| "POS_STRIPE_DATABASE_UNAVAILABLE")?;
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(DRAIN_LOCK)
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| "POS_STRIPE_LOCK_UNAVAILABLE")?;
    if !locked {
        return Ok(serde_json::json!({"holdings": [{"holdingId": holding, "applied": 0, "reason": "catalog-lock"}]}).to_string());
    }
    let mut applied = 0usize;
    let mut errors: Vec<String> = Vec::new();
    for _ in 0..MAX_CYCLE {
        let mut tx = connection
            .begin()
            .await
            .map_err(|_| "POS_STRIPE_TRANSACTION_UNAVAILABLE")?;
        let token = uuid_v4();
        let now_ms = now();
        let claimed: Option<(String, String, String, Option<String>)> = sqlx::query_as(
            "UPDATE pos_jobs SET state='running',lease_token=$1,lease_until=$2,attempts=attempts+1 \
             WHERE id=(SELECT id FROM pos_jobs WHERE kind='complete_sale' \
             AND (state='pending' OR (state='running' AND lease_until<$3)) \
             AND next_attempt_at<=$4 ORDER BY created_at FOR UPDATE SKIP LOCKED LIMIT 1) \
             RETURNING id, company_id, record_id, payload_json::text AS payload",
        )
        .bind(&token)
        .bind(now_ms + 120_000)
        .bind(now_ms)
        .bind(now_ms)
        .fetch_optional(&mut *tx)
        .await
        .map_err(|e| { eprintln!("POS_STRIPE_JOB_CLAIM_ERROR: {e:?}"); "POS_STRIPE_JOB_CLAIM_FAILED" })?;
        let Some((id, company_id, record_id, payload)) = claimed else {
            tx.commit().await.map_err(|_| "POS_STRIPE_TRANSACTION_FAILED")?;
            break;
        };
        let job = ClaimedJob { id, company_id, record_id, payload };
        let job_id = job.id.clone();
        match process_one(&mut tx, holding, job, &token).await {
            Ok(retry) => {
                let state = if retry { "pending" } else { "completed" };
                sqlx::query(
                    "UPDATE pos_jobs SET state=$1,next_attempt_at=$2,attempts=0,lease_until=NULL, \
                     lease_token=NULL,last_error=NULL WHERE id=$3 AND lease_token=$4",
                )
                .bind(state)
                .bind(now_ms + 15_000)
                .bind(&job_id)
                .bind(&token)
                .execute(&mut *tx)
                .await
                .map_err(|_| "POS_STRIPE_JOB_COMPLETE_FAILED")?;
                tx.commit().await.map_err(|_| "POS_STRIPE_TRANSACTION_FAILED")?;
                applied += 1;
            }
            Err(code) => {
                eprintln!("POS_STRIPE_JOB_ERROR: {job_id} {code}");
                sqlx::query(
                    "UPDATE pos_jobs SET state='pending',next_attempt_at=$1,last_error=$2, \
                     lease_until=NULL,lease_token=NULL WHERE id=$3 AND lease_token=$4",
                )
                .bind(now_ms + 60_000)
                .bind(code)
                .bind(&job_id)
                .bind(&token)
                .execute(&mut *tx)
                .await
                .map_err(|_| "POS_STRIPE_JOB_RETRY_FAILED")?;
                tx.commit().await.map_err(|_| "POS_STRIPE_TRANSACTION_FAILED")?;
                errors.push(format!("{job_id}:{code}"));
                break;
            }
        }
    }
    let _: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
        .bind(DRAIN_LOCK)
        .fetch_one(&mut *connection)
        .await
        .map_err(|_| "POS_STRIPE_UNLOCK_FAILED")?;
    Ok(serde_json::json!({
        "holdings": [{"holdingId": holding, "applied": applied, "errors": errors}]
    })
    .to_string())
}

fn uuid_v4() -> String {
    use std::fmt::Write as _;
    let bytes: [u8; 16] = std::array::from_fn(|_| (now() % 251) as u8);
    let mut out = String::with_capacity(36);
    for (index, byte) in bytes.iter().enumerate() {
        if matches!(index, 4 | 6 | 8 | 10) {
            out.push('-');
        }
        let _ = write!(out, "{byte:02x}");
    }
    out
}
