import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import type {OrganizationNodeChange} from "@erpx/feature-organization/lib/organization";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {organizationChangeAssignmentActive} from "@erpx/server-identity_access/public/organization-change-assignment";
import {createOrganizationWorkApplication} from "@erpx/server-organization/application/organization-work";
export {organizationChangeNotice} from "@erpx/server-collaboration/public/organization-change-notice";
function application(db:SqlDatabase,actor:WorkContext){return createOrganizationWorkApplication({canManage:hasPermission(actor,'organization.manage'),assignmentScope:actor.assignmentScope,scopeKind:actor.scope.kind,companyId:actor.companyId,userId:actor.userId,samePerson:(id)=>samePerson(actor,id),freshAssignment:()=>organizationChangeAssignmentActive(db,actor),exists:async(id)=>!!await db.prepare('SELECT 1 FROM organization_node_changes WHERE id=? AND company_id=?').bind(id,actor.companyId).first(),rows:async()=>{
  return (await db.prepare(`SELECT c.*,iam_same_person(c.requested_by,?,c.company_id) same_requester
    FROM organization_node_changes c WHERE c.company_id=?
    AND (c.requested_by=? OR c.state='submitted' OR c.reviewed_by=?) ORDER BY c.updated_at DESC,c.id`)
    .bind(actor.userId,actor.companyId,actor.userId,actor.userId).all<OrganizationNodeChange & {company_id:string;same_requester:boolean}>()).results;
} });}
export const canReadOrganizationChanges=(db:SqlDatabase,a:WorkContext)=>application(db,a).canRead();
export const organizationChangeNotificationTarget=(db:SqlDatabase,a:WorkContext,id:string)=>application(db,a).notificationTarget(id);
export const organizationChangeWork=(db:SqlDatabase,a:WorkContext)=>application(db,a).work();
