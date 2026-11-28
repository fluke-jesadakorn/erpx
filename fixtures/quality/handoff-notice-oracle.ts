/**
 * FIXTURE-ONLY ORACLE — retained pre-cutover Quality handoff notification.
 *
 * Production handoff notice generation now belongs to the native
 * `customer_return_hold_statements` port (server-quality), which stages the
 * exact INSERT in the caller's serializable unit. This copy seeds the same row
 * for isolated Quality hold fixtures without the TS writer.
 *
 * Tracked for removal by `migrate.commerce.return-approval-native`: delete once
 * the Quality hold fixture seeds the handoff through the native statement port.
 */
import type { SqlDatabase } from '@erpx/platform-runtime-server/public/runtime/types';

const HANDOFF_SQL = `INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
 SELECT DISTINCT ?||'-'||a.user_id,h.company_id,a.user_id,'quality_hold',h.id,h.version,'Quality handoff: review the source record',h.updated_at
 FROM (SELECT "id","company_id","branch_id","warehouse_id","product_id","reason","due_date","status","created_by","last_inspector","last_result","released_by","release_reason","released_at","version","created_at","updated_at","cancel_requested_by","cancel_reason","cancelled_by","cancelled_at","lot_id","held_quantity_milli","source_type","source_id","disposition_kind","disposition_reason","disposition_requested_by","disposition_count_id","disposed_by","disposed_at","spec_id","spec_version","quantity_scope","disposition_quantity_milli" FROM inventory_quality_holds) h
 JOIN (SELECT "id","user_id","role","modules_json","company_id","branch_id","valid_from","valid_until","revoked_at","version","scope_kind","scope_node_id","parent_id","source","source_id" FROM assignments) a
 ON ((a.company_id=h.company_id AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=h.branch_id)) AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,CASE WHEN h.status IN ('awaiting_release','cancel_pending') THEN 'quality.release' ELSE 'quality.inspect' END) AND NOT iam_same_person(a.user_id,h.created_by,h.company_id) AND (h.status<>'cancel_pending' OR NOT iam_same_person(a.user_id,h.cancel_requested_by,h.company_id)) AND (h.status<>'awaiting_release' OR NOT iam_same_person(a.user_id,h.last_inspector,h.company_id)) AND h.status NOT IN ('released','cancelled','disposed')) OR (h.status IN ('released','cancelled','disposed') AND a.company_id=h.company_id AND a.user_id=h.created_by AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=h.branch_id)) AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint) AND iam_role_allows(a.role,a.company_id,'quality.read'))) WHERE h.id=? ON CONFLICT DO NOTHING`;

export function seedQualityHandoffNotice(db: SqlDatabase, key: string, id: string) {
  return db.prepare(HANDOFF_SQL).bind(key, id);
}
