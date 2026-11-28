//! Shared flush for the POS owner effects queued by the native POS command.
//!
//! The native POS command collects the exact Finance/Inventory facts it
//! decided on the owner transaction and hands them back through the owner
//! ports. The composition layer owns the assembly: it writes those queued
//! effects with the owning Rust owner on the same serializable Holding unit,
//! so the native HTTP route and the native Stripe `complete_sale` drain
//! worker produce byte-identical owner rows.
use erpx_finance_server::public::FinanceActor;
use erpx_finance_server::public::money_snapshot::{
    MoneySnapshotInput, lock_money_snapshot_sources, prepare_money_snapshot,
};
use erpx_finance_server::public::pos_journal::{
    PosInvoiceRecord, PosJournalLine as FinancePosJournalLine, PosJournalPosting, PosReceiptRecord,
    insert_pos_invoice, insert_pos_receipt, insert_pos_refund_payment, post_pos_journal,
};
use erpx_finance_server::public::pos_sources::attach_pos_bank_payment;
use erpx_inventory_server::public::pos_consumption::{
    PosConsumptionPart as InventoryConsumptionPart, consume_pos_parts, return_pos_parts,
};
use erpx_pos_server::public::commands::{
    PgPosCommandRepository, PosCommandRepository, PosConsumptionPart, PosInvoiceAllocationRequest,
    PosInvoiceRequest, PosJournalRequest, PosReceiptRequest, PosRefundPaymentRequest,
    PosReturnPart, PosSaleMoneyRequest,
};
use erpx_platform_runtime_server::public::postgres::PgTransactionError;
use erpx_platform_shared_server::public::{BusinessError, Clock};
use sqlx::PgConnection;

type Error = PgTransactionError<BusinessError>;

/// The exact owner effects the native POS command queued for this unit.
pub struct PosEffectFlush<'a> {
    pub journals: &'a [PosJournalRequest],
    pub receipts: &'a [PosReceiptRequest],
    pub invoices: &'a [PosInvoiceRequest],
    pub refund_payments: &'a [PosRefundPaymentRequest],
    pub sale_money: &'a [PosSaleMoneyRequest],
    pub invoice_allocations: &'a [PosInvoiceAllocationRequest],
    pub bank_matches: &'a [(String, String)],
    pub consumption: &'a [PosConsumptionPart],
    pub returns: &'a [PosReturnPart],
}

impl<'a> PosEffectFlush<'a> {
    pub fn empty() -> Self {
        Self {
            journals: &[],
            receipts: &[],
            invoices: &[],
            refund_payments: &[],
            sale_money: &[],
            invoice_allocations: &[],
            bank_matches: &[],
            consumption: &[],
            returns: &[],
        }
    }
}

/// Write every queued POS owner effect on the caller's open serializable
/// Holding unit. The Finance and Inventory owners revalidate their own
/// preconditions; a rejected effect fails the whole unit.
pub async fn flush_pos_effects<C: Clock + Sync>(
    connection: &mut PgConnection,
    clock: &C,
    actor: &FinanceActor,
    effects: PosEffectFlush<'_>,
) -> Result<(), Error> {
    if !effects.consumption.is_empty() {
        consume_pos_parts(
            connection,
            &effects
                .consumption
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
        .await?;
    }
    if !effects.returns.is_empty() {
        return_pos_parts(
            connection,
            &effects
                .returns
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
        .await?;
    }
    for request in effects.journals {
        let posting = PosJournalPosting {
            journal_id: format!("journal-{}", request.source_key),
            actor: actor.clone(),
            chart_id: request.chart_id.clone(),
            currency: request.currency.clone(),
            business_date: request.business_date.clone(),
            memo: format!("POS {}", request.source_key),
            source_key: request.source_key.clone(),
            source_id: request.source_id.clone(),
            lines: request
                .lines
                .iter()
                .map(|line| FinancePosJournalLine {
                    account_id: line.account_id.clone(),
                    debit_minor: line.debit,
                    credit_minor: line.credit,
                })
                .collect(),
        };
        post_pos_journal(connection, clock, &posting).await?;
    }
    for receipt in effects.receipts {
        let pos_payment_id = receipt.pos_payment_id.clone();
        let finance_payment_id = receipt.finance_payment_id.clone();
        insert_pos_receipt(
            connection,
            &PosReceiptRecord {
                finance_payment_id: receipt.finance_payment_id.clone(),
                company_id: receipt.company_id.as_str().to_owned(),
                party_id: receipt.party_id.clone(),
                payment_number: receipt.number.clone(),
                payment_date: receipt.payment_date.clone(),
                currency: receipt.currency.clone(),
                amount_minor: receipt.amount_minor,
                account_id: receipt.account_id.clone(),
                journal_id: receipt.journal_id.clone(),
                actor_id: receipt.actor_id.as_str().to_owned(),
                now: receipt.now.get(),
            },
        )
        .await?;
        PgPosCommandRepository::new(connection)
            .attach_finance_payment(&pos_payment_id, &finance_payment_id)
            .await?;
    }
    for invoice in effects.invoices {
        insert_pos_invoice(
            connection,
            &PosInvoiceRecord {
                invoice_id: invoice.invoice_id.clone(),
                company_id: invoice.company_id.as_str().to_owned(),
                party_id: invoice.party_id.clone(),
                number: invoice.number.clone(),
                invoice_date: invoice.invoice_date.clone(),
                currency: invoice.currency.clone(),
                subtotal_minor: invoice.subtotal_minor,
                tax_minor: invoice.tax_minor,
                total_minor: invoice.total_minor,
                receivable_account_id: invoice.receivable_account_id.clone(),
                revenue_account_id: invoice.revenue_account_id.clone(),
                tax_account_id: invoice.tax_account_id.clone(),
                journal_id: invoice.journal_id.clone(),
                source_id: invoice.sale_id.clone(),
                actor_id: invoice.actor_id.as_str().to_owned(),
                now: invoice.now.get(),
            },
        )
        .await?;
        PgPosCommandRepository::new(connection)
            .attach_invoice(&invoice.sale_id, &invoice.invoice_id)
            .await?;
    }
    for refund_payment in effects.refund_payments {
        insert_pos_refund_payment(
            connection,
            &PosReceiptRecord {
                finance_payment_id: refund_payment.finance_payment_id.clone(),
                company_id: refund_payment.company_id.as_str().to_owned(),
                party_id: refund_payment.party_id.clone(),
                payment_number: refund_payment.number.clone(),
                payment_date: refund_payment.payment_date.clone(),
                currency: refund_payment.currency.clone(),
                amount_minor: refund_payment.amount_minor,
                account_id: refund_payment.account_id.clone(),
                journal_id: refund_payment.journal_id.clone(),
                actor_id: refund_payment.actor_id.as_str().to_owned(),
                now: refund_payment.now.get(),
            },
        )
        .await?;
    }
    for (bank_transaction_id, finance_payment_id) in effects.bank_matches {
        attach_pos_bank_payment(connection, bank_transaction_id, finance_payment_id).await?;
    }
    for allocation in effects.invoice_allocations {
        for payment in &allocation.allocations {
            if payment.amount_minor <= 0 {
                continue;
            }
            sqlx::query("UPDATE finance_payments SET party_id=$1 WHERE id=$2")
                .bind(allocation.party_id.as_deref())
                .bind(&payment.finance_payment_id)
                .execute(&mut *connection)
                .await?;
            sqlx::query(
                "INSERT INTO finance_allocations(id,company_id,payment_id,invoice_id,amount_minor,created_by,created_at) \
                 VALUES($1,$2,$3,$4,$5,$6,$7)",
            )
            .bind(uuid::Uuid::new_v4().to_string())
            .bind(allocation.company_id.as_str())
            .bind(&payment.finance_payment_id)
            .bind(&allocation.invoice_id)
            .bind(payment.amount_minor)
            .bind(allocation.actor_id.as_str())
            .bind(allocation.now.get())
            .execute(&mut *connection)
            .await?;
        }
    }
    for request in effects.sale_money {
        // Finance owns the money utility fences. The snapshot unit refuses to
        // write without them, so take them on this same transaction.
        lock_money_snapshot_sources(connection).await?;
        let evidence = prepare_money_snapshot(
            connection,
            &request.company_id,
            MoneySnapshotInput {
                source_type: request.source_type.clone(),
                source_id: request.source_id.clone(),
                amount_kind: request.amount_kind.clone(),
                transaction_date: request.transaction_date.clone(),
                currency: request.currency.clone(),
                currency_scale: Some(request.currency_scale),
                amount_units: request.amount_units,
            },
            clock,
        )
        .await?;
        evidence.persist(connection).await?;
    }
    Ok(())
}
