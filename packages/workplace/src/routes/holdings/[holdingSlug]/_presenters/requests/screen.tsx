import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { redirect } from "next/navigation";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
export default async function Requests({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  redirect(canonicalUiHref(`${workspaceBase(publicScope(actor))}/work?type=service_request`));
}
