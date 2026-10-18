import { PayrollHistoryContent } from "@/components/people_payroll/payroll-workspace";
import { ListPagination } from "@/components/ui/list-pagination";
import { hasPermission } from "@/lib/identity_access/permissions";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { listPayrollRuns, payrollRunsPage } from "@/server/modules/people_payroll/public/people-payroll";
import { PageContent } from "@/components/ui/foundation";

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
