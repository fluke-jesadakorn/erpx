import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { redirect } from "next/navigation";

export const dynamic = "force-dynamic";

export default async function LegacyPayrollAdvances({ params }: { params: Promise<{ holdingSlug: string }> }) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  redirect(canonicalUiHref(`${workspaceBase(publicScope(actor))}/people/payroll/advances`));
}
