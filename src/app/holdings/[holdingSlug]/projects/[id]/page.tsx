import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { ProjectEditor } from "@/components/modules/staff-work-controls";
import { ProjectMilestoneConsole } from "@/components/modules/project-milestones";
import { ProjectActualsConsole } from "@/components/modules/project-actuals";
import { pageError } from "@/server/page-error";
import { staffPage } from "@/server/staff-page";
import { getProject,listTasks } from "@/server/staff-tasks";
import { samePerson } from "@/lib/iam";
import { listProjectMilestones } from "@/server/project-milestones";
import { listProjectCostAllocations, listProjectCostSources, listProjectWorkLogs } from "@/server/project-actuals";
import { hasPermission } from "@/lib/permissions";
import Link from "next/link";
export default async function Project({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    { actor, write, base, options } = await staffPage(holdingSlug),
    project = await getProject(write, actor, id).catch((e) => {
      return pageError(e, holdingSlug);
    }),
    tasks = await listTasks(write, actor, id),
    milestones = await listProjectMilestones(write, actor, id),
    workLogs = await listProjectWorkLogs(write, actor, id),
    members = (
      await write
        .prepare("SELECT user_id FROM staff_project_members WHERE project_id=?")
        .bind(id)
        .all<{ user_id: string }>()
    ).results;
  const isProjectOwner = samePerson(actor, project.owner_user_id);
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
    <FeaturePage className="content-stack" data-source="live">
      <Link href={`${base}/projects`}>← Projects</Link>
      <h1>{project.title}</h1><FeatureTabs/><WidgetGrid pageKey="projects-detail">
      <p>{project.description}</p>
      {isProjectOwner && (
        <ProjectEditor
          base={base}
          options={options}
          project={project}
          members={members.map((m) => m.user_id)}
        />
      )}
      <FeatureWidget id="projects-detail-milestones" title="Project milestones"><ProjectMilestoneConsole
        base={base}
        projectId={id}
        projectOwnerId={project.owner_user_id}
        members={members.map((m) => m.user_id)}
        people={options.people}
        milestones={milestones}
        canManage={isProjectOwner}
      /></FeatureWidget>
      <FeatureWidget id="projects-detail-actuals" title="Project actuals"><ProjectActualsConsole
        base={base}
        projectId={id}
        isProjectOwner={isProjectOwner}
        workLogs={workLogs}
        costs={costs}
        costSources={costSources}
        canReadCosts={canReadCosts}
        canWriteCosts={canWriteCosts}
        canApproveCosts={canApproveCosts}
      /></FeatureWidget>
      <FeatureWidget id="projects-detail-project-tasks" title="Project tasks"><section className="card">
        
        <Link className="button" href={`${base}/work?new=task`}>
          Create a task
        </Link>
        {tasks.map((t) => (
          <Link className="row" key={t.id} href={`${base}/staff-tasks/${t.id}`}>
            {t.title} · {t.state.replaceAll("_", " ")} · {t.assignee_name}
          </Link>
        ))}
      </section></FeatureWidget>
    </WidgetGrid></FeaturePage>
  );
}
