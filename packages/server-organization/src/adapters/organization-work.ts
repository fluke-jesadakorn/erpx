import {hasPermission} from "@erpx/feature-identity_access/lib/permissions";
import {samePerson} from "@erpx/feature-identity_access/lib/iam";
import type {OrganizationNodeChange} from "@erpx/feature-organization/lib/organization";
import type {WorkContext} from "@erpx/server-identity_access/public/authorization";
import type {SqlDatabase} from "@erpx/server-runtime/public/runtime/types";
import {organizationChangeAssignmentActive} from "@erpx/server-identity_access/public/organization-change-assignment";
import {createOrganizationWorkApplication} from "@erpx/server-organization/application/organization-work";
import {nativeOrgAll} from './native-organization-owner-write';
// Both reads are owner-local Company-keyed SELECTs over organization_node_changes.
// SQL is authoritative in erpx-server-organization::adapters::organization_owner_write
// and reached only by typed kind with positional values on the caller's unit.
function application(db:SqlDatabase,actor:WorkContext){return createOrganizationWorkApplication({canManage:hasPermission(actor,'organization.manage'),assignmentScope:actor.assignmentScope,scopeKind:actor.scope.kind,companyId:actor.companyId,userId:actor.userId,samePerson:(id)=>samePerson(actor,id),freshAssignment:()=>organizationChangeAssignmentActive(db,actor),exists:async(id)=>(await nativeOrgAll(db,actor.companyId,'org_work_read_change_exists',[id,actor.companyId])).length>0,rows:async()=>
  nativeOrgAll<OrganizationNodeChange & {company_id:string;same_requester:boolean}>(db,actor.companyId,'org_work_read_changes',[actor.userId,actor.companyId,actor.userId,actor.userId])
});}
export const canReadOrganizationChanges=(db:SqlDatabase,a:WorkContext)=>application(db,a).canRead();
export const organizationChangeNotificationTarget=(db:SqlDatabase,a:WorkContext,id:string)=>application(db,a).notificationTarget(id);
export const organizationChangeWork=(db:SqlDatabase,a:WorkContext)=>application(db,a).work();
