import Link from "next/link";
import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { minorToDecimal } from "@/lib/finance-money";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { paymentObligationsWorkspace } from "@/server/finance-workspace";

export const dynamic = "force-dynamic";

export default async function FinancePaymentObligationsPage({ params }: { params: Promise<{ holdingSlug: string }> }) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const obligations = await paymentObligationsWorkspace(actor);
  return <FeaturePage className="content-stack" data-source="live" data-module="finance">
    <FinanceWorkspace base={base} active="work" title="Payment obligations" description="One queue for every approved or pending payment source, with the source owner and next action visible.">
      <section className={styles.panel} aria-labelledby="payment-obligations-title">
        <div className={styles.sectionHead}>
          <div><p className="eyebrow">Finance · Treasury</p><h2 id="payment-obligations-title">Payment obligations</h2><p className="muted">Amounts are grouped by source. Open a row to continue in the owning workflow; the queue never bypasses its source authorization or approval controls.</p></div>
          <Link className="button secondary" href={`${base}/finance`}>Back to Finance work</Link>
        </div>
        {obligations.length ? <div className="business-table-wrap"><table className="business-table"><thead><tr><th>Source</th><th>Due</th><th>Currency</th><th>Remaining</th><th>Status</th><th>Next action</th></tr></thead><tbody>{obligations.map(item => <tr key={`${item.sourceType}:${item.id}`}><td><Link href={`${base}/${item.href}`}>{item.sourceLabel}</Link><div className="muted">{item.sourceId}</div></td><td>{item.dueDate ?? "—"}</td><td>{item.currency}</td><td>{minorToDecimal(item.amountMinor)}</td><td>{item.status}</td><td>{item.nextAction}</td></tr>)}</tbody></table></div> : <p className="muted">No open payment obligations in this Company.</p>}
      </section>
    </FinanceWorkspace>
  </FeaturePage>;
}
