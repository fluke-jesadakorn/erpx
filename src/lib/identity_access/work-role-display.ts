import type {WorkAssignment} from '@/server/modules/identity_access/public/authorization';

/** Display and normal selection hints only. This carries no effective module,
 * record, permission, validity or authority snapshot into a Client Component. */
export type WorkRoleDisplayChoice=Pick<WorkAssignment,'id'|'role'|'scopeKind'|'companyId'|'version'|'label'>;
export type PlatformRoleDisplayChoice=Pick<WorkAssignment,'id'|'role'>;
export function workRoleDisplayChoices(assignments:readonly WorkRoleDisplayChoice[]):WorkRoleDisplayChoice[]{
 return assignments.map(assignment=>({id:assignment.id,role:assignment.role,scopeKind:assignment.scopeKind,companyId:assignment.companyId,
  ...(assignment.version===undefined?{}:{version:assignment.version}),...(assignment.label===undefined?{}:{label:assignment.label})}));
}
export function platformRoleDisplayChoices(assignments:readonly PlatformRoleDisplayChoice[]):PlatformRoleDisplayChoice[]{
 return assignments.map(assignment=>({id:assignment.id,role:assignment.role}));
}
