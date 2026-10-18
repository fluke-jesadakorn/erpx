import {handbookCreateHref} from '@/lib/collaboration/handbook-navigation';
import {handbookPresentationContext} from '@/server/modules/collaboration/public/handbook-purchases';
import { forbidden, notFound } from "next/navigation";
import { SectionWorkspace } from "@/components/workplace/section-workspace";
import { RecordLink as Link } from "@/components/records/record-link";
import { HandbookEditors } from "@/components/workplace/handbook-controls";
import { collaborationPresentationQueries } from "@/server/modules/collaboration/public/presentation-queries";
import { companyMembers } from "@/server/modules/organization/public/member-directory";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { hasModuleAccess } from "@/data/module-access";
import { hasPermission } from "@/lib/identity_access/permissions";
import { sectionWorkspaceTopics, knowledgeTopicsFor, workspaceTopicLinks } from "@/lib/work_projection/section-workspaces";
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
