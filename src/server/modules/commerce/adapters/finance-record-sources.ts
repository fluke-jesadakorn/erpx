import {financeSalesReceiptOriginalSources} from '../../finance/public/sales-receipt-sources';
import {supplierPaymentOriginalSources,financePaymentReversalSources} from "../../finance/public/supplier-payment-correction-sources";
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";

/** Physical Commerce ownership does not distinguish Sales from Supply. These
 * closed declarations follow actual same-Company order/fulfillment/return FKs.
 * A payment is included only when all its allocations belong to this owner;
 * one purchased invoice cannot reveal a mixed payment's other documents. */
function sources(kind:"sales"|"purchase",retainedCorrections=true){
 const invoiceKind=kind==="sales"?"receivable":"payable",returnKind=kind==="sales"?"customer_return":"supplier_return";
 const invoice=`SELECT o.company_id,f.invoice_id id FROM commerce_orders o JOIN commerce_fulfillments f ON f.order_id=o.id JOIN finance_invoices i ON i.id=f.invoice_id AND i.company_id=o.company_id AND i.kind='${invoiceKind}' WHERE o.kind='${kind}'`;
 const returns=`SELECT r.* FROM commerce_returns r JOIN commerce_orders o ON o.id=r.order_id AND o.company_id=r.company_id AND o.kind='${kind}' JOIN commerce_fulfillments f ON f.id=r.fulfillment_id AND f.order_id=o.id AND f.invoice_id=r.invoice_id JOIN (${invoice}) owned_invoice ON owned_invoice.id=r.invoice_id AND owned_invoice.company_id=r.company_id WHERE r.kind='${returnKind}' AND r.status IN ('approved','posted')`;
 const refund=`SELECT b.company_id,b.id FROM finance_refund_obligations b JOIN (${returns}) r ON r.id=b.return_id AND r.company_id=b.company_id AND r.invoice_id=b.invoice_id AND r.credit_journal_id=b.source_credit_journal_id WHERE r.status='posted' AND b.billing_correction_id IS NULL`;
 const settledPayment=`SELECT p.company_id,p.id FROM finance_payments p WHERE p.status='settled' AND (
   (EXISTS(SELECT 1 FROM finance_allocations a JOIN (${invoice}) owned_invoice ON owned_invoice.id=a.invoice_id AND owned_invoice.company_id=a.company_id WHERE a.payment_id=p.id AND a.company_id=p.company_id)
    AND NOT EXISTS(SELECT 1 FROM finance_allocations a WHERE a.payment_id=p.id AND (a.company_id<>p.company_id OR NOT EXISTS(SELECT 1 FROM (${invoice}) owned_invoice WHERE owned_invoice.id=a.invoice_id AND owned_invoice.company_id=a.company_id))))
   OR (NOT EXISTS(SELECT 1 FROM finance_allocations a WHERE a.payment_id=p.id) AND EXISTS(SELECT 1 FROM finance_refund_instructions instruction JOIN (${refund}) owned_refund ON owned_refund.id=instruction.obligation_id AND owned_refund.company_id=instruction.company_id WHERE instruction.payment_id=p.id AND instruction.company_id=p.company_id AND instruction.status='confirmed')))`;
 const originalCorrection=kind==="purchase"?`SELECT original.company_id,original.id FROM purchasing_original_payments original JOIN purchasing_reversed_payments reversal ON reversal.id=original.id AND reversal.company_id=original.company_id JOIN (${invoice}) actual_invoice ON actual_invoice.id=original.invoice_id AND actual_invoice.company_id=original.company_id`:"SELECT company_id,id FROM finance_payments WHERE FALSE";
 const controlledCorrection=kind==="purchase"?`SELECT payment.company_id,payment.id FROM finance_payments payment JOIN command_receipts created ON created.record_id=payment.id AND created.operation='finance.payment.create' AND created.actor_id=payment.created_by AND created.authority_json::jsonb->>'companyId'=payment.company_id
 JOIN purchasing_original_payments original ON original.id=created.result_json::jsonb->>'correctionPaymentId' AND original.company_id=payment.company_id AND original.invoice_id=created.result_json::jsonb->>'settlementInvoiceId'
 JOIN purchasing_reversed_payments reversal ON reversal.id=original.id AND reversal.company_id=original.company_id JOIN (${invoice}) actual_invoice ON actual_invoice.id=original.invoice_id AND actual_invoice.company_id=original.company_id
 WHERE payment.kind='payment' AND payment.party_id=original.party_id AND payment.currency=original.currency AND payment.bank_account=original.bank_account AND payment.amount_minor=original.amount_minor
 AND NOT EXISTS(SELECT 1 FROM finance_allocations allocation WHERE allocation.payment_id=payment.id AND (allocation.company_id<>payment.company_id OR allocation.invoice_id<>original.invoice_id))`:"SELECT company_id,id FROM finance_payments WHERE FALSE";
 // Keep one exact immutable correction proof per declaration. Repeating the
 // full reversal/book query in every journal UNION caused source-only record
 // reads to spend seconds planning/evaluating the same closed relation.
 const purchasingCtes=`purchasing_original_payments AS MATERIALIZED (${supplierPaymentOriginalSources}),purchasing_reversed_payments AS MATERIALIZED (${financePaymentReversalSources})`;
 // Retained correction history needs only voided original receipts. Keep the
 // exact Finance proof, without materializing every settled receipt or its
 // unused private image on each ordinary included document read.
 const salesCtes=`sales_original_receipts AS MATERIALIZED (SELECT original.company_id,original.id,original.invoice_id FROM (${financeSalesReceiptOriginalSources}) original WHERE original.status='voided'),sales_reversed_payments AS MATERIALIZED (SELECT reversal.* FROM (${financePaymentReversalSources}) reversal JOIN sales_original_receipts original ON original.id=reversal.id AND original.company_id=reversal.company_id)`;
 const salesRetained=`SELECT original.company_id,original.id FROM sales_original_receipts original JOIN (${invoice}) owned_invoice ON owned_invoice.id=original.invoice_id AND owned_invoice.company_id=original.company_id JOIN sales_reversed_payments reversal ON reversal.id=original.id AND reversal.company_id=original.company_id`;
 const paymentRows=kind==="purchase"?(retainedCorrections?`${settledPayment} UNION ${originalCorrection} UNION ${controlledCorrection}`:settledPayment):retainedCorrections?`${settledPayment} UNION ${salesRetained}`:settledPayment;
 const payment=kind==="purchase"?(retainedCorrections?`WITH ${purchasingCtes} ${paymentRows}`:paymentRows):retainedCorrections?`WITH ${salesCtes} ${paymentRows}`:paymentRows;
 const journalPayment=kind==="purchase"?"purchasing_payments":"sales_payments";
 const journal=`${kind==="purchase"?`WITH ${retainedCorrections?`${purchasingCtes},`:""}purchasing_payments AS MATERIALIZED (${paymentRows}) `:`WITH ${retainedCorrections?`${salesCtes},`:""}sales_payments AS MATERIALIZED (${paymentRows}) `}SELECT o.company_id,f.journal_id id FROM commerce_orders o JOIN commerce_fulfillments f ON f.order_id=o.id WHERE o.kind='${kind}' AND f.journal_id IS NOT NULL
   UNION SELECT g.company_id,g.journal_id id FROM commerce_goods_valuations g JOIN commerce_fulfillments f ON f.id=g.fulfillment_id AND f.journal_id=g.journal_id JOIN commerce_orders o ON o.id=f.order_id AND o.company_id=g.company_id WHERE o.kind='${kind}'
   UNION SELECT i.company_id,i.journal_id id FROM finance_invoices i JOIN (${invoice}) owned_invoice ON owned_invoice.id=i.id AND owned_invoice.company_id=i.company_id WHERE i.journal_id IS NOT NULL
   UNION SELECT g.company_id,g.fx_journal_id id FROM commerce_invoice_goods_carrying g JOIN (${invoice}) owned_invoice ON owned_invoice.id=g.invoice_id AND owned_invoice.company_id=g.company_id WHERE g.fx_journal_id IS NOT NULL
   UNION SELECT r.company_id,owned_journal.id FROM (${returns}) r CROSS JOIN LATERAL (VALUES(r.stock_journal_id),(r.credit_journal_id),(r.fx_journal_id),(r.goods_fx_journal_id)) owned_journal(id) WHERE owned_journal.id IS NOT NULL
   UNION SELECT p.company_id,owned_journal.id FROM finance_payments p JOIN ${journalPayment} owned_payment ON owned_payment.id=p.id AND owned_payment.company_id=p.company_id CROSS JOIN LATERAL (VALUES(p.journal_id),(p.fx_journal_id)) owned_journal(id) WHERE owned_journal.id IS NOT NULL
   ${kind==="purchase"?(retainedCorrections?`UNION SELECT reversal.company_id,owned_journal.id FROM purchasing_reversed_payments reversal JOIN ${journalPayment} owned_payment ON owned_payment.id=reversal.id AND owned_payment.company_id=reversal.company_id CROSS JOIN LATERAL(VALUES(reversal.reversal_journal_id),(reversal.fx_reversal_journal_id)) owned_journal(id) WHERE owned_journal.id IS NOT NULL`:""):retainedCorrections?`UNION SELECT reversal.company_id,owned_journal.id FROM sales_reversed_payments reversal JOIN ${journalPayment} owned_payment ON owned_payment.id=reversal.id AND owned_payment.company_id=reversal.company_id CROSS JOIN LATERAL(VALUES(reversal.reversal_journal_id),(reversal.fx_reversal_journal_id)) owned_journal(id) WHERE owned_journal.id IS NOT NULL`:""}`;
 const supplierBatch=kind==="purchase"?`SELECT b.company_id,b.id FROM finance_supplier_batches b WHERE EXISTS(SELECT 1 FROM finance_supplier_instructions item WHERE item.batch_id=b.id) AND NOT EXISTS(SELECT 1 FROM finance_supplier_instructions item WHERE item.batch_id=b.id AND NOT EXISTS(SELECT 1 FROM (${invoice}) owned_invoice WHERE owned_invoice.id=item.invoice_id AND owned_invoice.company_id=b.company_id))`:"SELECT company_id,id FROM finance_supplier_batches WHERE FALSE";
 return {invoice,journal,payment,refund,"supplier-batch":supplierBatch};
}
export const commerceFinanceRecordSources={sales:sources("sales"),purchase:sources("purchase")} as const;
/** The ordinary relation is a strict subset of the retained source. Finance
 * chooses it from an actual native read and repeats it through final effects;
 * a later status/link change cannot silently upgrade to correction history. */
export const commerceSalesFinanceOrdinaryRecordSources=sources("sales",false);
/** An ordinary Purchase source is selected by a native membership read. Its
 * settled payment/actual owner FKs remain fixed through the whole Finance/File
 * unit; exact correction/reversal history always uses the full declaration. */
export const commercePurchaseFinanceOrdinaryRecordSources=sources("purchase",false);

/** Keep actual source relationships, including absent mixed allocations,
 * stable through the included Finance command/Files transaction. */
export function commerceFinanceSourceReadLocks(database:SqlDatabase,paymentWriter=false,refundCashWriter=false){
 if(refundCashWriter)return [database.prepare("LOCK TABLE commerce_orders,commerce_fulfillments,commerce_returns,finance_customer_receipts,finance_allocations IN SHARE MODE"),database.prepare("LOCK TABLE finance_refund_instructions IN SHARE ROW EXCLUSIVE MODE"),database.prepare("LOCK TABLE finance_supplier_batches,finance_supplier_instructions IN SHARE MODE")];
 return paymentWriter?[database.prepare("LOCK TABLE commerce_orders,commerce_fulfillments,commerce_returns,finance_customer_receipts IN SHARE MODE"),database.prepare("LOCK TABLE finance_allocations IN SHARE ROW EXCLUSIVE MODE"),database.prepare("LOCK TABLE finance_refund_instructions,finance_supplier_batches,finance_supplier_instructions IN SHARE MODE")]:[database.prepare("LOCK TABLE commerce_orders,commerce_fulfillments,commerce_returns,finance_customer_receipts,finance_allocations,finance_refund_instructions,finance_supplier_batches,finance_supplier_instructions IN SHARE MODE")];
}
