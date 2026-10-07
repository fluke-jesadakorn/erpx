import {collaborationNotificationSnapshotSources as procurementSnapshotSources} from '../../procurement/public/collaboration-notification-snapshot-sources';
import {collaborationNotificationSnapshotSources as identityaccessSnapshotSources} from '../../identity_access/public/collaboration-notification-snapshot-sources';
const procurementSources=procurementSnapshotSources();
const identityaccessSources=identityaccessSnapshotSources();
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export function stageSupplierBatchNotifications(db:SqlDatabase,key:string,id:string,message:string,actor:string) {
 return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
 SELECT DISTINCT ?||'-'||a.user_id,b.company_id,a.user_id,'supplier_batch',b.id,b.version,?,floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 ${procurementSources.supplier_batch_notifications_finance_supplier_batches_b_from} ${identityaccessSources.supplier_batch_notifications_assignments_a_join} ON a.company_id=b.company_id WHERE b.id=? AND a.user_id<>? AND a.scope_kind='holding'
 AND a.role IN ('holding_owner','finance_approver','finance_preparer','treasury') AND a.revoked_at IS NULL AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint ON CONFLICT DO NOTHING`).bind(key,message,id,actor);
}
