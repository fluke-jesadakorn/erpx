import Link from "next/link";
import { notFound } from "next/navigation";
import { ProjectEditor } from "@/components/modules/staff-work-controls";
import { staffPage } from "@/server/staff-page";
import { getProject, listTasks } from "@/server/staff-tasks";
import { ApiError } from "@/server/http";
export default async function Project({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    { actor, write, base, options } = await staffPage(holdingSlug),
    project = await getProject(write, actor, id).catch((e) => {
      if (e instanceof ApiError && e.status === 404) notFound();
      throw e;
    }),
    tasks = await listTasks(write, actor, id),
    members = (
      await write
        .prepare("SELECT user_id FROM staff_project_members WHERE project_id=?")
        .bind(id)
        .all<{ user_id: string }>()
    ).results;
  return (
    <main className="content-stack" data-source="live">
      <Link href={`${base}/projects`}>← Projects</Link>
      <h1>{project.title}</h1>
      <p>{project.description}</p>
      {project.owner_user_id === actor.userId && (
        <ProjectEditor
          base={base}
          options={options}
          project={project}
          members={members.map((m) => m.user_id)}
        />
      )}
      <section className="card">
        <h2>Project tasks</h2>
        <Link className="button" href={`${base}/work?new=task`}>
          Create a task
        </Link>
        {tasks.map((t) => (
          <Link className="row" key={t.id} href={`${base}/staff-tasks/${t.id}`}>
            {t.title} · {t.state.replaceAll("_", " ")} · {t.assignee_name}
          </Link>
        ))}
      </section>
    </main>
  );
}
