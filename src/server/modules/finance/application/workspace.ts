import type {WorkContext} from '@/server/modules/identity_access/public/authorization';
import type {FinanceError} from '../domain/invoice-line-revision';
import type {FinanceAttention,PaymentObligation,PaymentObligationFilters,CountRow,ClosePeriodSource,ClosePeriodRow} from '../domain/workspace';
import type {CloseReconciliation} from '../domain/finance';
import type {VatCloseEvidence} from '../domain/vat-register';
type Row=Record<string,string|number|null>;
export interface FinanceWorkspacePorts<Page extends object,Consolidation extends object,Intercompany extends {statusCounts:unknown}>{error:FinanceError;permission(actor:WorkContext,permission:'finance.read'):WorkContext;hasPermission(actor:WorkContext,permission:string):boolean;ready(actor:WorkContext):Promise<void>;today():string;businessDate():string;paymentRecords(actor:WorkContext,query:{search?:string;status?:string;cursor?:string;limit:number}):Promise<Page>;workbenchCounters(actor:WorkContext,today:string):Promise<CountRow|null>;firstCurrency(actor:WorkContext):Promise<{base_currency:string}|null>;dueAmounts(actor:WorkContext,today:string):Promise<{kind:string;currency:string;amount_minor:number}[]>;genericPayments(actor:WorkContext):Promise<Row[]>;supplierBatches(actor:WorkContext):Promise<Row[]>;refunds(actor:WorkContext):Promise<Row[]>;peoplePayments(actor:WorkContext):Promise<Row[]>;periods(actor:WorkContext):Promise<ClosePeriodSource[]>;reconciliation(actor:WorkContext,period:ClosePeriodSource):Promise<{ready:boolean;rows:CloseReconciliation[];vat:VatCloseEvidence|null}>;pendingTax(actor:WorkContext,endDate:string):Promise<string|number|null>;expansionPending(actor:WorkContext,startDate:string,endDate:string):Promise<unknown[]>;consolidation(actor:WorkContext):Promise<Consolidation>;intercompany(actor:WorkContext):Promise<Intercompany>;charts(actor:WorkContext):Promise<Row[]>;accountCount(actor:WorkContext):Promise<{count:number}|null>;periodCount(actor:WorkContext):Promise<{count:number}|null>;taxCount(actor:WorkContext):Promise<{count:number}|null>;bankCount(actor:WorkContext):Promise<{count:number}|null>;parties(actor:WorkContext):Promise<Row[]>;setupAuthority(actor:WorkContext):Promise<{independent:boolean;preparerCount:number;approverCount:number}>;profileCount(actor:WorkContext,businessDate:string):Promise<{count:number}|null>;controlCount(actor:WorkContext):Promise<{count:number}|null>;}
export function createFinanceWorkspace<Page extends object,Consolidation extends object,Intercompany extends {statusCounts:unknown}>(ports:FinanceWorkspacePorts<Page,Consolidation,Intercompany>){
async function paymentRecordsWorkspace(actor:WorkContext,query:{q?:string;paymentStatus?:string;paymentCursor?:string},limit=30){actor=ports.permission(actor,'finance.read');await ports.ready(actor);return ports.paymentRecords(actor,{search:query.q?.trim().slice(0,180),status:query.paymentStatus,cursor:query.paymentCursor,limit});}
async function financeWorkbench(actor: WorkContext, asOf?: string) {
  actor = ports.permission(actor,"finance.read");
  await ports.ready(actor);
  const today = asOf ?? ports.today();
  const row = await ports.workbenchCounters(actor,today);
  const chart = await ports.firstCurrency(actor);
  const currency = chart?.base_currency ?? null;
  const dueAmounts=await ports.dueAmounts(actor,today);
  const attention: FinanceAttention[] = [
    { id: "invoice-review", label: "Documents to review", description: "Review the party, dates, tax, and accounts before approval", count: Number(row?.draft_invoices ?? 0), amountMinor: null, currency, href: "finance/invoices?status=draft", tone: "attention" },
    { id: "journal-review", label: "Journals to post", description: "Journals prepared for independent approval", count: Number(row?.draft_journals ?? 0), amountMinor: null, currency, href: "finance/accounting?status=draft", tone: "attention" },
    { id: "receivable-overdue", label: "Overdue receivables", description: "Follow up collection and record the contact evidence", count: Number(row?.overdue_receivables ?? 0), amountMinor: Number(row?.overdue_receivable_minor ?? 0), currency, href: "finance/invoices?kind=receivable&queue=open", tone: "urgent" },
    { id: "payable-due", label: "Due payables", description: "Schedule payment for approved obligations", count: Number(row?.due_payables ?? 0), amountMinor: Number(row?.due_payable_minor ?? 0), currency, href: "finance/invoices?kind=payable&queue=open", tone: "urgent" },
    { id: "payment-review", label: "Payment obligations", description: "Review approval, export, and bank confirmation across every source", count: Number(row?.pending_payments ?? 0), amountMinor: null, currency, href: "finance/payments", tone: "attention" },
    { id: "bank-unmatched", label: "Unreconciled bank items", description: "Match bank evidence to receipts and payments", count: Number(row?.unmatched_bank ?? 0), amountMinor: null, currency, href: "finance/bank", tone: "attention" },
    { id: "ai-review", label: "AI proposals to review", description: "Open evidence, compare the proposal, and accept or correct it", count: Number(row?.ready_ai_proposals ?? 0), amountMinor: null, currency, href: "expenses", tone: "normal" },
    { id: "period-close", label: "Period-close work", description: "Resolve blockers and collect close evidence", count: Number(row?.closing_periods ?? 0) + Number(row?.expired_open_periods ?? 0), amountMinor: null, currency, href: "finance/close", tone: "urgent" },
  ];
  const visible = attention.filter((item) => {
    if (item.id === "payment-review") return ports.hasPermission(actor, "finance.pay");
    if (["journal-review", "period-close"].includes(item.id)) return ports.hasPermission(actor, "finance.write");
    return true;
  });
  for(const item of visible){
    const kind=item.id==="receivable-overdue"?"receivable":item.id==="payable-due"?"payable":null;
    if(!kind)continue;
    item.amounts=dueAmounts.filter(row=>row.kind===kind).map(row=>({currency:row.currency,amountMinor:Number(row.amount_minor)}));
    // A cross-currency total has no valid currency label. Keep the compatible
    // scalar only when all obligations use the same transaction currency.
    item.currency=item.amounts.length===1?item.amounts[0].currency:null;
    item.amountMinor=item.amounts.length===1?item.amounts[0].amountMinor:null;
  }
  return {
    role: actor.role,
    currency,
    attention: visible,
    totalAttention: visible.reduce((sum, item) => sum + item.count, 0),
    canPrepare: ports.hasPermission(actor, "finance.write"),
    canApprove: ports.hasPermission(actor, "finance.approve"),
    canPay: ports.hasPermission(actor, "finance.pay"),
  };
}

async function paymentObligationsWorkspace(actor: WorkContext, filters: PaymentObligationFilters = {}): Promise<PaymentObligation[]> {
  actor = ports.permission(actor,"finance.read");
  await ports.ready(actor);
  const canReadPeoplePayments = ports.hasPermission(actor, "payroll.payment.read") || ports.hasPermission(actor, "finance.pay");
  const [generic, supplier, refunds, people] = await Promise.all([
    ports.genericPayments(actor),
    ports.supplierBatches(actor),
    ports.refunds(actor),
    canReadPeoplePayments
      ? ports.peoplePayments(actor)
      : Promise.resolve([] as Record<string,string|number|null>[]),
  ]);
  const nextAction = (status: string, sourceType: string) => {
    if (sourceType === "generic_payment") return status === "draft" || status === "in_review" ? "Submit for independent approval" : status === "approved" ? "Release through Treasury" : "Match bank evidence";
    if (sourceType === "supplier_batch") return status === "draft" ? "Review beneficiaries and approve" : status === "approved" ? "Export through Treasury" : "Confirm each bank result";
    if (sourceType === "refund") return status === "pending" ? "Review and approve refund" : "Prepare or confirm refund instruction";
    return status === "draft" ? "Approve payroll or expense payment" : status === "approved" ? "Export payment instruction" : "Confirm each bank result";
  };
  const rows: PaymentObligation[] = [
    ...generic.map(row => ({ id: String(row.id), sourceType: "generic_payment", sourceLabel: String(row.kind) === "receipt" ? "Customer receipt payment" : "Supplier payment", sourceId: String(row.source_id), dueDate: row.due_date ? String(row.due_date) : null, currency: String(row.currency), amountMinor: Number(row.amount_minor), status: String(row.status), nextAction: nextAction(String(row.status), "generic_payment"), href: `finance/payments/${row.id}` })),
    ...supplier.map(row => ({ id: String(row.id), sourceType: "supplier_batch", sourceLabel: "Supplier payment batch", sourceId: String(row.id), dueDate: row.due_date ? String(row.due_date) : null, currency: String(row.currency), amountMinor: Number(row.amount_minor), status: String(row.status), nextAction: nextAction(String(row.status), "supplier_batch"), href: `finance/supplier-batches/${row.id}` })),
    ...refunds.map(row => ({ id: String(row.id), sourceType: "refund", sourceLabel: String(row.direction) === "outgoing" ? "Customer refund" : "Supplier refund", sourceId: String(row.return_id), dueDate: row.due_date ? String(row.due_date) : null, currency: String(row.currency), amountMinor: Number(row.amount_minor), status: String(row.status), nextAction: nextAction(String(row.status), "refund"), href: `finance/refunds/${row.id}` })),
    ...people.map(row => ({ id: String(row.id), sourceType: "people_payment", sourceLabel: String(row.source_type) === "payroll" ? "Payroll payment batch" : String(row.source_type) === "expense" ? "Expense payment batch" : "Contractor payment batch", sourceId: String(row.source_id), dueDate: row.due_date ? String(row.due_date) : null, currency: String(row.currency), amountMinor: Number(row.amount_minor), status: String(row.status), nextAction: nextAction(String(row.status), "people_payment"), href: String(row.source_type) === "payroll" ? `people/payroll/runs/${encodeURIComponent(String(row.source_id))}` : String(row.source_type) === "expense" ? `expenses/${row.source_id}` : `finance/invoices/${row.source_id}` })),
  ];
  return rows
    .filter(row => !filters.sourceType || row.sourceType === filters.sourceType)
    .filter(row => !filters.status || row.status === filters.status)
    .sort((left, right) => `${left.dueDate ?? "9999-99-99"}:${left.id}`.localeCompare(`${right.dueDate ?? "9999-99-99"}:${right.id}`));
}

async function closeWorkspace(actor: WorkContext) {
  actor = ports.permission(actor,"finance.read");
  await ports.ready(actor);
  const periods = await ports.periods(actor);
  return Promise.all(periods.map(async(period): Promise<ClosePeriodRow> => {
    const reconciliation=await ports.reconciliation(actor,period);
    const pendingTax=Number(await ports.pendingTax(actor,period.end_date)??0),expansionPending=await ports.expansionPending(actor,String(period.start_date),String(period.end_date));
    return {
      ...period,
      pending_tax:pendingTax,
      pending_source_tasks:expansionPending.length,
      blockers: Number(period.draft_journals) + Number(period.unmatched_bank) + Number(period.draft_invoices) + Number(period.open_manufacturing) + Number(period.pending_calculations) + pendingTax + expansionPending.length + (reconciliation.ready?0:1),
      reconciliation_ready: reconciliation.ready?1:0,
      reconciliations: reconciliation.rows,
      vat_reconciliation: reconciliation.vat,
    };
  }));
}

async function groupFinanceWorkspace(actor: WorkContext) {
  actor = ports.permission(actor,"finance.read");
  if (actor.scope.kind !== "holding") throw ports.error(403, "HOLDING_FINANCE_REQUIRED", "Group Finance is available only in a Holding workspace.");
  const [consolidation, intercompany] = await Promise.all([ports.consolidation(actor), ports.intercompany(actor)]);
  return { ...consolidation, ...intercompany, intercompanyStatus: intercompany.statusCounts };
}

async function financeSetupWorkspace(actor: WorkContext) {
  actor = ports.permission(actor,"finance.read");
  await ports.ready(actor);
  const [charts, accounts, periods, taxes, bankStatements, parties,authority,profiles,controls] = await Promise.all([
    ports.charts(actor),
    ports.accountCount(actor),
    ports.periodCount(actor),
    ports.taxCount(actor),
    ports.bankCount(actor),
    ports.parties(actor),
    ports.setupAuthority(actor),
    ports.profileCount(actor,ports.businessDate()),
    ports.controlCount(actor),
  ]);
  return {
    charts: charts,
    parties: parties,
    checks: [
      { id: "chart", label: "Chart of accounts and base currency", complete: charts.length > 0, detail: `${charts.length} chart(s)` },
      { id: "accounts", label: "Posting accounts", complete: Number(accounts?.count ?? 0) > 0, detail: `${Number(accounts?.count ?? 0)} active account(s)` },
      {id:'controls',label:'Bank, receivable and payable controls',complete:Number(controls?.count??0)>0,detail:`${Number(controls?.count??0)} primary book(s) with all three control mappings`},
      {id:'profile',label:'Reviewed Company accounting policy',complete:Number(profiles?.count??0)>0,detail:`${Number(profiles?.count??0)} effective independently approved profile(s)`},
      {id:'approvers',label:'Independent Finance preparer and approver',complete:authority.independent,detail:`${authority.preparerCount} preparer(s), ${authority.approverCount} approver(s) with current Company authority`},
      { id: "periods", label: "Accounting periods", complete: Number(periods?.count ?? 0) > 0, detail: `${Number(periods?.count ?? 0)} period(s)` },
      { id: "tax", label: "Tax codes", complete: Number(taxes?.count ?? 0) > 0, detail: `${Number(taxes?.count ?? 0)} active code(s)` },
      { id: "bank", label: "Bank accounts", complete: Number(bankStatements?.count ?? 0) > 0, detail: `${Number(bankStatements?.count ?? 0)} account(s) from statements` },
    ],
  };
}
return {paymentRecordsWorkspace,financeWorkbench,paymentObligationsWorkspace,closeWorkspace,groupFinanceWorkspace,financeSetupWorkspace};}
