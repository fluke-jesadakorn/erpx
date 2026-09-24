import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { leaveSummary } from "@/server/leave-self-service";
import { leaveListFilterSchema } from "@/server/leave-self-service";
import { LeaveSelfService } from "@/components/leave/leave-self-service";
import { today } from "@/lib/calendar-date";

export const dynamic = "force-dynamic";

export default async function LeavePage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ month?: string | string[]; filter?: string | string[] }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const query = await searchParams;
  const summary = await leaveSummary(actor);
  const monthValue = typeof query.month === "string" ? query.month : "";
  const initialMonth = /^\d{4}-(0[1-9]|1[0-2])$/.test(monthValue)
    ? monthValue
    : today(summary.policy?.timezone ?? "Asia/Bangkok").slice(0, 7);
  const filterValue = typeof query.filter === "string" ? query.filter : "all";
  const parsedFilter = leaveListFilterSchema.safeParse(filterValue);
  return (
    <main className="content" data-source="live">
      <LeaveSelfService
        base={workspaceBase(publicScope(actor))}
        initial={summary}
        initialMonth={initialMonth}
        initialFilter={parsedFilter.success ? parsedFilter.data : "all"}
      />
    </main>
  );
}
