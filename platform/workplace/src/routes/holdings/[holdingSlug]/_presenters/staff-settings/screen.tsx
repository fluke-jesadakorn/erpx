import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { redirect } from "next/navigation";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
export default async function StaffSettings({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  redirect(
    canonicalUiHref(`${workspaceBase(publicScope(actor))}/settings?section=organization`),
  );
}
