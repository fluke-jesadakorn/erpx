import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { collaborationWorkspace } from "@/server/modules/collaboration/public/collaboration-work";
import { CollaborationWorkConsole } from "@/components/modules/collaboration-work-console";
import { PageContent } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function Page({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params,
    a = await requireWorkContext(holdingSlug),
    base = workspaceBase(publicScope(a)),
    data = await collaborationWorkspace(a).catch((e) =>
      pageError(e, holdingSlug),
    );
  return (
    <PageContent as="div" className="ui-page-content">
      <Link href={`${base}/work`}>Work</Link>
      <h1>Cross-record collaboration</h1>
      <p>
        Keep typed backlinks, accountable next actions and authorized
        conversation files connected to their source records.
      </p>
      <CollaborationWorkConsole base={base} data={data} />
    </PageContent>
  );
}
