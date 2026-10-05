import "server-only";
import type { WorkContext } from "./authorization";
import { assertion } from "./commands";
import { ApiError } from "./http";
import type { SqlDatabase, SqlStatement } from "./runtime/types";
import { calculateInvoiceLines, type InvoiceLineInput } from "@/lib/finance-invoice-lines";
import { minorToDecimal } from "@/lib/finance-money";
import { snapshotMoney } from "./company-money";

type Header = { partyId: string; kind: "payable" | "receivable"; invoiceNumber: string; invoiceDate: string; currency: string; debitAccountId: string; creditAccountId: string; taxAccountId?: string; taxCode?: string; lines?: InvoiceLineInput[] };
type Account = { id: string; chart_id: string; code: string; name: string; category: string; currency: string; version: number; status: string };
export type PersistedInvoiceLine = { id: string; invoice_id: string; source_version: number; ordinal: number; description: string; quantity_millis: number | null; unit_price_minor: number | null; tax_bps: number | null; subtotal_minor: number; tax_minor: number; total_minor: number; account_id: string; tax_account_id: string | null; source_snapshot_json: string; currency: string; origin_kind: string };

export async function prepareInvoiceLineRevision(db: SqlDatabase, actor: WorkContext, input: Header, invoiceId: string, revision: number, amounts: { subtotal: number; tax: number }, commandId: string) {
  let calculated: ReturnType<typeof calculateInvoiceLines> | null = null;
  try { calculated = input.lines ? calculateInvoiceLines(input.lines) : null; }
  catch (error) { throw new ApiError(422, "INVALID_INVOICE_LINE", error instanceof Error ? error.message : "Invalid invoice lines."); }
  if (calculated && (calculated.subtotalMinor !== amounts.subtotal || calculated.taxMinor !== amounts.tax)) throw new ApiError(422, "INVOICE_LINE_TOTAL_MISMATCH", "The header subtotal and tax must equal the sum of the calculated lines.");
  const netAccount = input.kind === "payable" ? input.debitAccountId : input.creditAccountId;
  const controlAccount = input.kind === "payable" ? input.creditAccountId : input.debitAccountId;
  const rows = calculated?.lines.map(line => ({ description: line.description, quantityMillis: line.quantityMillis, unitPriceMinor: line.unitPriceMinor, taxBps: line.taxBps, subtotalMinor: line.subtotalMinor, taxMinor: line.taxMinor, totalMinor: line.totalMinor, accountId: line.accountId, taxAccountId: line.taxAccountId ?? input.taxAccountId ?? null })) ?? [{ description: `Whole document ${input.invoiceNumber}`, quantityMillis: null, unitPriceMinor: null, taxBps: null, subtotalMinor: amounts.subtotal, taxMinor: amounts.tax, totalMinor: amounts.subtotal + amounts.tax, accountId: netAccount, taxAccountId: input.taxAccountId ?? null }];
  const ids = [...new Set([input.debitAccountId, input.creditAccountId, ...rows.flatMap(line => [line.accountId, ...(line.taxMinor && line.taxAccountId ? [line.taxAccountId] : [])])])];
  const accounts = (await db.prepare(`SELECT id,chart_id,code,name,category,currency,version,status FROM finance_accounts WHERE company_id=? AND id IN (${ids.map(() => "?").join(",")})`).bind(actor.companyId, ...ids).all<Account>()).results;
  if (accounts.length !== ids.length || accounts.some(account => account.status !== "active" || account.currency !== input.currency) || new Set(accounts.map(account => account.chart_id)).size !== 1) throw new ApiError(422, "ACCOUNT_INVALID", "Every invoice line must use the same active Company chart and currency.");
  const byId = new Map(accounts.map(account => [account.id, account]));
  for (const line of rows) {
    if (line.accountId === controlAccount || (line.taxMinor && (!line.taxAccountId || [line.accountId, controlAccount].includes(line.taxAccountId)))) throw new ApiError(422, "ACCOUNT_INVALID", "Invoice net, control and tax accounts must be distinct.");
  }
  const party = await db.prepare("SELECT id,code,name,kind,currency,tax_registration,version,status FROM finance_parties WHERE id=? AND company_id=? AND status='active'").bind(input.partyId, actor.companyId).first<Record<string, unknown>>();
  if (!party) throw new ApiError(422, "PARTY_INVALID", "The invoice party is unavailable.");
  const guards: SqlStatement[] = [
    ...accounts.map((account, index) => assertion(db, commandId, `line-account-${index}`, "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND status='active' AND version=?)", [account.id, actor.companyId, account.version])),
    assertion(db, commandId, "line-party", "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND version=? AND status='active')", [input.partyId, actor.companyId, Number(party.version)]),
  ];
  const now = Date.now();
  const steps: SqlStatement[] = rows.map((line, ordinal) => db.prepare(`INSERT INTO finance_invoice_line_versions(id,company_id,invoice_id,source_version,ordinal,description,quantity_millis,unit_price_minor,tax_bps,subtotal_minor,tax_minor,total_minor,currency,account_id,tax_account_id,source_snapshot_json,origin_kind,created_by,created_at) VALUES(?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?,?)`).bind(`${invoiceId}:v${revision}:${ordinal}`, actor.companyId, invoiceId, revision, ordinal, line.description, line.quantityMillis, line.unitPriceMinor, line.taxBps, line.subtotalMinor, line.taxMinor, line.totalMinor, input.currency, line.accountId, line.taxAccountId, JSON.stringify({ schema: "finance.invoice.line.v1", companyId: actor.companyId, invoiceDate: input.invoiceDate, invoiceNumber: input.invoiceNumber, kind: input.kind, taxCode: input.taxCode ?? null, party, netAccount: byId.get(line.accountId), taxAccount: line.taxAccountId ? byId.get(line.taxAccountId) : null, controlAccount: byId.get(controlAccount) }), calculated ? "native_line" : "whole_document", actor.userId, now));
  const snapshot = await snapshotMoney(db, actor, { sourceType: "finance_invoice", sourceId: invoiceId, amountKind: `draft:v${revision}`, transactionDate: input.invoiceDate, currency: input.currency, currencyScale: 2, amountUnits: String(amounts.subtotal + amounts.tax) });
  steps.push(snapshot.statement);
  return { guards, steps, mode: calculated ? "native_lines" : "whole_document", revision, count: rows.length };
}

export async function currentInvoiceLines(db: SqlDatabase, companyId: string, invoiceId: string, revision: number) {
  return (await db.prepare("SELECT * FROM finance_invoice_line_versions WHERE invoice_id=? AND company_id=? AND source_version=? ORDER BY ordinal").bind(invoiceId, companyId, revision).all<PersistedInvoiceLine>()).results;
}

export async function nativeInvoicePosting(db: SqlDatabase, actor: WorkContext, invoice: Record<string, string | number | null>, commandId: string) {
  if (invoice.source_type !== "standalone") return null;
  const rows = await currentInvoiceLines(db, actor.companyId, String(invoice.id), Number(invoice.line_revision));
  if (!rows.length || rows.reduce((sum, row) => sum + Number(row.subtotal_minor), 0) !== Number(invoice.subtotal_minor) || rows.reduce((sum, row) => sum + Number(row.tax_minor), 0) !== Number(invoice.tax_minor)) throw new ApiError(409, "INVOICE_LINES_CHANGED", "Review the invoice line revision before posting.");
  const payable = invoice.kind === "payable", controlId = String(payable ? invoice.credit_account_id : invoice.debit_account_id);
  const lines = rows.flatMap(row => [
    { accountId: row.account_id, description: row.description, debit: payable ? minorToDecimal(Number(row.subtotal_minor)) : "0", credit: payable ? "0" : minorToDecimal(Number(row.subtotal_minor)) },
    ...(Number(row.tax_minor) ? [{ accountId: String(row.tax_account_id), description: `VAT: ${row.description}`, debit: payable ? minorToDecimal(Number(row.tax_minor)) : "0", credit: payable ? "0" : minorToDecimal(Number(row.tax_minor)) }] : []),
  ]);
  lines.push({ accountId: controlId, description: `Control: ${invoice.invoice_number}`, debit: payable ? "0" : minorToDecimal(Number(invoice.total_minor)), credit: payable ? minorToDecimal(Number(invoice.total_minor)) : "0" });
  const guards: SqlStatement[] = [assertion(db, commandId, "invoice-line-revision", "EXISTS(SELECT 1 FROM finance_invoices WHERE id=? AND company_id=? AND line_revision=?)", [invoice.id, actor.companyId, invoice.line_revision])];
  for (const [index, row] of rows.entries()) {
    const snapshot = JSON.parse(row.source_snapshot_json) as { party?: { id: string; version: number }; netAccount?: Account; taxAccount?: Account | null; controlAccount?: Account };
    if (row.origin_kind === "historical_header") continue;
    for (const [key, account] of Object.entries({ net: snapshot.netAccount, tax: snapshot.taxAccount, control: snapshot.controlAccount })) if (account) guards.push(assertion(db, commandId, `invoice-line-${index}-${key}`, "EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND version=? AND status='active')", [account.id, actor.companyId, account.version]));
    if (snapshot.party) guards.push(assertion(db, commandId, `invoice-line-${index}-party`, "EXISTS(SELECT 1 FROM finance_parties WHERE id=? AND company_id=? AND version=? AND status='active')", [snapshot.party.id, actor.companyId, Number(snapshot.party.version)]));
  }
  const billing=await db.prepare("SELECT COALESCE(b.contract_content_json,c.content_json) content_json FROM finance_business_billing_runs b JOIN finance_business_contracts c ON c.id=b.contract_id WHERE b.invoice_id=? AND b.company_id=?").bind(invoice.id,actor.companyId).first<{content_json:string}>();
  const dimensions=billing?(JSON.parse(billing.content_json) as {dimensions:Record<string,string>}).dimensions:undefined;
  return { lines:dimensions?lines.map(line=>({...line,dimensions})):lines, guards };
}
