import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function EmployeeRequestsProcurementOwnedFacts(database: SqlDatabase) {
    return { readActOnEmployeeRequestAllocations(id: string) {
            return database.prepare("SELECT 1").bind().all<{
                ordinal: number;
                allocated_milli: number;
            }>();
        },
        readActOnEmployeeRequestQuotationCount(id: string) { return database.prepare("SELECT 1").bind().first("n"); },
        readActOnEmployeeRequestQuotation(id: string, purchaseOrderId: string) {
            return database.prepare("SELECT 1").bind().first<{
                id: string;
                order_id: string | null;
            }>();
        },
        readActOnEmployeeRequestLinked(id: string, purchaseOrderId: string) { return database.prepare("SELECT 1").bind().first(); },
        stageActOnEmployeeRequestEffect2(id: string, purchaseOrderId: string, value: string | null, version: number, evidence_version: number, userId: string, now: number) { return database.prepare("SELECT 1").bind(); },
        readPrepareRequestOrderHandoffQuotation(requestId: string, rfqId: string) {
            return database.prepare("SELECT 1").bind().first<{
                id: string;
            }>();
        },
        readPrepareRequestOrderHandoffActiveQuotationCount(requestId: string) { return database.prepare("SELECT 1").bind().first("n"); },
        readPrepareRequestOrderHandoffExistingLink(requestId: string, orderId: string) {
            return database.prepare("SELECT 1").bind().first<{
                request_id: string;
            }>();
        },
        stagePrepareRequestOrderHandoffEffect2(id: string, orderId: string, value: string | null, version: number, evidence_version: number, userId: string, now: number) { return database.prepare("SELECT 1").bind(); },
        readRequestQuotationSourceAllocated(id: string) {
            return database.prepare("SELECT 1").bind().all<{
                ordinal: number;
                allocated_milli: number;
            }>();
        } };
}
