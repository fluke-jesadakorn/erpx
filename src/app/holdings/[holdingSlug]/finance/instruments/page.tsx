import { FeaturePage } from "@/components/shared/features/feature-page";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { FinanceInstrumentControls } from "@/components/finance/finance-instrument-controls";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { hasPermission } from "@/lib/identity_access/permissions";
import { instrumentWorkspace } from "@/server/modules/finance/public/finance-instruments";
import { financeCompanyTools } from "@/server/modules/finance/public/finance-company-tools";
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
