import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Table, TableFrame } from "@/components/ui/foundation";
import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { hasPermission } from "@/lib/permissions";
import Link from "next/link";
import { requireWorkContext, publicScope } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import { listSupplierBatches, supplierBatchWorkspace } from "@/server/supplier-payments";
import { SupplierBatchConsole } from "@/components/modules/supplier-batch-console";
import { pageError } from "@/server/page-error";
import { Card } from "@/components/ui/foundation";
export const dynamic = "force-dynamic";
export default async function SupplierBatchesPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ new?: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor));
  const query = await searchParams;
  const [rows, data] = await Promise.all([
    listSupplierBatches(actor).catch((error) => pageError(error, actor.holdingSlug)),
    query.new === "1"
      ? supplierBatchWorkspace(actor).catch((error) => pageError(error, actor.holdingSlug))
      : null,
  ]);
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace
        base={base}
        active="cash"
        title={data ? "New supplier batch" : "Supplier payment batches"}
        description="Review invoice balances and beneficiaries, then continue through independent approval and bank confirmation."
        actions={
          !data &&
          hasPermission(actor, "finance.write") &&
          !actor.branchId &&
          !actor.departmentId ? (
            <UiActionLink appearance="standard" variant="primary" href={`${base}/finance/supplier-batches?new=1`}>
              Prepare batch
            </UiActionLink>
          ) : undefined
        }
      >
        {!data && (
          <Card as="section" appearance="standard" className="ui-panel">
            <h2>Recent batches</h2>
            {rows.length ? (
              <TableFrame appearance="standard"
                label="Records"
                className="business-table-wrap"
                tabIndex={0}
                role="region"
                aria-label="Supplier batches — scroll for more columns"
              >
                <Table className="business-table">
                  <thead>
                    <tr>
                      <th>Planned date</th>
                      <th>Currency</th>
                      <th>State</th>
                      <th>Pending instructions</th>
                    </tr>
                  </thead>
                  <tbody>
                    {rows.map((b) => (
                      <tr key={String(b.id)}>
                        <td>
                          <Link href={`${base}/finance/supplier-batches/${b.id}`}>
                            {String(b.planned_date)}
                          </Link>
                        </td>
                        <td>{String(b.currency)}</td>
                        <td>{String(b.status)}</td>
                        <td>
                          {String(b.pending_count)}
                          {Number(b.failed_count) > 0
                            ? ` · ${b.failed_count} failed with unpaid invoices`
                            : ""}
                          {Number(b.unknown_count) > 0 ? " · Bank result uncertain" : ""}
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </Table>
              </TableFrame>
            ) : (
              <p>No supplier payment batches in this Company.</p>
            )}
            {rows.length === 200 && <p>Showing the 200 most recently updated batches.</p>}
          </Card>
        )}
        {data && <SupplierBatchConsole base={base} data={data} />}
      </FinanceWorkspace>
    </FeaturePage>
  );
}
