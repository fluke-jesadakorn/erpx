import {handbookCreateHref} from '@erpx/feature-collaboration/lib/handbook-navigation';
import {handbookPresentationContext} from '@erpx/server-collaboration/public/handbook-purchases';
import { forbidden, notFound } from "next/navigation";
import { SectionWorkspace } from "@erpx/workplace/components/section-workspace";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { HandbookEditors } from "@erpx/workplace/components/handbook-controls";
import { collaborationPresentationQueries } from "@erpx/server-collaboration/public/presentation-queries";
import { companyMembers } from "@erpx/server-organization/public/member-directory";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { sectionWorkspaceTopics, knowledgeTopicsFor, workspaceTopicLinks } from "@erpx/feature-work_projection/lib/section-workspaces";
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
    const view = "handbook", section = "editors", selected = `${view}:${section}`;
    if (!sectionWorkspaceTopics.knowledge.some(topic => topic.id === selected))
        notFound();
    if (!topics.some(topic => topic.id === selected))
        forbidden();
    
    
    
    return <SectionWorkspace title="Knowledge" selected={selected} items={workspaceTopicLinks(base, query, topics)} context={topics.find(topic => topic.id === selected)?.title} toolbar={access.editor&&(!access.purchase||access.purchase.newWork) ? <Link href={handbookCreateHref(base,section,query)}>New article</Link> : undefined}>
 {false}
 {false}
 {<HandbookEditors key={handbookPresentationContext(actor)} contextKey={handbookPresentationContext(actor)} purchase={access.purchase} base={base} companyId={actor.companyId} userId={actor.userId} options={await presentation.handbookEditorOptions(actor)} members={await companyMembers(actor)}/>}
 {false}
 </SectionWorkspace>;
}
