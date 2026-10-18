import {ProjectPurchaseProvider} from "@/components/projects/purchase-context";
import {projectPurchased} from "@/lib/projects/purchase-controls";
import {projectMemberPickerOptions,projectWorkerDisplayChoices} from "@/lib/projects/worker-display";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import {SectionWorkspace} from "@/components/workplace/section-workspace";
import {sectionWorkspaceTopics,workspaceTopicLinks} from "@/lib/work_projection/section-workspaces";
import type {RouteSearch} from "@/lib/shared/route-context";
import {notFound} from "next/navigation";
import { staffPresentationQueries } from "@/server/modules/work_projection/public/presentation-context-queries";


import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { FeatureWidget,WidgetGrid } from "@/components/shared/features/feature-page";
import { ProjectEditor } from "@/components/records/loaders/modules--staff-work-controls";
import { ProjectMilestoneConsole } from "@/components/records/loaders/modules--project-milestones";
import { ProjectActualsConsole } from "@/components/records/loaders/modules--project-actuals";
import { ProjectStructureConsole } from "@/components/records/loaders/modules--project-structure-console";
import {projectGraph,projectHandoffs} from "@/server/modules/projects/public/project-structure";
import { pageError } from "@/server/modules/work_projection/public/page-error";


import { samePerson } from "@/lib/identity_access/iam";

import { listProjectCostAllocations, listProjectCostSources } from "@/server/modules/projects/public/project-actuals";
import { hasPermission } from "@/lib/identity_access/permissions";
import { RecordDocumentPanel } from "@/components/records/loaders/attachments--record-document-panel";
import { RecordLink as Link } from "@/components/records/record-link";
import { Card } from "@/components/ui/foundation";
export default async function Project({
  params, searchParams,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
  searchParams?:Promise<RouteSearch>;
}) {
  const query=await searchParams??{},view=typeof query.view==="string"?query.view:"overview";
  if(!sectionWorkspaceTopics.projects.some(topic=>topic.id===view))notFound();
  const { holdingSlug, id } = await params,
    { actor, queries, base, options } = await staffPresentationQueries(holdingSlug),
    purchases=await queries.projectPurchases(),
    project = await queries.getProject(actor, id).catch((e) => {
      return pageError(e, holdingSlug);
    }),
    tasks = view==="tasks"&&projectPurchased(purchases,"projects.tasks","history")?await queries.listTasks(actor, id):[],
    milestones = view==="structure"?await queries.listProjectMilestones(actor, id):[],
    workLogs = ["actuals","structure","handoff"].includes(view)&&projectPurchased(purchases,"projects.time","history")?await queries.listProjectWorkLogs(actor, id):[],
    members = (
      await queries.projectMemberIds(id)
    ).results;
  const isProjectOwner = samePerson(actor, project.owner_user_id);
  const workerChoices=projectWorkerDisplayChoices(options.people);
  const active=project.lifecycle_status==='active',graph=["structure","handoff"].includes(view)?await projectGraph(actor,id):null,handoffs=view==="handoff"&&projectPurchased(purchases,"projects.handover","history")?await projectHandoffs(actor,id):[];
  const canReadCosts = hasPermission(actor, "finance.read")&&projectPurchased(purchases,"projects.costs","history"),
    canWriteCosts = hasPermission(actor, "finance.write"),
    canApproveCosts = hasPermission(actor, "finance.approve"),
    [costs, costSources] = canReadCosts
      &&["actuals","structure","handoff"].includes(view)? await Promise.all([
          listProjectCostAllocations(actor, id),
          canWriteCosts&&view==="actuals"&&projectPurchased(purchases,"projects.costs","newWork") ? listProjectCostSources(actor, id) : Promise.resolve([]),
        ])
      : [[], []];
  const recordOwner={companyId:project.company_id,actorId:actor.userId,projectId:project.id,version:Number(project.version),lifecycleStatus:project.lifecycle_status,isProjectOwner,canWriteCosts,canApproveCosts};
  return (
    <ProjectPurchaseProvider purchases={purchases}><SectionWorkspace title={project.title} items={workspaceTopicLinks(base,query,sectionWorkspaceTopics.projects.map(topic=>({...topic,path:`projects/${id}`})))} selected={view} context={project.lifecycle_status}>
      <Link href={canonicalUiHref(`${base}/projects`)}>← Projects</Link>
      <WidgetGrid pageKey="projects-detail">
      <p>{project.description}</p>
      {view==="overview"&&<ProjectPurchaseProvider purchases={purchases} editorSource={{companyId:actor.companyId,actorId:actor.userId,available:isProjectOwner&&active,record:{kind:"project",id:project.id,version:Number(project.version),lifecycleStatus:project.lifecycle_status}}}>{isProjectOwner && active && (
        <ProjectEditor appearance="standard"
          base={base}
          commandOwner={{companyId:actor.companyId,actorId:actor.userId}}
          options={projectMemberPickerOptions(options)}
          project={project}
          members={members.map((m) => m.user_id)}
        />
      )}</ProjectPurchaseProvider>}
      {view==="documents"&&hasPermission(actor,"records.read") && <RecordDocumentPanel appearance="standard"
        base={base}
        targetType="project"
        targetId={id}
        expectedTargetVersion={Number(project.version)}
        editable={isProjectOwner && active && hasPermission(actor,"records.write")&&projectPurchased(purchases,"projects.wbs","settleExisting")}
        title="Project documents"
        purposes={[
          {key:"project_document",label:"Project document",description:"Attach the approved brief, WBS, specification, or project record."},
          {key:"deliverable",label:"Deliverable",description:"Attach a reviewed project deliverable or acceptance artifact."},
          {key:"handoff",label:"Handoff",description:"Attach the final handoff, acceptance, or transition record."},
        ]}
      />}
      {view==="structure"&&<FeatureWidget appearance="standard" id="projects-detail-milestones" title="Project milestones"><ProjectMilestoneConsole
        base={base}
        projectId={id}
        ownerContext={recordOwner}
        projectOwnerId={project.owner_user_id}
        members={members.map((m) => m.user_id)}
        people={workerChoices}
        milestones={milestones}
        canManage={isProjectOwner&&active}
      /></FeatureWidget>}
      {view==="structure"&&<FeatureWidget appearance="standard" id="projects-detail-structure" title="Work structure and handoff"><ProjectStructureConsole base={base} section="structure" graph={graph!} handoffs={[]} actorId={actor.userId} people={workerChoices} members={members.map(member=>member.user_id)} workLogs={workLogs} costAllocations={costs} canWriteCosts={canWriteCosts&&active}/></FeatureWidget>}
      {view==="actuals"&&<FeatureWidget appearance="standard" id="projects-detail-actuals" title="Project actuals"><ProjectActualsConsole
        base={base}
        projectId={id}
        ownerContext={recordOwner}
        isProjectOwner={isProjectOwner&&active}
        workLogs={workLogs}
        costs={costs}
        costSources={costSources}
        canReadCosts={canReadCosts}
        canWriteCosts={canWriteCosts&&active}
        canApproveCosts={canApproveCosts&&active}
        editable={active}
      /></FeatureWidget>}
      {view==="tasks"&&<FeatureWidget appearance="standard" id="projects-detail-project-tasks" title="Project tasks"><Card as="section" appearance="standard" className="ui-panel">
        
        {active&&projectPurchased(purchases,"projects.tasks","newWork")&&<UiActionLink appearance="standard" variant="primary" href={canonicalUiHref(`${base}/projects/${encodeURIComponent(id)}/tasks/new`)}>
          Create a task
        </UiActionLink>}
        {tasks.map((t) => (
          <Link className="ui-record-item" key={t.id} href={canonicalUiHref(`${base}/staff-tasks/${t.id}`)}>
            {t.title} · {t.state.replaceAll("_", " ")} · {t.assignee_name}
          </Link>
        ))}
      </Card></FeatureWidget>}
    {view==="overview"&&<p>Select Tasks, Work structure, Actuals, Handoff or Documents to inspect the current project.</p>}{view==="handoff"&&graph&&<ProjectStructureConsole section="handoff" base={base} graph={graph} handoffs={handoffs} actorId={actor.userId} people={workerChoices} members={members.map(member=>member.user_id)} workLogs={workLogs} costAllocations={costs} canWriteCosts={canWriteCosts&&active}/> }
    </WidgetGrid></SectionWorkspace></ProjectPurchaseProvider>
  );
}
