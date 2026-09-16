import { headers } from "next/headers";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { createAuth } from "@/server/auth";
import {
  requirePlatformContext,
  requireAccountAccess,
  assignmentApplies,
  firstAuthorizedWorkspace,
} from "@/server/authorization";
import { readWorkspacePreference } from "@/server/workspace-preferences";
import { activeHoldingBindings, runtimeEnv } from "@/server/runtime";
import { parseHoldingWorkspace, workspaceHref } from "@/lib/workspace";

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
    query.from.startsWith("/holdings/")
      ? query.from
      : undefined;
  const saved = await readWorkspacePreference((await runtimeEnv()).CONTROL_DB,session.user.id);
  const preferred = saved?.workspace_path ?? (accountSwitch ? switchSource : (await cookies()).get("erpx_last_workspace")?.value);
  if (preferred?.startsWith("/platform/")) {
    try {
      await requirePlatformContext();
      redirect(preferred);
    } catch (error) {
      if ((error as { digest?: string }).digest) throw error;
    }
  }
  if (preferred?.startsWith("/holdings/")) {
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
  try { await requirePlatformContext(); } catch { redirect("/onboarding"); }
  redirect("/platform/home");
}
