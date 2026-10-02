import "server-only";

import type { WorkContext } from "./authorization";
import { ApiError } from "./http";
import { requirePermission } from "./iam-permissions";
import { holdingPeopleDirectory, type HoldingPeopleDirectory } from "./holding-people-directory";
import { organizationAccessData } from "./organization-access";
import { holdingResources } from "./runtime";
import type { OrganizationAccessData } from "@/lib/organization-access";

export type PeopleMoveKind = "person" | "position" | "department";
export type PeopleMoveTargetKind = "position" | "department" | "level" | "branch";

export type PeopleMoveContext = {
  directory: HoldingPeopleDirectory;
  source: { kind: PeopleMoveKind; id: string; label: string; companyId: string };
  company: HoldingPeopleDirectory["companies"][number];
  accessOrganization: OrganizationAccessData;
};

function isMoveKind(value: unknown): value is PeopleMoveKind {
  return value === "person" || value === "position" || value === "department";
}

/** Load a current HR move source only after checking Company-level authority. */
export async function loadPeopleMove(
  actor: WorkContext,
  kind: unknown,
  id: unknown,
): Promise<PeopleMoveContext> {
  if (!isMoveKind(kind) || typeof id !== "string" || !id.trim()) {
    throw new ApiError(422, "MOVE_SOURCE_REQUIRED", "Choose a person, position, or department to move.");
  }
  requirePermission(actor, "people.manage");
  requirePermission(actor, "organization.manage");
  if (actor.scope.kind !== "holding" || actor.assignmentScope !== "holding" || actor.branchId || actor.departmentId) {
    throw new ApiError(403, "COMPANY_AUTHORITY_REQUIRED", "Open the Company workspace with HR authority to move organization items.");
  }

  // The directory verifies the active assignment and is bound to this Holding.
  const directory = await holdingPeopleDirectory(actor);
  const publicSource = kind === "person"
    ? directory.people.find(item => item.id === id)
    : kind === "position"
      ? directory.positions.find(item => item.id === id)
      : directory.nodes.find(item => item.id === id && item.kind === "department");
  if (!publicSource || publicSource.companyId !== actor.companyId) {
    throw new ApiError(404, "MOVE_SOURCE_NOT_FOUND", "This item is not available in your Company.");
  }
  const company = directory.companies.find(item => item.id === publicSource.companyId);
  if (!company) throw new ApiError(409, "MOVE_SOURCE_CHANGED", "The Company changed. Refresh and choose the item again.");

  const { write } = await holdingResources(actor.holdingId);
  const accessOrganization = await organizationAccessData(write, actor);
  if (!accessOrganization.canManage) {
    throw new ApiError(403, "COMPANY_AUTHORITY_REQUIRED", "Your selected role cannot move organization items in this Company.");
  }
  const currentSource = kind === "person"
    ? accessOrganization.people.some(item => item.engagement_id === id && item.status !== "ended")
    : kind === "position"
      ? accessOrganization.positions.some(item => item.id === id && item.active)
      : accessOrganization.nodes.some(item => item.id === id && item.kind === "department" && item.status === "active");
  if (!currentSource) throw new ApiError(409, "MOVE_SOURCE_CHANGED", "This item changed. Refresh and choose it again.");

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
}

/** UI validation only. Preview and submit commands recheck current state and authority. */
export function validatePeopleMoveTarget(
  context: PeopleMoveContext,
  targetKind: unknown,
  targetId: unknown,
): { valid: boolean; reason?: string; targetName?: string } {
  if (typeof targetId !== "string" || !targetId.trim()) return { valid: false, reason: "Choose a destination." };
  const { source, directory } = context;
  if (source.kind === "person" && targetKind === "position") {
    const person = directory.people.find(item => item.id === source.id);
    const target = directory.positions.find(item => item.id === targetId);
    if (!target || target.companyId !== source.companyId) return { valid: false, reason: "Choose an active position in this Company." };
    if (person?.positionId === target.id) return { valid: false, reason: "This person already holds that position." };
    return { valid: true, targetName: target.title };
  }
  if (source.kind === "position" && targetKind === "department") {
    const position = directory.positions.find(item => item.id === source.id);
    const target = directory.nodes.find(item => item.id === targetId && item.kind === "department");
    if (!target || target.companyId !== source.companyId) return { valid: false, reason: "Choose an active department in this Company." };
    if (!directory.nodes.some(item => item.id === target.branchId && item.kind === "branch")) {
      return { valid: false, reason: "This department is not attached to an active branch." };
    }
    if (position?.departmentId === target.id) return { valid: false, reason: "This position is already in that department." };
    return { valid: true, targetName: target.name };
  }
  if (source.kind === "position" && targetKind === "level") {
    const position = directory.positions.find(item => item.id === source.id);
    const target = directory.levels.find(item => item.id === targetId);
    if (!target || target.companyId !== source.companyId || target.number === null || target.number < 1 || target.number > 6) {
      return { valid: false, reason: "Choose one of the six active Levels in this Company." };
    }
    if (position?.levelId === target.id) return { valid: false, reason: "This position already has that Level." };
    return { valid: true, targetName: `${target.code} · ${target.name}` };
  }
  if (source.kind === "department" && targetKind === "branch") {
    const department = directory.nodes.find(item => item.id === source.id && item.kind === "department");
    const target = directory.nodes.find(item => item.id === targetId && item.kind === "branch");
    if (!target || target.companyId !== source.companyId) return { valid: false, reason: "Choose an active branch in this Company." };
    if (department?.branchId === target.id) return { valid: false, reason: "This department is already in that branch." };
    if (department && directory.nodes.some(item => item.kind === "department" && item.companyId === source.companyId && item.branchId === target.id && item.slug === department.slug)) {
      return { valid: false, reason: "A department with this URL code already exists in that branch." };
    }
    return { valid: true, targetName: target.name };
  }
  return { valid: false, reason: "That destination type cannot receive this item." };
}
