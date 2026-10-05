import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { FinanceBusinessContractControls } from "@/components/finance/finance-business-contract-controls";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { hasPermission } from "@/lib/permissions";
import { businessContractWorkspace } from "@/server/finance-business-contracts";
import { financeCompanyTools } from "@/server/finance-company-tools";
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
      <FinanceWorkspace
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
