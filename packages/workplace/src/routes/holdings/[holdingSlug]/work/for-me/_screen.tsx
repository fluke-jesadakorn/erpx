import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { PageContent } from "@erpx/ui/components/patterns";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { WorkDashboard } from "@erpx/workplace/components/work-dashboard";
import { workResponsibilities, workTypes, workViews } from "@erpx/feature-work_projection/lib/work-item";
import { queues } from "@erpx/feature-work_projection/lib/work-queue";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { getWorkPage } from "@erpx/server-work_projection/public/work-summary";
import { redirect } from "next/navigation";
export default async function WorkPage({ params, searchParams, }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<Record<string, string | string[] | undefined>>;
}) {
    const actor = await requireWorkContext((await params).holdingSlug), base = workspaceBase(publicScope(actor)), query = await searchParams;
    const queue = queues.some(([id]) => id === query.queue)
        ? String(query.queue)
        : "todo";
    const type = workTypes.some(([id]) => id === query.type)
        ? String(query.type)
        : "all";
    const responsibility = workResponsibilities.some(([id]) => id === query.responsibility)
        ? String(query.responsibility)
        : "all";
    const q = typeof query.q === "string" ? query.q : "", newKind = typeof query.new === "string" ? query.new : "";
    if (newKind === "expense")
        redirect(canonicalUiHref(`${base}/expenses/new`));
    if (newKind === "purchase_request")
        redirect(canonicalUiHref(`${base}/requests/new?kind=purchase`));
    if (newKind === "task") {
        const taskQuery = new URLSearchParams({
            ...(typeof query.projectId === "string" ? { projectId: query.projectId } : {}),
            ...(typeof query.dueDate === "string" ? { dueDate: query.dueDate } : {}),
        });
        redirect(canonicalUiHref(`${base}/staff-tasks/new${taskQuery.size ? `?${taskQuery}` : ""}`));
    }
    const view = workViews.some(([id]) => id === "for-me") ? String("for-me")
        : query.queue || query.responsibility ? "legacy" : "actions";
    const tracking = ["requests", "waiting", "completed"].includes(String(query.tracking)) ? String(query.tracking) : "all";
    const current = new URLSearchParams({
        ...(Number(query.offset) > 0 ? { offset: String(query.offset) } : {}),
        ...({ view }),
        ...(responsibility !== "all" ? { responsibility } : {}),
        ...(type !== "all" ? { type } : {}),
        ...(q ? { q } : {}),
        ...({}),
    }).toString();
    if (!newKind) {
        const summary = await getWorkPage(actor, { view, queue, type, q, responsibility, tracking, offset: Number(query.offset ?? 0) });
        return (<PageContent as={FeaturePage} data-source="live">
        <WorkDashboard key={`${actor.companyId}:${actor.userId}:${base}:${current}:${summary.generatedAt}`} base={base} beta={!!actor.beta} initial={summary} queue={queue} view={view} type={type} responsibility={responsibility} q={q} current={current}/>
      </PageContent>);
    }
    const { CreateWork } = await import("@erpx/workplace/routes/holdings/[holdingSlug]/_presenters/work/create");
    return <CreateWork actor={actor} base={base} newKind={newKind} current={current} query={query}/>;
}
