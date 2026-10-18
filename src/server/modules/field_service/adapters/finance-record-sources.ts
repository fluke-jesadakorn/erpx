import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";

/** Only the immutable original release, event, command receipt and captured
 * amount establish an invoice handoff. Caller source labels grant nothing. */
const invoice=`SELECT service.company_id,invoice.id,billing.id money_source_id
 FROM field_service_billing_releases billing
 JOIN field_service_orders service ON service.id=billing.order_id AND service.agreement_id=billing.agreement_id AND service.version>=billing.source_order_version
 JOIN field_service_agreements agreement ON agreement.id=service.agreement_id AND agreement.company_id=service.company_id AND agreement.branch_id=service.branch_id AND agreement.department_id IS NOT DISTINCT FROM service.department_id AND agreement.customer_id=billing.customer_id
 JOIN field_service_order_events event ON event.order_id=service.id AND event.version=billing.source_order_version AND event.action='release-billing' AND event.status='billed' AND event.actor_id=billing.released_by
 JOIN command_receipts receipt ON billing.id='field-billing-'||receipt.command_id AND event.id='service-order-event-'||receipt.command_id AND receipt.operation='field.order.release-billing' AND receipt.record_id=service.id AND receipt.actor_id=billing.released_by AND receipt.authority_json::jsonb->>'companyId'=service.company_id
 JOIN finance_invoices invoice ON invoice.id='field-invoice-'||receipt.command_id AND invoice.company_id=service.company_id AND invoice.kind='receivable' AND invoice.source_type='field_service' AND invoice.source_id=service.id AND invoice.party_id=billing.customer_id AND invoice.currency=billing.currency AND invoice.total_minor=billing.amount_minor AND invoice.invoice_date=billing.pricing_date
 JOIN money_transaction_snapshots money ON money.company_id=service.company_id AND money.source_type='field_service_billing_release' AND money.source_id=billing.id AND money.amount_kind='invoice_total' AND money.currency=billing.currency AND money.amount_units=billing.amount_minor
 WHERE billing.status='released'`;
const partsJournal=`SELECT service.company_id,journal.id
 FROM operational_inventory_consumptions consumption
 JOIN field_service_orders service ON service.id=consumption.source_id AND service.company_id=consumption.company_id AND service.branch_id=consumption.branch_id AND service.department_id IS NOT DISTINCT FROM consumption.department_id AND service.version>=consumption.source_version
 JOIN field_service_order_events event ON event.order_id=service.id AND event.version=consumption.source_version AND event.action='issue-part' AND event.details_json->>'consumptionId'=consumption.id AND event.details_json->>'journalId'=consumption.journal_id
 JOIN field_service_part_reservations reservation ON reservation.id=event.details_json->>'reservationId' AND reservation.order_id=service.id AND reservation.warehouse_id=consumption.warehouse_id AND reservation.product_id=consumption.product_id
 JOIN command_receipts receipt ON consumption.id='operational-part-'||receipt.command_id AND event.id='service-order-event-'||receipt.command_id AND receipt.operation='field.order.issue-part' AND receipt.record_id=service.id AND receipt.actor_id=event.actor_id AND receipt.actor_id=consumption.created_by AND receipt.authority_json::jsonb->>'companyId'=service.company_id
 JOIN finance_journals journal ON journal.id=consumption.journal_id AND journal.company_id=service.company_id AND journal.source_type='field_service_parts' AND journal.source_id=consumption.id AND journal.currency=consumption.currency
 JOIN money_transaction_snapshots money ON money.company_id=service.company_id AND money.source_type='operational_inventory_consumption' AND money.source_id=consumption.id AND money.amount_kind='stock_cost' AND money.currency=consumption.currency AND money.amount_units=consumption.value_minor
 WHERE consumption.source_kind='field_service' AND consumption.quantity_milli>0 AND consumption.value_minor>0
 AND event.details_json->>'quantityMilli'=consumption.quantity_milli::text AND event.details_json->>'costMinor'=consumption.value_minor::text
 AND (SELECT sum(debit_minor) FROM finance_journal_lines WHERE journal_id=journal.id)=consumption.value_minor
 AND (SELECT sum(credit_minor) FROM finance_journal_lines WHERE journal_id=journal.id)=consumption.value_minor`;
/** A controlled receipt is owned only by its immutable normal create receipt,
 * exact released invoice, and every actual allocation. A party or source label
 * alone cannot include a generic or mixed payment. */
const payment=`SELECT p.company_id,p.id,source.id invoice_id
 FROM finance_payments p
 JOIN command_receipts receipt ON receipt.record_id=p.id AND receipt.operation='finance.payment.create' AND receipt.actor_id=p.created_by AND receipt.authority_json::jsonb->>'companyId'=p.company_id AND receipt.result_json::jsonb->>'id'=p.id
 JOIN (${invoice}) source ON source.id=receipt.result_json::jsonb->>'settlementInvoiceId' AND source.company_id=p.company_id
 JOIN finance_invoices original ON original.id=source.id AND original.company_id=p.company_id AND original.party_id=p.party_id AND original.currency=p.currency
 WHERE p.kind='receipt'
 AND NOT EXISTS(SELECT 1 FROM finance_allocations a WHERE a.payment_id=p.id AND (a.company_id<>p.company_id OR a.invoice_id<>source.id))
 AND (p.status NOT IN ('settled','voided') OR EXISTS(SELECT 1 FROM finance_allocations a WHERE a.payment_id=p.id AND a.company_id=p.company_id AND a.invoice_id=source.id HAVING sum(a.amount_minor)=p.amount_minor))`;
export const fieldServiceFinanceRecordSources={invoice,partsJournal,payment,
 journal:`${partsJournal} UNION SELECT invoice.company_id,invoice.journal_id id FROM finance_invoices invoice JOIN (${invoice}) source ON source.id=invoice.id AND source.company_id=invoice.company_id WHERE invoice.journal_id IS NOT NULL
 UNION SELECT p.company_id,owned.id FROM finance_payments p JOIN (${payment}) source ON source.id=p.id AND source.company_id=p.company_id CROSS JOIN LATERAL (VALUES(p.journal_id),(p.fx_journal_id)) owned(id) WHERE owned.id IS NOT NULL
 UNION SELECT p.company_id,owned.id FROM finance_payments p JOIN (${payment}) source ON source.id=p.id AND source.company_id=p.company_id JOIN finance_payment_reversals reversal ON reversal.payment_id=p.id CROSS JOIN LATERAL (VALUES(reversal.reversal_journal_id),(reversal.fx_reversal_journal_id)) owned(id) WHERE owned.id IS NOT NULL`,
} as const;

/** Legacy Service invoices already captured their real billing release quote.
 * Finance may carry that immutable source, never choose a later FX quote from
 * an invoice's caller-controlled source label. */
export async function fieldServiceInvoiceMoneySource(database:SqlDatabase,companyId:string,invoiceId:string){
 const source=await database.prepare(`SELECT money_source_id FROM (${invoice}) actual_service_invoice WHERE id=? AND company_id=?`).bind(invoiceId,companyId).first<{money_source_id:string}>();
 return source?{sourceType:"field_service_billing_release",sourceId:source.money_source_id,amountKind:"invoice_total"}:null;
}

/** These read leases precede the first Finance/Files source snapshot. Service
 * starts with the same Support relation, so an owner writer never holds later
 * source leases while waiting to upgrade its earlier case relation. */
export function fieldServiceFinanceSourceReadLocks(database:SqlDatabase){return [database.prepare("LOCK TABLE support_tickets IN SHARE MODE"),database.prepare("LOCK TABLE operational_assets,field_service_agreements,field_service_orders,field_service_order_events,field_service_part_reservations,field_service_billing_releases,operational_inventory_consumptions IN SHARE MODE")];}
