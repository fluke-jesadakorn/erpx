import { roomChargeReversalWork } from "./room-charge-reversal-queue";
import { posGuestAccountInterface } from "../../pms/public/pos-guest-account-interface";
import { hospitalityOwnerProjectionSources as commerceProjectionSources } from "../../commerce/public/hospitality-owner-projection-sources";
import { hospitalityOwnerProjectionSources as pmsProjectionSources } from "../../pms/public/hospitality-owner-projection-sources";
import { readListPage } from "@/server/modules/work_projection/public/list-page";
import 'server-only';
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import { holdingResources } from "@/server/modules/runtime/public/runtime";
import { requirePermission } from "@/server/modules/identity_access/public/iam-permissions";
import { all, one } from "@/server/modules/pos/public/pos-core";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { ApiError } from "@/server/modules/runtime/public/http";
import { posCustomers } from "@/server/modules/pos/public/pos-daily";
import type { SqlStatement } from "../../runtime/public/runtime/types";
import { createPosRead, type PosReadUnit, type PosExecutiveEvidenceMetric } from "../application/pos";
import { createHash, randomUUID } from "node:crypto";
import { PosReadCommerceFacts } from "../../commerce/public/pos-facts";
import { PosReadInventoryFacts } from "../../inventory/public/pos-facts";
import { PosReadFinanceFacts } from "../../finance/public/pos-facts";
import { PosReadOrganizationFacts } from "../../organization/public/pos-facts";
function unit(db: SqlDatabase): PosReadUnit<SqlStatement> {
    return {
        hotelLinks: (company,branch) => posGuestAccountInterface(db).hotels(company,branch),
        readPosRegisters1: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
        }) => all(db, `SELECT r.*,w.name warehouse_name FROM pos_registers r JOIN (${commerceProjectionSources.commerce_warehouses}) w ON w.id=r.warehouse_id WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY r.name`, facts.company, facts.branch, facts.branch2),
        readPosSales1: (facts: {
            saleId: string;
        }) => one(db, 'SELECT * FROM pos_sales WHERE id=?', facts.saleId),
        readPosSettings1: (facts: {
            company: string;
        }) => one(db, 'SELECT * FROM pos_settings WHERE company_id=?', facts.company),
        readCommerceProducts1: facts => PosReadCommerceFacts(db).readCommerceProducts1(facts),
        readPosRegisters2: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
        }) => all(db, `SELECT r.id register_id,p.name hotel_name,p.state hotel_state,p.business_date,
   (p.config_json::jsonb->>'approvedBy' IS NOT NULL) finance_approved
   FROM pos_registers r LEFT JOIN (${pmsProjectionSources.pms_properties}) p ON p.id=r.property_id AND p.company_id=r.company_id AND p.branch_id=r.branch_id
   WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND r.active=true`, facts.company, facts.branch, facts.branch2),
        readCommerceSettings1: facts => PosReadCommerceFacts(db).readCommerceSettings1(facts),
        readCommerceSettings2: facts => PosReadCommerceFacts(db).readCommerceSettings2(facts),
        readPosRegisterCatalog1: (facts: {
            branch: string | null;
            branch2: string | null;
            company: string;
            fact3: boolean;
            branch3: string | null;
            branch4: string | null;
        }) => all(db, `SELECT p.*,c.barcode,c.category,COALESCE(c.kind,'stock') kind,COALESCE(c.station,'Kitchen') station,COALESCE(c.recipe_json,'[]') recipe_json,COALESCE(c.modifiers_json,'[]') modifiers_json,COALESCE(c.tax_inclusive,false) tax_inclusive,COALESCE(c.version,0) catalog_version,c.enabled,c.image_id,
   COALESCE((SELECT json_agg(rc.register_id ORDER BY rc.register_id)::text FROM pos_register_catalog rc JOIN pos_registers r ON r.id=rc.register_id WHERE rc.product_id=p.id AND rc.enabled=true AND r.active=true AND (?::text IS NULL OR r.branch_id=?)),'[]') register_ids_json
   FROM (${commerceProjectionSources.commerce_products}) p LEFT JOIN pos_catalog c ON c.product_id=p.id AND c.company_id=p.company_id WHERE p.company_id=? AND p.status='active'
   AND (?=true OR c.enabled=true AND EXISTS(SELECT 1 FROM pos_register_catalog rc JOIN pos_registers r ON r.id=rc.register_id WHERE rc.product_id=p.id AND rc.enabled=true AND r.active=true AND (?::text IS NULL OR r.branch_id=?))) ORDER BY p.name`, facts.branch, facts.branch2, facts.company, facts.fact3, facts.branch3, facts.branch4),
        readCommerceWarehouses1: facts => PosReadCommerceFacts(db).readCommerceWarehouses1(facts),
        readPosPayments1: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
        }) => all(db, `SELECT s.*,r.name register_name,
   s.opening_minor+COALESCE((SELECT SUM(p.amount_minor) FROM pos_payments p JOIN pos_sales b ON b.id=p.sale_id WHERE b.shift_id=s.id AND p.method='cash' AND p.state='succeeded'),0)
   -COALESCE((SELECT SUM(rp.amount_minor) FROM pos_refund_payments rp JOIN pos_refunds rf ON rf.id=rp.refund_id JOIN pos_payments p ON p.id=rp.payment_id WHERE rf.payout_shift_id=s.id AND p.method='cash' AND rp.state='succeeded'),0)
   +COALESCE((SELECT SUM(m.amount_minor) FROM pos_cash_movements m WHERE m.shift_id=s.id AND m.approved_by IS NOT NULL),0) cash_expected_minor
   FROM pos_shifts s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY s.opened_at DESC LIMIT 100`, facts.company, facts.branch, facts.branch2),
        readPosTables1: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
        }) => all(db, 'SELECT t.* FROM pos_tables t JOIN pos_registers r ON r.id=t.register_id WHERE r.company_id=? AND (?::text IS NULL OR r.branch_id=?) ORDER BY t.name', facts.company, facts.branch, facts.branch2),
        readPosSales2: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
            fact3: string;
            fact4: string;
            fact5: 0 | 1;
        }) => all(db, "SELECT s.*,r.name register_name FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?='' OR s.number ILIKE ?) AND (?=0 OR s.state IN ('open','held','payment_pending')) ORDER BY s.created_at DESC LIMIT 150", facts.company, facts.branch, facts.branch2, facts.fact3, facts.fact4, facts.fact5),
        readPosLines1: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
        }) => all(db, `SELECT f.*,p.name product_name,(SELECT l.modifiers_json FROM pos_lines l WHERE l.fulfillment_id=f.id LIMIT 1) modifiers_json,(SELECT string_agg(DISTINCT s.number, ', ') FROM pos_lines l JOIN pos_sales s ON s.id=l.sale_id WHERE l.fulfillment_id=f.id) sale_numbers FROM pos_fulfillments f JOIN (${commerceProjectionSources.commerce_products}) p ON p.id=f.product_id WHERE f.company_id=? AND (?::text IS NULL OR f.branch_id=?) AND f.kind='recipe' AND f.state NOT IN ('delivered','cancelled') ORDER BY f.created_at`, facts.company, facts.branch, facts.branch2),
        readPosRefunds1: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
            fact3: string | number | boolean | null | undefined;
            fact4: string | number | boolean | null | undefined;
        }) => all(db, 'SELECT f.*,s.number sale_number,s.currency FROM pos_refunds f JOIN pos_sales s ON s.id=f.sale_id WHERE f.company_id=? AND (?::text IS NULL OR f.branch_id=?) AND (?::text IS NULL OR f.sale_id=?) ORDER BY f.created_at DESC LIMIT 100', facts.company, facts.branch, facts.branch2, facts.fact3, facts.fact4),
        readPosCashMovements1: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
        }) => all(db, 'SELECT m.*,s.register_id FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) ORDER BY m.created_at DESC LIMIT 100', facts.company, facts.branch, facts.branch2),
        readInventoryActiveReservations1: facts => PosReadInventoryFacts(db).readInventoryActiveReservations1(facts),
        readFinanceAccounts1: facts => PosReadFinanceFacts(db).readFinanceAccounts1(facts),
        readPosLines2: (facts: {
            id: string | number | boolean | null;
        }) => all(db, 'SELECT l.*,f.state fulfillment_state,f.version fulfillment_version,f.kind,f.cost_minor fulfillment_cost FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE l.sale_id=? ORDER BY l.id', facts.id),
        readPosPayments2: (facts: {
            id: string | number | boolean | null;
        }) => all(db, 'SELECT * FROM pos_payments WHERE sale_id=? ORDER BY created_at', facts.id),
        readFinanceBankTransactions1: facts => PosReadFinanceFacts(db).readFinanceBankTransactions1(facts),
        readPmsSources1: (facts: {
            id: string | number | boolean | null;
            fact1: string;
            company: string;
            branch_id: string | number | boolean | null;
        }) => one(db, `SELECT ps.property_id,ps.record_id guest_account_entry_id,p.version property_version,r.reversal_entry_id,r.reason reversal_reason,r.reversed_at FROM (${pmsProjectionSources.pms_sources}) ps JOIN (${pmsProjectionSources.pms_properties}) p ON p.id=ps.property_id LEFT JOIN pos_room_charge_reversals r ON r.sale_id=? WHERE ps.source_key=? AND p.company_id=? AND p.branch_id=?`, facts.id, facts.fact1, facts.company, facts.branch_id),
        readPosConnections1: (facts: {
            company: string;
        }) => one(db, "SELECT * FROM pos_connections WHERE company_id=? AND state='ready'", facts.company),
        readPmsProperties1: facts => posGuestAccountInterface(db).choices(facts.company,facts.branch_id,facts.currency,facts.propertyId),
        readPosRefundPayments1: (facts: {
            company: string;
        }) => all(db, "SELECT id,kind,record_id,state,last_error,payload_json,created_at,(SELECT r.version FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id WHERE rp.id=pos_jobs.record_id AND r.company_id=pos_jobs.company_id AND pos_jobs.kind='refund') refund_version FROM pos_jobs WHERE company_id=? AND (state='exception' OR kind IN ('connect','connect_refresh')) ORDER BY created_at DESC LIMIT 50", facts.company),
        readPosLines3: (facts: {
            company: string;
            company2: string;
            branch: string | null;
            branch2: string | null;
        }) => all(db, `WITH allocated AS (
 SELECT l.*,f.state fulfillment_state,
 ROUND(f.cost_minor::numeric*SUM(l.quantity_milli) OVER(PARTITION BY f.id ORDER BY l.id)/f.quantity_milli)
 -ROUND(f.cost_minor::numeric*(SUM(l.quantity_milli) OVER(PARTITION BY f.id ORDER BY l.id)-l.quantity_milli)/f.quantity_milli) cost_share
 FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE f.company_id=?
 ), totals AS (SELECT sale_id,SUM(refunded_minor) refunds_minor,SUM(refunded_tax_minor) refunded_tax_minor,
 SUM(CASE WHEN fulfillment_state='delivered' THEN cost_share ELSE 0 END-returned_cost_minor) cost_minor FROM allocated GROUP BY sale_id)
 SELECT s.business_date,s.branch_id,s.register_id,s.shift_id,s.currency,COUNT(*) bills,SUM(s.subtotal_minor) subtotal_minor,SUM(s.tax_minor) tax_minor,
 SUM(s.discount_minor) discount_minor,SUM(s.total_minor) total_minor,SUM(t.refunds_minor) refunds_minor,
 SUM(t.cost_minor) cost_minor,SUM(s.subtotal_minor-t.refunds_minor+t.refunded_tax_minor-t.cost_minor) gross_profit_minor
 FROM pos_sales s JOIN totals t ON t.sale_id=s.id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.state='completed' AND NOT EXISTS(SELECT 1 FROM (${pmsProjectionSources.pms_sources}) ps WHERE ps.source_key='pos:'||s.id||':'||s.id)
 GROUP BY s.business_date,s.branch_id,s.register_id,s.shift_id,s.currency ORDER BY s.business_date DESC LIMIT 90`, facts.company, facts.company2, facts.branch, facts.branch2),
        readPosRefundPayments2: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
        }) => all(db, `SELECT s.business_date,s.register_id,s.shift_id,s.currency,p.method,SUM(p.amount_minor) received_minor,
 SUM(COALESCE((SELECT SUM(rp.amount_minor) FROM pos_refund_payments rp WHERE rp.payment_id=p.id AND rp.state='succeeded'),0)) refunded_minor
 FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id WHERE p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND p.state='succeeded'
 GROUP BY s.business_date,s.register_id,s.shift_id,s.currency,p.method ORDER BY s.business_date DESC LIMIT 300`, facts.company, facts.branch, facts.branch2),
        readPosSaleDocuments1: (facts: {
            id: string | number | boolean | null;
            company: string;
        }) => one(db, 'SELECT snapshot_json::text FROM pos_sale_documents WHERE sale_id=? AND company_id=?', facts.id, facts.company),
        readOrganizationNodes1: facts => PosReadOrganizationFacts(db).readOrganizationNodes1(facts),
        readFinanceBankTransactions2: facts => PosReadFinanceFacts(db).readFinanceBankTransactions2(facts),
        readOrganizationNodes2: facts => PosReadOrganizationFacts(db).readOrganizationNodes2(facts),
        readPosDayCloses1: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
        }) => all(db, 'SELECT id,company_id,branch_id,business_date,currency,state,prepared_by,confirmed_by,version,snapshot_json::text FROM pos_day_closes WHERE company_id=? AND (?::text IS NULL OR branch_id=?) ORDER BY business_date DESC LIMIT 90', facts.company, facts.branch, facts.branch2),
        readFinanceAccounts2: facts => PosReadFinanceFacts(db).readFinanceAccounts2(facts),
        readFinanceCharts1: facts => PosReadFinanceFacts(db).readFinanceCharts1(facts),
        readPosConnections2: (facts: {
            company: string;
        }) => one(db, 'SELECT * FROM pos_connections WHERE company_id=?', facts.company),
        readPosSettlements1: (facts: {
            company: string;
        }) => all(db, 'SELECT * FROM pos_settlements WHERE company_id=? ORDER BY created_at DESC LIMIT 100', facts.company),
        readPosSales3: (facts: {
            id: string;
            company: string;
            branch: string | null;
            branch2: string | null;
            currency: string;
            from: string;
            to: string;
        }) => db.prepare(`SELECT s.id source_id,s.id sale_id,'sale' source_type,s.number receipt_number,s.business_date,
     NULL::text accounting_date,s.currency,s.state,s.total_minor::text amount_minor,s.total_minor::text contribution_minor,
     '0'::text refunded_minor,s.version revision,s.version sale_version,s.branch_id
   FROM pos_sales s WHERE s.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
     AND s.business_date BETWEEN ? AND ? AND s.state='completed' LIMIT 1`).bind(facts.id, facts.company, facts.branch, facts.branch2, facts.currency, facts.from, facts.to).first(),
        readPosSales4: (facts: {
            id: string;
            company: string;
            branch: string | null;
            branch2: string | null;
            currency: string;
            from: string;
            to: string;
        }) => db.prepare(`SELECT s.id source_id,s.id sale_id,'sale' source_type,s.number receipt_number,s.business_date,
     NULL::text accounting_date,s.currency,s.state,s.total_minor::text amount_minor,s.total_minor::text contribution_minor,
     '0'::text refunded_minor,s.version revision,s.version sale_version,s.branch_id
   FROM pos_sales s WHERE s.id=? AND s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
     AND s.business_date BETWEEN ? AND ? AND s.state='completed' LIMIT 1`).bind(facts.id, facts.company, facts.branch, facts.branch2, facts.currency, facts.from, facts.to).first(),
        readPosRefunds2: (facts: {
            id: string;
            company: string;
            branch: string | null;
            branch2: string | null;
            currency: string;
            from: string;
            to: string;
        }) => db.prepare(`SELECT r.id source_id,s.id sale_id,'refund' source_type,s.number receipt_number,
     s.business_date,r.accounting_date,s.currency,r.state,r.amount_minor::text amount_minor,
     (-r.amount_minor::numeric)::text contribution_minor,'0'::text refunded_minor,r.version revision,s.version sale_version,r.branch_id
   FROM pos_refunds r JOIN pos_sales s ON s.id=r.sale_id AND s.company_id=r.company_id
   WHERE r.id=? AND r.company_id=? AND (?::text IS NULL OR r.branch_id=?) AND s.currency=?
     AND r.accounting_date BETWEEN ? AND ? AND r.kind='sale' AND r.state='completed' LIMIT 1`).bind(facts.id, facts.company, facts.branch, facts.branch2, facts.currency, facts.from, facts.to).first(),
        readPosRefundPayments3: (facts: {
            id: string;
            company: string;
            branch: string | null;
            branch2: string | null;
            currency: string;
            from: string;
            to: string;
        }) => db.prepare(`SELECT p.id source_id,'payment' source_type,s.id sale_id,s.number receipt_number,
     s.business_date,NULL::text accounting_date,s.currency,s.state sale_state,p.state,p.method,p.amount_minor::text amount_minor,
     COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
       WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0)::text refunded_minor,
     (p.amount_minor::numeric-COALESCE((SELECT sum(rp.amount_minor)::numeric FROM pos_refund_payments rp JOIN pos_refunds r ON r.id=rp.refund_id AND r.company_id=p.company_id
       WHERE rp.payment_id=p.id AND rp.state='succeeded' AND r.kind='sale'),0))::text contribution_minor,
     p.version revision,s.version sale_version,s.branch_id,p.created_at
   FROM pos_payments p JOIN pos_sales s ON s.id=p.sale_id AND s.company_id=p.company_id
   WHERE p.id=? AND p.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.currency=?
     AND s.business_date BETWEEN ? AND ? AND s.state='completed' AND p.state='succeeded' LIMIT 1`).bind(facts.id, facts.company, facts.branch, facts.branch2, facts.currency, facts.from, facts.to).first(),
        readPosRefunds3: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
            userId: string;
        }) => all(db, "SELECT * FROM pos_refunds WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND state='requested' AND requested_by<>?", facts.company, facts.branch, facts.branch2, facts.userId),
        readPosShifts1: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
            userId: string;
        }) => all(db, "SELECT * FROM pos_shifts WHERE company_id=? AND (?::text IS NULL OR branch_id=?) AND state='closed' AND opened_by<>?", facts.company, facts.branch, facts.branch2, facts.userId),
        readPosCashMovements2: (facts: {
            company: string;
            branch: string | null;
            branch2: string | null;
            userId: string;
        }) => all(db, 'SELECT m.* FROM pos_cash_movements m JOIN pos_shifts s ON s.id=m.shift_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND m.approved_by IS NULL AND m.created_by<>?', facts.company, facts.branch, facts.branch2, facts.userId),
        readPosJobs1: (facts: {
            company: string;
        }) => all(db, "SELECT * FROM pos_jobs WHERE company_id=? AND state='exception'", facts.company),
        readPosRegisters3: (facts: {
            registerId: string;
        }) => one(db, 'SELECT * FROM pos_registers WHERE id=?', facts.registerId),
        readPosLines4: (facts: {
            companyId: string;
            companyId2: string;
            branchId: string | null;
            branchId2: string | null;
            businessDate: string;
            registerId: string;
            shiftId: string;
            currency: string;
            fact8: number;
        }) => all(db, `WITH allocated AS (
 SELECT l.*,f.state fulfillment_state,
 ROUND(f.cost_minor::numeric*SUM(l.quantity_milli) OVER(PARTITION BY f.id ORDER BY l.id)/f.quantity_milli)
 -ROUND(f.cost_minor::numeric*(SUM(l.quantity_milli) OVER(PARTITION BY f.id ORDER BY l.id)-l.quantity_milli)/f.quantity_milli) cost_share
 FROM pos_lines l JOIN pos_fulfillments f ON f.id=l.fulfillment_id WHERE f.company_id=?
 ), totals AS (SELECT sale_id,SUM(refunded_minor) refunds_minor,SUM(refunded_tax_minor) refunded_tax_minor,
 SUM(CASE WHEN fulfillment_state='delivered' THEN cost_share ELSE 0 END-returned_cost_minor) cost_minor FROM allocated GROUP BY sale_id), sources AS (
 SELECT s.*,t.refunds_minor,t.cost_minor,s.subtotal_minor-t.refunds_minor+t.refunded_tax_minor-t.cost_minor gross_profit_minor
 FROM pos_sales s JOIN totals t ON t.sale_id=s.id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND s.state='completed'
 AND NOT EXISTS(SELECT 1 FROM (${pmsProjectionSources.pms_sources}) ps WHERE ps.source_key='pos:'||s.id||':'||s.id)
 AND s.business_date=? AND s.register_id=? AND s.shift_id=? AND s.currency=?
 ) SELECT *,SUM(total_minor) OVER() group_total_minor,COUNT(*) OVER() group_count FROM sources ORDER BY created_at DESC,id LIMIT 31 OFFSET ?`, facts.companyId, facts.companyId2, facts.branchId, facts.branchId2, facts.businessDate, facts.registerId, facts.shiftId, facts.currency, facts.fact8),
        posCustomers: (arg0: WorkContext) => posCustomers(db, arg0),
        salesPage: (facts: {
            company: string;
            branch: string | null;
            historyQuery: string;
            cursor?: string;
        }) => readListPage<import("@/server/modules/pos/public/pos-core").PosRow & {
            id: string;
            page_key: string;
        }>(db, {
            sql: "SELECT s.*,r.name register_name,s.created_at::text page_key FROM pos_sales s JOIN pos_registers r ON r.id=s.register_id WHERE s.company_id=? AND (?::text IS NULL OR s.branch_id=?) AND (?='' OR s.number ILIKE ?)",
            args: [facts.company, facts.branch, facts.branch, facts.historyQuery.slice(0, 100), `%${facts.historyQuery.slice(0, 100)}%`], cursor: facts.cursor, descending: true, scope: [facts.company, facts.branch, 'pos-history', facts.historyQuery],
        }), batch: effects => db.batch(effects)
    };
}
const application = createPosRead<SqlStatement>({ clock: { now: () => Date.now() }, uuid: randomUUID, hash: value => createHash("sha256").update(value).digest("hex"), environment: key => process.env[key], require: requirePermission, error: (status, code, message) => new ApiError(status, code, message), forActor: async (actor) => unit((await holdingResources(actor.holdingId)).write) });
export function posWorkspace(actor: WorkContext, saleId?: string, historyQuery = '', section?: string, cursor?: string, frontMode = false, financeMode = false) { return application.posWorkspace(actor, saleId, historyQuery, section, cursor, frontMode, financeMode); }
export function posExecutiveEvidenceRow(db: SqlDatabase, actor: WorkContext, metric: PosExecutiveEvidenceMetric, recordId: string, filters: {
    from: string;
    to: string;
    currency: string;
}) { return application.posExecutiveEvidenceRow(unit(db), actor, metric, recordId, filters); }
export async function posWork(actor: WorkContext) { return [...await application.posWork(actor),...await roomChargeReversalWork(actor,"pos")]; }
export function posReportReceipts(actor: WorkContext, group: {
    businessDate: string;
    registerId: string;
    shiftId: string;
    currency: string;
}, page = 1) { return application.posReportReceipts(actor, group, page); }
export type PosWorkspace = Awaited<ReturnType<typeof posWorkspace>>;
export type { PosExecutiveEvidenceMetric };
