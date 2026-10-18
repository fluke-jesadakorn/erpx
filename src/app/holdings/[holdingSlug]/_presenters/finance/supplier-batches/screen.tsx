import {financePurchaseControls} from "@/server/modules/finance/public/purchases";
import {financeActionAllowed} from "@/server/modules/finance/public/rules/purchase-controls";
import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { Table, TableFrame } from "@/components/ui/foundation";
import { FeaturePage } from "@/components/shared/features/feature-page";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";
import { hasPermission } from "@/lib/identity_access/permissions";
import { RecordLink as Link } from "@/components/records/record-link";
import { requireWorkContext, publicScope } from "@/server/modules/identity_access/public/authorization";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { listSupplierBatches, supplierBatchWorkspace } from "@/server/modules/procurement/public/supplier-payments";
import { SupplierBatchConsole,SupplierBatchPreparation } from "@/components/procurement/supplier-batch-console";
import { pageError } from "@/server/modules/work_projection/public/page-error";
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
  const query = await searchParams,purchases=await financePurchaseControls(actor),canPrepare=hasPermission(actor,"finance.write")&&!actor.branchId&&!actor.departmentId&&financeActionAllowed(purchases,"settle-existing","finance.banking");
  const [rows, data] = await Promise.all([
    listSupplierBatches(actor).catch((error) => pageError(error, actor.holdingSlug)),
    canPrepare
      ? supplierBatchWorkspace(actor).catch((error) => pageError(error, actor.holdingSlug))
      : null,
  ]);
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace actor={actor}
        base={base}
        active="cash"
        title={query.new==="1" ? "New supplier batch" : "Supplier payment batches"}
        description="Review invoice balances and beneficiaries, then continue through independent approval and bank confirmation."
        actions={
          query.new!=="1" ? (
            <SupplierBatchPreparation owner={{companyId:actor.companyId,userId:actor.userId,assignmentId:actor.assignmentId,branchId:actor.branchId,departmentId:actor.departmentId}} base={base} data={data}/>
          ) : undefined
        }
      >
        {query.new!=="1" && (
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
                          <Link href={canonicalUiHref(`${base}/finance/supplier-batches/${b.id}`)}>
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
        {query.new==="1" && <SupplierBatchConsole owner={{companyId:actor.companyId,userId:actor.userId,assignmentId:actor.assignmentId,branchId:actor.branchId,departmentId:actor.departmentId}} base={base} data={data} />}
      </FinanceWorkspace>
    </FeaturePage>
  );
}
