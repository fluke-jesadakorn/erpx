import {hasPermission} from '@/lib/permissions';import type {WorkContext} from '../../identity_access/public/authorization';import type {holdingPeopleDirectory,archivedPeopleDirectory} from '../../organization/public/holding-people-directory';import type {organizationAccessData} from '../../organization/public/organization-access';
export interface PeopleWorkspacePorts{directory(actor:WorkContext):ReturnType<typeof holdingPeopleDirectory>;archived(actor:WorkContext):ReturnType<typeof archivedPeopleDirectory>;prepare(actor:WorkContext):Promise<void>;arrangement(actor:WorkContext):ReturnType<typeof organizationAccessData>;}
export function createPeopleWorkspaceApplication(ports:PeopleWorkspacePorts){/** Public graph stays complete for search/counts; HR sections follow the opened view. */
async function peopleWorkspaceData(actor: WorkContext, query: { status?: string } = {}) {
  const canManage = hasPermission(actor, "people.manage");
  const canManageStructure = canManage && hasPermission(actor, "organization.manage")
    && actor.assignmentScope === "holding" && actor.scope.kind === "holding"
    && !actor.branchId && !actor.departmentId;
  const [directory, archivedPeople] = await Promise.all([
    ports.directory(actor),
    canManage && query.status === "archived"
      ? ports.archived(actor) : Promise.resolve([]),
  ]);
  await ports.prepare(actor);
  const accessOrganization = canManageStructure
    ? await ports.arrangement(actor) : null;
  return { directory, archivedPeople, accessOrganization, canManage, canManageStructure };
}
return {peopleWorkspaceData};}
