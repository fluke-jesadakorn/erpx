import "server-only";
import { createHash } from "node:crypto";
import { z } from "zod";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { samePerson } from "@erpx/feature-identity_access/lib/iam";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { calculateInvoiceLines } from "@erpx/server-finance/domain/finance-invoice-lines";
import { salesInvoiceDraftSchema, salesDefaultsSchema, salesDefaultsDecisionSchema, salesQuerySchema, renderCustomerReceipt, type SalesDefaults, type ReceiptSnapshot } from "@erpx/server-finance/domain/sales";
import { requirePermission } from "@erpx/server-identity_access/public/iam-permissions";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { holdingResources } from "../../../../packages/server-runtime/src/public/runtime";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { ApiError } from "@erpx/server-runtime/public/http";
import { financeTransaction } from "@erpx/server-finance/public/finance-transactions";
import { financeSalesCompanyName } from "@erpx/server-organization/public/finance-sales-company";
import { commerceFinanceSalesSources } from "@erpx/server-commerce/public/finance-sales-sources";
import { commerceSalesDocuments } from "@erpx/server-commerce/public/commerce";
import { readListPage } from "@erpx/server-work_projection/public/list-page";
import {assertion} from "@erpx/server-workflow/public/command-assertions";
import {executeCommand,replayCommand} from "@erpx/server-workflow/public/commands";
import {createInvoice} from '@erpx/server-finance/public/finance';
import {withSalesCustomerReceiptRead,withSalesCustomerReceiptIssue} from '@erpx/server-finance/adapters/sales-customer-receipts';
import {salesDefaultsReferences,withSalesPostingCommand} from '@erpx/server-finance/adapters/sales-posting-setup';
import { withFinancePurchasedCommand } from '@erpx/server-finance/adapters/purchase-boundary';
import {standaloneSalesCompletionReceiptQuery} from '@erpx/server-finance/adapters/sales-completion-proof';
import {salesPostingDefaultsSchema,salesPostingDecisionSchema} from '@erpx/server-finance/domain/sales-posting-setup';
type Row = Record<string, string | number | null>;
async function database(actor: WorkContext, permission = "finance.read") { requirePermission(actor, permission); return (await holdingResources(actor.holdingId)).write; }
function companyAuthority(actor: WorkContext) { if (actor.branchId || actor.departmentId) throw new ApiError(403, "COMPANY_SCOPE_REQUIRED", "Use a Company Finance assignment for sales configuration."); }
const invoiceScope = `((?::text IS NULL AND ?::text IS NULL) OR EXISTS(SELECT 1 FROM (${commerceFinanceSalesSources.invoiceScope}) sales_scope WHERE sales_scope.invoice_id=i.id AND sales_scope.company_id=i.company_id AND (?::text IS NULL OR sales_scope.branch_id=?) AND (?::text IS NULL OR sales_scope.department_id=?)))`;
const scopeArgs = (actor: WorkContext) => [actor.branchId, actor.departmentId, actor.branchId, actor.branchId, actor.departmentId, actor.departmentId];
const receiptScope = `((?::text IS NULL AND ?::text IS NULL) OR (EXISTS(SELECT 1 FROM finance_allocations WHERE payment_id=p.id) AND NOT EXISTS(SELECT 1 FROM finance_allocations a JOIN finance_invoices i ON i.id=a.invoice_id WHERE a.payment_id=p.id AND NOT ${invoiceScope})))`;
const receiptScopeArgs = (actor: WorkContext) => [actor.branchId, actor.departmentId, ...scopeArgs(actor)];

export async function salesDefaultsWorkspace(actor:WorkContext){return salesDefaultsReferences(actor);}
async function validateDefaults(db: SqlDatabase, actor: WorkContext, input: z.infer<typeof salesDefaultsSchema>) {
  const ids = [input.receivableAccountId, input.revenueAccountId, ...(input.taxAccountId ? [input.taxAccountId] : [])];
  if (new Set(ids).size !== ids.length) throw new ApiError(422, "SALES_ACCOUNTS_INVALID", "Choose distinct receivable, revenue and VAT accounts.");
  const accounts = (await db.prepare(`SELECT a.*,c.status chart_status FROM finance_accounts a JOIN finance_charts c ON c.id=a.chart_id WHERE a.company_id=? AND a.chart_id=? AND a.currency=? AND a.id IN (${ids.map(() => "?").join(",")}) FOR SHARE OF a,c`).bind(actor.companyId, input.chartId, input.currency, ...ids).all<Row>()).results;
  const receivable = accounts.find(row => row.id === input.receivableAccountId), revenue = accounts.find(row => row.id === input.revenueAccountId), tax = accounts.find(row => row.id === input.taxAccountId);
  if (accounts.length !== ids.length || accounts.some(row => row.status !== "active" || row.chart_status !== "active") || receivable?.control_kind !== "receivable" || revenue?.category !== "revenue" || (input.taxAccountId && (tax?.category !== "liability" || tax?.control_kind !== "none")))
    throw new ApiError(422, "SALES_ACCOUNTS_INVALID", "Use active accounts on one Company ledger: trade receivables, revenue and output VAT.");
  return accounts;
}
async function prepareDefaults(tx:SqlDatabase,actor:WorkContext,input:z.infer<typeof salesDefaultsSchema>,key:string,request:unknown){
 const id=`sales-defaults-${key}`,previous=await replayCommand<{id:string;status:'draft';version:1}>(tx,actor,key,'finance.sales-defaults.prepare',id,request);if(previous)return previous;
 const accounts=await validateDefaults(tx,actor,input),now=Date.now();
 return executeCommand(tx,actor,key,'finance.sales-defaults.prepare',request,id,1,{id,status:'draft' as const,version:1 as const},[
  ...accounts.map((account,index)=>assertion(tx,key,`account-${index}`,'EXISTS(SELECT 1 FROM finance_accounts WHERE id=? AND company_id=? AND version=? AND status=\'active\')',[account.id,actor.companyId,account.version])),
  tx.prepare('INSERT INTO finance_sales_defaults(id,company_id,chart_id,currency,receivable_account_id,revenue_account_id,tax_account_id,reason,created_by,created_at,updated_at) VALUES(?,?,?,?,?,?,?,?,?,?,?)').bind(id,actor.companyId,input.chartId,input.currency,input.receivableAccountId,input.revenueAccountId,input.taxAccountId??null,input.reason,actor.userId,now,now),
 ]);
}
async function decideDefaults(tx:SqlDatabase,actor:WorkContext,id:string,input:z.infer<typeof salesDefaultsDecisionSchema>,key:string,request:unknown){
 const previous=await replayCommand<{id:string;status:'approved'|'rejected';version:number}>(tx,actor,key,'finance.sales-defaults.decide',id,request);if(previous)return previous;
 const row=await tx.prepare('SELECT * FROM finance_sales_defaults WHERE id=? AND company_id=? FOR UPDATE').bind(id,actor.companyId).first<SalesDefaults>();
 if(!row||row.status!=='draft'||Number(row.version)!==input.expectedVersion)throw new ApiError(409,'VERSION_CONFLICT','Refresh the sales defaults before review.');
 if(samePerson(actor,row.created_by))throw new ApiError(403,'INDEPENDENT_APPROVER_REQUIRED','A different Finance approver must review these defaults.');
 await validateDefaults(tx,actor,{chartId:row.chart_id,currency:row.currency,receivableAccountId:row.receivable_account_id,revenueAccountId:row.revenue_account_id,taxAccountId:row.tax_account_id??undefined,reason:row.reason});
 const status=input.action==='approve'?'approved' as const:'rejected' as const,now=Date.now();
 return executeCommand(tx,actor,key,'finance.sales-defaults.decide',request,id,input.expectedVersion+1,{id,status,version:input.expectedVersion+1},[
  ...(status==='approved'?[tx.prepare("UPDATE finance_sales_defaults SET status='superseded',version=version+1,updated_at=? WHERE company_id=? AND currency=? AND status='approved'").bind(now,actor.companyId,row.currency)]:[]),
  tx.prepare('UPDATE finance_sales_defaults SET status=?,reviewed_by=?,review_reason=?,version=version+1,updated_at=? WHERE id=?').bind(status,actor.userId,input.reason,now,id),
 ]);
}
export async function prepareSalesDefaults(actor:WorkContext,raw:z.input<typeof salesDefaultsSchema>,key:string){
 const input=salesDefaultsSchema.parse(raw);return withSalesPostingCommand(actor,'defaults',input,key,tx=>prepareDefaults(tx,actor,input,key,input),true,`sales-defaults-${key}`);
}
export async function decideSalesDefaults(actor:WorkContext,id:string,raw:z.input<typeof salesDefaultsDecisionSchema>,key:string){
 const input=salesDefaultsDecisionSchema.parse(raw);return withSalesPostingCommand(actor,'decision',input,key,tx=>decideDefaults(tx,actor,id,input,key,input),true,id);
}
export async function prepareSalesPostingDefaults(actor:WorkContext,raw:unknown,key:string){
 const request=salesPostingDefaultsSchema.parse(raw);return withSalesPostingCommand(actor,'defaults',request,key,tx=>prepareDefaults(tx,actor,request.input,key,request),false,`sales-defaults-${key}`);
}
export async function decideSalesPostingDefaults(actor:WorkContext,id:string,raw:unknown,key:string){
 const request=salesPostingDecisionSchema.parse(raw);return withSalesPostingCommand(actor,'decision',request,key,tx=>decideDefaults(tx,actor,id,request.input,key,request),false,id);
}

export async function salesWorkspace(actor: WorkContext, raw: z.input<typeof salesQuerySchema>) {
  const db = await database(actor), query = salesQuerySchema.parse(raw);
  const start = query.startDate ?? "", end = query.endDate ?? "", common = [actor.companyId, ...scopeArgs(actor), query.q, query.q, start, start, end, end];
  const invoiceWhere = `i.company_id=? AND i.kind='receivable' AND ${invoiceScope} AND (?='' OR strpos(lower(i.invoice_number||' '||party.name),lower(?))>0) AND (?='' OR i.invoice_date>=?) AND (?='' OR i.invoice_date<=?)`;
  const summary = await db.prepare(`SELECT i.currency,COALESCE(SUM(CASE WHEN i.status IN ('posted','partially_paid') THEN i.outstanding_minor ELSE 0 END),0) awaiting_minor,COALESCE(SUM(CASE WHEN i.status IN ('posted','partially_paid') AND i.due_date<(now() AT TIME ZONE 'Asia/Bangkok')::date::text THEN i.outstanding_minor ELSE 0 END),0) overdue_minor FROM finance_invoices i JOIN finance_parties party ON party.id=i.party_id WHERE ${invoiceWhere} GROUP BY i.currency ORDER BY i.currency`).bind(...common).all<Row>();
  const received = await db.prepare(`SELECT p.currency,COALESCE(SUM(p.amount_minor),0) received_minor FROM finance_payments p LEFT JOIN finance_customer_receipts r ON r.payment_id=p.id JOIN finance_parties party ON party.id=p.party_id JOIN finance_journals j ON j.id=p.journal_id WHERE p.company_id=? AND p.kind='receipt' AND p.status='settled' AND j.status='posted' AND ${receiptScope} AND (?='' OR strpos(lower(COALESCE(r.document_number,'')||' '||party.name||' '||p.payment_number),lower(?))>0 OR EXISTS(SELECT 1 FROM finance_allocations a JOIN finance_invoices related ON related.id=a.invoice_id WHERE a.payment_id=p.id AND strpos(lower(related.invoice_number),lower(?))>0)) AND (?='' OR p.payment_date>=?) AND (?='' OR p.payment_date<=?) GROUP BY p.currency ORDER BY p.currency`).bind(actor.companyId, ...receiptScopeArgs(actor), query.q, query.q, query.q, start, start, end, end).all<Row>();
  let page;
  if (query.tab === "quotations" || query.tab === "orders") {
    if (!hasPermission(actor, "commerce.read")) return { query, summary: summary.results, received: received.results, page: null, permissionRequired: "commerce.read" };
    page = await commerceSalesDocuments(actor, { view: query.tab === "quotations" ? "quote" : "sales", q: query.q, status: query.status, cursor: query.cursor, startDate: query.startDate, endDate: query.endDate });
  } else if (query.tab === "receipts") {
    page = await readListPage<Row & { id: string; page_key: string }>(db, { sql: `SELECT p.id, r.id receipt_id,COALESCE(r.document_number,p.payment_number) number,p.payment_date date,p.currency,p.amount_minor total_minor,party.name party_name,CASE WHEN p.status IN ('cancelled','reversed','voided') THEN 'cancelled' WHEN r.id IS NULL THEN 'ready' ELSE 'issued' END status,p.created_at::text page_key FROM finance_payments p LEFT JOIN finance_customer_receipts r ON r.payment_id=p.id JOIN finance_parties party ON party.id=p.party_id JOIN finance_journals j ON j.id=p.journal_id WHERE p.company_id=? AND p.kind='receipt' AND (p.status='settled' AND j.status='posted' OR r.id IS NOT NULL) AND ${receiptScope} AND (?='' OR strpos(lower(COALESCE(r.document_number,'')||' '||party.name||' '||p.payment_number),lower(?))>0 OR EXISTS(SELECT 1 FROM finance_allocations a JOIN finance_invoices related ON related.id=a.invoice_id WHERE a.payment_id=p.id AND strpos(lower(related.invoice_number),lower(?))>0)) AND (?='' OR p.payment_date>=?) AND (?='' OR p.payment_date<=?) AND (?='' OR CASE WHEN p.status IN ('cancelled','reversed','voided') THEN 'cancelled' WHEN r.id IS NULL THEN 'ready' ELSE 'issued' END=?) AND (?='' OR p.currency=?) AND (?!='received' OR p.status='settled' AND j.status='posted')`, args: [actor.companyId, ...receiptScopeArgs(actor), query.q, query.q, query.q, start, start, end, end, query.status, query.status, query.currency ?? "", query.currency ?? "", query.queue ?? ""], cursor: query.cursor, descending: true, scope: [actor.companyId, actor.branchId, actor.departmentId, query.tab, query.q, query.status, start, end, query.currency, query.queue] });
  } else {
    page = await readListPage<Row & { id: string; page_key: string }>(db, { sql: `SELECT i.*,i.invoice_number number,i.invoice_date date,party.name party_name,i.created_at::text page_key FROM finance_invoices i JOIN finance_parties party ON party.id=i.party_id WHERE ${invoiceWhere} AND (?='' OR i.status=?) AND (?='' OR i.currency=?) AND (? NOT IN ('awaiting','overdue') OR i.status IN ('posted','partially_paid') AND i.outstanding_minor>0) AND (?!='overdue' OR i.due_date<(now() AT TIME ZONE 'Asia/Bangkok')::date::text)`, args: [...common, query.status, query.status, query.currency ?? "", query.currency ?? "", query.queue ?? "", query.queue ?? ""], cursor: query.cursor, descending: true, scope: [actor.companyId, actor.branchId, actor.departmentId, query.tab, query.q, query.status, start, end, query.currency, query.queue] });
  }
  return { query, summary: summary.results, received: received.results, page, permissionRequired: null };
}

/** Called only inside the authorized settlement owner's PostgreSQL transaction. */
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
export async function issueCustomerReceipt(actor:WorkContext,paymentId:string,key:string){
 requirePermission(actor,'finance.write');companyAuthority(actor);
 return withSalesCustomerReceiptIssue(actor,paymentId,key,async tx=>{
  const input={paymentId},previous=await replayCommand(tx,actor,key,'finance.customer-receipt.issue',paymentId,input);if(previous)return previous;
  const receipt=await ensureCustomerReceipt(tx,actor,paymentId);if(!receipt)throw new ApiError(409,'POSTED_RECEIPT_REQUIRED','A settled customer receipt with a posted journal is required.');
  return executeCommand(tx,actor,key,'finance.customer-receipt.issue',input,paymentId,1,receipt,[]);
 });
}
async function receiptDetail(tx:SqlDatabase,actor:WorkContext,id:string){
 const row=await tx.prepare(`SELECT r.*,p.status payment_status,p.version payment_version FROM finance_customer_receipts r JOIN finance_payments p ON p.id=r.payment_id WHERE r.id=? AND r.company_id=? AND ${receiptScope}`).bind(id,actor.companyId,...receiptScopeArgs(actor)).first<Row>();
 if(!row)throw new ApiError(404,'RECEIPT_NOT_FOUND','Receipt not found in this workspace.');
 return {sha256:String(row.export_sha256),receipt:{...row,export_html:undefined},snapshot:JSON.parse(String(row.snapshot_json)) as ReceiptSnapshot,status:['cancelled','reversed','voided'].includes(String(row.payment_status))?'cancelled':'issued'};
}
export async function customerReceiptDetail(actor:WorkContext,id:string){return withSalesCustomerReceiptRead(actor,id,tx=>receiptDetail(tx,actor,id));}
export async function customerReceiptExport(actor:WorkContext,id:string){return withSalesCustomerReceiptRead(actor,id,async tx=>{
 await receiptDetail(tx,actor,id);return (await tx.prepare('SELECT export_html content,export_sha256 sha256,document_number FROM finance_customer_receipts WHERE id=? AND company_id=?').bind(id,actor.companyId).first<{content:string;sha256:string;document_number:string}>())!;
});}
export async function createSalesInvoiceDraft(actor: WorkContext, raw: z.input<typeof salesInvoiceDraftSchema>, key: string) {
  const db = await database(actor, "finance.write"); companyAuthority(actor); const input = salesInvoiceDraftSchema.parse(raw);
  const execute = async (tx: SqlDatabase) => {
    const previous = await replayCommand(tx, actor, key, "finance.sales-invoice.prepare", `invoice-${key}-invoice`, input); if (previous) return previous;
    const defaults = await tx.prepare("SELECT * FROM finance_sales_defaults WHERE company_id=? AND currency=? AND status='approved' FOR SHARE").bind(actor.companyId, input.currency).first<SalesDefaults>();
    if (!defaults) throw new ApiError(409, "SALES_DEFAULTS_REQUIRED", "Ask Finance to approve sales defaults for this currency.");
    await validateDefaults(tx, actor, { chartId: defaults.chart_id, currency: defaults.currency, receivableAccountId: defaults.receivable_account_id, revenueAccountId: defaults.revenue_account_id, taxAccountId: defaults.tax_account_id ?? undefined, reason: defaults.reason });
    const lines = input.lines.map(line => ({ ...line, accountId: defaults.revenue_account_id, taxAccountId: line.taxBps ? defaults.tax_account_id ?? undefined : undefined })), totals = calculateInvoiceLines(lines);
    const number = await tx.prepare("SELECT erpx_sales_document_number(?,'INV',?) number").bind(actor.companyId, input.invoiceDate).first<string>("number");
    const result = await createInvoice(actor, { kind: "receivable", partyId: input.partyId, invoiceNumber: number!, invoiceDate: input.invoiceDate, dueDate: input.dueDate, currency: input.currency, debitAccountId: defaults.receivable_account_id, creditAccountId: defaults.revenue_account_id, taxAccountId: defaults.tax_account_id ?? undefined, subtotal: totals.subtotal, tax: totals.tax, lines }, `${key}-invoice`, undefined, tx);
    return executeCommand(tx, actor, key, "finance.sales-invoice.prepare", input, result.id, 1, { ...result, number, total: minorToDecimal(totals.totalMinor), completionEvidence: { note: input.completionNote, confirmedBy: actor.userId, confirmedAt: Date.now(), invoiceRevision: 1 } }, []);
  };
  return withFinancePurchasedCommand(actor, { key, operation: "finance.sales-invoice.prepare", purpose: "new-work", capability: "finance.accounting", alternativeCapabilities: ["finance.sales-posting"] }, tx => tx ? execute(tx) : financeTransaction(db, execute));
}

export async function invoiceSalesCompletion(actor: WorkContext, invoiceId: string) {
  const db = await database(actor);
  const receipt = await db.prepare(`SELECT sales_completion.result_json FROM finance_invoices i
    CROSS JOIN LATERAL (${standaloneSalesCompletionReceiptQuery("i")}) sales_completion
    WHERE i.id=? AND i.company_id=? AND ${invoiceScope}
      AND i.kind='receivable' AND i.source_type='standalone' AND i.source_id IS NULL`).bind(invoiceId, actor.companyId, ...scopeArgs(actor)).first<{result_json:string}>();
  return receipt ? (JSON.parse(receipt.result_json).completionEvidence ?? null) as { note: string; confirmedBy: string; confirmedAt: number; invoiceRevision: number } | null : null;
}

export async function invoiceCustomerReceipts(actor: WorkContext, invoiceId: string) {
  const db = await database(actor);
  return (await db.prepare(`SELECT r.id,r.document_number,r.currency,a.amount_minor,p.status payment_status FROM finance_customer_receipts r JOIN finance_payments p ON p.id=r.payment_id JOIN finance_allocations a ON a.payment_id=p.id JOIN finance_invoices i ON i.id=a.invoice_id WHERE i.id=? AND i.company_id=? AND r.company_id=i.company_id AND ${invoiceScope} ORDER BY r.issued_at,r.id`).bind(invoiceId, actor.companyId, ...scopeArgs(actor)).all<Row>()).results;
}
