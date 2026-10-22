import { requireWorkContext, publicScope } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { leavePageSnapshot } from "@erpx/server-people_payroll/public/leave-page-snapshot";
import { workforcePresentationContext } from "@erpx/server-people_payroll/public/workforce-presentation";
import { LeaveSelfService } from "@erpx/feature-people_payroll/components/leave/leave-self-service";

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
