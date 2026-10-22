import {organizationChangeHref,type OrganizationNodeChange} from "@/server/modules/organization/public/rules/organization-domain";
import type {WorkItem} from "@/lib/work_projection/work-item";
export function canReadOrganizationChangeScope(canManage:boolean,assignmentScope:string,scopeKind:string){return canManage&&assignmentScope==='holding'&&scopeKind==='holding';}
export function organizationChangeWorkItems(rows:(OrganizationNodeChange&{company_id:string;same_requester:boolean})[],authority:{userId:string;samePerson(id:unknown):boolean}):WorkItem[]{
  return rows.map(change=>{
    const own=change.requested_by===authority.userId;
    const review=change.state==='submitted'&&!change.same_requester&&!authority.samePerson(change.requested_by);
    const correct=own&&['draft','rejected'].includes(change.state);
    const completed=['applied','cancelled'].includes(change.state);
    return {
      id:change.id,type:'organization_change',title:`${change.action} ${change.node_kind}: ${change.display_name}`,
      detail:change.review_reason??change.reason,href:organizationChangeHref(change.id,change.company_id),status:change.state,updatedAt:change.updated_at,
      queue:completed?'completed':review||correct?'todo':'waiting',canAct:review||correct,
      responsibleQueue:completed?'Company HR':change.state==='submitted'?'Independent Company HR administrator':'Requester',
      nextAction:completed?'Read the retained decision history':review?'Review the proposed organization change':correct?'Correct and submit the organization change':'Wait for independent Company HR review',
      blocker:change.state==='submitted'&&!review?'An independent Company HR administrator must review this request.':null,
      relationships:own?['requested']:review?['approval']:['team'],
    };
  });

}
