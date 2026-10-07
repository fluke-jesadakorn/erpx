import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { FinanceBookAdjustmentControls } from "@/components/finance/finance-book-adjustment-controls";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/workspace";
import { hasPermission } from "@/lib/permissions";
import { bookAdjustmentConsoleWorkspace } from "@/server/modules/finance/public/finance-book-adjustments";
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
