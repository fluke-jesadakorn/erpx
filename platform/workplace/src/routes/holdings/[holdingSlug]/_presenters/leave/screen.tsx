import { requireWorkContext, publicScope } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { leavePageSnapshot } from "@erpx/people-payroll-server/public/leave-page-snapshot";
import { workforcePresentationContext } from "@erpx/people-payroll-server/public/workforce-presentation";
import { LeaveSelfService } from "@erpx/people-payroll-ui/components/leave/leave-self-service";

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
