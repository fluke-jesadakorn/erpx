import {ProjectPurchaseProvider} from "@/components/projects/purchase-context";
import { staffPickerDisplayOptions } from "@/lib/work_projection/staff-picker-display";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { staffPresentationQueries } from "@/server/modules/work_projection/public/presentation-context-queries";

import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { FeaturePage } from "@/components/shared/features/feature-page";
import { TaskEditor } from "@/components/work_projection/staff-work-controls";


import { RecordLink as Link } from "@/components/records/record-link";
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
  const { actor, queries, base, options, projects, staff } = await staffPresentationQueries(holdingSlug,{projectOptions:'optional'});
  const capabilities = await queries.getWorkCapabilities(actor);
  if (!capabilities.creation.some((item) => item.kind === "task")) {
    if (staff) notFound();
    return (
      <FeaturePage className="ui-page-content" data-source="live">
        <Link href={canonicalUiHref(`${base}/home`)}>← Today</Link>
        <h1>Connect your staff profile to create tasks</h1>
        <p>This account needs an active staff profile in this workspace before it can own a task.</p>
        <UiActionLink appearance="standard" variant="primary" href={canonicalUiHref(`${base}/people/settings`)}>Open People settings</UiActionLink>
      </FeaturePage>
    );
  }

  const purchases=projects.length?await queries.projectPurchases():undefined;
  return (
    <ProjectPurchaseProvider purchases={purchases}><FeaturePage className="ui-page-content" data-source="live">
      <Link href={canonicalUiHref(`${base}/home`)}>← Today</Link>
      <h1>Create a task</h1>
      <p>Give it a name and an owner. A deadline or review is only needed when the work calls for one.</p>
      <ProjectPurchaseProvider purchases={purchases} editorSource={{companyId:actor.companyId,actorId:actor.userId,available:capabilities.creation.some(item=>item.kind==="task"),record:null}}><TaskEditor appearance="standard"
        base={base}
        options={staffPickerDisplayOptions(options)}
        projects={projects.map(({id,title})=>({id,title}))}
        userId={actor.userId}
        companyId={actor.companyId}
        assignmentId={actor.assignmentId}
        initialDueDate={query.dueDate}
        initialProjectId={projects.some((project) => project.id === query.projectId) ? query.projectId : undefined}
      /></ProjectPurchaseProvider>
    </FeaturePage></ProjectPurchaseProvider>
  );
}
