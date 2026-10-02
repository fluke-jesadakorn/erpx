import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { redirect } from "next/navigation";

export const dynamic = "force-dynamic";

export default async function LegacyPayrollRunDetail({ params, searchParams }: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams: Promise<{ print?: string; brand?: string }>;
}) {
  const { holdingSlug, id } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const { print, brand } = await searchParams;
  const query = new URLSearchParams();
  if (print === "1") query.set("print", "1");
  if (brand) query.set("brand", brand);
  redirect(`${workspaceBase(publicScope(actor))}/people/payroll/runs/${encodeURIComponent(id)}${query.size ? `?${query}` : ""}`);
}
