import Link from "next/link";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { workspaceBase } from "@/lib/workspace";
import {
  listCharts,
  listInvoices,
  listJournals,
  listPayments,
  statements,
} from "@/server/finance";
import { minorToDecimal } from "@/lib/finance-money";

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
  const [journals, report, invoices, payments] = await Promise.all([
    listJournals(actor, {}),
    statements(actor, undefined, String(chart?.id)),
    listInvoices(actor),
    listPayments(actor),
  ]);
  const cards = [
    ["Charts", charts.length, "Chart of accounts and posting profiles"],
    ["Journals", journals.length, "Draft, posted and reversed entries"],
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
    ["Invoices", invoices.length, "Receivables and payables in this Company"],
    ["Payments", payments.length, "Receipts, settlements, and releases"],
  ];
  return (
    <main className="content-stack" data-source="live" data-module="finance">
      <div className="section-head">
        <div>
          <p className="eyebrow">Finance · General ledger</p>
          <h1>Finance overview</h1>
          <p className="muted">
            Company accounting, controlled posting and period close.
          </p>
        </div>
      </div>
      <section className="card-grid" aria-label="Finance health">
        {cards.map(([label, value, detail]) => (
          <article className="card" key={label}>
            <span className="eyebrow">{label}</span>
            <strong>{value}</strong>
            <span className="muted">{detail}</span>
          </article>
        ))}
      </section>
      <section className="card-grid">
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
      </section>
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
    </main>
  );
}
