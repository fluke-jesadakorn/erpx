import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { redirect } from "next/navigation";

export const dynamic = "force-dynamic";

export default async function LegacyPayrollSettings({ params }: { params: Promise<{ holdingSlug: string }> }) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  redirect(canonicalUiHref(`${workspaceBase(publicScope(actor))}/people/payroll/settings`));
}
