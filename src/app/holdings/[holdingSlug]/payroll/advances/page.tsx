import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { redirect } from "next/navigation";

export const dynamic = "force-dynamic";

export default async function LegacyPayrollAdvances({ params }: { params: Promise<{ holdingSlug: string }> }) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  redirect(`${workspaceBase(publicScope(actor))}/people/payroll/advances`);
}
