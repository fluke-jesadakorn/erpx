import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
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
  redirect(canonicalUiHref(`${workspaceBase(publicScope(actor))}/people/payroll/runs/${encodeURIComponent(id)}${query.size ? `?${query}` : ""}`));
}
