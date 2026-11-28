import { workProjectionPresentationQueries } from "@erpx/work-projection-server/public/presentation-queries";
import { StandardCalendarWorkspace as CalendarWorkspace } from "@erpx/work-projection-ui/components/calendar/standard-calendar-workspace";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";


import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import {calendarPresentationContext} from '@erpx/work-projection-server/public/calendar/presentation';
export const dynamic = "force-dynamic";
export default async function CalendarPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    presentation = await workProjectionPresentationQueries(actor.holdingId),
    query = await searchParams;
  const pageQuery = {
    ...query,
    ...(typeof query.view === "string"
      ? {}
      : { view: query.preset === "coverage" ? "week" : "month" }),
    ...(typeof query.layers === "string" || query.preset !== "coverage"
      ? {}
      : { layers: "roster,leave" }),
  };
  const [calendar, hierarchy, accounts] = await Promise.all([
    presentation.calendarData(actor, pageQuery),
    presentation.calendarHierarchy(actor),
    presentation.calendarAccounts(actor),
  ]);
  return (
    <div className="content" data-source="live">
      <CalendarWorkspace
        base={workspaceBase(publicScope(actor))}
        initial={calendar}
        initialHierarchy={hierarchy}
        initialAccounts={accounts}
        contextKey={calendarPresentationContext(actor)}
        initialEvent={typeof query.event === "string" ? query.event : undefined}
      />
    </div>
  );
}
