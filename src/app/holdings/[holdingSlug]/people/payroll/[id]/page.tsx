import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { redirect } from "next/navigation";

export const dynamic = "force-dynamic";

export default async function LegacyPayrollRun({ params, searchParams }: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<{ print?: string; brand?: string }>;
}) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const query = await searchParams;
  const suffix = new URLSearchParams();
  if (query.print === "1") suffix.set("print", "1");
  if (query.brand) suffix.set("brand", query.brand);
  redirect(`${workspaceBase(publicScope(actor))}/people/payroll/runs/${encodeURIComponent(id)}${suffix.size ? `?${suffix}` : ""}`);
}
