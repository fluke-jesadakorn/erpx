import {learningConsoleSnapshot} from "@erpx/learning-ui/lib/purchase-controls";
import { canonicalUiHref } from "@erpx/work-projection-ui/lib/destination-registry";
import { forbidden, notFound } from "next/navigation";
import { SectionWorkspace } from "@erpx/platform-workplace/components/section-workspace";
import { RecordCatalog } from "@erpx/records-ui/components/record-catalog";
import { LearningConsole } from "@erpx/learning-ui/components/learning-console";
import { ListFilters } from "@erpx/platform-ui/components/list-filters";
import { nativeLearningWorkspacePage } from "@erpx/platform-workplace/lib/native-learning";
import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { hasModuleAccess } from "@erpx/platform-catalogue/module-access";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { sectionWorkspaceTopics, workspaceTopicLinks, workspaceSearchContext } from "@erpx/work-projection-ui/lib/section-workspaces";
import type { RouteSearch } from "@erpx/platform-shared-ui/lib/route-context";

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
    
    const data = await nativeLearningWorkspacePage(actor, false).catch(error => pageError(error, holdingSlug));
    const matching = (value: unknown) => String(value ?? '').toLocaleLowerCase().includes(term.toLocaleLowerCase());
    return <SectionWorkspace title="Knowledge" selected={selected} items={workspaceTopicLinks(base, query, topics)} context={topics.find(topic => topic.id === selected)?.title} toolbar={undefined}>
 {<ListFilters appearance="standard" action={canonicalUiHref(`${base}/knowledge`)} query={term} placeholder="Search this view" preserve={workspaceSearchContext(query, { view, section })}/>}
 {false}
 {false}
 {data && <>{<LearningConsole embedded section={'courses'} base={base} data={learningConsoleSnapshot(data as unknown as Parameters<typeof learningConsoleSnapshot>[0])}/>} {<RecordCatalog base={base} kind="learning_course" title="Courses" rows={data.courses.filter(row => !term || matching(row.title))} label="title"/>}</>}
 </SectionWorkspace>;
}
