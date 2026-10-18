import {handbookCreateHref} from '@/lib/collaboration/handbook-navigation';
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { forbidden, notFound } from "next/navigation";
import { SectionWorkspace } from "@/components/workplace/section-workspace";
import { RecordLink as Link } from "@/components/records/record-link";
import { ListFilters } from "@/components/ui/list-filters";
import { ListPagination } from "@/components/ui/list-pagination";
import { collaborationPresentationQueries } from "@/server/modules/collaboration/public/presentation-queries";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { hasModuleAccess } from "@/data/module-access";
import { hasPermission } from "@/lib/identity_access/permissions";
import { sectionWorkspaceTopics, knowledgeTopicsFor, workspaceTopicLinks, workspaceSearchContext } from "@/lib/work_projection/section-workspaces";
import type { RouteSearch } from "@/lib/shared/route-context";

export default async function Knowledge({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<RouteSearch>;
}) {
    const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), query = await searchParams, presentation = await collaborationPresentationQueries(actor.holdingId);
    const access = await presentation.handbookAccess(actor).catch(error => pageError(error, holdingSlug));
    const learning = hasModuleAccess(actor.modules, 'learning') && hasPermission(actor, 'records.read');
    const topics = knowledgeTopicsFor(access, learning);
    const view = "handbook", section = "drafts", selected = `${view}:${section}`;
    if (!sectionWorkspaceTopics.knowledge.some(topic => topic.id === selected))
        notFound();
    if (!topics.some(topic => topic.id === selected))
        forbidden();
    const term = typeof query.q === 'string' ? query.q : '', cursor = typeof query.cursor === 'string' ? query.cursor : undefined;
    const articles = await presentation.handbookList(actor, term, true, cursor).catch(error => pageError(error, holdingSlug));
    
    
    return <SectionWorkspace title="Knowledge" selected={selected} items={workspaceTopicLinks(base, query, topics)} context={topics.find(topic => topic.id === selected)?.title} toolbar={access.editor&&(!access.purchase||access.purchase.newWork) ? <Link href={handbookCreateHref(base,section,query)}>New article</Link> : undefined}>
 {<ListFilters appearance="standard" action={canonicalUiHref(`${base}/knowledge`)} query={term} placeholder="Search this view" preserve={workspaceSearchContext(query, { view, section })}/>}
 {articles && <><div className="ui-stack">{articles.articles.map(article => <article className="ui-record-item" key={article.id}><small>{article.category.replaceAll('_', ' ')}</small><h2><Link href={canonicalUiHref(`${base}/handbook/${article.id}${'?edit=1'}`)}>{article.title || 'Untitled draft'}</Link></h2><p>{article.excerpt}</p></article>)}{!articles.articles.length && <p>No articles match this view.</p>}</div><ListPagination {...articles} query={{ ...query, view, section }} count={articles.articles.length}/></>}
 {false}
 {false}
 </SectionWorkspace>;
}
