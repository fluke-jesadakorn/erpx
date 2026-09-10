import Link from "next/link";
import {
  StaffTaskLinks,
  TaskEditor,
} from "@/components/modules/staff-work-controls";
import { staffPage } from "@/server/staff-page";
import { listTemplates } from "@/server/routines";
export default async function Routines({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    { actor, write, base, options, projects, staff } =
      await staffPage(holdingSlug),
    templates = await listTemplates(write, actor);
  return (
    <main className="content-stack" data-source="live">
      <h1>Routine templates</h1>
      <p className="lede">
        Reusable company work. Every run has its own task and approval history.
      </p>
      <StaffTaskLinks base={base} />
      {staff && (
        <TaskEditor
          createAs="routine"
          base={base}
          options={options}
          projects={projects}
          userId={actor.userId}
        />
      )}
      <section className="card">
        <h2>Company templates</h2>
        {!templates.length && <p>No templates published yet.</p>}
        {templates.map((t) => (
          <Link
            className="row"
            key={t.id}
            href={`${base}/routine-templates/${t.id}`}
          >
            <strong>{t.title}</strong>
            <span>
              {t.published ? "Published" : "Draft"} · version {t.version}
            </span>
          </Link>
        ))}
      </section>
    </main>
  );
}
