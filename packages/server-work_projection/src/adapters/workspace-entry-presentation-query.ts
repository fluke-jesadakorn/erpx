import { parseHoldingWorkspace, workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import "server-only";
import { runtimeEnv, activeHoldingBindings } from "../../../../packages/server-runtime/src/public/runtime";
import { readWorkspacePreference, readCompanyPreference } from "@erpx/server-work_projection/public/workspace-preferences";
export async function savedWorkspaceForPresentation(userId: string) {
  return readWorkspacePreference((await runtimeEnv()).CONTROL_DB, userId);
}

/** Account memory is only a selection hint; Apps intersects it with fresh launch choices. */
export async function savedAppsWorkspaceForPresentation(userId: string) {
  const db = (await runtimeEnv()).CONTROL_DB;
  const saved = await readWorkspacePreference(db, userId);
  const parts = saved?.workspace_path?.split("/").filter(Boolean);
  const parsed = parts?.[0] === "holdings" ? parseHoldingWorkspace(parts[1], parts.slice(2)) : null;
  if (!parsed) return null;
  const binding = (await activeHoldingBindings()).find(item => item.slug === parsed.scope.holdingSlug);
  const company = binding ? await readCompanyPreference(db, userId, binding.id) : null;
  return { companyId: JSON.stringify([parsed.scope.holdingSlug, company?.company_id ?? ""]), workspaceId: workspaceBase(parsed.scope) };
}
