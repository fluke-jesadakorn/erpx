import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { Button, Field, Select, Table, TableFrame } from "@erpx/ui/components/foundation";
import { JournalBatchReview } from "@erpx/feature-finance/components/journal-batch-review";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@erpx/feature-finance/components/finance-workspace";
import { JournalEditor } from "@erpx/feature-finance/components/ledger-controls";
import { ListPagination } from "@erpx/ui/components/list-pagination";
import { StatusBadge } from "@erpx/ui/components/primitives";
import { minorToDecimal } from "@erpx/feature-finance/lib/finance-money";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { ledgerWorkspace } from "@erpx/server-finance/public/ledger";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { FilterBar } from "@erpx/ui/components/foundation";

export const dynamic = "force-dynamic";
export default async function AccountingPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{ chart?: string; status?: string; cursor?: string; new?: string }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    query = await searchParams;
  const data = await ledgerWorkspace(actor, query.chart, query).catch((error) =>
    pageError(error, actor.holdingSlug),
  );
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace actor={actor}
        base={base}
        context={{
          chartId: String(data.chart.id),
          chartName: String(data.chart.name),
          currency: String(data.chart.base_currency),
        }}
        active="accounting"
        title={query.new === "1" ? "New journal" : "Journals"}
        description={`Chart ${String(data.chart.name)} · Base currency ${String(data.chart.base_currency)}`}
        actionsPurpose="new-work" actions={
          hasPermission(actor, "finance.write") && query.new !== "1" ? (
            <UiActionLink appearance="standard"
              variant="primary"
              href={canonicalUiHref(`${base}/finance/accounting?chart=${encodeURIComponent(String(data.chart.id))}&new=1`)}
            >
              Create journal
            </UiActionLink>
          ) : undefined
        }
      >
        <FilterBar className="ui-filter-bar">
          <Field appearance="standard">
            Chart of accounts
            <Select appearance="standard" name="chart" defaultValue={String(data.chart.id)}>
              {data.charts.map((c) => (
                <option value={String(c.id)} key={String(c.id)}>
                  {String(c.name)} · {String(c.base_currency)}
                </option>
              ))}
            </Select>
          </Field>
          <Field appearance="standard">
            Status
            <Select appearance="standard" name="status" defaultValue={query.status ?? "all"}>
              <option value="all">All</option>
              <option value="draft">Draft</option>
              <option value="in_review">In review</option>
              <option value="posted">Posted</option>
              <option value="reversed">Reversed</option>
            </Select>
          </Field>
          <Button appearance="standard" type="submit" variant="secondary">
            Show entries
          </Button>
        </FilterBar>
        {query.new === "1" && hasPermission(actor, "finance.write") ? (
          <section id="new-journal">
            <Link
              href={canonicalUiHref(`${base}/finance/accounting?chart=${encodeURIComponent(String(data.chart.id))}`)}
            >
              ← Back to journals
            </Link>
            <JournalEditor
              base={base}
              nativeCreate={process.env.ERPX_NATIVE_FINANCE_JOURNAL_CREATE_CONFIG === undefined ? undefined : {context: actor.authority && actor.iam ? {
                userId: actor.userId, holdingId: actor.holdingId, companyId: actor.companyId,
                branchId: actor.branchId, departmentId: actor.departmentId,
                branchSlug: actor.scope.kind === 'holding' ? null : actor.scope.branchSlug ?? null,
                departmentSlug: actor.scope.kind === 'department' ? actor.scope.departmentSlug : null,
                assignmentId: actor.assignmentId, assignmentVersion: actor.authority.assignmentVersion,
                revision: actor.authority.revision, role: actor.role, assignmentScopeKind: actor.assignmentScope,
                modules: [...actor.modules], permissions: [...actor.iam.permissions],
              } : undefined}}
              chart={data.chart}
              accounts={data.accounts}
              periods={data.periods}
              dimensions={data.dimensions}
              dimensionValues={data.dimensionValues}
              initialOpen
            />
          </section>
        ) : (
          <>
            {hasPermission(actor, "finance.approve") &&
              data.journals.some(
                (j) =>
                  ["draft", "in_review"].includes(String(j.status)) &&
                  ["manual", "opening_import"].includes(String(j.source_type)) &&
                  j.created_by !== actor.userId,
              ) && (
                <details className={styles.disclosure}>
                  <summary>Batch posting review</summary>
                  <div>
                    <JournalBatchReview
                      base={base}
                      journals={data.journals}
                      periods={data.periods}
                      userId={actor.userId}
                    />
                  </div>
                </details>
              )}
            <section className={styles.panel}>
              <div className={styles.sectionHead}>
                <div>
                  <h2>Journal entries</h2>
                  <p className="muted">
                    Open an entry to review its preparer, evidence, and debit/credit impact
                  </p>
                </div>
                <span className="ui-badge">{data.journals.length}</span>
              </div>
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
                      <th>Number</th>
                      <th>Date</th>
                      <th>Source</th>
                      <th className={styles.money}>Debit</th>
                      <th className={styles.money}>Credit</th>
                      <th>Status</th>
                    </tr>
                  </thead>
                  <tbody>
                    {data.journals.map((j) => (
                      <tr key={String(j.id)}>
                        <td>
                          <Link href={canonicalUiHref(`${base}/finance/journals/${j.id}`)}>
                            {String(j.journal_number)}
                          </Link>
                        </td>
                        <td>{String(j.entry_date)}</td>
                        <td>{String(j.source_type)}</td>
                        <td className={styles.money}>
                          {String(j.currency)} {minorToDecimal(Number(j.debit))}
                        </td>
                        <td className={styles.money}>
                          {String(j.currency)} {minorToDecimal(Number(j.credit))}
                        </td>
                        <td>
                          <StatusBadge appearance="standard" status={String(j.status)} />
                        </td>
                      </tr>
                    ))}
                  </tbody>
                </Table>
              </TableFrame>
              {!data.journals.length && (
                <p className={styles.empty}>No journals match these filters.</p>
              )}
              {data.journalPage && (
                <ListPagination {...data.journalPage} query={query} count={data.journals.length} />
              )}
            </section>
          </>
        )}
      </FinanceWorkspace>
    </FeaturePage>
  );
}
