import {handbookCreateHref} from '@erpx/collaboration-ui/lib/handbook-navigation';
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { forbidden, notFound } from "next/navigation";
import { SectionWorkspace } from "@erpx/platform-workplace/components/section-workspace";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import { ListFilters } from "@erpx/platform-ui/components/list-filters";
import { ListPagination } from "@erpx/platform-ui/components/list-pagination";
import { handbookReads } from "@erpx/collaboration-ui/lib/native-handbook-page";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { hasModuleAccess } from "@erpx/platform-catalogue/module-access";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { sectionWorkspaceTopics, knowledgeTopicsFor, workspaceTopicLinks, workspaceSearchContext } from "@erpx/work-projection-ui/lib/section-workspaces";
import type { RouteSearch } from "@erpx/platform-shared-ui/lib/route-context";

export default async function Knowledge({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<RouteSearch>;
}) {
    const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), query = await searchParams, presentation = await handbookReads(actor, holdingSlug);
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
