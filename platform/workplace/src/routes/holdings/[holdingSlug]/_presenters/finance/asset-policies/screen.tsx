import { FeaturePage } from "@erpx/platform-shared-ui/components/features/feature-page";
import { FinanceWorkspace } from "@erpx/finance-ui/components/finance-workspace";
import { FinanceAssetPolicyControls } from "@erpx/finance-ui/components/finance-asset-policy-controls";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { assetPolicyWorkspace } from "@erpx/finance-server/public/finance-asset-policies";
import { financeCompanyTools } from "@erpx/finance-server/public/finance-company-tools";
export const dynamic = "force-dynamic";
export default async function AssetPoliciesPage({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    [data, tools] = await Promise.all([
      assetPolicyWorkspace(actor),
      financeCompanyTools(actor),
    ]),
    enabled = tools.some((t) => t.tool === "asset_policies" && t.enabled === 1);
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace actor={actor}
        base={base}
        active="accounting"
        title="Asset depreciation policies"
        description="Independent book policies, monthly review and retained corrections"
      >
        <FinanceAssetPolicyControls
          base={base}
          data={data}
          enabled={enabled}
          canWrite={hasPermission(actor, "finance.write")}
          canApprove={hasPermission(actor, "finance.approve")}
          userId={actor.userId}
        />
      </FinanceWorkspace>
    </FeaturePage>
  );
}
