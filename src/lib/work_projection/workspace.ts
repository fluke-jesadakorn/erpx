import { canonicalUiHref } from './destination-registry';
export type WorkspaceScope =
  | { kind: "platform" }
  | { kind: "holding"; holdingSlug: string }
  | { kind: "branch"; holdingSlug: string; branchSlug: string }
  | {
      kind: "department";
      holdingSlug: string;
      departmentSlug: string;
      branchSlug?: string;
    };

export type HoldingWorkspaceScope = Exclude<
  WorkspaceScope,
  { kind: "platform" }
>;

export function workspaceBase(scope: WorkspaceScope) {
  if (scope.kind === "platform") return "/platform";
  const holding = canonicalUiHref(`/holdings/${encodeURIComponent(scope.holdingSlug)}`);
  if (scope.kind === "holding") return holding;
  if (scope.kind === "branch")
    return `${holding}/branches/${encodeURIComponent(scope.branchSlug)}`;
  const branch = scope.branchSlug
    ? `/branches/${encodeURIComponent(scope.branchSlug)}`
    : "";
  return `${holding}${branch}/departments/${encodeURIComponent(scope.departmentSlug)}`;
}

export function workspaceHref(scope: WorkspaceScope, destination = "home") {
  return canonicalUiHref(`${workspaceBase(scope)}/${destination.replace(/^\/+/, "")}`);
}

export function workspaceApiHref(
  scope: WorkspaceScope,
  resource: string,
) {
  return `/api/v1${workspaceBase(scope)}/${resource.replace(/^\/+/, "")}`;
}

export function parseHoldingWorkspace(
  holdingSlug: string,
  segments: readonly string[],
): { scope: HoldingWorkspaceScope; destination: string[] } | null {
  if (segments[0] === "branches" && segments[1]) {
    if (segments[2] === "departments" && segments[3])
      return {
        scope: {
          kind: "department",
          holdingSlug,
          branchSlug: segments[1],
          departmentSlug: segments[3],
        },
        destination: segments.slice(4),
      };
    return {
      scope: { kind: "branch", holdingSlug, branchSlug: segments[1] },
      destination: segments.slice(2),
    };
  }
  if (segments[0] === "departments" && segments[1])
    return {
      scope: {
        kind: "department",
        holdingSlug,
        departmentSlug: segments[1],
      },
      destination: segments.slice(2),
    };
  return {
    scope: { kind: "holding", holdingSlug },
    destination: [...segments],
  };
}

export function workspaceKey(scope: WorkspaceScope) {
  return workspaceBase(scope).slice(1).replaceAll("/", ":");
}
