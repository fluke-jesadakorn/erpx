import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as pmsProjectionSources } from "../../pms/public/hospitality-owner-projection-sources";
import 'server-only';
import { createHash } from 'node:crypto';
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { all, one, run } from "@/server/modules/pos/public/pos-core";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import type { SqlStatement } from "../../runtime/public/runtime/types";
import { createPosDay, type PosDayUnit, type PosReceiptSnapshot } from "../application/pos-daily";
import { randomUUID } from "node:crypto";
import { ApiError } from "../../runtime/public/http";
import { holdingResources } from "../../runtime/public/runtime";
import { PosDayFinanceFacts } from "../../finance/public/pos-daily-facts";
import { PosDayOrganizationFacts } from "../../organization/public/pos-daily-facts";
import { requirePosPurchased, requirePosReportingPurchase } from "./purchase-boundary";
function unit(db: SqlDatabase): PosDayUnit<SqlStatement> {
    return {
        readPosSales1: (facts: {
            saleId: string;
        }) => one(db, 'SELECT * FROM pos_sales WHERE id=?', facts.saleId),
        readFinanceParties1: facts => PosDayFinanceFacts(db).readFinanceParties1(facts),
        readPosLines1: (facts: {
            saleId: string;
        }) => all(db, 'SELECT id,name,quantity_milli,price_minor,tax_bps,tax_inclusive,discount_minor,subtotal_minor,tax_minor,modifiers_json,note FROM pos_lines WHERE sale_id=? ORDER BY id', facts.saleId),
        readPosPayments1: (facts: {
            saleId: string;
        }) => all(db, "SELECT id,method,amount_minor,tendered_minor,reference,finance_payment_id FROM pos_payments WHERE sale_id=? AND state='succeeded' ORDER BY created_at,id", facts.saleId),
        readPmsSources1: (facts: {
            fact0: string;
        }) => one(db, `SELECT property_id,record_id guest_account_entry_id FROM (${pmsProjectionSources.pms_sources}) pms_sources WHERE source_key=?`, facts.fact0),
        writePosSaleDocuments1: (facts: {
            saleId: string;
            company_id: string | number | boolean | null;
            branch_id: string | number | boolean | null;
            fact3: string;
            fact4: number;
        }) => run(db, 'INSERT INTO pos_sale_documents(sale_id,company_id,branch_id,snapshot_json,created_at) VALUES(?,?,?,?::jsonb,?) ON CONFLICT(sale_id) DO NOTHING', facts.saleId, facts.company_id, facts.branch_id, facts.fact3, facts.fact4),
        readFinanceParties2: facts => PosDayFinanceFacts(db).readFinanceParties2(facts),
        readFinanceParties3: facts => PosDayFinanceFacts(db).readFinanceParties3(facts),
        readPosSettings1: (facts: {
            companyId: string;
        }) => one(db, 'SELECT currency FROM pos_settings WHERE company_id=?', facts.companyId),
        writeFinanceParties1: facts => PosDayFinanceFacts(db).writeFinanceParties1(facts),
        readPosDayCloses1: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => one(db, 'SELECT id FROM pos_day_closes WHERE company_id=? AND branch_id=? AND business_date=?', facts.company, facts.branch, facts.date),
        readOrganizationNodes1: facts => PosDayOrganizationFacts(db).readOrganizationNodes1(facts),
        readPosSales2: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => all(db, `SELECT s.*,ps.record_id room_charge_id,rr.reversal_entry_id FROM pos_sales s LEFT JOIN (${pmsProjectionSources.pms_sources}) ps ON ps.source_key='pos:'||s.id||':'||s.id LEFT JOIN pos_room_charge_reversals rr ON rr.sale_id=s.id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? ORDER BY s.id`, facts.company, facts.branch, facts.date),
        readPosShifts1: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => all(db, 'SELECT * FROM pos_shifts WHERE company_id=? AND branch_id=? AND business_date=? ORDER BY id', facts.company, facts.branch, facts.date),
        readPosPayments2: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => all(db, 'SELECT p.*,s.number sale_number FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? ORDER BY p.id', facts.company, facts.branch, facts.date),
        readPosRefunds1: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => all(db, 'SELECT r.*,s.number sale_number FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id WHERE r.company_id=? AND r.branch_id=? AND r.accounting_date=? ORDER BY r.id', facts.company, facts.branch, facts.date),
        readPosRefundPayments1: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => all(db, 'SELECT rp.*,p.method FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE r.company_id=? AND r.branch_id=? AND r.accounting_date=? ORDER BY rp.id', facts.company, facts.branch, facts.date),
        readPosStockMoves1: (facts: {
            company: string;
            date: string;
            company2: string;
            branch: string;
        }) => all(db, `SELECT * FROM pos_stock_moves WHERE company_id=? AND movement_date=? AND warehouse_id IN (SELECT id FROM (${commerceProjectionSources.commerce_warehouses}) commerce_warehouses WHERE company_id=? AND branch_id=?) ORDER BY id`, facts.company, facts.date, facts.company2, facts.branch),
        readFinanceJournals1: facts => PosDayFinanceFacts(db).readFinanceJournals1(facts),
        readFinanceJournalLines1: facts => PosDayFinanceFacts(db).readFinanceJournalLines1(facts),
        readFinanceJournalLines2: facts => PosDayFinanceFacts(db).readFinanceJournalLines2(facts),
        readPosSettings2: (facts: {
            company: string;
        }) => one(db, 'SELECT currency FROM pos_settings WHERE company_id=?', facts.company),
        readPosFulfillments1: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => all(db, "SELECT id,state FROM pos_fulfillments WHERE company_id=? AND branch_id=? AND business_date=? AND state IN ('queued','preparing','ready') ORDER BY id", facts.company, facts.branch, facts.date),
        readPosCashMovements1: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => all(db, 'SELECT m.id FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND s.branch_id=? AND s.business_date=? AND m.approved_by IS NULL ORDER BY m.id', facts.company, facts.branch, facts.date),
        readPosSettlements1: (facts: {
            company: string;
        }) => all(db, "SELECT id,kind,state,amount_minor,fee_minor,amount_minor-fee_minor net_minor FROM pos_settlements WHERE company_id=? AND ((kind='payout' AND state<>'matched') OR (kind='balance' AND state='unmapped')) ORDER BY id", facts.company),
        readFinanceJournalLines3: facts => PosDayFinanceFacts(db).readFinanceJournalLines3(facts),
        writePosDayCloses1: (facts: {
            close: `${string}-${string}-${string}-${string}-${string}`;
            companyId: string;
            branchId: string;
            businessDate: string;
            currency: string;
            fact5: string;
            digest: string;
            userId: string;
            fact8: number;
        }) => run(db, "INSERT INTO pos_day_closes(id,company_id,branch_id,business_date,currency,state,snapshot_json,source_digest,prepared_by,prepared_at) VALUES(?,?,?,?,?,'prepared',?::jsonb,?,?,?)", facts.close, facts.companyId, facts.branchId, facts.businessDate, facts.currency, facts.fact5, facts.digest, facts.userId, facts.fact8),
        readPosDayCloses2: (facts: {
            id: string;
        }) => one(db, 'SELECT * FROM pos_day_closes WHERE id=? FOR UPDATE', facts.id),
        writePosDayCloses2: (facts: {
            userId: string;
            fact1: number;
            id: string | number | boolean | null;
        }) => run(db, "UPDATE pos_day_closes SET state='confirmed',confirmed_by=?,confirmed_at=?,version=version+1 WHERE id=?", facts.userId, facts.fact1, facts.id), batch: effects => db.batch(effects)
    };
}
const application = createPosDay<SqlStatement>({ clock: { now: () => Date.now() }, uuid: randomUUID, hash: value => createHash("sha256").update(value).digest("hex"), environment: key => process.env[key], require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), forActor: async (actor) => unit((await holdingResources(actor.holdingId)).write) });
export function capturePosReceipt(db: SqlDatabase, saleId: string) { return application.capturePosReceipt(unit(db), saleId); }
export async function posCustomers(db: SqlDatabase, actor: WorkContext, query = '') { requirePermission(actor, "pos.read"); await requirePosPurchased(db, actor); return application.posCustomers(unit(db), actor, query); }
export function createPosCustomer(db: SqlDatabase, actor: WorkContext, input: unknown) { return application.createPosCustomer(unit(db), actor, input); }
export function assertPosDayWritable(db: SqlDatabase, company: string, branch: string, date: string) { return application.assertPosDayWritable(unit(db), company, branch, date); }
export async function posDayReport(db: SqlDatabase, actor: WorkContext, branch: string, date: string) { requirePermission(actor, "pos.reports"); await requirePosReportingPurchase(db, actor); return application.posDayReport(unit(db), actor, branch, date); }
export function posDayCommand(db: SqlDatabase, actor: WorkContext, op: string, input: unknown) { return application.posDayCommand(unit(db), actor, op, input); }
export type { PosReceiptSnapshot };
