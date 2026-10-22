import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
/** A notification statement shares the owner command's atomic commit and replay receipt. */
export function prepareFieldServiceNotification(db: SqlDatabase, input: { key: string; companyId: string; technicianId: string; orderId: string; version: number; created: boolean; now: number }) {
  return db.prepare("INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at) VALUES(?,?,?,'field_service_order',?,?,?,?)").bind(`service-order-notice-${input.key}`, input.companyId, input.technicianId, input.orderId, input.version, input.created ? "New field service order: review appointment and required context" : "Field service order updated: review current next action", input.now);
}
