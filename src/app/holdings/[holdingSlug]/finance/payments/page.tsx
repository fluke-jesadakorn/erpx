import Link from "next/link";
import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { minorToDecimal } from "@/lib/finance-money";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { paymentObligationsWorkspace } from "@/server/finance-workspace";

export const dynamic = "force-dynamic";

export default async function FinancePaymentObligationsPage({ params, searchParams }: { params: Promise<{ holdingSlug: string }>; searchParams: Promise<{ source?: string; status?: string }> }) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const query = await searchParams;
  const source = ["generic_payment", "supplier_batch", "refund", "people_payment"].includes(query.source ?? "") ? query.source : undefined;
  const status = query.status?.trim() || undefined;
  const obligations = await paymentObligationsWorkspace(actor, { sourceType: source, status });
  const sourceOptions = [...new Set(obligations.map(item => item.sourceType))];
  const statusOptions = [...new Set(obligations.map(item => item.status))].sort();
  const totals = obligations.reduce<Record<string, number>>((result, item) => {
    result[item.currency] = (result[item.currency] ?? 0) + item.amountMinor;
    return result;
  }, {});
  return <FeaturePage className="content-stack" data-source="live" data-module="finance">
    <FinanceWorkspace base={base} active="work" title="Payment obligations" description="One queue for every approved or pending payment source, with the source owner and next action visible.">
      <section className={styles.panel} aria-labelledby="payment-obligations-title">
        <div className={styles.sectionHead}>
          <div><p className="eyebrow">Finance · Treasury</p><h2 id="payment-obligations-title">Payment obligations</h2><p className="muted">Amounts are grouped by source. Open a row to continue in the owning workflow; the queue never bypasses its source authorization or approval controls.</p></div>
          <Link className="button secondary" href={`${base}/finance`}>Back to Finance work</Link>
        </div>
        <form className="business-form" method="get">
          <label>Source<select name="source" defaultValue={source ?? ""}><option value="">All sources</option>{sourceOptions.map(value => <option key={value} value={value}>{value === "generic_payment" ? "Customer/supplier payments" : value === "supplier_batch" ? "Supplier batches" : value === "refund" ? "Refunds" : "Payroll, expense and contractor"}</option>)}</select></label>
          <label>Status<select name="status" defaultValue={status ?? ""}><option value="">All statuses</option>{statusOptions.map(value => <option key={value}>{value}</option>)}</select></label>
          <button className="button secondary" type="submit">Filter</button>
          {(source || status) && <Link className="button secondary" href={`${base}/finance/payments`}>Clear filters</Link>}
        </form>
        <p className="muted" role="status">{obligations.length} {obligations.length === 1 ? "obligation" : "obligations"}{Object.entries(totals).length ? ` · ${Object.entries(totals).map(([currency, amount]) => `${currency} ${minorToDecimal(amount)}`).join(" · ")}` : ""}</p>
        {obligations.length ? <div className="business-table-wrap"><table className="business-table"><thead><tr><th>Source</th><th>Due</th><th>Currency</th><th>Remaining</th><th>Status</th><th>Next action</th></tr></thead><tbody>{obligations.map(item => <tr key={`${item.sourceType}:${item.id}`}><td><Link href={`${base}/${item.href}`}>{item.sourceLabel}</Link><div className="muted">{item.sourceId}</div></td><td>{item.dueDate ?? "—"}</td><td>{item.currency}</td><td>{minorToDecimal(item.amountMinor)}</td><td>{item.status}</td><td>{item.nextAction}</td></tr>)}</tbody></table></div> : <p className="muted">No payment obligations match these filters.</p>}
      </section>
    </FinanceWorkspace>
  </FeaturePage>;
}
