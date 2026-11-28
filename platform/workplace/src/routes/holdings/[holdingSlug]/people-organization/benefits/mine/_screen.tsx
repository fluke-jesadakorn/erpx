import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { notFound } from "next/navigation";
import { SectionWorkspace } from "@erpx/platform-workplace/components/section-workspace";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import { ListFilters } from "@erpx/platform-ui/components/list-filters";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { nativeBenefitsWorkspace } from "@erpx/benefits-ui/public/native-benefits-workspace-page";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks, workspaceSearchContext } from "@erpx/work-projection-ui/lib/section-workspaces";
import type { RouteSearch } from "@erpx/platform-shared-ui/lib/route-context";

export default async function Benefits({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<RouteSearch>;
}) {
    const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), query = await searchParams, view = "mine";
    if (!sectionWorkspaceTopics.benefits.some(topic => topic.id === view))
        notFound();
    const data = await nativeBenefitsWorkspace(actor, true).catch(error => pageError(error, holdingSlug)), term = typeof query.q === 'string' ? query.q : '';
    const matches = (value: unknown) => String(value ?? '').toLocaleLowerCase().includes(term.toLocaleLowerCase()), elections = data.elections.filter(row => !term || matches(row.plan_name) || matches(row.employee_name));
    return <SectionWorkspace title="Benefits" selected={view} items={workspaceTopicLinks(base, query, sectionWorkspaceTopics.benefits)} context={'Your coverage and enrollment'}>
 <ListFilters appearance="standard" action={canonicalUiHref(`${base}/benefits?view=${view}`)} query={term} placeholder="Search this view" preserve={workspaceSearchContext(query, { view })}/>
 {false}
 {<div className="ui-stack">{elections.map(row => <article className="ui-record-item" key={String(row.id)}><Link href={canonicalUiHref(`${base}/benefits/${row.id}`)}>{String(row.employee_name)} · {String(row.plan_name)}</Link><p>{String(row.status)}</p></article>)}{!elections.length && <p>No enrollments match this view.</p>}</div>}
 </SectionWorkspace>;
}
