import type { WorkContext } from "../../identity_access/public/authorization";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { assertion } from "../../workflow/public/commands";
import { ApiError } from "../../runtime/public/http";
import { snapshotMoney } from "../public/company-money";

/** A draft receivable and its immutable money snapshot commit with the Field Service command. */
export async function prepareFieldServiceInvoice(db: SqlDatabase, actor: WorkContext, input: { key: string; orderId: string; customerId: string; currency: string; amountMinor: number; invoiceDate: string; dueDate: string; now: number }) {
  const accounts = await db.prepare(`SELECT ar.id receivable_account_id,rev.id revenue_account_id
    FROM finance_accounts ar JOIN finance_charts c ON c.id=ar.chart_id AND c.company_id=ar.company_id
    JOIN finance_accounts rev ON rev.chart_id=ar.chart_id AND rev.company_id=ar.company_id
    WHERE ar.company_id=? AND ar.status='active' AND ar.control_kind='receivable'
    AND rev.status='active' AND rev.category='revenue' AND ar.currency=? AND rev.currency=? AND c.base_currency=?
    ORDER BY ar.code,rev.code LIMIT 1`).bind(actor.companyId, input.currency, input.currency, input.currency).first<{ receivable_account_id: string; revenue_account_id: string }>();
  if (!accounts) throw new ApiError(422, "FIELD_SERVICE_FINANCE_SETUP_REQUIRED", "Configure an active receivable control account and revenue account in Finance before releasing field service billing.");
  const money = await snapshotMoney(db, actor, { sourceType: "field_service_billing_release", sourceId: `field-billing-${input.key}`, amountKind: "invoice_total", transactionDate: input.invoiceDate, currency: input.currency, currencyScale: 2, amountUnits: String(input.amountMinor) });
  return { money: money.statement, steps: [
    assertion(db, input.key, "finance-accounts", "EXISTS(SELECT 1 FROM finance_accounts ar JOIN finance_accounts rev ON rev.chart_id=ar.chart_id AND rev.company_id=ar.company_id JOIN finance_charts c ON c.id=ar.chart_id WHERE ar.id=? AND rev.id=? AND ar.company_id=? AND ar.status='active' AND ar.control_kind='receivable' AND rev.status='active' AND rev.category='revenue' AND ar.currency=? AND rev.currency=? AND c.base_currency=?)", [accounts.receivable_account_id, accounts.revenue_account_id, actor.companyId, input.currency, input.currency, input.currency]),
    db.prepare("INSERT INTO finance_invoices(id,company_id,party_id,kind,invoice_number,invoice_date,due_date,currency,subtotal_minor,tax_minor,total_minor,outstanding_minor,debit_account_id,credit_account_id,source_type,source_id,status,created_by,created_at,updated_at) VALUES(?,?,?,'receivable',?,?,?,?,?,0,?,?,?,?,'field_service',?,'draft',?,?,?)").bind(`field-invoice-${input.key}`, actor.companyId, input.customerId, `FS-${input.orderId}`, input.invoiceDate, input.dueDate, input.currency, input.amountMinor, input.amountMinor, input.amountMinor, accounts.receivable_account_id, accounts.revenue_account_id, input.orderId, actor.userId, input.now, input.now),
  ] };
}
