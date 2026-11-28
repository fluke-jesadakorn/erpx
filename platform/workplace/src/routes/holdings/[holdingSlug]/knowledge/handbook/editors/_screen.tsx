import {handbookCreateHref} from '@erpx/collaboration-ui/lib/handbook-navigation';
import {handbookPresentationContext} from '@erpx/collaboration-server/public/handbook-purchases';
import { forbidden, notFound } from "next/navigation";
import { SectionWorkspace } from "@erpx/platform-workplace/components/section-workspace";
import { RecordLink as Link } from "@erpx/records-ui/components/record-link";
import { HandbookEditors } from "@erpx/platform-workplace/components/handbook-controls";
import { handbookReads } from "@erpx/collaboration-ui/lib/native-handbook-page";
import { companyMembers } from "@erpx/organization-server/public/member-directory";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { hasModuleAccess } from "@erpx/platform-catalogue/module-access";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { sectionWorkspaceTopics, knowledgeTopicsFor, workspaceTopicLinks } from "@erpx/work-projection-ui/lib/section-workspaces";
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
