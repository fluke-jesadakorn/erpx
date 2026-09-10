import Link from "next/link";
import {
  ProjectEditor,
  StaffTaskLinks,
} from "@/components/modules/staff-work-controls";
import { staffPage } from "@/server/staff-page";
export default async function Projects({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    { base, options, projects, staff } = await staffPage(holdingSlug);
  return (
    <main className="content-stack" data-source="live">
      <h1>Projects</h1>
      <StaffTaskLinks base={base} />
      {staff && <ProjectEditor base={base} options={options} />}
      <section className="card">
        <h2>Your projects</h2>
        {!projects.length && <p>No projects yet.</p>}
        {projects.map((p) => (
          <Link className="row" key={p.id} href={`${base}/projects/${p.id}`}>
            <div>
              <strong>{p.title}</strong>
              <p>{p.description}</p>
            </div>
          </Link>
        ))}
      </section>
    </main>
  );
}
