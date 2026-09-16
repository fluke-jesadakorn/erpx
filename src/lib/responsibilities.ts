import { hasPermission, permissionForRoles } from "./permissions";
import type { WorkAssignment, WorkContext } from "@/server/authorization";
import { assignmentApplies } from "./workspace-access";
export function applicableAssignments(actor:Pick<WorkContext,"assignments"|"scope"|"assignmentId"|"role">):WorkAssignment[] {
 return actor.assignments.filter(a=>a.id===actor.assignmentId && a.role===actor.role && assignmentApplies(a,actor.scope));
}
export function responsibilityRoles(actor:WorkContext){return [actor.role];}
export function hasBusinessRole(actor:WorkContext,roles:readonly string[]){return hasPermission(actor,permissionForRoles(roles));}
export function responsibilityContext(actor:WorkContext,roles:readonly string[]):WorkContext|null {return hasPermission(actor,permissionForRoles(roles))?actor:null;}
export function responsibilityContexts(actor:WorkContext){return [actor];}
