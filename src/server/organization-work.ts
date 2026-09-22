import { hasPermission } from "@/lib/permissions";
import { samePerson } from "@/lib/iam";
import { organizationChangeHref, type OrganizationNodeChange } from "@/lib/organization";
import type { WorkItem } from "@/lib/work-item";
import type { WorkContext } from "./authorization";
import type { SqlDatabase } from "./runtime/types";

// Recheck the selected grant against the authority tables, including expiry and
// revoked parents. A notification or an old WorkContext is never an access grant.
const eligibleAssignment = `a.scope_kind='holding' AND a.revoked_at IS NULL
  AND iam_role_allows(a.role,a.company_id,'organization.manage')
  AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
  AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)`;

export async function canReadOrganizationChanges(db:SqlDatabase,actor:WorkContext) {
  if (!hasPermission(actor,"organization.manage") || actor.assignmentScope!=="holding" || actor.scope.kind!=="holding") return false;
  return !!await db.prepare(`SELECT 1 FROM assignments a JOIN organization_nodes c ON c.id=a.company_id AND c.status='active'
    WHERE a.id=? AND a.user_id=? AND a.company_id=? AND a.role=? AND ${eligibleAssignment}`)
    .bind(actor.assignmentId,actor.userId,actor.companyId,actor.role).first();
}

export async function organizationChangeNotificationTarget(db:SqlDatabase,actor:WorkContext,id:string) {
  if (!await canReadOrganizationChanges(db,actor)) return null;
  return await db.prepare("SELECT 1 FROM organization_node_changes WHERE id=? AND company_id=?").bind(id,actor.companyId).first()
    ? organizationChangeHref(id) : null;
}

export async function organizationChangeWork(db:SqlDatabase,actor:WorkContext):Promise<WorkItem[]> {
  if (!await canReadOrganizationChanges(db,actor)) return [];
  const rows=(await db.prepare(`SELECT c.*,iam_same_person(c.requested_by,?,c.company_id) same_requester
    FROM organization_node_changes c WHERE c.company_id=?
    AND (c.requested_by=? OR c.state='submitted' OR c.reviewed_by=?) ORDER BY c.updated_at DESC,c.id`)
    .bind(actor.userId,actor.companyId,actor.userId,actor.userId).all<OrganizationNodeChange & {same_requester:boolean}>()).results;
  return rows.map(change=>{
    const own=change.requested_by===actor.userId;
    const review=change.state==='submitted'&&!change.same_requester&&!samePerson(actor,change.requested_by);
    const correct=own&&['draft','rejected'].includes(change.state);
    const completed=['applied','cancelled'].includes(change.state);
    return {
      id:change.id,type:'organization_change',title:`${change.action} ${change.node_kind}: ${change.display_name}`,
      detail:change.review_reason??change.reason,href:organizationChangeHref(change.id),status:change.state,updatedAt:change.updated_at,
      queue:completed?'completed':review||correct?'todo':'waiting',canAct:review||correct,
      responsibleQueue:completed?'Company HR':change.state==='submitted'?'Independent Company HR administrator':'Requester',
      nextAction:completed?'Read the retained decision history':review?'Review the proposed organization change':correct?'Correct and submit the organization change':'Wait for independent Company HR review',
      blocker:change.state==='submitted'&&!review?'An independent Company HR administrator must review this request.':null,
      relationships:own?['requested']:review?['approval']:['team'],
    };
  });
}

/** Notification delivery commits with the change and immutable decision event. */
export function organizationChangeNotice(db:SqlDatabase,companyId:string,id:string,key:string,event:'submitted'|'applied'|'rejected'|'cancelled') {
  const recipient=event==='submitted'
    ? "NOT iam_same_person(a.user_id,c.requested_by,c.company_id)"
    : event==='cancelled'
      ? "EXISTS(SELECT 1 FROM staff_notifications n WHERE n.company_id=c.company_id AND n.record_type='organization_change' AND n.record_id=c.id AND n.user_id=a.user_id) AND a.user_id<>c.requested_by"
      : "a.user_id=c.requested_by";
  const message={submitted:'Review organization change: ',applied:'Organization change applied: ',rejected:'Organization change returned for correction: ',cancelled:'Organization change cancelled: '}[event];
  return db.prepare(`INSERT INTO staff_notifications(id,company_id,user_id,record_type,record_id,record_version,message,created_at)
    SELECT DISTINCT ?||a.user_id,c.company_id,a.user_id,'organization_change',c.id,c.version,?||c.display_name,c.updated_at
    FROM organization_node_changes c JOIN assignments a ON a.company_id=c.company_id
    WHERE c.id=? AND c.company_id=? AND ${eligibleAssignment} AND ${recipient} ON CONFLICT DO NOTHING`)
    .bind(`org-change-notice-${key}-`,message,id,companyId);
}
