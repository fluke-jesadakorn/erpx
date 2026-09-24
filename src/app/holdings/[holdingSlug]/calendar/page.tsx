import { CalendarWorkspace } from "@/components/calendar/calendar-workspace";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { holdingResources } from "@/server/runtime";
import { calendarData, calendarHierarchy } from "@/server/calendar";
import { workspaceBase } from "@/lib/workspace";
export const dynamic = "force-dynamic";
export default async function CalendarPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    { write } = await holdingResources(actor.holdingId),
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
  const [calendar, hierarchy] = await Promise.all([
    calendarData(write, actor, pageQuery),
    calendarHierarchy(write, actor),
  ]);
  return (
    <main className="content" data-source="live">
      <CalendarWorkspace
        base={workspaceBase(publicScope(actor))}
        initial={calendar}
        initialHierarchy={hierarchy}
        initialEvent={typeof query.event === "string" ? query.event : undefined}
      />
    </main>
  );
}
