import type { PageProps } from "@erpx/contracts/next-routes";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { headers } from "next/headers";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { createAuth } from "@erpx/server-identity_access/public/auth";
import {
  requireManageErpxContext,
  requireAccountAccess,
  assignmentApplies,
  firstAuthorizedWorkspace,
} from "@erpx/server-identity_access/public/authorization";

import { activeHoldingBindings } from "../../../../packages/server-runtime/src/public/runtime";
import { parseHoldingWorkspace, workspaceHref } from "@erpx/feature-work_projection/lib/workspace";

import { savedWorkspaceForPresentation } from "@erpx/server-work_projection/public/workspace-entry-presentation-query";

export const dynamic = "force-dynamic";

export default async function RootPage({
  searchParams,
}: PageProps<"/">) {
  const session = await (await createAuth()).api.getSession({ headers: await headers() });
  if (!session) redirect("/login");

  const query = await searchParams;
  // Prefer this account's persisted area, including after an account switch.
  // Older browser state is only a fallback and is checked against current grants.
  const accountSwitch = query?.account === "switch";
  const switchSource =
    accountSwitch &&
    typeof query?.from === "string" &&
    query.from.startsWith(canonicalUiHref("/holdings/"))
      ? query.from
      : undefined;
  const saved = await savedWorkspaceForPresentation(session.user.id);
  const remembered = saved?.workspace_path ?? (accountSwitch ? switchSource : (await cookies()).get("erpx_last_workspace")?.value);
  const preferred=remembered?canonicalUiHref(remembered):undefined;
  if (preferred?.startsWith(canonicalUiHref("/holdings/"))) {
    const parts = preferred.split("/").filter(Boolean);
    const parsed = parseHoldingWorkspace(parts[1] ?? "", parts.slice(2));
    if (parsed) {
      try {
        const account=await requireAccountAccess(parsed.scope);
        if(account.assignments.some(assignment=>assignmentApplies(assignment,account.scope)))redirect(preferred);
      } catch (error) {
        if ((error as { digest?: string }).digest) throw error;
      }
    }
  }
  for (const binding of await activeHoldingBindings()) {
    const workspace = await firstAuthorizedWorkspace(binding.slug);
    if (workspace) redirect(workspaceHref(workspace));
  }
  try { await requireManageErpxContext(); } catch { redirect("/onboarding"); }
  redirect(canonicalUiHref("/holdings/erpx/manage-erpx"));
}
