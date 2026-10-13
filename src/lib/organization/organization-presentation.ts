import type { HoldingPeopleDirectory } from "@/server/modules/organization/public/holding-people-directory";
import type { OrganizationExplorerContext, OrganizationSelection } from "@/components/people_payroll/organization-explorer";

export function organizationLabel(data: HoldingPeopleDirectory, selection: OrganizationSelection) {
  if (selection.kind === "person")
    return data.people.find((p) => p.engagementId === selection.id)?.legalName ?? "Employee";
  if (selection.kind === "position")
    return data.positions.find((p) => p.id === selection.id)?.title ?? "Position";
  if (selection.kind === "company" || selection.kind === "unassigned")
    return data.companies.find((c) => c.id === selection.id)?.name ?? "Company";
  if (selection.kind === "level")
    return data.levels.find((l) => l.id === selection.id)?.name ?? "Level";
  return data.nodes.find((n) => n.id === selection.id)?.name ?? "Unit";
}
export function organizationCompanyOf(data: HoldingPeopleDirectory, selection: OrganizationSelection) {
  if (selection.kind === "company" || selection.kind === "unassigned") return selection.id;
  if (selection.kind === "person")
    return data.people.find((p) => p.engagementId === selection.id)?.companyId;
  if (selection.kind === "position")
    return data.positions.find((p) => p.id === selection.id)?.companyId;
  if (selection.kind === "level") return data.levels.find((l) => l.id === selection.id)?.companyId;
  return data.nodes.find((n) => n.id === selection.id)?.companyId;
}

export function organizationLocationPath(data: HoldingPeopleDirectory, selection: OrganizationSelection) {
  const person = selection.kind === "person"
    ? data.people.find((item) => item.engagementId === selection.id)
    : undefined;
  const position = data.positions.find((item) =>
    item.id === (selection.kind === "position" ? selection.id : person?.positionId),
  );
  let node = data.nodes.find((item) => item.id === (
    ["branch", "department", "team"].includes(selection.kind)
      ? selection.id
      : position?.departmentId ?? person?.departmentId
  ));
  const companyId = organizationCompanyOf(data, selection);
  const nodes: HoldingPeopleDirectory["nodes"] = [];
  const visited = new Set<string>();
  while (node && node.kind !== "company" && node.companyId === companyId && !visited.has(node.id)) {
    visited.add(node.id);
    nodes.unshift(node);
    node = data.nodes.find((item) => item.id === node?.parentId);
  }
  const branchId = position?.branchId ?? person?.branchId ?? nodes[0]?.branchId;
  const branch = data.nodes.find((item) => item.id === branchId && item.companyId === companyId);
  if (branch && !visited.has(branch.id)) nodes.unshift(branch);
  const path: OrganizationSelection[] = companyId ? [{ kind: "company", id: companyId }] : [];
  path.push(...nodes.map((item) => ({
    kind: item.kind === "headquarters" ? "branch" as const : item.kind,
    id: item.id,
  })));
  if (position && selection.kind === "person") path.push({ kind: "position", id: position.id });
  if (!path.some((item) => item.kind === selection.kind && item.id === selection.id)) path.push(selection);
  return path;
}

function descendants(data: HoldingPeopleDirectory, id: string) {
  const ids = new Set([id]);
  let growing = true;
  while (growing) {
    growing = false;
    for (const node of data.nodes)
      if (node.parentId && ids.has(node.parentId) && !ids.has(node.id)) {
        ids.add(node.id);
        growing = true;
      }
  }
  return ids;
}
export function organizationContextFor(
  data: HoldingPeopleDirectory,
  selection: OrganizationSelection,
): OrganizationExplorerContext {
  const companyId = organizationCompanyOf(data, selection),
    ids = descendants(data, selection.id);
  const positions = data.positions.filter((p) =>
    selection.kind === "unassigned"
      ? false
      : selection.kind === "position"
        ? p.id === selection.id
        : selection.kind === "person"
          ? p.id === data.people.find((p) => p.engagementId === selection.id)?.positionId
          : selection.kind === "branch"
            ? p.branchId === selection.id
            : selection.kind === "department" || selection.kind === "team"
              ? !!p.departmentId && ids.has(p.departmentId)
              : selection.kind === "level"
                ? p.levelId === selection.id
                : p.companyId === companyId,
  );
  const people = data.people.filter((p) =>
    selection.kind === "person"
      ? p.engagementId === selection.id
      : selection.kind === "position"
        ? p.positionId === selection.id
        : selection.kind === "branch"
          ? p.branchId === selection.id
          : selection.kind === "department" || selection.kind === "team"
            ? !!p.departmentId && ids.has(p.departmentId)
            : selection.kind === "unassigned"
              ? p.companyId === companyId && !p.positionId
              : selection.kind === "level"
                ? positions.some((position) => position.id === p.positionId)
                : p.companyId === companyId,
  );
  return {
    company: data.companies.find((c) => c.id === companyId),
    people,
    positions,
    nodes: data.nodes.filter((n) => n.companyId === companyId),
    level: data.levels.find((l) => l.id === selection.id),
    directory: data,
  };
}
