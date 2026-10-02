import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { redirect } from "next/navigation";

export const dynamic = "force-dynamic";

export default async function LegacyPayrollHistory({ params, searchParams }: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ cursor?: string }>;
}) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const { cursor } = await searchParams;
  const query = cursor ? `?cursor=${encodeURIComponent(cursor)}` : "";
  redirect(`${workspaceBase(publicScope(actor))}/people/payroll/history${query}`);
}
