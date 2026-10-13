import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { leaveSummary } from "@/server/modules/people_payroll/public/leave-self-service";
import { leaveListFilterSchema } from "@/server/modules/people_payroll/public/leave-self-service";
import { LeaveSelfService } from "@/components/people_payroll/leave/leave-self-service";
import { today } from "@/lib/work_projection/calendar-date";

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
    <div className="content" data-source="live">
      <LeaveSelfService
        base={workspaceBase(publicScope(actor))}
        initial={summary}
        initialMonth={initialMonth}
        initialFilter={parsedFilter.success ? parsedFilter.data : "all"}
      />
    </div>
  );
}
