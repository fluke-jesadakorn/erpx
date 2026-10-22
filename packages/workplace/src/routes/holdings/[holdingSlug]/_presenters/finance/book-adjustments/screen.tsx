import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace } from "@erpx/feature-finance/components/finance-workspace";
import { FinanceBookAdjustmentControls } from "@erpx/feature-finance/components/finance-book-adjustment-controls";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { bookAdjustmentConsoleWorkspace } from "@erpx/server-finance/public/finance-book-adjustments";
export const dynamic = "force-dynamic";
export default async function BookAdjustmentsPage({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    data = await bookAdjustmentConsoleWorkspace(actor);
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace
        actor={actor}
        base={base}
        active="accounting"
        title="Secondary book adjustments"
        description="Reviewed policy differences linked to the business source"
      >
        <FinanceBookAdjustmentControls
          base={base}
          data={data}
          canWrite={hasPermission(actor, "finance.write")}
          canApprove={hasPermission(actor, "finance.approve")}
          userId={actor.userId}
        />
      </FinanceWorkspace>
    </FeaturePage>
  );
}
