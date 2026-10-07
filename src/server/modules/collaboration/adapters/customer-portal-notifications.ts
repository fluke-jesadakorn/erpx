import {collaborationNotificationSnapshotSources as identityaccessSnapshotSources} from '../../identity_access/public/collaboration-notification-snapshot-sources';
import {collaborationNotificationSnapshotSources as commerceSnapshotSources} from '../../commerce/public/collaboration-notification-snapshot-sources';
const identityaccessSources=identityaccessSnapshotSources();
const commerceSources=commerceSnapshotSources();
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
export async function notifyCustomerPortalStaff(db:SqlDatabase,input:{id:string;message:string;now:number;orderId:string;companyId:string}) {
 await db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
     SELECT DISTINCT 'customer-portal-'||?||'-'||a.user_id,a.company_id,a.user_id,'commerce_order',o.id,o.version,?,?::bigint ${identityaccessSources.customer_portal_notifications_assignments_a_from} ${commerceSources.customer_portal_notifications_commerce_orders_o_join} ON o.id=? AND o.company_id=a.company_id
     WHERE a.company_id=? AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
       AND iam_role_allows(a.role,a.company_id,'commerce.read')
       AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=o.branch_id) OR (a.scope_kind='department' AND a.scope_node_id=o.department_id)) ON CONFLICT DO NOTHING`).bind(input.id,input.message,input.now,input.orderId,input.companyId).run();
}
