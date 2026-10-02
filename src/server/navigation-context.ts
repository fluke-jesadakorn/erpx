import "server-only";
import { executiveVisible } from "./executive/access";
import { holdingResources } from "./runtime";
import { administrationAccess } from "./administration";
import { publicScope, type WorkContext } from "./authorization";
import { workspaceBase } from "@/lib/workspace";
import { financeWorkspaceV2Enabled } from "./finance-presentation";
export async function navigationContext(actor: WorkContext) {
  const admin = await administrationAccess(actor);
  const {write}=await holdingResources(actor.holdingId);
  return { workspaceBase: workspaceBase(publicScope(actor)), scopeKind: actor.scope.kind,
    financeWorkspaceV2: financeWorkspaceV2Enabled(),
    executiveVisible: await executiveVisible(write,actor),
    beta: actor.beta, role: actor.role, permissions: actor.iam?.permissions,
    adminSections: admin.sections, canManageApprovals: admin.manageOrganization };
}
