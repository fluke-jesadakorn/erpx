import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { nativeProcurementStatement } from '@erpx/feature-procurement/adapters/server/native-procurement-command';
import {nativeProcurementReadFirst,PROCUREMENT_RD_ACTIVE_QUOTATION_COUNT} from "./native-procurement-read";
export function EmployeeRequestsProcurementOwnedFacts(database: SqlDatabase) {
    /** Owner-native active-quotation count. The statement is planned by the
     * Procurement command-plan (`RD_ACTIVE_QUOTATION_COUNT`) and executed on
     * the caller's open unit; this module carries no SQL for these reads. */
    const activeQuotationCount = (requestId: string): Promise<Record<string, unknown> | null> =>
        nativeProcurementReadFirst<{ n: number }>(PROCUREMENT_RD_ACTIVE_QUOTATION_COUNT, [requestId], database)
            // The consumer port declares the scalar `.first('n')` shape as a
            // Record; keep that port type and the original scalar runtime value.
            .then(row => (row?.n ?? null) as unknown as Record<string, unknown> | null);
    return { readActOnEmployeeRequestAllocations(id: string) {
            return database.prepare("SELECT l.request_item_ordinal ordinal,COALESCE(sum(l.request_quantity_milli),0) allocated_milli FROM procurement_rfq_lines l JOIN procurement_rfqs r ON r.id=l.rfq_id WHERE r.request_id=? AND r.status<>'cancelled' GROUP BY l.request_item_ordinal").bind(id).all<{
                ordinal: number;
                allocated_milli: number;
            }>();
        },
        readActOnEmployeeRequestQuotationCount(id: string) { return activeQuotationCount(id); },
        readActOnEmployeeRequestQuotation(id: string, purchaseOrderId: string) {
            return database.prepare("SELECT id,order_id FROM procurement_rfqs WHERE request_id=? AND order_id=? AND status='ordered'").bind(id, purchaseOrderId).first<{
                id: string;
                order_id: string | null;
            }>();
        },
        readActOnEmployeeRequestLinked(id: string, purchaseOrderId: string) { return database.prepare("SELECT 1 ok FROM procurement_request_orders WHERE request_id=? AND order_id=?").bind(id, purchaseOrderId).first(); },
        stageActOnEmployeeRequestEffect2(id: string, purchaseOrderId: string, value: string | null, version: number, evidence_version: number, userId: string, now: number) { return nativeProcurementStatement('procurement_stmt_21', { now: Date.now() }).bind(id, purchaseOrderId, value, version, evidence_version, userId, now); },
        readPrepareRequestOrderHandoffQuotation(requestId: string, rfqId: string) {
            return database.prepare("SELECT id FROM procurement_rfqs WHERE request_id=? AND id=? AND status='approved'").bind(requestId, rfqId).first<{
                id: string;
            }>();
        },
        readPrepareRequestOrderHandoffActiveQuotationCount(requestId: string) { return activeQuotationCount(requestId); },
        readPrepareRequestOrderHandoffExistingLink(requestId: string, orderId: string) {
            return database.prepare("SELECT request_id FROM procurement_request_orders WHERE request_id=? AND order_id=?").bind(requestId, orderId).first<{
                request_id: string;
            }>();
        },
        stagePrepareRequestOrderHandoffEffect2(id: string, orderId: string, value: string | null, version: number, evidence_version: number, userId: string, now: number) { return nativeProcurementStatement('procurement_stmt_21', { now: Date.now() }).bind(id, orderId, value, version, evidence_version, userId, now); },
        readRequestQuotationSourceAllocated(id: string) {
            return database.prepare("SELECT l.request_item_ordinal ordinal,COALESCE(sum(l.request_quantity_milli),0) allocated_milli FROM procurement_rfq_lines l JOIN procurement_rfqs r ON r.id=l.rfq_id WHERE r.request_id=? AND r.status<>'cancelled' GROUP BY l.request_item_ordinal").bind(id).all<{
                ordinal: number;
                allocated_milli: number;
            }>();
        } };
}
