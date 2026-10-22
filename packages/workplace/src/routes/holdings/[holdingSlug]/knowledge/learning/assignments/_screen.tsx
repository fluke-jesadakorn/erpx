import WorkspaceLink from "@erpx/workplace/components/workspace-link";
import {learningWorkerStartupWorkspace} from "@erpx/server-people_payroll/public/learning-worker-startup";
import {learningConsoleSnapshot} from "@erpx/feature-learning/lib/purchase-controls";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { forbidden, notFound } from "next/navigation";
import { SectionWorkspace } from "@erpx/workplace/components/section-workspace";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
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
    const view = "learning", section = "assignments", selected = `${view}:${section}`;
    if (!sectionWorkspaceTopics.knowledge.some(topic => topic.id === selected))
        notFound();
    if (!topics.some(topic => topic.id === selected))
        forbidden();
    const term = typeof query.q === 'string' ? query.q : '';
    
    const data = await learningWorkspace(actor, false).catch(error => pageError(error, holdingSlug));
    const workerPreparation=data.people.length===0&&!actor.beta&&actor.scope?.kind==='holding'&&!actor.branchId&&!actor.departmentId&&hasPermission(actor,'people.manage')&&hasPermission(actor,'organization.manage')
        ?await learningWorkerStartupWorkspace(actor,{}).catch(error=>pageError(error,holdingSlug)):null;
    const matching = (value: unknown) => String(value ?? '').toLocaleLowerCase().includes(term.toLocaleLowerCase());
    return <SectionWorkspace title="Knowledge" selected={selected} items={workspaceTopicLinks(base, query, topics)} context={topics.find(topic => topic.id === selected)?.title} toolbar={undefined}>
 {<ListFilters appearance="standard" action={canonicalUiHref(`${base}/knowledge`)} query={term} placeholder="Search this view" preserve={workspaceSearchContext(query, { view, section })}/>}
 {false}
 {false}
 {workerPreparation?.available&&workerPreparation.companyId===actor.companyId&&<p>No eligible learners are available yet. <WorkspaceLink href={`${base}/knowledge/learning/settings/learners`}>Prepare a Learning learner</WorkspaceLink> using current Company HR and organization authority.</p>}
 {data && <>{<LearningConsole embedded section={'assignments'} base={base} data={learningConsoleSnapshot(data)}/>} {<div className="ui-stack">{data.assignments.filter(row => !term || matching(row.course_title) || matching(row.learner_name)).map(row => <article className="ui-record-item" key={String(row.id)}><Link href={canonicalUiHref(`${base}/learning/${row.id}`)}>{String(row.course_title)} · {String(row.learner_name)}</Link><p>{String(row.status)}</p></article>)}{!data.assignments.length && <p>No learning assignments in this view.</p>}</div>}</>}
 </SectionWorkspace>;
}
