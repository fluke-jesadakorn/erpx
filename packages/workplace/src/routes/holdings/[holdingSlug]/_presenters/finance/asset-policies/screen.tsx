import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace } from "@erpx/feature-finance/components/finance-workspace";
import { FinanceAssetPolicyControls } from "@erpx/feature-finance/components/finance-asset-policy-controls";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { assetPolicyWorkspace } from "@erpx/server-finance/public/finance-asset-policies";
import { financeCompanyTools } from "@erpx/server-finance/public/finance-company-tools";
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
