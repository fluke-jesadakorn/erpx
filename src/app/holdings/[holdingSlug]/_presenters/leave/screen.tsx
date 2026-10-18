import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { leavePageSnapshot } from "@/server/modules/people_payroll/public/leave-page-snapshot";
import { workforcePresentationContext } from "@/server/modules/people_payroll/public/workforce-presentation";
import { LeaveSelfService } from "@/components/people_payroll/leave/leave-self-service";

export const dynamic = "force-dynamic";

export default async function LeavePage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ month?: string | string[]; filter?: string | string[] }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const query = await searchParams;
  const snapshot = await leavePageSnapshot(actor, query);
  return (
    <div className="content" data-source="live">
      <LeaveSelfService
        key={workforcePresentationContext(actor)}
        base={workspaceBase(publicScope(actor))}
        initial={snapshot.summary}
        initialMonth={snapshot.month}
        initialFilter={snapshot.filter}
        snapshot={snapshot}
      />
    </div>
  );
}
