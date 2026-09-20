import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { leaveSummary } from "@/server/leave-self-service";
import { LeaveSelfService } from "@/components/leave/leave-self-service";

export const dynamic = "force-dynamic";

export default async function LeavePage({ params }: PageProps<"/holdings/[holdingSlug]/leave">) {
  const actor = await requireWorkContext((await params).holdingSlug);
  return <main className="content" data-source="live"><LeaveSelfService base={workspaceBase(publicScope(actor))} initial={await leaveSummary(actor)} /></main>;
}
