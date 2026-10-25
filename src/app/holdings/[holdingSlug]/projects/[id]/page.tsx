import { staffPresentationQueries } from "@/server/modules/work_projection/public/presentation-context-queries";


import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { ProjectEditor } from "@/components/records/loaders/modules--staff-work-controls";
import { ProjectMilestoneConsole } from "@/components/records/loaders/modules--project-milestones";
import { ProjectActualsConsole } from "@/components/records/loaders/modules--project-actuals";
import { ProjectStructureConsole } from "@/components/records/loaders/modules--project-structure-console";
import {projectGraph,projectHandoffs} from "@/server/modules/projects/public/project-structure";
import { pageError } from "@/server/modules/work_projection/public/page-error";


import { samePerson } from "@/lib/iam";

import { listProjectCostAllocations, listProjectCostSources } from "@/server/modules/projects/public/project-actuals";
import { hasPermission } from "@/lib/permissions";
import { RecordDocumentPanel } from "@/components/records/loaders/attachments--record-document-panel";
import { RecordLink as Link } from "@/components/records/record-link";
import { Card } from "@/components/ui/foundation";
export default async function Project({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    { actor, queries, base, options } = await staffPresentationQueries(holdingSlug),
    project = await queries.getProject(actor, id).catch((e) => {
      return pageError(e, holdingSlug);
    }),
    tasks = await queries.listTasks(actor, id),
    milestones = await queries.listProjectMilestones(actor, id),
    workLogs = await queries.listProjectWorkLogs(actor, id),
    members = (
      await queries.projectMemberIds(id)
    ).results;
  const isProjectOwner = samePerson(actor, project.owner_user_id);
  const active=project.lifecycle_status==='active',graph=await projectGraph(actor,id),handoffs=await projectHandoffs(actor,id);
  const canReadCosts = hasPermission(actor, "finance.read"),
    canWriteCosts = hasPermission(actor, "finance.write"),
    canApproveCosts = hasPermission(actor, "finance.approve"),
    [costs, costSources] = canReadCosts
      ? await Promise.all([
          listProjectCostAllocations(actor, id),
          canWriteCosts ? listProjectCostSources(actor, id) : Promise.resolve([]),
        ])
      : [[], []];
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <Link href={`${base}/projects`}>← Projects</Link>
      <h1>{project.title}</h1><FeatureTabs/><WidgetGrid pageKey="projects-detail">
      <p>{project.description}</p>
      {isProjectOwner && active && (
        <ProjectEditor appearance="standard"
          base={base}
          options={options}
          project={project}
          members={members.map((m) => m.user_id)}
        />
      )}
      {hasPermission(actor,"records.read") && <RecordDocumentPanel appearance="standard"
        base={base}
        targetType="project"
        targetId={id}
        expectedTargetVersion={Number(project.version)}
        editable={isProjectOwner && active && hasPermission(actor,"records.write")}
        title="Project documents"
        purposes={[
          {key:"project_document",label:"Project document",description:"Attach the approved brief, WBS, specification, or project record."},
          {key:"deliverable",label:"Deliverable",description:"Attach a reviewed project deliverable or acceptance artifact."},
          {key:"handoff",label:"Handoff",description:"Attach the final handoff, acceptance, or transition record."},
        ]}
      />}
      <FeatureWidget appearance="standard" id="projects-detail-milestones" title="Project milestones"><ProjectMilestoneConsole
        base={base}
        projectId={id}
        projectOwnerId={project.owner_user_id}
        members={members.map((m) => m.user_id)}
        people={options.people}
        milestones={milestones}
        canManage={isProjectOwner&&active}
      /></FeatureWidget>
      <FeatureWidget appearance="standard" id="projects-detail-structure" title="Work structure and handoff"><ProjectStructureConsole base={base} graph={graph} handoffs={handoffs} actorId={actor.userId} people={options.people} members={members.map(member=>member.user_id)} workLogs={workLogs} costAllocations={costs} canWriteCosts={canWriteCosts&&active}/></FeatureWidget>
      <FeatureWidget appearance="standard" id="projects-detail-actuals" title="Project actuals"><ProjectActualsConsole
        base={base}
        projectId={id}
        isProjectOwner={isProjectOwner&&active}
        workLogs={workLogs}
        costs={costs}
        costSources={costSources}
        canReadCosts={canReadCosts}
        canWriteCosts={canWriteCosts&&active}
        canApproveCosts={canApproveCosts&&active}
        editable={active}
      /></FeatureWidget>
      <FeatureWidget appearance="standard" id="projects-detail-project-tasks" title="Project tasks"><Card as="section" appearance="standard" className="ui-panel">
        
        {active&&<UiActionLink appearance="standard" variant="primary" href={`${base}/staff-tasks/new?projectId=${encodeURIComponent(id)}`}>
          Create a task
        </UiActionLink>}
        {tasks.map((t) => (
          <Link className="ui-record-item" key={t.id} href={`${base}/staff-tasks/${t.id}`}>
            {t.title} · {t.state.replaceAll("_", " ")} · {t.assignee_name}
          </Link>
        ))}
      </Card></FeatureWidget>
    </WidgetGrid></FeaturePage>
  );
}
