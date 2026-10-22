import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { redirect } from "next/navigation";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
export default async function Tasks({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ queue?: string; q?: string; new?: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    query = await searchParams;
  const search = new URLSearchParams({
    queue: query.queue ?? "all",
    type: "staff_task",
    ...(query.q ? { q: query.q } : {}),
    ...(query.new && !actor.beta ? { new: "task" } : {}),
  });
  redirect(canonicalUiHref(`${workspaceBase(publicScope(actor))}/work?${search}`));
}
