import {handbookCreateHref} from '@erpx/feature-collaboration/lib/handbook-navigation';
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { forbidden, notFound } from "next/navigation";
import { SectionWorkspace } from "@erpx/workplace/components/section-workspace";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { ListFilters } from "@erpx/ui/components/list-filters";
import { ListPagination } from "@erpx/ui/components/list-pagination";
import { collaborationPresentationQueries } from "@erpx/server-collaboration/public/presentation-queries";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { sectionWorkspaceTopics, knowledgeTopicsFor, workspaceTopicLinks, workspaceSearchContext } from "@erpx/feature-work_projection/lib/section-workspaces";
import type { RouteSearch } from "@erpx/feature-shared/lib/route-context";

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
