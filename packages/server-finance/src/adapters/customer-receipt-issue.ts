type Row=Record<string,string|number|null>;
import "server-only";
import { createHash } from "node:crypto";

import { renderCustomerReceipt, type ReceiptSnapshot } from "@erpx/server-finance/domain/sales";

import type { WorkContext } from "@erpx/server-identity_access/public/authorization";

import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

import { financeSalesCompanyName } from "@erpx/server-organization/public/finance-sales-company";

export async function ensureCustomerReceipt(tx: SqlDatabase, actor: WorkContext, paymentId: string) {
  const existing = await tx.prepare("SELECT id FROM finance_customer_receipts WHERE payment_id=? AND company_id=?").bind(paymentId, actor.companyId).first<{ id: string }>(); if (existing) return existing;
  const payment = await tx.prepare("SELECT p.*,party.name customer_name,party.code customer_code FROM finance_payments p JOIN finance_journals j ON j.id=p.journal_id JOIN finance_parties party ON party.id=p.party_id WHERE p.id=? AND p.company_id=? AND p.kind='receipt' AND p.status='settled' AND j.status='posted' FOR UPDATE OF p").bind(paymentId, actor.companyId).first<Row>();
  if (!payment) return null;
  // A second transaction may have issued the receipt while this one waited for the payment lock.
  const afterLock = await tx.prepare("SELECT id FROM finance_customer_receipts WHERE payment_id=?").bind(paymentId).first<{ id: string }>(); if (afterLock) return afterLock;
  const allocations = (await tx.prepare("SELECT i.id invoice_id,i.invoice_number,a.amount_minor FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE a.payment_id=? AND a.company_id=? ORDER BY a.created_at,a.id").bind(paymentId, actor.companyId).all<Row>()).results;
  const number = await tx.prepare("SELECT erpx_sales_document_number(?,'RC',?) number").bind(actor.companyId, payment.payment_date).first<string>("number");
  const snapshot: ReceiptSnapshot = { schema: "finance.customer-receipt.v1", documentNumber: number!, companyName: await financeSalesCompanyName(tx, actor.companyId), customerName: String(payment.customer_name), customerCode: String(payment.customer_code), date: String(payment.payment_date), currency: String(payment.currency), amountMinor: String(payment.amount_minor), paymentId, paymentVersion: Number(payment.version), bankReference: String(payment.payment_number), journalId: String(payment.journal_id), allocations: allocations.map(row => ({ invoiceId: String(row.invoice_id), invoiceNumber: String(row.invoice_number), amountMinor: String(row.amount_minor) })) };
  const html = renderCustomerReceipt(snapshot), id = `customer-receipt-${paymentId}`, hash = createHash("sha256").update(html).digest("hex");
  await tx.prepare("INSERT INTO finance_customer_receipts(id,company_id,payment_id,document_number,receipt_date,currency,amount_minor,snapshot_json,export_html,export_sha256,issued_by,issued_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?)").bind(id, actor.companyId, paymentId, number, payment.payment_date, payment.currency, payment.amount_minor, JSON.stringify(snapshot), html, hash, actor.userId, Date.now()).run();
  return { id };
}
