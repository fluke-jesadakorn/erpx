import { canonicalUiHref } from '@erpx/feature-work_projection/lib/destination-registry';
import {parseHoldingWorkspace, type WorkspaceScope} from '@erpx/contracts/workspace-scope';
export {parseHoldingWorkspace} from '@erpx/contracts/workspace-scope';
export type {WorkspaceScope, HoldingWorkspaceScope} from '@erpx/contracts/workspace-scope';

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

export function workspaceKey(scope: WorkspaceScope) {
  return workspaceBase(scope).slice(1).replaceAll("/", ":");
}
