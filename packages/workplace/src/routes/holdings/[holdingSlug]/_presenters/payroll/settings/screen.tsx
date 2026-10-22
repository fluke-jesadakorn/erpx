import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { redirect } from "next/navigation";

export const dynamic = "force-dynamic";

export default async function LegacyPayrollSettings({ params }: { params: Promise<{ holdingSlug: string }> }) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  redirect(canonicalUiHref(`${workspaceBase(publicScope(actor))}/people/payroll/settings`));
}
