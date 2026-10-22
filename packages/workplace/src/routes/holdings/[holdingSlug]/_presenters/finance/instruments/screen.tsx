import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace } from "@erpx/feature-finance/components/finance-workspace";
import { FinanceInstrumentControls } from "@erpx/feature-finance/components/finance-instrument-controls";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { instrumentWorkspace } from "@erpx/server-finance/public/finance-instruments";
import { financeCompanyTools } from "@erpx/server-finance/public/finance-company-tools";
export const dynamic = "force-dynamic";
export default async function InstrumentsPage({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    [data, tools] = await Promise.all([
      instrumentWorkspace(actor),
      financeCompanyTools(actor),
    ]),
    enabled = tools.some((t) => t.tool === "instruments" && t.enabled === 1);
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace
        actor={actor}
        base={base}
        active="accounting"
        title="Leases and loans"
        description="Reviewed terms, fixed monthly schedules and actual bank evidence"
      >
        <FinanceInstrumentControls
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
