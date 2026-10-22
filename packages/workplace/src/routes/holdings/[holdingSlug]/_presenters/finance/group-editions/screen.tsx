import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace } from "@erpx/feature-finance/components/finance-workspace";
import { FinanceGroupEditionControls } from "@erpx/feature-finance/components/finance-group-edition-controls";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { groupExpansionWorkspace } from "@erpx/server-finance/public/finance-group-editions";
import { financeCompanyTools } from "@erpx/server-finance/public/finance-company-tools";
import { pageError } from "@erpx/server-work_projection/public/page-error";
export const dynamic = "force-dynamic";
export default async function GroupEditionsPage({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const slug = (await params).holdingSlug,
    actor = await requireWorkContext(slug),
    base = workspaceBase(publicScope(actor)),
    [data, tools] = await Promise.all([
      groupExpansionWorkspace(actor).catch((e) => pageError(e, slug)),
      financeCompanyTools(actor),
    ]);
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace
        actor={actor}
        base={base}
        active="group"
        title="Group reporting editions"
        description="Reviewed Company books, aligned closed periods and retained group adjustments"
      >
        <FinanceGroupEditionControls
          base={base}
          data={data}
          enabled={tools.some(
            (t) => t.tool === "group_reporting" && t.enabled === 1,
          )}
          canWrite={hasPermission(actor, "finance.write")}
          canApprove={hasPermission(actor, "finance.approve")}
          userId={actor.userId}
        />
      </FinanceWorkspace>
    </FeaturePage>
  );
}
