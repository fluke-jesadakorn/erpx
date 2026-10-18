import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { notFound } from "next/navigation";
import { SectionWorkspace } from "@/components/workplace/section-workspace";
import { RecordLink as Link } from "@/components/records/record-link";
import { BenefitsConsole } from "@/components/benefits/benefits-console";
import { ListFilters } from "@/components/ui/list-filters";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { benefitsWorkspace } from "@/server/modules/benefits/public/benefits";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks, workspaceSearchContext } from "@/lib/work_projection/section-workspaces";
import type { RouteSearch } from "@/lib/shared/route-context";

export default async function Benefits({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<RouteSearch>;
}) {
    const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), query = await searchParams, view = "enrollments";
    if (!sectionWorkspaceTopics.benefits.some(topic => topic.id === view))
        notFound();
    const data = await benefitsWorkspace(actor, false).catch(error => pageError(error, holdingSlug)), term = typeof query.q === 'string' ? query.q : '';
    const matches = (value: unknown) => String(value ?? '').toLocaleLowerCase().includes(term.toLocaleLowerCase()), elections = data.elections.filter(row => !term || matches(row.plan_name) || matches(row.employee_name));
    return <SectionWorkspace title="Benefits" selected={view} items={workspaceTopicLinks(base, query, sectionWorkspaceTopics.benefits)} context={'Scoped enrollments'}>
 <ListFilters appearance="standard" action={canonicalUiHref(`${base}/benefits?view=${view}`)} query={term} placeholder="Search this view" preserve={workspaceSearchContext(query, { view })}/>
 {<BenefitsConsole embedded section={'enrollments'} base={base} data={data}/>}
 {<div className="ui-stack">{elections.map(row => <article className="ui-record-item" key={String(row.id)}><Link href={canonicalUiHref(`${base}/benefits/${row.id}`)}>{String(row.employee_name)} · {String(row.plan_name)}</Link><p>{String(row.status)}</p></article>)}{!elections.length && <p>No enrollments match this view.</p>}</div>}
 </SectionWorkspace>;
}
