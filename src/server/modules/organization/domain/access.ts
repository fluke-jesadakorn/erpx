import { reportingIssues, type OrganizationPerson, type OrganizationPosition } from "@/server/modules/organization/public/rules/organization-domain";
import type { OrganizationAccessData } from "@/lib/organization-access";
import { BusinessRuleError } from "../../shared";

export type OrganizationAccessActor = { actorId: string; branchId: string | null; departmentId: string | null; scopeKind: string; assignmentScope: string; mayReadIam: boolean; mayManagePeople: boolean; mayManageOrganization: boolean };
export type OrganizationAccessSnapshot = { people: OrganizationPerson[]; positions: OrganizationPosition[]; nodes: OrganizationAccessData["nodes"]; levels: OrganizationAccessData["levels"] };

export function assertOrganizationAccess(actor: OrganizationAccessActor): void {
  if (!actor.mayReadIam && !actor.mayManagePeople && !actor.mayManageOrganization) throw new BusinessRuleError(403, "PERMISSION_REQUIRED", "Your selected role cannot view organization and access.");
}

/** Placement readers can see the reporting graph; only HR can see private contact/dates. */
export function projectOrganizationAccess(actor: OrganizationAccessActor, snapshot: OrganizationAccessSnapshot, today: string): OrganizationAccessData {
  const inScope = (item: { branch_id: string | null; department_id: string | null }) => (!actor.branchId || item.branch_id === actor.branchId) && (!actor.departmentId || item.department_id === actor.departmentId);
  const company = actor.scopeKind === "holding" && actor.assignmentScope === "holding" && !actor.branchId && !actor.departmentId;
  return {
    positions: snapshot.positions.filter(position => position.active === 1 && inScope(position)),
    people: snapshot.people.filter(inScope).map(person => ({ ...person, email: actor.mayManagePeople ? person.email : null, start_date: actor.mayManagePeople ? person.start_date : "", end_date: actor.mayManagePeople ? person.end_date : null, issues: reportingIssues(person, snapshot.people, snapshot.positions, today) })),
    nodes: snapshot.nodes, levels: snapshot.levels,
    canManage: company && actor.mayManageOrganization && actor.mayManagePeople,
    canMovePeople: company && actor.mayManagePeople, actorId: actor.actorId,
  };
}
