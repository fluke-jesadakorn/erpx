import { headers } from "next/headers";
import { cookies } from "next/headers";
import { redirect } from "next/navigation";
import { createAuth } from "@/server/auth";
import {
  requirePlatformContext,
  requireWorkContext,
  firstAuthorizedWorkspace,
} from "@/server/authorization";
import { activeHoldingBindings } from "@/server/runtime";
import { parseHoldingWorkspace, workspaceHref } from "@/lib/workspace";

export const dynamic = "force-dynamic";

export default async function RootPage({
  searchParams,
}: PageProps<"/">) {
  const session = await (await createAuth()).api.getSession({ headers: await headers() });
  if (!session) redirect("/login");

  const query = await searchParams;
  // Account switches must resolve to a clean holding home. The last page can
  // be a branch or inbox in the previous account, and reusing it can send a
  // valid account into a different scope (or an intentional 404).
  const accountSwitch = query?.account === "switch";
  const switchSource =
    accountSwitch &&
    typeof query?.from === "string" &&
    query.from.startsWith("/holdings/")
      ? query.from
      : undefined;
  const preferred = accountSwitch
    ? switchSource
    : (await cookies()).get("erpx_last_workspace")?.value;
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
        await requireWorkContext(parsed.scope);
        redirect(preferred);
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
