import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { pageError } from "@/server/page-error";
import { collaborationWorkspace } from "@/server/collaboration-work";
import { CollaborationWorkConsole } from "@/components/modules/collaboration-work-console";
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
    <div className="content-stack">
      <Link href={`${base}/work`}>Work</Link>
      <h1>Cross-record collaboration</h1>
      <p>
        Keep typed backlinks, accountable next actions and authorized
        conversation files connected to their source records.
      </p>
      <CollaborationWorkConsole base={base} data={data} />
    </div>
  );
}
