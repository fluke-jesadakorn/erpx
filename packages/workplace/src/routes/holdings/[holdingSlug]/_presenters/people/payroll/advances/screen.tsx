import {PayrollEnvironment} from "@erpx/feature-people_payroll/components/payroll-environment";
import {payrollPurchaseControls} from "@erpx/server-people_payroll/public/payroll-purchases";
import { PayrollAdvanceConsole } from "@erpx/feature-people_payroll/components/payroll-controls";
import { PayrollPageIntro } from "@erpx/feature-people_payroll/components/payroll-workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { payrollAdvancePage } from "@erpx/server-people_payroll/public/people-payroll";
import { PageContent } from "@erpx/ui/components/foundation";

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
