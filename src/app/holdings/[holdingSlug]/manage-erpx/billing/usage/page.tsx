import { UsageConsole } from "@/components/billing/usage/usage-console";
import { requireManageErpxContext } from "@/server/modules/identity_access/public/authorization";

export const dynamic = "force-dynamic";

function currentBangkokMonth() {
  return new Date(Date.now() + 7 * 60 * 60 * 1000).toISOString().slice(0, 7);
}

export default async function CustomerUsagePage({
  searchParams,
}: {
  searchParams: Promise<{ month?: string; holdingId?: string }>;
}) {
  const { platform } = await requireManageErpxContext();
  if (platform.role !== "platform_admin" && platform.role !== "platform_auditor") throw new Error("FORBIDDEN");
  const query = await searchParams;
  const currentMonth = currentBangkokMonth();
  const month = query.month && /^\d{4}-(0[1-9]|1[0-2])$/.test(query.month) ? query.month : currentMonth;
  return <UsageConsole key={`${month}:${query.holdingId ?? "all"}`} base="/holdings/erpx" mode="platform" initialMonth={month} initialHoldingId={query.holdingId} />;
}
