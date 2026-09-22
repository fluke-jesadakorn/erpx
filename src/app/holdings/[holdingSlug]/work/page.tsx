import { FeaturePage } from "@/components/features/feature-page";
import { WorkDashboard } from "@/components/workplace/work-dashboard";
import { workResponsibilities,workTypes,workViews } from "@/lib/work-item";
import { queues } from "@/lib/work-queue";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import { getWorkPage } from "@/server/work-summary";

export default async function WorkPage({
  params,
  searchParams,
}: PageProps<"/holdings/[holdingSlug]/work">) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    query = await searchParams;
  const queue = queues.some(([id]) => id === query.queue)
    ? String(query.queue)
    : "todo";
  const type = workTypes.some(([id]) => id === query.type)
    ? String(query.type)
    : "all";
  const responsibility = workResponsibilities.some(
    ([id]) => id === query.responsibility,
  )
    ? String(query.responsibility)
    : "all";
  const q = typeof query.q === "string" ? query.q : "",
    newKind = typeof query.new === "string" ? query.new : "";
  const view = workViews.some(([id]) => id === query.view) ? String(query.view)
    : query.queue || query.responsibility ? "legacy" : "actions";
  const current = new URLSearchParams({
    ...(Number(query.offset)>0?{offset:String(query.offset)}:{}),
    ...(view !== "legacy" ? { view } : { queue }),
    ...(responsibility !== "all" ? { responsibility } : {}),
    ...(type !== "all" ? { type } : {}),
    ...(q ? { q } : {}),
  }).toString();
  if (!newKind) {
    const summary = await getWorkPage(actor,{view,queue,type,q,responsibility,offset:Number(query.offset??0)});
    return (
      <FeaturePage className="content" data-source="live">
        <WorkDashboard
          key={`${actor.companyId}:${actor.userId}:${base}:${current}:${summary.generatedAt}`}
          base={base}
          beta={!!actor.beta}
          initial={summary}
          queue={queue}
          view={view}
          type={type}
          responsibility={responsibility}
          q={q}
          current={current}
        />
      </FeaturePage>
    );
  }
  const { CreateWork } = await import("./create");
  return <CreateWork actor={actor} base={base} newKind={newKind} current={current} query={query}/>;
}
