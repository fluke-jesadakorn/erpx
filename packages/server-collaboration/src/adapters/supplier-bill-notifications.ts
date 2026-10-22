import {collaborationNotificationSnapshotSources as procurementSnapshotSources} from '@erpx/server-procurement/public/collaboration-notification-snapshot-sources';
import {collaborationNotificationSnapshotSources as identityaccessSnapshotSources} from '@erpx/server-identity_access/public/collaboration-notification-snapshot-sources';
const procurementSources=procurementSnapshotSources();
const identityaccessSources=identityaccessSnapshotSources();
import "server-only";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
export function stageSupplierBillNotifications(db: SqlDatabase, id: string, key: string) {
  return db.prepare(`
    INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
    SELECT DISTINCT ?||'-'||a.user_id,m.company_id,a.user_id,'supplier_bill',m.id,m.version,
      CASE m.status
        WHEN 'returned' THEN 'Supplier invoice returned: obtain corrected evidence and resubmit from the receipt'
        WHEN 'approved' THEN 'Supplier invoice matched: assigned PO owner can prepare AP'
        WHEN 'invoiced' THEN 'Matched AP draft needs independent Finance posting'
        WHEN 'posted' THEN 'Matched supplier invoice posted to GL'
        ELSE 'Supplier invoice needs independent quantity and price review'
      END,m.updated_at
    ${procurementSources.supplier_bill_notifications_procurement_bill_matches_m_from}
    ${identityaccessSources.supplier_bill_notifications_assignments_a_join} ON a.company_id=m.company_id
      AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_role_allows(a.role,a.company_id,'finance.write')
      AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=m.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=m.department_id))
    WHERE m.id=? ON CONFLICT DO NOTHING
  `).bind(key, id);
}

