import {DestinationFilterForm} from "@erpx/ui/components/destination-filter-form";
import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { Button, Field, Select, Table, TableFrame, TextInput } from "@erpx/ui/components/foundation";
import { FeaturePage, FeatureWidget, WidgetGrid } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@erpx/feature-finance/components/finance-workspace";
import { financeCompanyTools } from "@erpx/server-finance/public/finance-company-tools";
import { listCharts } from "@erpx/server-finance/public/finance";
import { ListPagination } from "@erpx/ui/components/list-pagination";
import { InvoiceEditor } from "@erpx/feature-finance/components/lazy-invoice-editor";
import { MoneyAmount } from "@erpx/feature-finance/components/money/currency-provider";
import { formatMoneyUnits } from "@erpx/feature-finance/lib/company-money";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { aging, invoiceWorkspace, invoicesPage } from "@erpx/server-finance/public/finance";
import { pageError } from "@erpx/server-work_projection/public/page-error";
import { RecordLink as Link } from "@erpx/feature-records/components/record-link";
import { Card } from "@erpx/ui/components/foundation";
export const dynamic = "force-dynamic";
type AgingRow = {
  bucket: string;
  currency: string;
  due_date: string;
  outstanding_minor: number | string;
};
type AgingSummary = {
  bucket: string;
  currency: string;
  amountMinor: bigint;
  count: number;
  oldestDue: string;
};
function summarizeAging(rows: AgingRow[]): AgingSummary[] {
  const grouped = new Map<string, AgingSummary>();
  for (const row of rows) {
    const key = `${String(row.bucket)}:${String(row.currency)}`;
    const current = grouped.get(key) ?? {
      bucket: String(row.bucket),
      currency: String(row.currency),
      amountMinor: BigInt(0),
      count: 0,
      oldestDue: String(row.due_date),
    };
    current.amountMinor += BigInt(row.outstanding_minor);
    current.count += 1;
    if (String(row.due_date) < current.oldestDue) current.oldestDue = String(row.due_date);
    grouped.set(key, current);
  }
  return [...grouped.values()].sort(
    (a, b) => a.bucket.localeCompare(b.bucket) || a.currency.localeCompare(b.currency),
  );
}
export default async function InvoicesPage({
  params,
  searchParams,
}: {
  params: Promise<{ holdingSlug: string }>;
  searchParams: Promise<{
    q?: string;
    kind?: string;
    status?: string;
    queue?: string;
    cursor?: string;
    new?: string;
    asOf?: string;
    chartId?: string;
  }>;
}) {
  const actor = await requireWorkContext((await params).holdingSlug),
    base = workspaceBase(publicScope(actor)),
    search = await searchParams,
    asOf = search.asOf || new Date().toISOString().slice(0, 10);
  const [page, charts, data, receivableAging, payableAging, tools] = await Promise.all([
    invoicesPage(actor, search),
    listCharts(actor),
    search.new === "1" && hasPermission(actor, "finance.write") ? invoiceWorkspace(actor) : null,
    search.kind === "payable" ? [] : aging(actor, "receivable", asOf, search.chartId),
    search.kind === "receivable" ? [] : aging(actor, "payable", asOf, search.chartId),
    financeCompanyTools(actor),
  ]).catch((error) => pageError(error, actor.holdingSlug));
  const rows = page.items,
    receivableSummary = summarizeAging(receivableAging as unknown as AgingRow[]),
    payableSummary = summarizeAging(payableAging as unknown as AgingRow[]);
  const agingCard = (
    title: string,
    summary: AgingSummary[],
    legacyCount: number,
    kind: "receivable" | "payable",
  ) => (
    <Card as="section" appearance="standard" className="ui-panel" aria-labelledby={`${kind}-aging-title`}>
      <div className="section-heading">
        <div>
          <h2 id={`${kind}-aging-title`}>{title}</h2>
          <p className="muted">
            Open posted obligations grouped by days overdue as of {asOf}. Amounts remain separated
            by currency. Open an invoice for its frozen exchange equivalent.
          </p>
        </div>
        <Link href={canonicalUiHref(`${base}/finance/invoices?kind=${kind}&queue=open`)}>Open {kind}s →</Link>
      </div>
      {legacyCount > 0 && (
        <p className="alert">
          {legacyCount} legacy documents lack dated recognition or settlement journals. Retain
          opening-balance evidence before using these historical amounts for period close.
        </p>
      )}
      {summary.length ? (
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
                <th>Bucket</th>
                <th>Currency</th>
                <th>Open documents</th>
                <th>Outstanding</th>
                <th>Oldest due</th>
              </tr>
            </thead>
            <tbody>
              {summary.map((item) => (
                <tr key={`${item.bucket}:${item.currency}`}>
                  <td><Link href={canonicalUiHref(`${base}/record-details/aging_bucket/${encodeURIComponent(item.bucket)}`)} record={{kind:"aging_bucket",id:item.bucket,args:{kind,asOf,currency:item.currency,...(search.chartId?{chartId:search.chartId}:{})}}}>{item.bucket}</Link></td>
                  <td>{item.currency}</td>
                  <td>{item.count}</td>
                  <td>{formatMoneyUnits(item.amountMinor, item.currency, 2)}</td>
                  <td>{item.oldestDue}</td>
                </tr>
              ))}
            </tbody>
          </Table>
        </TableFrame>
      ) : (
        <p>No open {kind}s.</p>
      )}
    </Card>
  );
  const title =
    search.kind === "payable"
      ? "Payables"
      : search.kind === "receivable"
        ? "Receivables"
        : "Receivables and payables";
  const createQuery = new URLSearchParams(
    Object.entries(search).flatMap(([key, value]) => (value ? [[key, value]] : [])),
  );
  createQuery.delete("cursor");
  createQuery.set("new", "1");
  const createHref = canonicalUiHref(`${base}/finance/invoices?${createQuery}`);
  const listQuery = new URLSearchParams(
    Object.entries(search).flatMap(([key, value]) => (value ? [[key, value]] : [])),
  );
  listQuery.delete("new");
  listQuery.delete("cursor");
  const chart = charts.find((chart) => chart.id === search.chartId);
  return (
    <FeaturePage className="ui-page-content" data-source="live">
      <FinanceWorkspace actor={actor}
        base={base}
        context={
          chart
            ? {
                chartId: String(chart.id),
                chartName: String(chart.name),
                currency: String(chart.base_currency),
              }
            : undefined
        }
        active={search.kind === "payable" ? "payables" : "receivables"}
        title={search.new === "1" ? "New document" : title}
        description="Review source documents, dates, tax and accounts. Posted obligations continue through payment and bank reconciliation."
        actionsPurpose="new-work" actions={
          hasPermission(actor, "finance.write") && search.new !== "1" ? (
            <UiActionLink appearance="standard" variant="primary" href={createHref}>
              Add document
            </UiActionLink>
          ) : undefined
        }
      >
        {tools.some(t=>t.tool==="business_contracts"&&t.enabled===1)&&search.kind!=="payable"&&<UiActionLink appearance="standard" href={canonicalUiHref(`${base}/finance/contracts`)}>Contracts and billing</UiActionLink>}
        <WidgetGrid pageKey="finance-invoices">
          {data ? (
            <>
              <Link href={canonicalUiHref(`${base}/finance/invoices?${listQuery}`)}>← Back to documents</Link>
              <FeatureWidget appearance="standard" id="finance-invoices-invoice-details" title="Invoice details">
                <InvoiceEditor
                  base={base}
                  options={data}
                  initialOpen
                  initialKind={search.kind === "payable" ? "payable" : "receivable"}
                  initialChartId={search.chartId}
                />
              </FeatureWidget>
            </>
          ) : (
            <>
              <DestinationFilterForm className={`business-form ${styles.filterForm}`} method="get">
                <Field appearance="standard">
                  Search documents
                  <TextInput appearance="standard"
                    name="q"
                    defaultValue={search.q}
                    placeholder="Invoice number or party"
                  />
                </Field>
                <Field appearance="standard">
                  Status
                  <Select appearance="standard" name="status" defaultValue={search.status ?? ""}>
                    <option value="">All statuses</option>
                    {["draft", "posted", "partially_paid", "paid", "voided", "no_charge_confirmed"].map((value) => (
                      <option key={value} value={value}>
                        {value==="no_charge_confirmed"?"No-charge confirmed":value.replaceAll("_", " ")}
                      </option>
                    ))}
                  </Select>
                </Field>
                <Button appearance="standard" type="submit" variant="secondary">
                  Apply filters
                </Button>
                <details className={styles.filterDetails}>
                  <summary>
                    More filters · {chart ? String(chart.name) : "All charts"}
                    {search.queue === "open" ? " · Open obligations" : ""}
                  </summary>
                  <div>
                    <Field appearance="standard">
                      Chart
                      <Select appearance="standard" name="chartId" defaultValue={search.chartId ?? ""}>
                        <option value="">All charts</option>
                        {charts.map((chart) => (
                          <option value={String(chart.id)} key={String(chart.id)}>
                            {String(chart.name)} · {String(chart.base_currency)}
                          </option>
                        ))}
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Type
                      <Select appearance="standard" name="kind" defaultValue={search.kind ?? ""}>
                        <option value="">All types</option>
                        <option value="receivable">Receivables</option>
                        <option value="payable">Payables</option>
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Queue
                      <Select appearance="standard" name="queue" defaultValue={search.queue ?? ""}>
                        <option value="">All documents</option>
                        <option value="open">Open posted obligations</option>
                      </Select>
                    </Field>
                    <Field appearance="standard">
                      Aging as of
                      <TextInput appearance="standard" type="date" name="asOf" defaultValue={asOf} />
                    </Field>
                  </div>
                </details>
              </DestinationFilterForm>
              <section className={styles.panel} aria-labelledby="documents-list-title">
                <div className={styles.sectionHead}>
                  <h2 id="documents-list-title">Documents</h2>
                  <span className="muted">{rows.length} shown</span>
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
                        <th>Invoice</th>
                        <th>Party</th>
                        <th>Due date</th>
                        <th className={styles.money}>Total</th>
                        <th className={styles.money}>Outstanding</th>
                        <th>Status</th>
                      </tr>
                    </thead>
                    <tbody>
                      {rows.map((i) => (
                        <tr key={String(i.id)}>
                          <td>
                            <Link
                              href={canonicalUiHref(`${base}/finance/invoices/${i.id}?kind=${i.kind}${search.chartId ? `&chartId=${encodeURIComponent(search.chartId)}` : ""}`)}
                            >
                              {String(i.invoice_number)}
                            </Link>
                          </td>
                          <td>{String(i.party_name)}</td>
                          <td>{String(i.due_date)}</td>
                          <td className={styles.money}>
                            <MoneyAmount
                              units={String(i.total_minor)}
                              currency={String(i.currency)}
                              date={String(i.invoice_date)}
                              legacyScale={2}
                              snapshotUsdUnits={i.snapshot_usd_units as string | null}
                              snapshotSourceUnits={i.snapshot_source_units as string | null}
                              snapshotRateId={i.snapshot_rate_id as string | null}
                            />
                          </td>
                          <td className={styles.money}>
                            <MoneyAmount
                              units={String(i.outstanding_minor)}
                              currency={String(i.currency)}
                              date={String(i.invoice_date)}
                              legacyScale={2}
                              snapshotUsdUnits={i.snapshot_usd_units as string | null}
                              snapshotSourceUnits={i.snapshot_source_units as string | null}
                              snapshotRateId={i.snapshot_rate_id as string | null}
                            />
                          </td>
                          <td>{String(i.status).replaceAll("_", " ")}</td>
                        </tr>
                      ))}
                    </tbody>
                  </Table>
                </TableFrame>
                {!rows.length && <p className={styles.empty}>No documents match these filters.</p>}
                <ListPagination {...page} query={search} count={rows.length} />
              </section>
              <details className={styles.disclosure}>
                <summary>Aging summary · {asOf}</summary>
                <div>
                  {search.kind !== "payable" &&
                    agingCard(
                      "Receivables aging",
                      receivableSummary,
                      receivableAging.filter((r) => Boolean(r.legacy_date_evidence)).length,
                      "receivable",
                    )}
                  {search.kind !== "receivable" &&
                    agingCard(
                      "Payables aging",
                      payableSummary,
                      payableAging.filter((r) => Boolean(r.legacy_date_evidence)).length,
                      "payable",
                    )}
                </div>
              </details>
            </>
          )}
        </WidgetGrid>
      </FinanceWorkspace>
    </FeaturePage>
  );
}
