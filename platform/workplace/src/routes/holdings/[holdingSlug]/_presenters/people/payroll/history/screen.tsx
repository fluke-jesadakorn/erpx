import { PayrollHistoryContent } from "@erpx/people-payroll-ui/components/payroll-workspace";
import { ListPagination } from "@erpx/platform-ui/components/list-pagination";
import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { pageError } from "@erpx/work-projection-server/public/page-error";
import { ApiError } from "@erpx/platform-runtime-server/public/http";
import { payrollRunsPage } from "@erpx/people-payroll-server/public/people-payroll";
import { ownedPeoplePayrollRuns } from "@erpx/people-payroll-ui/public/native-payroll-summary-owned";
import { PageContent } from "@erpx/platform-ui/components/foundation";

export const dynamic = "force-dynamic";

export default async function PayrollHistory({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ cursor?: string }>;
}) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const query = await searchParams;
  const redacted = !hasPermission(actor, "payroll.read");
  let page;
  let runs;
  try {
    page = redacted ? null : await payrollRunsPage(actor, query.cursor);
    if (page) {
      runs = page.items;
    } else {
      const owned = await ownedPeoplePayrollRuns(actor, holdingSlug);
      if (owned === undefined)
        throw new ApiError(503, "NATIVE_PEOPLE_PAYROLL_UNAVAILABLE", "The native People Payroll reader is unavailable.");
      runs = owned;
    }
  } catch (error) {
    return pageError(error, actor.holdingSlug);
  }
  const base = workspaceBase(publicScope(actor));
  return (
    <PageContent as="div" className="ui-page-content" data-source="live">
      <PayrollHistoryContent base={base} userId={actor.userId} runs={runs} />
      {page && <ListPagination {...page} query={query} count={runs.length} />}
    </PageContent>
  );
}
