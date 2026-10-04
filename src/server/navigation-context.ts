import "server-only";
import { executiveVisible } from "./executive/access";
import { holdingResources } from "./runtime";
import { administrationAccess } from "./administration";
import { publicScope, type WorkContext } from "./authorization";
import { workspaceBase } from "@/lib/workspace";
import { financeWorkspaceV2Enabled } from "./finance-presentation";
import { companyFeatureCatalogue } from "./hospitality-rollout";
import { availableNavigationAppIds } from "@/lib/workplace-catalogue";
export async function navigationContext(actor: WorkContext) {
  const admin = await administrationAccess(actor);
  const {write}=await holdingResources(actor.holdingId);
  return { workspaceBase: workspaceBase(publicScope(actor)), scopeKind: actor.scope.kind,
    moduleIds: actor.modules,
    ...(process.env.NODE_ENV === "production" ? { availableAppIds: availableNavigationAppIds(await companyFeatureCatalogue(actor)) } : {}),
    financeWorkspaceV2: financeWorkspaceV2Enabled(),
    executiveVisible: await executiveVisible(write,actor),
    beta: actor.beta, role: actor.role, permissions: actor.iam?.permissions,
    adminSections: admin.sections, canManageApprovals: admin.manageOrganization };
}
