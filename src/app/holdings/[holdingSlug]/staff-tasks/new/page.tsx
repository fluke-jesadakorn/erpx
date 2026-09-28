import { FeaturePage } from "@/components/features/feature-page";
import { TaskEditor } from "@/components/modules/staff-work-controls";
import { getWorkCapabilities } from "@/server/work-capabilities";
import { staffPage } from "@/server/staff-page";
import Link from "next/link";
import { notFound } from "next/navigation";

export default async function NewTask({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ projectId?: string; dueDate?: string }>;
}) {
  const { holdingSlug } = await params;
  const query = await searchParams;
  const { actor, write, base, options, projects, staff } = await staffPage(holdingSlug);
  const capabilities = await getWorkCapabilities(write, actor);
  if (!capabilities.creation.some((item) => item.kind === "task")) {
    if (staff) notFound();
    return (
      <FeaturePage className="content-stack" data-source="live">
        <Link href={`${base}/home`}>← Today</Link>
        <h1>Connect your staff profile to create tasks</h1>
        <p>This account needs an active staff profile in this workspace before it can own a task.</p>
        <Link className="button" href={`${base}/people/settings`}>Open People settings</Link>
      </FeaturePage>
    );
  }

  return (
    <FeaturePage className="content-stack" data-source="live">
      <Link href={`${base}/home`}>← Today</Link>
      <h1>Create a task</h1>
      <p>Give it a name and an owner. A deadline or review is only needed when the work calls for one.</p>
      <TaskEditor
        base={base}
        options={options}
        projects={projects}
        userId={actor.userId}
        companyId={actor.companyId}
        assignmentId={actor.assignmentId}
        initialDueDate={query.dueDate}
        initialProjectId={projects.some((project) => project.id === query.projectId) ? query.projectId : undefined}
      />
    </FeaturePage>
  );
}
