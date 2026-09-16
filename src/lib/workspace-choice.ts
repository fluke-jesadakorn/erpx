import { workspaceHref } from "./workspace";

export type WorkspaceDepartmentChoice = {
  kind: "department";
  id: string;
  label: string;
  destination: string;
};

export type WorkspaceBranchChoice = {
  kind: "branch";
  id: string;
  label: string;
  destination?: string;
  departments: WorkspaceDepartmentChoice[];
};

export type WorkspaceCompanyChoice = {
  id: string;
  label: string;
  destination?: string;
  switchEndpoint?: string;
  branches: WorkspaceBranchChoice[];
  departments: WorkspaceDepartmentChoice[];
};

export type WorkspaceHoldingChoice = {
  kind: "holding";
  holdingSlug: string;
  label: string;
  companies: WorkspaceCompanyChoice[];
};

export type WorkspacePlatformChoice = {
  kind: "platform";
  label: string;
  destination: string;
};

export type WorkspaceChoice = WorkspaceHoldingChoice | WorkspacePlatformChoice;

export type CurrentWorkspaceChoice =
  | { kind: "platform" }
  | {
      kind: "holding";
      holdingSlug: string;
      companyId: string;
    }
  | {
      kind: "branch";
      holdingSlug: string;
      companyId: string;
      branchId: string;
    }
  | {
      kind: "department";
      holdingSlug: string;
      companyId: string;
      branchId: string | null;
      departmentId: string;
    };

export type WorkspaceChoiceAssignment = {
  companyId: string;
  companyName: string;
  scopeKind: "holding" | "branch" | "department";
  scopeNodeId: string | null;
};

export type WorkspaceChoiceNode = {
  id: string;
  kind: "branch" | "department";
  slug: string;
  label: string;
  companyId: string;
  branchId: string | null;
  branchSlug: string | null;
};

function compareLabels(a: { label: string; id?: string }, b: { label: string; id?: string }) {
  return a.label.localeCompare(b.label, undefined, { sensitivity: "base" }) ||
    (a.id ?? "").localeCompare(b.id ?? "");
}

/**
 * Builds the serializable workspace tree from already-authorized evidence.
 * Holding grants expose the Company and its Branches; Branch grants expose the
 * Branch; direct Department grants are shown only when no selectable parent
 * already represents the same access.
 */
export function buildWorkspaceHoldingChoice(
  holdingSlug: string,
  holdingLabel: string,
  assignments: readonly WorkspaceChoiceAssignment[],
  nodes: readonly WorkspaceChoiceNode[],
): WorkspaceHoldingChoice | null {
  const companies = new Map<string, WorkspaceCompanyChoice>();
  const companyAssignments = new Map<string, WorkspaceChoiceAssignment[]>();

  for (const assignment of assignments) {
    const rows = companyAssignments.get(assignment.companyId) ?? [];
    rows.push(assignment);
    companyAssignments.set(assignment.companyId, rows);
  }

  for (const [companyId, grants] of companyAssignments) {
    const companyName = grants[0]?.companyName;
    if (!companyName) continue;
    const holdingGrant = grants.some((grant) => grant.scopeKind === "holding");
    const branchGrantIds = new Set(
      grants
        .filter((grant) => grant.scopeKind === "branch" && grant.scopeNodeId)
        .map((grant) => grant.scopeNodeId as string),
    );
    const departmentGrantIds = new Set(
      grants
        .filter((grant) => grant.scopeKind === "department" && grant.scopeNodeId)
        .map((grant) => grant.scopeNodeId as string),
    );
    const companyNodes = nodes.filter((node) => node.companyId === companyId);
    const branchNodes = companyNodes.filter((node) => node.kind === "branch");
    const departmentNodes = companyNodes.filter(
      (node) => node.kind === "department",
    );
    const visibleBranchIds = new Set(
      branchNodes
        .filter((branch) => holdingGrant || branchGrantIds.has(branch.id))
        .map((branch) => branch.id),
    );
    const fallbackDepartments = departmentNodes.filter(
      (department) =>
        departmentGrantIds.has(department.id) &&
        (!department.branchId || !visibleBranchIds.has(department.branchId)),
    );
    const fallbackBranchIds = new Set(
      fallbackDepartments
        .map((department) => department.branchId)
        .filter((id): id is string => !!id),
    );

    const branches = branchNodes
      .filter(
        (branch) =>
          visibleBranchIds.has(branch.id) || fallbackBranchIds.has(branch.id),
      )
      .map((branch): WorkspaceBranchChoice => {
        const selectable = visibleBranchIds.has(branch.id);
        return {
          kind: "branch",
          id: branch.id,
          label: branch.label,
          ...(selectable
            ? {
                destination: workspaceHref({
                  kind: "branch",
                  holdingSlug,
                  branchSlug: branch.slug,
                }),
              }
            : {}),
          departments: fallbackDepartments
            .filter((department) => department.branchId === branch.id)
            .map((department) => ({
              kind: "department" as const,
              id: department.id,
              label: department.label,
              destination: workspaceHref({
                kind: "department",
                holdingSlug,
                branchSlug: department.branchSlug ?? branch.slug,
                departmentSlug: department.slug,
              }),
            }))
            .sort(compareLabels),
        };
      })
      .sort(compareLabels);

    const directDepartments = fallbackDepartments
      .filter((department) => !department.branchId)
      .map((department) => ({
        kind: "department" as const,
        id: department.id,
        label: department.label,
        destination: workspaceHref({
          kind: "department",
          holdingSlug,
          departmentSlug: department.slug,
        }),
      }))
      .sort(compareLabels);

    companies.set(companyId, {
      id: companyId,
      label: companyName,
      ...(holdingGrant
        ? {
            destination: workspaceHref({ kind: "holding", holdingSlug }),
            switchEndpoint: `/api/v1/holdings/${encodeURIComponent(holdingSlug)}/companies/active`,
          }
        : {}),
      branches,
      departments: directDepartments,
    });
  }

  const sortedCompanies = [...companies.values()].sort(compareLabels);
  return sortedCompanies.length
    ? {
        kind: "holding",
        holdingSlug,
        label: holdingLabel,
        companies: sortedCompanies,
      }
    : null;
}
