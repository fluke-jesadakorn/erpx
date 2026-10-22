import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function EmployeeRequestsProcurementOwnedFacts(database: SqlDatabase) {
    return { readActOnEmployeeRequestAllocations(id: string) {
            return database.prepare("SELECT l.request_item_ordinal ordinal,COALESCE(sum(l.request_quantity_milli),0) allocated_milli FROM procurement_rfq_lines l JOIN procurement_rfqs r ON r.id=l.rfq_id WHERE r.request_id=? AND r.status<>'cancelled' GROUP BY l.request_item_ordinal").bind(id).all<{
                ordinal: number;
                allocated_milli: number;
            }>();
        },
        readActOnEmployeeRequestQuotationCount(id: string) { return database.prepare("SELECT count(*) n FROM procurement_rfqs WHERE request_id=? AND status<>'cancelled'").bind(id).first("n"); },
        readActOnEmployeeRequestQuotation(id: string, purchaseOrderId: string) {
            return database.prepare("SELECT id,order_id FROM procurement_rfqs WHERE request_id=? AND order_id=? AND status='ordered'").bind(id, purchaseOrderId).first<{
                id: string;
                order_id: string | null;
            }>();
        },
        readActOnEmployeeRequestLinked(id: string, purchaseOrderId: string) { return database.prepare("SELECT 1 ok FROM procurement_request_orders WHERE request_id=? AND order_id=?").bind(id, purchaseOrderId).first(); },
        stageActOnEmployeeRequestEffect2(id: string, purchaseOrderId: string, value: string | null, version: number, evidence_version: number, userId: string, now: number) { return database.prepare("INSERT INTO procurement_request_orders(request_id,order_id,rfq_id,request_version,request_evidence_version,linked_by,created_at) VALUES(?,?,?,?,?,?,?)").bind(id, purchaseOrderId, value, version, evidence_version, userId, now); },
        readPrepareRequestOrderHandoffQuotation(requestId: string, rfqId: string) {
            return database.prepare("SELECT id FROM procurement_rfqs WHERE request_id=? AND id=? AND status='approved'").bind(requestId, rfqId).first<{
                id: string;
            }>();
        },
        readPrepareRequestOrderHandoffActiveQuotationCount(requestId: string) { return database.prepare("SELECT count(*) n FROM procurement_rfqs WHERE request_id=? AND status<>'cancelled'").bind(requestId).first("n"); },
        readPrepareRequestOrderHandoffExistingLink(requestId: string, orderId: string) {
            return database.prepare("SELECT request_id FROM procurement_request_orders WHERE request_id=? AND order_id=?").bind(requestId, orderId).first<{
                request_id: string;
            }>();
        },
        stagePrepareRequestOrderHandoffEffect2(id: string, orderId: string, value: string | null, version: number, evidence_version: number, userId: string, now: number) { return database.prepare("INSERT INTO procurement_request_orders(request_id,order_id,rfq_id,request_version,request_evidence_version,linked_by,created_at) VALUES(?,?,?,?,?,?,?)").bind(id, orderId, value, version, evidence_version, userId, now); },
        readRequestQuotationSourceAllocated(id: string) {
            return database.prepare("SELECT l.request_item_ordinal ordinal,COALESCE(sum(l.request_quantity_milli),0) allocated_milli FROM procurement_rfq_lines l JOIN procurement_rfqs r ON r.id=l.rfq_id WHERE r.request_id=? AND r.status<>'cancelled' GROUP BY l.request_item_ordinal").bind(id).all<{
                ordinal: number;
                allocated_milli: number;
            }>();
        } };
}
