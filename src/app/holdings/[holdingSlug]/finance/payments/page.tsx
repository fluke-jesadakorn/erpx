import Link from "next/link";
import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { minorToDecimal } from "@/lib/finance-money";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { paymentObligationsWorkspace, paymentRecordsWorkspace } from "@/server/finance-workspace";
import { ListPagination } from "@/components/ui/list-pagination";

export const dynamic = "force-dynamic";

export default async function FinancePaymentObligationsPage({ params, searchParams }: { params: Promise<{ holdingSlug: string }>; searchParams: Promise<{ source?: string; status?: string; view?: string; q?: string; paymentStatus?: string; paymentCursor?: string }> }) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const query = await searchParams;
  const source = ["generic_payment", "supplier_batch", "refund", "people_payment"].includes(query.source ?? "") ? query.source : undefined;
  const status = query.status?.trim() || undefined;
  const [allObligations, records] = await Promise.all([paymentObligationsWorkspace(actor),paymentRecordsWorkspace(actor,query)]);
  const obligations = allObligations.filter(item=>(!source||item.sourceType===source)&&(!status||item.status===status));
  const sourceOptions = ["generic_payment", "supplier_batch", "refund", "people_payment"];
  const statusOptions = [...new Set(allObligations.map(item => item.status))].sort();
  const totals = obligations.reduce<Record<string, number>>((result, item) => {
    result[item.currency] = (result[item.currency] ?? 0) + item.amountMinor;
    return result;
  }, {});
  return <FeaturePage className="content-stack" data-source="live" data-module="finance">
    <FinanceWorkspace base={base} active="cash" title="Payments" description="One queue for every approved or pending payment source, with the source owner and next action visible.">
      <section className={styles.panel} aria-labelledby="payment-obligations-title">
        <div className={styles.sectionHead}>
          <div><p className="eyebrow">Finance · Treasury</p><h2 id="payment-obligations-title">Payment obligations</h2><p className="muted">Amounts are grouped by source. Open a row to continue in the owning workflow; the queue never bypasses its source authorization or approval controls.</p></div>

        </div>
        <form className="business-form" method="get">
          <label>Source<select name="source" defaultValue={source ?? ""}><option value="">All sources</option>{sourceOptions.map(value => <option key={value} value={value}>{value === "generic_payment" ? "Customer/supplier payments" : value === "supplier_batch" ? "Supplier batches" : value === "refund" ? "Refunds" : "Payroll, expense and contractor"}</option>)}</select></label>
          <label>Status<select name="status" defaultValue={status ?? ""}><option value="">All statuses</option>{statusOptions.map(value => <option key={value}>{value}</option>)}</select></label>
          <button className="button secondary" type="submit">Filter</button>
          {(source || status) && <Link className="button secondary" href={`${base}/finance/payments`}>Clear filters</Link>}
        </form>
        <p className="muted" role="status">{obligations.length} {obligations.length === 1 ? "obligation" : "obligations"}{Object.entries(totals).length ? ` · ${Object.entries(totals).map(([currency, amount]) => `${currency} ${minorToDecimal(amount)}`).join(" · ")}` : ""}</p>
        {obligations.length ? <div className="business-table-wrap" tabIndex={0} role="region" aria-label="Financial table — scroll for more columns"><table className="business-table"><thead><tr><th>Source</th><th>Due</th><th>Currency</th><th>Remaining</th><th>Status</th><th>Next action</th></tr></thead><tbody>{obligations.map(item => <tr key={`${item.sourceType}:${item.id}`}><td><Link href={`${base}/${item.href}`}>{item.sourceLabel}</Link><div className="muted">{item.sourceId}</div></td><td>{item.dueDate ?? "—"}</td><td>{item.currency}</td><td>{minorToDecimal(item.amountMinor)}</td><td>{item.status}</td><td>{item.nextAction}</td></tr>)}</tbody></table></div> : <p className="muted">No payment obligations match these filters.</p>}
      </section>
      <details className={styles.disclosure} open={query.view==="records"}><summary>Payment records · receipts and retained settlements</summary><div>
        <p className="muted">Open an existing record to inspect allocations, accounting and retained corrections. Each amount stays in its transaction currency.</p>
        <form className={`business-form ${styles.filterForm}`} method="get">
          <input type="hidden" name="view" value="records"/>
          <label>Find payment records<input name="q" defaultValue={query.q} placeholder="Payment number or party"/></label>
          <label>Payment status<select name="paymentStatus" defaultValue={query.paymentStatus??""}><option value="">All statuses</option>{["draft","approved","released","settled","voided"].map(value=><option key={value} value={value}>{value}</option>)}</select></label>
          <button className="button secondary">Find records</button>
        </form>
        <div className="business-table-wrap" tabIndex={0} role="region" aria-label="Payment records — scroll for more columns"><table className="business-table"><thead><tr><th>Reference</th><th>Party</th><th>Date</th><th>Type</th><th>Amount</th><th>Unallocated</th><th>Status</th></tr></thead><tbody>{records.items.map(payment=><tr key={payment.id}><td><Link href={`${base}/finance/payments/${payment.id}`}>{payment.payment_number}</Link></td><td>{payment.party_name??"Not assigned"}</td><td>{String(payment.payment_date)}</td><td>{payment.kind}</td><td className={styles.money}>{payment.currency} {minorToDecimal(Number(payment.amount_minor))}</td><td className={styles.money}>{payment.currency} {minorToDecimal(Number(payment.outstanding_minor))}</td><td>{payment.status}</td></tr>)}</tbody></table></div>
        {!records.items.length&&<p className="muted">No payment records match this search.</p>}
        <ListPagination {...records} query={{...query,view:"records"}} cursorName="paymentCursor" count={records.items.length}/>
      </div></details>
    </FinanceWorkspace>
  </FeaturePage>;
}
