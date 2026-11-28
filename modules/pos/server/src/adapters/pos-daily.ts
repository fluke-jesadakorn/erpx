import { nativePosOne, nativePosRun } from './native-owner-write';
import { nativePosSaleByIdStatement, nativePosSettingsCurrencyStatement } from './native-pos-core-statement';
import { nativePosDayClosesStatement, nativePosDaySalesStatement, nativePosDayShiftsStatement, nativePosDayPaymentsStatement, nativePosDayRefundsStatement, nativePosDayRefundPaymentsStatement, nativePosDayStockMovesStatement, nativePosDayFulfillmentsStatement, nativePosDayCashMovementsStatement, nativePosDaySettlementsStatement } from './native-pos-day-report';
import { nativePosDayPlannerLinesStatement, nativePosDayPlannerPaymentsStatement } from './native-pos-sale-child';
import { hospitalityOwnerProjectionSources as pmsProjectionSources } from "@erpx/pms-server/public/hospitality-owner-projection-sources";
import 'server-only';
import { createHash } from 'node:crypto';
import type { WorkContext } from "@erpx/identity-access-server/public/authorization";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import {one} from '@erpx/platform-runtime-server/public/runtime/sql';
/** Day-close writes are owned by the native axum service; the retained TS
 * planner must never write them. Fail closed rather than fake a durable write. */
function requireNativeDayClose(): never {
    throw Object.assign(new Error('POS day-close writes require the native owner service.'), { code: 'POS_NATIVE_DAY_CLOSE_REQUIRED', status: 503 });
}
import { requirePermission } from "@erpx/identity-access-server/public/iam-permissions";
import type { SqlStatement } from "@erpx/platform-runtime-server/public/runtime/types";
import { createPosDay, type PosDayUnit, type PosReceiptSnapshot } from "@erpx/pos-server/application/pos-daily";
import { randomUUID } from "node:crypto";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { holdingResources } from "../../../../../platform/runtime/server/src/public/runtime";
import { PosDayFinanceFacts } from "@erpx/finance-server/public/pos-daily-facts";
import { PosDayOrganizationFacts } from "@erpx/organization-server/public/pos-daily-facts";
import { requirePosPurchased, requirePosReportingPurchase } from "@erpx/pos-server/adapters/purchase-boundary";
function unit(db: SqlDatabase): PosDayUnit<SqlStatement> {
    return {
        readPosSales1: (facts: {
            saleId: string;
        }) => nativePosSaleByIdStatement(db, facts.saleId),
        readFinanceParties1: facts => PosDayFinanceFacts(db).readFinanceParties1(facts),
        readPosLines1: (facts: {
            saleId: string;
        }) => nativePosDayPlannerLinesStatement(db, facts.saleId),
        readPosPayments1: (facts: {
            saleId: string;
        }) => nativePosDayPlannerPaymentsStatement(db, facts.saleId),
        readPmsSources1: (facts: {
            fact0: string;
        }) => one(db, `SELECT property_id,record_id guest_account_entry_id FROM (${pmsProjectionSources.pms_sources}) pms_sources WHERE source_key=?`, facts.fact0),
        writePosSaleDocuments1: (facts: {
            saleId: string;
            company_id: string | number | boolean | null;
            branch_id: string | number | boolean | null;
            fact3: string;
            fact4: number;
        }) => nativePosRun(db, String(facts.company_id ?? ''), 'daily_writepossaledocuments1_43', [facts.saleId, facts.company_id, facts.branch_id, facts.fact3, facts.fact4]),
        readFinanceParties2: facts => PosDayFinanceFacts(db).readFinanceParties2(facts),
        readFinanceParties3: facts => PosDayFinanceFacts(db).readFinanceParties3(facts),
        readPosSettings1: (facts: {
            companyId: string;
        }) => nativePosSettingsCurrencyStatement(db, facts.companyId),
        writeFinanceParties1: facts => PosDayFinanceFacts(db).writeFinanceParties1(facts),
        readPosDayCloses1: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => nativePosDayClosesStatement(db, facts),
        readOrganizationNodes1: facts => PosDayOrganizationFacts(db).readOrganizationNodes1(facts),
        readPosSales2: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => nativePosDaySalesStatement(db, facts),
        readPosShifts1: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => nativePosDayShiftsStatement(db, facts),
        readPosPayments2: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => nativePosDayPaymentsStatement(db, facts),
        readPosRefunds1: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => nativePosDayRefundsStatement(db, facts),
        readPosRefundPayments1: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => nativePosDayRefundPaymentsStatement(db, facts),
        readPosStockMoves1: (facts: {
            company: string;
            date: string;
            company2: string;
            branch: string;
        }) => nativePosDayStockMovesStatement(db, facts.company, facts.date, facts.company2, facts.branch),
        readFinanceJournals1: facts => PosDayFinanceFacts(db).readFinanceJournals1(facts),
        readFinanceJournalLines1: facts => PosDayFinanceFacts(db).readFinanceJournalLines1(facts),
        readFinanceJournalLines2: facts => PosDayFinanceFacts(db).readFinanceJournalLines2(facts),
        readPosSettings2: (facts: {
            company: string;
        }) => nativePosSettingsCurrencyStatement(db, facts.company),
        readPosFulfillments1: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => nativePosDayFulfillmentsStatement(db, facts),
        readPosCashMovements1: (facts: {
            company: string;
            branch: string;
            date: string;
        }) => nativePosDayCashMovementsStatement(db, facts),
        readPosSettlements1: (facts: {
            company: string;
        }) => nativePosDaySettlementsStatement(db, facts.company),
        readFinanceJournalLines3: facts => PosDayFinanceFacts(db).readFinanceJournalLines3(facts),
        writePosDayCloses1: () => requireNativeDayClose(),
        readPosDayCloses2: (facts: {
            id: string;
        }) => one(db, 'SELECT * FROM pos_day_closes WHERE id=? FOR UPDATE', facts.id),
        writePosDayCloses2: () => requireNativeDayClose(), batch: effects => db.batch(effects)
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
