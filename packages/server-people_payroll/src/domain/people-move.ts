import type {HoldingPeopleDirectory} from '@erpx/server-organization/public/holding-people-directory';import type {OrganizationAccessData} from "@erpx/feature-organization/lib/organization-access";
export type PeopleMoveKind = "person" | "position" | "department";
export type PeopleMoveTargetKind = "position" | "department" | "level" | "branch";
export type PeopleMoveContext = {
  directory: HoldingPeopleDirectory;
  source: { kind: PeopleMoveKind; id: string; label: string; companyId: string };
  company: HoldingPeopleDirectory["companies"][number];
  accessOrganization: OrganizationAccessData;
};
export function isMoveKind(value: unknown): value is PeopleMoveKind {
  return value === "person" || value === "position" || value === "department";
}
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

