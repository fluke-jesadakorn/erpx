import {learningConsoleSnapshot} from "@erpx/feature-learning/lib/purchase-controls";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { forbidden, notFound } from "next/navigation";
import { SectionWorkspace } from "@erpx/workplace/components/section-workspace";
import { RecordCatalog } from "@erpx/feature-records/components/record-catalog";
import { LearningConsole } from "@erpx/feature-learning/components/learning-console";
import { ListFilters } from "@erpx/ui/components/list-filters";
import { learningWorkspace } from "@erpx/server-learning/public/learning";
import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hasModuleAccess } from "@erpx/catalogue/module-access";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { sectionWorkspaceTopics, workspaceTopicLinks, workspaceSearchContext } from "@erpx/feature-work_projection/lib/section-workspaces";
import type { RouteSearch } from "@erpx/feature-shared/lib/route-context";

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
