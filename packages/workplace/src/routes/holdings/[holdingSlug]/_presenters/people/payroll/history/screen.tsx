import { PayrollHistoryContent } from "@erpx/feature-people_payroll/components/payroll-workspace";
import { ListPagination } from "@erpx/ui/components/list-pagination";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { listPayrollRuns, payrollRunsPage } from "@erpx/server-people_payroll/public/people-payroll";
import { PageContent } from "@erpx/ui/components/foundation";

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
    runs = page?.items ?? (await listPayrollRuns(actor));
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
