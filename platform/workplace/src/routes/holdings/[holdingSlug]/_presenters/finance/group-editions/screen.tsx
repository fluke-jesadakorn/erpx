import { FeaturePage } from "@erpx/platform-shared-ui/components/features/feature-page";
import { FinanceWorkspace } from "@erpx/finance-ui/components/finance-workspace";
import { FinanceGroupEditionControls } from "@erpx/finance-ui/components/finance-group-edition-controls";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { groupExpansionWorkspace } from "@erpx/finance-server/public/finance-group-editions";
import { financeCompanyTools } from "@erpx/finance-server/public/finance-company-tools";
import { pageError } from "@erpx/work-projection-server/public/page-error";
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
