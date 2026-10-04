import { PayrollHistoryContent } from "@/components/modules/payroll-workspace";
import { ListPagination } from "@/components/ui/list-pagination";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { pageError } from "@/server/page-error";
import { listPayrollRuns, payrollRunsPage } from "@/server/people-payroll";
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
