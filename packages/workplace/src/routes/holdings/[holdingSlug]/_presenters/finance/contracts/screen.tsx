import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace } from "@erpx/feature-finance/components/finance-workspace";
import { FinanceBusinessContractControls } from "@erpx/feature-finance/components/finance-business-contract-controls";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { businessContractWorkspace } from "@erpx/server-finance/public/finance-business-contracts";
import { financeCompanyTools } from "@erpx/server-finance/public/finance-company-tools";
export const dynamic = "force-dynamic";
export default async function BusinessContractsPage({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    [data, tools] = await Promise.all([
      businessContractWorkspace(actor),
      financeCompanyTools(actor),
    ]),
    enabled = tools.some(
      (t) => t.tool === "business_contracts" && t.enabled === 1,
    );
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace actor={actor}
        base={base}
        active="receivables"
        title="Business contracts and billing"
        description="Service agreements, ordinary invoices and retained revenue recognition"
      >
        <FinanceBusinessContractControls
          base={base}
          data={data}
          canWrite={enabled && hasPermission(actor, "finance.write")}
          canApprove={hasPermission(actor, "finance.approve")}
          userId={actor.userId}
        />
      </FinanceWorkspace>
    </FeaturePage>
  );
}
