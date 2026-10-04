import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { Button, Field, Select, Table, TableFrame, TextInput } from "@/components/ui/foundation";
import Link from "next/link";
import { BusinessDocumentBranding } from "@/components/brand/business-document";
import { RecordDocumentDrawer } from "@/components/attachments/record-document-drawer";
import { FeaturePage, FeatureWidget, WidgetGrid } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { BankImport, BankMatch } from "@/components/modules/bank-controls";
import { ListPagination } from "@/components/ui/list-pagination";
import { MoneyAmount } from "@/components/money/currency-provider";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { bankWorkspace, listCharts } from "@/server/finance";
import { pageError } from "@/server/page-error";
import { FilterBar } from "@/components/ui/foundation";

export const dynamic = "force-dynamic";
type Query = {
  print?: string;
  brand?: string;
  transactionCursor?: string;
  statementCursor?: string;
  match?: string;
  new?: string;
  chart?: string;
  view?: string;
  q?: string;
};
export default async function BankPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<Query>;
}) {
  const query = await searchParams,
    printing = query.print === "1";
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor));
  const [data, charts] = await Promise.all([bankWorkspace(actor, query), listCharts(actor)]).catch(
    (error) => pageError(error, actor.holdingSlug),
  );
  const link = (changes: Record<string, string | undefined>) => {
    const values = new URLSearchParams(
      Object.entries({ ...query, ...changes }).flatMap(([key, value]) =>
        value ? [[key, value]] : [],
      ),
    );
    return `${base}/finance/bank?${values}`;
  };
  return (
    <FeaturePage
      className="ui-page-content"
      data-source="live"
      data-print-document={printing || undefined}
    >
      <BusinessDocumentBranding
        actor={actor}
        branchId={null}
        requestedMode={query.brand}
        title="Bank reconciliation"
        printing={printing}
      />
      <FinanceWorkspace
        base={base}
        active="cash"
        title={query.new === "1" ? "Import bank statement" : "Bank reconciliation"}
        description="Review statement balances and match posted payments to bank evidence."
        actions={
          !printing && query.new !== "1" && hasPermission(actor, "finance.write") ? (
            <UiActionLink appearance="standard" variant="primary" href={link({ new: "1", match: undefined })}>
              Import statement
            </UiActionLink>
          ) : undefined
        }
      >
        <WidgetGrid pageKey="finance-bank">
          {!printing && query.new === "1" && hasPermission(actor, "finance.write") ? (
            <>
              <Link href={link({ new: undefined })}>← Back to bank reconciliation</Link>
              <FeatureWidget appearance="standard" id="finance-bank-import-statement" title="Import statement">
                <BankImport
                  base={base}
                  accounts={data.accounts.filter(
                    (account) => !query.chart || account.chart_id === query.chart,
                  )}
                  initialOpen
                  returnHref={link({ new: undefined })}
                />
              </FeatureWidget>
            </>
          ) : (
            <>
              {!printing && (
                <FilterBar className="ui-filter-bar" method="get">
                  <Field appearance="standard">
                    Chart
                    <Select appearance="standard" name="chart" defaultValue={query.chart ?? ""}>
                      <option value="">All charts</option>
                      {charts.map((chart) => (
                        <option key={String(chart.id)} value={String(chart.id)}>
                          {String(chart.name)} · {String(chart.base_currency)}
                        </option>
                      ))}
                    </Select>
                  </Field>
                  <Field appearance="standard">
                    View
                    <Select appearance="standard" name="view" defaultValue={query.view ?? "all"}>
                      <option value="all">All movements</option>
                      <option value="unmatched">Needs reconciliation</option>
                    </Select>
                  </Field>
                  <Field appearance="standard">
                    Search movements
                    <TextInput appearance="standard"
                      name="q"
                      defaultValue={query.q}
                      placeholder="Bank reference or description"
                    />
                  </Field>
                  <Button appearance="standard" type="submit" variant="secondary">
                    Apply filters
                  </Button>
                </FilterBar>
              )}
              {!printing &&
                query.match &&
                hasPermission(actor, "finance.approve") &&
                [data.selectedTransaction]
                  .filter((t): t is NonNullable<typeof t> => !!t && t.match_status !== "matched")
                  .map((t) => (
                    <section
                      className={styles.panel}
                      key={String(t.id)}
                      aria-labelledby="bank-match-title"
                    >
                      <div className={styles.sectionHead}>
                        <div>
                          <h2 id="bank-match-title">Review bank match</h2>
                          <p className="muted">
                            {String(t.external_id)} · {String(t.transaction_date)} ·{" "}
                            {String(t.currency)}
                          </p>
                        </div>
                        <Link href={link({ match: undefined })}>Close review</Link>
                      </div>
                      <BankMatch base={base} transaction={t} payments={data.payments} />
                    </section>
                  ))}
              <FeatureWidget
                id="finance-bank-transactions-and-matches"
                title="Transactions and matches"
              >
                <section className={styles.panel}>
                  <div className={styles.sectionHead}>
                    <h2>Bank movements</h2>
                    <span className="muted">{data.transactions.length} shown</span>
                  </div>
                  <TableFrame appearance="standard"
                    label="Records"
                    className="business-table-wrap"
                    tabIndex={0}
                    role="region"
                    aria-label="Financial table — scroll for more columns"
                  >
                    <Table className="business-table">
                      <thead>
                        <tr>
                          <th>Date</th>
                          <th>Bank reference / description</th>
                          <th>Statement</th>
                          <th className={styles.money}>Amount</th>
                          <th>Status</th>
                          <th>Next step</th>
                        </tr>
                      </thead>
                      <tbody>
                        {data.transactions.map((t) => (
                          <tr key={String(t.id)}>
                            <td>{String(t.transaction_date)}</td>
                            <td>
                              <strong>{String(t.external_id)}</strong>
                              <div className="muted">{String(t.description ?? "")}</div>
                            </td>
                            <td>{String(t.statement_number)}</td>
                            <td className={styles.money}>
                              <MoneyAmount
                                units={String(t.amount_minor)}
                                currency={String(t.currency)}
                                date={String(t.transaction_date)}
                                legacyScale={2}
                              />
                            </td>
                            <td>{String(t.match_status)}</td>
                            <td>
                              {t.match_status !== "matched" &&
                              hasPermission(actor, "finance.approve") &&
                              !printing ? (
                                <Link href={`${link({ match: String(t.id) })}#bank-match-title`}>
                                  Review match →
                                </Link>
                              ) : t.match_status === "matched" ? (
                                "Reconciled"
                              ) : (
                                "Awaiting Finance reviewer"
                              )}
                            </td>
                          </tr>
                        ))}
                      </tbody>
                    </Table>
                  </TableFrame>
                  {!data.transactions.length && (
                    <p className={styles.empty}>No bank movements match these filters.</p>
                  )}
                  {data.transactionPage && (
                    <ListPagination
                      {...data.transactionPage}
                      cursorName="transactionCursor"
                      query={query}
                      count={data.transactions.length}
                    />
                  )}
                </section>
              </FeatureWidget>
              <FeatureWidget appearance="standard" id="finance-bank-statements" title="Statements">
                <details className={styles.disclosure} open={printing || undefined}>
                  <summary>Statement balances & source files</summary>
                  <div>
                    {data.statements.map((s) => (
                      <div className={styles.statement} key={String(s.id)}>
                        <div>
                          <strong>{String(s.statement_number)}</strong>
                          <p className="muted">
                            {String(s.statement_date)} · {String(s.currency)} · Functional currency{" "}
                            {String(s.base_currency)}
                          </p>
                        </div>
                        <div>
                          Opening{" "}
                          <MoneyAmount
                            units={String(s.opening_minor)}
                            currency={String(s.currency)}
                            date={String(s.statement_date)}
                            legacyScale={2}
                          />
                          <br />
                          Closing{" "}
                          <MoneyAmount
                            units={String(s.closing_minor)}
                            currency={String(s.currency)}
                            date={String(s.statement_date)}
                            legacyScale={2}
                          />
                        </div>
                        {!printing && (
                          <RecordDocumentDrawer
                            base={base}
                            targetType="finance_bank_statement"
                            targetId={String(s.id)}
                            expectedTargetVersion={Number(s.version)}
                            editable={hasPermission(actor, "finance.write")}
                            title={`${String(s.statement_number)} · bank statement files`}
                            triggerLabel="Source files"
                            purposes={[
                              {
                                key: "bank_statement",
                                label: "Bank statement",
                                description:
                                  "Attach the source bank statement for this imported statement revision.",
                              },
                            ]}
                          />
                        )}
                      </div>
                    ))}
                    {!data.statements.length && (
                      <p className="muted">No imported statements yet.</p>
                    )}
                    {data.statementPage && (
                      <ListPagination
                        {...data.statementPage}
                        cursorName="statementCursor"
                        query={query}
                        count={data.statements.length}
                      />
                    )}
                  </div>
                </details>
              </FeatureWidget>
            </>
          )}
        </WidgetGrid>
      </FinanceWorkspace>
    </FeaturePage>
  );
}
