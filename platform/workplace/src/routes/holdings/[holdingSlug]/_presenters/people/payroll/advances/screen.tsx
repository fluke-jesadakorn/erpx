import {PayrollEnvironment} from "@erpx/people-payroll-ui/components/payroll-environment";
import {payrollPurchaseControls} from "@erpx/people-payroll-server/public/payroll-purchases";
import { PayrollAdvanceConsole } from "@erpx/people-payroll-ui/components/payroll-controls";
import { PayrollPageIntro } from "@erpx/people-payroll-ui/components/payroll-workspace";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { payrollAdvancePage } from "@erpx/people-payroll-server/public/people-payroll";
import { PageContent } from "@erpx/platform-ui/components/foundation";

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
  const purchase = await payrollPurchaseControls(actor);
  const base = workspaceBase(publicScope(actor));
  return (
    <PayrollEnvironment purchase={purchase}><PageContent as="div" className="ui-page-content" data-source="live">
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
    </PageContent></PayrollEnvironment>
  );
}
