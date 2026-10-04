import { PayrollAdvanceConsole } from "@/components/modules/payroll-controls";
import { PayrollPageIntro } from "@/components/modules/payroll-workspace";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { pageError } from "@/server/page-error";
import { payrollAdvancePage } from "@/server/people-payroll";
import { PageContent } from "@/components/ui/foundation";

export const dynamic = "force-dynamic";

export default async function PayrollAdvances({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  let data;
  try {
    data = await payrollAdvancePage(actor);
  } catch (error) {
    return pageError(error, actor.holdingSlug);
  }
  const base = workspaceBase(publicScope(actor));
  return (
    <PageContent as="div" className="ui-page-content" data-source="live">
      <PayrollPageIntro
        base={base}
        title="Advances"
        description="Requests, approvals, and repayment schedules in one place."
      />
      <PayrollAdvanceConsole appearance="standard"
        base={base}
        data={data}
        userId={actor.userId}
        canPrepare={hasPermission(actor, "payroll.prepare")}
        canApprove={hasPermission(actor, "payroll.approve")}
      />
    </PageContent>
  );
}
