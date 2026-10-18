import {learningConsoleSnapshot} from "@/lib/learning/purchase-controls";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { forbidden, notFound } from "next/navigation";
import { SectionWorkspace } from "@/components/workplace/section-workspace";
import { RecordCatalog } from "@/components/records/record-catalog";
import { LearningConsole } from "@/components/learning/learning-console";
import { ListFilters } from "@/components/ui/list-filters";
import { learningWorkspace } from "@/server/modules/learning/public/learning";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { hasModuleAccess } from "@/data/module-access";
import { hasPermission } from "@/lib/identity_access/permissions";
import { sectionWorkspaceTopics, workspaceTopicLinks, workspaceSearchContext } from "@/lib/work_projection/section-workspaces";
import type { RouteSearch } from "@/lib/shared/route-context";

export default async function Knowledge({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams: Promise<RouteSearch>;
}) {
    const { holdingSlug } = await params, actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor)), query = await searchParams;
    const learning = hasModuleAccess(actor.modules, 'learning') && hasPermission(actor, 'records.read');
    const topics = sectionWorkspaceTopics.knowledge.filter(topic => learning && topic.id.startsWith('learning:'));
    const view = "learning", section = "courses", selected = `${view}:${section}`;
    if (!sectionWorkspaceTopics.knowledge.some(topic => topic.id === selected))
        notFound();
    if (!topics.some(topic => topic.id === selected))
        forbidden();
    const term = typeof query.q === 'string' ? query.q : '';
    
    const data = await learningWorkspace(actor, false).catch(error => pageError(error, holdingSlug));
    const matching = (value: unknown) => String(value ?? '').toLocaleLowerCase().includes(term.toLocaleLowerCase());
    return <SectionWorkspace title="Knowledge" selected={selected} items={workspaceTopicLinks(base, query, topics)} context={topics.find(topic => topic.id === selected)?.title} toolbar={undefined}>
 {<ListFilters appearance="standard" action={canonicalUiHref(`${base}/knowledge`)} query={term} placeholder="Search this view" preserve={workspaceSearchContext(query, { view, section })}/>}
 {false}
 {false}
 {data && <>{<LearningConsole embedded section={'courses'} base={base} data={learningConsoleSnapshot(data)}/>} {<RecordCatalog base={base} kind="learning_course" title="Courses" rows={data.courses.filter(row => !term || matching(row.title))} label="title"/>}</>}
 </SectionWorkspace>;
}
