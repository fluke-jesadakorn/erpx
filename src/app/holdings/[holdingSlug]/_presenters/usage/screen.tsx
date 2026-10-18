import { UsageConsole } from "@/components/billing/usage/usage-console";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { administrationAccess } from "@/server/modules/runtime/public/administration";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { accessPage } from "@/server/modules/work_projection/public/page-error";

export const dynamic = "force-dynamic";

function currentBangkokMonth() {
  return new Date(Date.now() + 7 * 60 * 60 * 1000).toISOString().slice(0, 7);
}

export default async function UsagePage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ month?: string }>;
}) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const holdingOwner = actor.scope.kind === "holding" && (await administrationAccess(actor)).owner;
  const branchManager = actor.scope.kind === "branch" && actor.assignmentScope === "branch" && actor.role === "operations_manager";
  if (!holdingOwner && !branchManager) return accessPage("permission", holdingSlug);
  const monthInput = (await searchParams).month;
  const currentMonth = currentBangkokMonth();
  const month = monthInput && /^\d{4}-(0[1-9]|1[0-2])$/.test(monthInput) ? monthInput : currentMonth;
  return <UsageConsole key={month} base={workspaceBase(publicScope(actor))} mode={branchManager ? "branch" : "holding"} initialMonth={month} />;
}
