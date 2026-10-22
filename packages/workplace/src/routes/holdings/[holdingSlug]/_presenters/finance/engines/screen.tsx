import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { Table, TableFrame } from "@erpx/ui/components/foundation";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import {
  CalculationDecision,
  CalculationRunForm,
} from "@erpx/feature-finance/components/finance-engine-controls";
import { AccrualScheduleConsole } from "@erpx/feature-finance/components/finance-accrual-controls";
import { FinanceWorkspace, financeStyles as styles } from "@erpx/feature-finance/components/finance-workspace";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { financeEngineWorkspace } from "@erpx/server-finance/public/finance-engines";
import { accrualSchedulePage } from "@erpx/server-finance/public/finance-accruals";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { financeCompanyTools } from "@erpx/server-finance/public/finance-company-tools";
export const dynamic = "force-dynamic";
export default async function EnginesPage({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    [data, accruals, tools] = await Promise.all([
      financeEngineWorkspace(actor),
      accrualSchedulePage(actor),
      financeCompanyTools(actor),
    ]);
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace
        actor={actor}
        base={base}
        active="engines"
        title="Finance calculation engines"
        description="Calculate, review, and approve results using versioned source data and policies"
      >
        <Link href={canonicalUiHref(`${base}/finance/assets`)}>Fixed assets, monthly depreciation and disposal →</Link>
        {tools.some(t=>t.tool==="instruments"&&t.enabled===1)&&<Link href={canonicalUiHref(`${base}/finance/instruments`)}>Leases, fixed-rate loans and actual bank repayments →</Link>}
        {tools.some(t=>t.tool==="asset_policies"&&t.enabled===1)&&<Link href={canonicalUiHref(`${base}/finance/asset-policies`)}>Asset depreciation policies by book →</Link>}
        <details className={styles.disclosure}>
          <summary>Prepare a calculation</summary>
          <div>
            <CalculationRunForm base={base} periods={data.periods} />
          </div>
        </details>
        <details className={styles.disclosure}>
          <summary>Accrual schedules</summary>
          <div>
            <AccrualScheduleConsole
              base={base}
              data={accruals}
              canWrite={hasPermission(actor, "finance.write")}
              canApprove={hasPermission(actor, "finance.approve")}
            />
          </div>
        </details>
        <section className={styles.panel}>
          <div className={styles.sectionHead}>
            <div>
              <h2>Calculation runs</h2>
              <p className="muted">
                Prepared results do not create a journal or count as a filing until the relevant
                workflow is complete.
              </p>
            </div>
            <span className="ui-badge">{data.runs.length}</span>
          </div>
          {data.runs.length ? (
            <TableFrame appearance="standard"
              label="Records"
              className="business-table-wrap"
              tabIndex={0}
              role="region"
              aria-label="Financial table — scroll for more columns"
            >
              <Table className={styles.table}>
                <thead>
                  <tr>
                    <th>Type</th>
                    <th>Currency</th>
                    <th>Status</th>
                    <th>Prepared by</th>
                    <th>Result and decision</th>
                  </tr>
                </thead>
                <tbody>
                  {data.runs.map((run) => (
                    <tr key={String(run.id)}>
                      <td>
                        <Link href={canonicalUiHref(`${base}/record-details/calculation_run/${encodeURIComponent(String(run.id))}`)}>{data.labels[String(run.kind) as keyof typeof data.labels] ?? String(run.kind)}</Link>
                        <br />
                        <small className="muted">{String(run.source_hash).slice(0, 12)}</small>
                      </td>
                      <td>
                        {String(run.currency)} · {String(run.currency_scale)} decimals
                      </td>
                      <td>
                        <span className={styles.status}>{String(run.status)}</span>
                      </td>
                      <td>{String(run.prepared_by)}</td>
                      <td>
                        <details>
                          <summary>View calculation</summary>
                          <pre>{JSON.stringify(JSON.parse(String(run.result_json)), null, 2)}</pre>
                        </details>
                        <CalculationDecision base={base} run={run} />
                      </td>
                    </tr>
                  ))}
                </tbody>
              </Table>
            </TableFrame>
          ) : (
            <p className={styles.empty}>No calculation runs yet.</p>
          )}
        </section>
      </FinanceWorkspace>
    </FeaturePage>
  );
}
