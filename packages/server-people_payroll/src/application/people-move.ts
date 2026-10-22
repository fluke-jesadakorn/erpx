import type {WorkContext} from '@erpx/server-identity_access/public/authorization';import {BusinessRuleError} from '@erpx/server-shared/index';import {isMoveKind,type PeopleMoveContext} from '@erpx/server-people_payroll/domain/people-move';import type {HoldingPeopleDirectory,holdingPeopleDirectory} from '@erpx/server-organization/public/holding-people-directory';import type {organizationAccessData} from '@erpx/server-organization/public/organization-access';
export interface PeopleMovePorts{authorize(actor:WorkContext,permission:string):unknown;directory(actor:WorkContext):ReturnType<typeof holdingPeopleDirectory>;arrangement(actor:WorkContext):ReturnType<typeof organizationAccessData>;}
export function createPeopleMoveApplication(ports:PeopleMovePorts){async function loadPeopleMove(
  actor: WorkContext,
  kind: unknown,
  id: unknown,
): Promise<PeopleMoveContext> {
  if (!isMoveKind(kind) || typeof id !== "string" || !id.trim()) {
    throw new BusinessRuleError(422, "MOVE_SOURCE_REQUIRED", "Choose a person, position, or department to move.");
  }
  ports.authorize(actor, "people.manage");
  ports.authorize(actor, "organization.manage");
  if (actor.scope.kind !== "holding" || actor.assignmentScope !== "holding" || actor.branchId || actor.departmentId) {
    throw new BusinessRuleError(403, "COMPANY_AUTHORITY_REQUIRED", "Open the Company workspace with HR authority to move organization items.");
  }

  // The directory verifies the active assignment and is bound to this Holding.
  const directory = await ports.directory(actor);
  const publicSource = kind === "person"
    ? directory.people.find(item => item.id === id)
    : kind === "position"
      ? directory.positions.find(item => item.id === id)
      : directory.nodes.find(item => item.id === id && item.kind === "department");
  if (!publicSource || publicSource.companyId !== actor.companyId) {
    throw new BusinessRuleError(404, "MOVE_SOURCE_NOT_FOUND", "This item is not available in your Company.");
  }
  const company = directory.companies.find(item => item.id === publicSource.companyId);
  if (!company) throw new BusinessRuleError(409, "MOVE_SOURCE_CHANGED", "The Company changed. Refresh and choose the item again.");

  const accessOrganization = await ports.arrangement(actor);
  if (!accessOrganization.canManage) {
    throw new BusinessRuleError(403, "COMPANY_AUTHORITY_REQUIRED", "Your selected role cannot move organization items in this Company.");
  }
  const currentSource = kind === "person"
    ? accessOrganization.people.some(item => item.engagement_id === id && item.status !== "ended")
    : kind === "position"
      ? accessOrganization.positions.some(item => item.id === id && item.active)
      : accessOrganization.nodes.some(item => item.id === id && item.kind === "department" && item.status === "active");
  if (!currentSource) throw new BusinessRuleError(409, "MOVE_SOURCE_CHANGED", "This item changed. Refresh and choose it again.");

  const label = kind === "person"
    ? (publicSource as HoldingPeopleDirectory["people"][number]).legalName
    : kind === "position"
      ? (publicSource as HoldingPeopleDirectory["positions"][number]).title
      : (publicSource as HoldingPeopleDirectory["nodes"][number]).name;
  return {
    directory,
    source: { kind, id, label, companyId: publicSource.companyId },
    company,
    accessOrganization,
  };
}return {loadPeopleMove};}
