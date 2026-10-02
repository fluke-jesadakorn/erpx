import "server-only";
import { hasPermission } from "@/lib/permissions";
import type { WorkContext } from "./authorization";
import { archivedPeopleDirectory, holdingPeopleDirectory } from "./holding-people-directory";
import { organizationAccessData } from "./organization-access";
import { holdingResources } from "./runtime";
import { measureReadPhase } from "./runtime/read-metrics";

/** Public graph stays complete for search/counts; HR sections follow the opened view. */
export async function peopleWorkspaceData(actor: WorkContext, query: { status?: string } = {}) {
  const canManage = hasPermission(actor, "people.manage");
  const canManageStructure = canManage && hasPermission(actor, "organization.manage")
    && actor.assignmentScope === "holding" && actor.scope.kind === "holding"
    && !actor.branchId && !actor.departmentId;
  const [directory, archivedPeople] = await Promise.all([
    measureReadPhase("people.directory", () => holdingPeopleDirectory(actor)),
    canManage && query.status === "archived"
      ? measureReadPhase("people.archived", () => archivedPeopleDirectory(actor)) : Promise.resolve([]),
  ]);
  const { write } = await holdingResources(actor.holdingId);
  const accessOrganization = canManageStructure
    ? await measureReadPhase("people.arrangement", () => organizationAccessData(write, actor)) : null;
  return { directory, archivedPeople, accessOrganization, canManage, canManageStructure };
}
