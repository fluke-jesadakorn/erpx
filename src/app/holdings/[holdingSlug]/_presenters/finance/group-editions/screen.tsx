import { FeaturePage } from "@/components/shared/features/feature-page";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { FinanceGroupEditionControls } from "@/components/finance/finance-group-edition-controls";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { hasPermission } from "@/lib/identity_access/permissions";
import { groupExpansionWorkspace } from "@/server/modules/finance/public/finance-group-editions";
import { financeCompanyTools } from "@/server/modules/finance/public/finance-company-tools";
import { pageError } from "@/server/modules/work_projection/public/page-error";
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
