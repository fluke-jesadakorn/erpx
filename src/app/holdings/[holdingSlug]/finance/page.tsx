import { FeaturePage, FeatureTabs,FeatureWidget,WidgetGrid } from "@/components/features/feature-page";
import { minorToDecimal } from "@/lib/finance-money";
import { workspaceBase } from "@/lib/workspace";
import { publicScope,requireWorkContext } from "@/server/authorization";
import {
financeOverviewCounts,
listCharts,
listInvoices,
listPayments,
statements,
} from "@/server/finance";
import Link from "next/link";

export const dynamic = "force-dynamic";

export default async function FinanceOverview({
  params,
}: {
  params: Promise<{ holdingSlug: string }>;
}) {
  const { holdingSlug } = await params;
  const actor = await requireWorkContext(holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const charts = await listCharts(actor);
  const chart = charts.find((c) => c.name === "Operations ledger") ?? charts[0];
  const [counts, report, invoices, payments] = await Promise.all([
    financeOverviewCounts(actor),
    statements(actor, undefined, String(chart?.id)),
    listInvoices(actor,8),
    listPayments(actor,6),
  ]);
  const queues=[
    {id:"receivable",title:"To collect",count:counts.receivable,href:`${base}/finance/invoices?kind=receivable&queue=open`,detail:"Posted customer balances"},
    {id:"payable",title:"To pay",count:counts.payable,href:`${base}/finance/invoices?kind=payable&queue=open`,detail:"Posted supplier balances"},
    {id:"posting",title:"To post",count:counts.posting,href:`${base}/finance/ledger?status=draft`,detail:"Independent ledger review"},
    {id:"reconcile",title:"To reconcile",count:counts.reconcile,href:`${base}/finance/bank`,detail:"Match bank evidence with payments"},
  ];
  const cards = [
    ["Charts", charts.length, "Chart of accounts and posting profiles"],
    ["Journals", counts.journals, "Draft, posted and reversed entries"],
    [
      "Assets",
      `${String(chart?.base_currency)} ${report.balanceSheet.assets}`,
      "Current posted asset balance",
    ],
    [
      "Revenue",
      `${String(chart?.base_currency)} ${report.profitAndLoss.revenue}`,
      "Posted revenue balance",
    ],
    ["Invoices", counts.invoices, "Receivables and payables in this Company"],
    ["Payments", counts.payments, "Receipts, settlements, and releases"],
  ];
  return (
    <FeaturePage className="content-stack" data-source="live" data-module="finance">
      <div className="section-head">
        <div>
          <p className="eyebrow">Finance · General ledger</p>
          <h1>Finance overview</h1>{!actor.branchId&&!actor.departmentId&&<Link href={`${base}/finance/supplier-batches`}>Supplier payment batches · approve and reconcile</Link>}<Link href={`${base}/finance/refunds`}>Return refunds · review, export and reconcile</Link>
          <p className="muted">
            Company accounting, controlled posting and period close.
          </p>
        </div>
      </div><FeatureTabs/><WidgetGrid pageKey="finance-overview">
      <FeatureWidget id="finance-overview-finance-work-queues" title="Finance work queues">{queues.map(queue=><Link className="card" key={queue.id} href={queue.href}><span className="eyebrow">{queue.title}</span><strong>{queue.count}</strong><span className="muted">{queue.detail}</span></Link>)}</FeatureWidget>
      <FeatureWidget id="finance-overview-finance-health" title="Finance health">
        {cards.map(([label, value, detail]) => (
          <article className="card" key={label}>
            <span className="eyebrow">{label}</span>
            <strong>{value}</strong>
            <span className="muted">{detail}</span>
          </article>
        ))}
      </FeatureWidget>
      <FeatureWidget id="finance-overview-recent-invoices" title="Recent invoices">
        <article className="card" style={{ gridColumn: "span 2" }}>
          <div className="section-head">
            <h2>Recent invoices</h2>
            <span className="badge">{invoices.length}</span>
          </div>
          {invoices.length === 0 ? (
            <p className="muted">No invoices for this company.</p>
          ) : (
            invoices.slice(0, 8).map((invoice) => (
              <Link
                className="row"
                href={`${base}/finance/invoices/${encodeURIComponent(String(invoice.id))}`}
                key={String(invoice.id)}
              >
                <div className="row-main">
                  <strong className="row-title">
                    {String(invoice.invoice_number)}
                  </strong>
                  <span className="muted">
                    {String(invoice.party_name)} · {String(invoice.kind)}
                  </span>
                </div>
                <strong>
                  {String(invoice.currency)}{" "}
                  {minorToDecimal(Number(invoice.total_minor))}
                </strong>
                <span className="badge">{String(invoice.status)}</span>
              </Link>
            ))
          )}
        </article>
        <article className="card">
          <div className="section-head">
            <h2>Payment control</h2>
            <span className="badge">{payments.length}</span>
          </div>
          {payments.length === 0 ? (
            <p className="muted">No payments for this company.</p>
          ) : (
            payments.slice(0, 6).map((payment) => (
              <Link
                className="row"
                href={`${base}/finance/payments/${encodeURIComponent(String(payment.id))}`}
                key={String(payment.id)}
              >
                <div className="row-main">
                  <strong className="row-title">
                    {String(payment.payment_number)}
                  </strong>
                  <span className="muted">
                    {String(payment.party_name ?? "Unassigned party")}
                  </span>
                </div>
                <span>
                  {String(payment.currency)}{" "}
                  {minorToDecimal(Number(payment.amount_minor))}
                </span>
              </Link>
            ))
          )}
        </article>
      </FeatureWidget>
      <section className="card-grid">
        {[
          [
            "Ledger & reports",
            "Journals, chart of accounts, reports and period close",
            "finance/ledger",
          ],
          [
            "Invoices",
            "Receivables, payables and payment records",
            "finance/invoices",
          ],
          [
            "Bank reconciliation",
            "Statements and human-reviewed matches",
            "finance/bank",
          ],
          ["Employee expenses", "Claims and reimbursements", "expenses"],
        ].map(([title, detail, route]) => (
          <Link className="card" href={`${base}/${route}`} key={route}>
            <strong>{title}</strong>
            <span className="muted">{detail}</span>
            <span className="text-link">Open →</span>
          </Link>
        ))}
      </section>
    </WidgetGrid></FeaturePage>
  );
}
