import type {SqlDatabase} from './runtime/types';
/** Messages carry only a source link; Inbox rechecks source authority before display. */
export function refundNotifications(db:SqlDatabase,key:string,id:string,audience:'approval'|'treasury'|'review'|'completed',message:string,actorId:string) {
 const eligibility=audience==='treasury'
  ? "a.role IN ('holding_owner','finance_approver','treasury') AND a.scope_kind='holding' AND a.user_id<>o.created_by AND a.user_id<>o.approved_by"
  : audience==='approval'
  ? "a.role IN ('holding_owner','finance_approver') AND a.user_id<>o.created_by AND a.user_id<>r.requested_by"
  : audience==='review'
  ? "a.role IN ('holding_owner','finance_approver')"
  : "a.role IN ('holding_owner','finance_approver','finance_preparer','treasury') AND a.user_id IN (o.created_by,o.approved_by,r.requested_by)";
 return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
 SELECT DISTINCT ?||'-'||a.user_id,o.company_id,a.user_id,'refund',o.id,o.version,?,floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 FROM finance_refund_obligations o JOIN commerce_returns r ON r.id=o.return_id JOIN assignments a ON a.company_id=o.company_id
 WHERE o.id=? AND a.user_id<>? AND ${eligibility} AND a.revoked_at IS NULL
 AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint
 AND (a.scope_kind='holding' OR a.scope_kind='branch' AND a.scope_node_id=o.branch_id OR a.scope_kind='department' AND a.scope_node_id=o.department_id)
 ON CONFLICT DO NOTHING`).bind(key,message,id,actorId);
}
