import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { redirect } from "next/navigation";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
export default async function Requests({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  redirect(canonicalUiHref(`${workspaceBase(publicScope(actor))}/work?type=service_request`));
}
