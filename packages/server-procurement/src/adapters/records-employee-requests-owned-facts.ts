import type { SqlDatabase, SqlResult } from "@erpx/server-runtime/public/runtime/types";
import { nativeProcurementStatement } from '@erpx/feature-procurement/adapters/server/native-procurement-command';
import {nativeProcurementReadAll,nativeProcurementReadFirst,PROCUREMENT_RD_ACTIVE_QUOTATION_COUNT,PROCUREMENT_RD_REQUEST_ALLOCATIONS,PROCUREMENT_RD_REQUEST_ORDER_LINK} from "./native-procurement-read";
export function EmployeeRequestsProcurementOwnedFacts(database: SqlDatabase) {
    /** Owner-native active-quotation count. The statement is planned by the
     * Procurement command-plan (`RD_ACTIVE_QUOTATION_COUNT`) and executed on
     * the caller's open unit; this module carries no SQL for these reads. */
    const activeQuotationCount = (requestId: string): Promise<Record<string, unknown> | null> =>
        nativeProcurementReadFirst<{ n: number }>(PROCUREMENT_RD_ACTIVE_QUOTATION_COUNT, [requestId], database)
            // The consumer port declares the scalar `.first('n')` shape as a
            // Record; keep that port type and the original scalar runtime value.
            .then(row => (row?.n ?? null) as unknown as Record<string, unknown> | null);
    /** Owner-native request allocations. The statement is planned by the
     * Procurement command-plan (`RD_REQUEST_ALLOCATIONS`) and executed on the
     * caller's open unit; this module carries no SQL for these reads. */
    const requestAllocations = (requestId: string): Promise<SqlResult<{ ordinal: number; allocated_milli: number }>> =>
        nativeProcurementReadAll<{ ordinal: number; allocated_milli: number }>(PROCUREMENT_RD_REQUEST_ALLOCATIONS, [requestId], database);
    /** Owner-native request-order link. The statement is planned by the
     * Procurement command-plan (`RD_REQUEST_ORDER_LINK`) and executed on the
     * caller's open unit; this module carries no SQL for these reads. */
    const requestOrderLink = <T>(requestId: string, orderId: string): Promise<T | null> =>
        nativeProcurementReadFirst<T>(PROCUREMENT_RD_REQUEST_ORDER_LINK, [requestId, orderId], database);
    return { readActOnEmployeeRequestAllocations(id: string) { return requestAllocations(id); },
        readActOnEmployeeRequestQuotationCount(id: string) { return activeQuotationCount(id); },
        readActOnEmployeeRequestQuotation(id: string, purchaseOrderId: string) {
            return database.prepare("SELECT id,order_id FROM procurement_rfqs WHERE request_id=? AND order_id=? AND status='ordered'").bind(id, purchaseOrderId).first<{
                id: string;
                order_id: string | null;
            }>();
        },
        readActOnEmployeeRequestLinked(id: string, purchaseOrderId: string) { return requestOrderLink<Record<string, unknown>>(id, purchaseOrderId); },
        stageActOnEmployeeRequestEffect2(id: string, purchaseOrderId: string, value: string | null, version: number, evidence_version: number, userId: string, now: number) { return nativeProcurementStatement('procurement_stmt_21', { now: Date.now() }).bind(id, purchaseOrderId, value, version, evidence_version, userId, now); },
        readPrepareRequestOrderHandoffQuotation(requestId: string, rfqId: string) {
            return database.prepare("SELECT id FROM procurement_rfqs WHERE request_id=? AND id=? AND status='approved'").bind(requestId, rfqId).first<{
                id: string;
            }>();
        },
        readPrepareRequestOrderHandoffActiveQuotationCount(requestId: string) { return activeQuotationCount(requestId); },
        readPrepareRequestOrderHandoffExistingLink(requestId: string, orderId: string) {
            return requestOrderLink<{ request_id: string }>(requestId, orderId);
        },
        stagePrepareRequestOrderHandoffEffect2(id: string, orderId: string, value: string | null, version: number, evidence_version: number, userId: string, now: number) { return nativeProcurementStatement('procurement_stmt_21', { now: Date.now() }).bind(id, orderId, value, version, evidence_version, userId, now); },
        readRequestQuotationSourceAllocated(id: string) { return requestAllocations(id); } };
}
