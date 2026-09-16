import { pageError } from "@/server/page-error";
import Link from "next/link";
import {
  TaskEditor,
  RoutineScheduleControls,
} from "@/components/modules/staff-work-controls";
import { staffPage } from "@/server/staff-page";
import {
  getTemplate,
  canEditTemplate,
  type ScheduleRow,
} from "@/server/routines";
export default async function Routine({
  params,
}: {
  params: Promise<{ holdingSlug: string; id: string }>;
}) {
  const { holdingSlug, id } = await params,
    { actor, write, base, options, projects } = await staffPage(holdingSlug),
    template = await getTemplate(write, actor, id).catch((e) => {
      return pageError(e, holdingSlug);
    }),
    canEdit = await canEditTemplate(write, actor, template),
    schedules = (
      await write
        .prepare(
          "SELECT * FROM routine_schedules WHERE template_id=? AND company_id=? ORDER BY created_at",
        )
        .bind(id, actor.companyId)
        .all<ScheduleRow>()
    ).results;
  return (
    <main className="content-stack" data-source="live">
      <Link href={`${base}/routine-templates`}>← Routine templates</Link>
      <h1>{template.title}</h1>
      <p>{template.description}</p>
      <p>
        Template version {template.version} ·{" "}
        {template.published ? "Published" : "Draft"}. Edits affect future
        occurrences.
      </p>
      {canEdit && (
        <TaskEditor
          key={template.version}
          base={base}
          options={options}
          projects={projects}
          userId={actor.userId}
          template={template}
        />
      )}
      <TaskEditor
        key={`clone-${template.version}`}
        base={base}
        options={options}
        projects={projects}
        userId={actor.userId}
        template={template}
        clone
      />
      <RoutineScheduleControls
        base={base}
        template={template}
        schedules={schedules}
        canEdit={canEdit}
      />
    </main>
  );
}
