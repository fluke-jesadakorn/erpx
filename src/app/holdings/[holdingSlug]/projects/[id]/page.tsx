import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { ProjectEditor } from "@/components/modules/staff-work-controls";
import { pageError } from "@/server/page-error";
import { staffPage } from "@/server/staff-page";
import { getProject,listTasks } from "@/server/staff-tasks";
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
    members = (
      await write
        .prepare("SELECT user_id FROM staff_project_members WHERE project_id=?")
        .bind(id)
        .all<{ user_id: string }>()
    ).results;
  return (
    <FeaturePage className="content-stack" data-source="live">
      <Link href={`${base}/projects`}>← Projects</Link>
      <h1>{project.title}</h1><FeatureTabs/><WidgetGrid pageKey="projects-detail">
      <p>{project.description}</p>
      {project.owner_user_id === actor.userId && (
        <ProjectEditor
          base={base}
          options={options}
          project={project}
          members={members.map((m) => m.user_id)}
        />
      )}
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
