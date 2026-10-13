import { FeaturePage } from "@/components/shared/features/feature-page";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { FinanceAssetPolicyControls } from "@/components/finance/finance-asset-policy-controls";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { hasPermission } from "@/lib/identity_access/permissions";
import { assetPolicyWorkspace } from "@/server/modules/finance/public/finance-asset-policies";
import { financeCompanyTools } from "@/server/modules/finance/public/finance-company-tools";
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
      <FinanceWorkspace
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
