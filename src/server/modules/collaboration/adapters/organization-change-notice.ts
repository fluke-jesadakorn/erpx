import {collaborationNotificationSnapshotSources as organizationSnapshotSources} from '../../organization/public/collaboration-notification-snapshot-sources';
import {collaborationNotificationSnapshotSources as identityaccessSnapshotSources} from '../../identity_access/public/collaboration-notification-snapshot-sources';
const organizationSources=organizationSnapshotSources();
const identityaccessSources=identityaccessSnapshotSources();
import "server-only";
import type {SqlDatabase} from "../../runtime/public/runtime/types";
const eligibleAssignment = `a.scope_kind='holding' AND a.revoked_at IS NULL
  AND iam_role_allows(a.role,a.company_id,'organization.manage')
  AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
  AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`;

export function organizationChangeNotice(db:SqlDatabase,companyId:string,id:string,key:string,event:'submitted'|'applied'|'rejected'|'cancelled') {
  const recipient=event==='submitted'
    ? "NOT iam_same_person(a.user_id,c.requested_by,c.company_id)"
    : event==='cancelled'
      ? "EXISTS(SELECT 1 FROM staff_notifications n WHERE n.company_id=c.company_id AND n.record_type='organization_change' AND n.record_id=c.id AND n.user_id=a.user_id) AND a.user_id<>c.requested_by"
      : "a.user_id=c.requested_by";
  const message={submitted:'Review organization change: ',applied:'Organization change applied: ',rejected:'Organization change returned for correction: ',cancelled:'Organization change cancelled: '}[event];
  return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
    SELECT DISTINCT ?||a.user_id,c.company_id,a.user_id,'organization_change',c.id,c.version,?||c.display_name,c.updated_at
    ${organizationSources.organization_change_notice_organization_node_changes_c_from} ${identityaccessSources.organization_change_notice_assignments_a_join} ON a.company_id=c.company_id
    WHERE c.id=? AND c.company_id=? AND ${eligibleAssignment} AND ${recipient} ON CONFLICT DO NOTHING`)
    .bind(`org-change-notice-${key}-`,message,id,companyId);
}
