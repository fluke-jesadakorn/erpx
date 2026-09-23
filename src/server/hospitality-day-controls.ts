import 'server-only';
import type { SqlDatabase } from './runtime/types';

type Count={n:number};
type Totals={direct_total:number;room_total:number;cash_received:number;transfer_verified:number};
async function count(db:SqlDatabase,sql:string,...args:unknown[]) {
  const row=await db.prepare(sql).bind(...args).first<Count>();
  return Number(row?.n??0);
}

/** Same scoped, read-only controls are used by the Night Audit UI and start command. */
export async function hospitalityDayControls(db:SqlDatabase,companyId:string,branchId:string,propertyId:string,businessDate:string) {
  const scope=[companyId,branchId];
  // A Night Audit command supplies a single transaction-bound connection; keep
  // these reads sequential so node-postgres never queues concurrent queries on it.
  const openShifts=await count(db,"SELECT count(*) n FROM pos_shifts WHERE company_id=? AND branch_id=? AND state='open'",...scope);
  const unreviewedVariances=await count(db,"SELECT count(*) n FROM pos_shifts WHERE company_id=? AND branch_id=? AND state='closed' AND COALESCE(variance_minor,0)<>0",...scope);
  const openBills=await count(db,"SELECT count(*) n FROM pos_sales WHERE company_id=? AND branch_id=? AND business_date=? AND state IN ('open','held','payment_pending')",...scope,businessDate);
  const pendingPayments=await count(db,"SELECT count(*) n FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND p.state IN ('requested','pending','exception')",...scope,businessDate);
  const pendingHotelTransfers=await count(db,"SELECT count(*) n FROM pms_transfer_requests WHERE company_id=? AND property_id=? AND business_date=? AND state='pending'",companyId,propertyId,businessDate);
  const unresolvedRefunds=await count(db,"SELECT count(*) n FROM pos_refunds WHERE company_id=? AND branch_id=? AND accounting_date=? AND state IN ('requested','approved','processing','exception')",...scope,businessDate);
  const failedJobs=await count(db,"SELECT count(*) n FROM pos_jobs WHERE company_id=? AND state='exception' AND kind NOT IN ('connect','connect_refresh')",companyId);
  const missingRoomCharges=await count(db,`SELECT count(*) n FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.state='completed' AND s.invoice_id IS NULL
      AND NOT EXISTS(SELECT 1 FROM pos_room_charge_reversals rr WHERE rr.sale_id=s.id)
      AND NOT EXISTS(SELECT 1 FROM pms_sources ps JOIN pms_postings pp ON pp.property_id=ps.property_id AND pp.record_id=ps.record_id
        JOIN pms_records e ON e.id=ps.record_id AND e.property_id=ps.property_id AND e.kind='entry'
        WHERE ps.property_id=? AND ps.source_key='pos:'||s.id||':'||s.id AND (e.data_json::jsonb->>'total')::bigint=s.total_minor)`,...scope,businessDate,propertyId);
  const missingDirectInvoices=await count(db,`SELECT count(*) n FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.state='completed'
      AND s.invoice_id IS NOT NULL AND (NOT EXISTS(SELECT 1 FROM finance_invoices i WHERE i.id=s.invoice_id AND i.company_id=s.company_id AND i.source_type='pos' AND i.source_id=s.id AND i.total_minor=s.total_minor AND i.tax_minor=s.tax_minor AND i.currency=s.currency AND i.status='paid')
        OR NOT EXISTS(SELECT 1 FROM pos_postings pp WHERE pp.company_id=s.company_id AND pp.source_key='sale:'||s.id))`,...scope,businessDate);
  const unmatchedVerifiedTransfers=await count(db,`SELECT count(*) n FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id
    WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND p.method='transfer' AND p.state='succeeded'
      AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t WHERE t.company_id=s.company_id AND t.matched_type='payment'
        AND t.matched_id=p.finance_payment_id AND t.match_status='matched' AND t.external_id=p.reference
        AND t.amount_minor=p.amount_minor AND t.currency=s.currency)`,...scope,businessDate);
  const unmatchedVerifiedHotelTransfers=await count(db,`SELECT count(*) n FROM pms_transfer_requests r WHERE r.company_id=? AND r.property_id=? AND r.business_date=? AND r.state='verified'
    AND NOT EXISTS(SELECT 1 FROM finance_bank_transactions t JOIN finance_bank_statements b ON b.id=t.statement_id
      WHERE t.id=r.bank_transaction_id AND t.company_id=r.company_id AND t.match_status='matched' AND t.matched_type='payment'
        AND t.matched_id=r.finance_payment_id AND t.external_id=r.reference AND t.amount_minor=r.amount_minor AND t.currency=r.currency
        AND b.bank_account=(SELECT p.config_json::jsonb->'accounts'->>'bank' FROM pms_properties p WHERE p.id=r.property_id))`,companyId,propertyId,businessDate);
  const totals=await db.prepare(`SELECT
    COALESCE(SUM(CASE WHEN s.invoice_id IS NOT NULL THEN s.total_minor ELSE 0 END),0) direct_total,
    COALESCE(SUM(CASE WHEN s.invoice_id IS NULL THEN s.total_minor ELSE 0 END),0) room_total,
    COALESCE(SUM((SELECT SUM(p.amount_minor) FROM pos_payments p WHERE p.sale_id=s.id AND p.method='cash' AND p.state='succeeded')),0) cash_received,
    COALESCE(SUM((SELECT SUM(p.amount_minor) FROM pos_payments p WHERE p.sale_id=s.id AND p.method='transfer' AND p.state='succeeded')),0) transfer_verified
    FROM pos_sales s WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND s.state='completed'
      AND NOT EXISTS(SELECT 1 FROM pos_room_charge_reversals rr WHERE rr.sale_id=s.id)`)
    .bind(...scope,businessDate).first<Totals>();
  const stockMoves=await db.prepare(`SELECT COALESCE(SUM(m.value_minor),0) n FROM pos_stock_moves m JOIN commerce_warehouses w ON w.id=m.warehouse_id
    WHERE m.company_id=? AND w.branch_id=? AND m.movement_date=?`).bind(...scope,businessDate).first<Count>();
  const stockLedger=await db.prepare(`SELECT COALESCE(SUM(l.debit_minor-l.credit_minor),0) n FROM finance_journal_lines l
    JOIN finance_journals j ON j.id=l.journal_id
    WHERE j.company_id=? AND j.entry_date=? AND j.source_type='pos' AND j.status='posted'
      AND l.dimension_json::jsonb->>'branchId'=?
      AND l.account_id IN (SELECT DISTINCT s.accounts_json::jsonb->>'inventory' FROM pos_sales s WHERE s.company_id=j.company_id AND s.branch_id=?)`)
    .bind(companyId,businessDate,branchId,branchId).first<Count>();
  const stockValueMinor=Number(stockMoves?.n??0),stockLedgerMinor=Number(stockLedger?.n??0);
  const unbalancedJournals=await count(db,`SELECT count(*) n FROM (
      SELECT j.id FROM finance_journals j JOIN finance_journal_lines l ON l.journal_id=j.id
      WHERE j.company_id=? AND j.entry_date=? AND j.status='posted'
        AND ((j.source_type='pos' AND EXISTS(SELECT 1 FROM finance_journal_lines scoped WHERE scoped.journal_id=j.id AND scoped.dimension_json::jsonb->>'branchId'=?))
          OR EXISTS(SELECT 1 FROM pms_postings pp WHERE pp.journal_id=j.id AND pp.property_id=?))
      GROUP BY j.id HAVING SUM(l.debit_minor)<>SUM(l.credit_minor)
    ) unbalanced`,companyId,businessDate,branchId,propertyId);
  const blockers=[
    openShifts?`${openShifts} restaurant cash session(s) still open`:null,
    unreviewedVariances?`${unreviewedVariances} restaurant cash variance(s) awaiting independent review`:null,
    openBills?`${openBills} restaurant bill(s) still open for this business date`:null,
    pendingPayments?`${pendingPayments} restaurant payment(s) awaiting a result or bank verification`:null,
    pendingHotelTransfers?`${pendingHotelTransfers} hotel bank transfer(s) awaiting Finance verification`:null,
    unresolvedRefunds?`${unresolvedRefunds} restaurant refund(s) awaiting resolution`:null,
    failedJobs?`${failedJobs} restaurant background job(s) need recovery`:null,
    missingRoomCharges?`${missingRoomCharges} room charge(s) lack a hotel revenue posting`:null,
    missingDirectInvoices?`${missingDirectInvoices} direct restaurant sale(s) lack an invoice or revenue posting`:null,
    unmatchedVerifiedTransfers?`${unmatchedVerifiedTransfers} verified bank transfer(s) lack matching imported bank evidence`:null,
    unmatchedVerifiedHotelTransfers?`${unmatchedVerifiedHotelTransfers} verified hotel transfer(s) lack matching imported bank evidence`:null,
    stockValueMinor!==stockLedgerMinor?'Restaurant stock movements do not match their inventory ledger posting':null,
    unbalancedJournals?`${unbalancedJournals} restaurant or hotel journal(s) are unbalanced`:null,
  ].filter((value):value is string=>Boolean(value));
  return {businessDate,blockers,openShifts,unreviewedVariances,openBills,pendingPayments,pendingHotelTransfers,unresolvedRefunds,failedJobs,missingRoomCharges,missingDirectInvoices,unmatchedVerifiedTransfers,unmatchedVerifiedHotelTransfers,unbalancedJournals,
    directSalesMinor:Number(totals?.direct_total??0),roomChargesMinor:Number(totals?.room_total??0),cashReceivedMinor:Number(totals?.cash_received??0),verifiedTransferMinor:Number(totals?.transfer_verified??0),stockValueMinor,stockLedgerMinor};
}
