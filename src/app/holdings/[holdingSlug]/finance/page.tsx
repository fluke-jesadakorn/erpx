import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { minorToDecimal } from "@/lib/finance-money";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { financeWorkbench } from "@/server/finance-workspace";
import Link from "next/link";

export const dynamic = "force-dynamic";

const roleDescription = (data: Awaited<ReturnType<typeof financeWorkbench>>) => {
  if (data.canApprove) return "Decisions and exceptions blocking posting or period close";
  if (data.canPay) return "Payment obligations, bank results, and unreconciled items";
  if (data.canPrepare) return "Documents and accounting entries to prepare and submit for approval";
  return "Financial status and records available to review";
};

export default async function FinanceWorkbenchPage({ params }: { params: Promise<{ holdingSlug: string }> }) {
  const actor = await requireWorkContext((await params).holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const data = await financeWorkbench(actor);
  return <FeaturePage className="content-stack" data-source="live" data-module="finance">
    <FinanceWorkspace base={base} active="work" title="My finance work" description={roleDescription(data)} actions={data.canPrepare ? <Link className="button" href={`${base}/finance/invoices?new=1`}>Add document</Link> : undefined}>
      <section className={styles.hero}>
        <div><p className="eyebrow">Needs attention</p><strong>{data.totalAttention}</strong><p>Current queue ordered by cash impact and period-close impact</p></div>
        <aside><span className="muted">Current role</span><b>{actor.role}</b><span className="muted">Company {actor.companyId} · {data.currency ?? "Currency not configured"}</span></aside>
      </section>
      <section className={styles.section} aria-labelledby="attention-title">
        <div className={styles.sectionHead}><div><h2 id="attention-title">Next actions</h2><p className="muted">Select a card to open the records that need attention</p></div></div>
        <div className={styles.grid}>
          {data.attention.map(item => <Link className={styles.queue} data-tone={item.tone} href={`${base}/${item.href}`} key={item.id}>
            <div><span className="eyebrow">{item.label}</span><p>{item.description}</p></div>
            <footer><strong>{item.count}</strong>{item.amountMinor !== null && item.amountMinor > 0 ? <small>{item.currency} {minorToDecimal(item.amountMinor)}</small> : <small>Open records →</small>}</footer>
          </Link>)}
        </div>
      </section>
      <section className={styles.section} aria-labelledby="flow-title">
        <div className={styles.sectionHead}><div><h2 id="flow-title">Invoice-to-bank workflow</h2><p className="muted">Every step has an owner, evidence, and next status</p></div></div>
        <div className={styles.process}>
          <article><b>Capture document</b><p className="muted">Record the source, party, dates, tax, and amount</p></article>
          <article><b>Review and approve</b><p className="muted">Review the proposal and posting result before confirming</p></article>
          <article><b>Schedule payment</b><p className="muted">Group approved obligations, export the file, and record the bank result</p></article>
          <article><b>Reconcile and close</b><p className="muted">Match evidence, resolve differences, and lock the period</p></article>
        </div>
      </section>
      <section className={styles.panel} aria-labelledby="ai-title"><div className={styles.sectionHead}><div><p className="eyebrow">Auditable AI</p><h2 id="ai-title">Let AI prepare; let people decide</h2></div><Link href={`${base}/expenses`}>Open document proposals →</Link></div><p className="muted">Receipt proposals pin the source version and evidence that was actually read. A changed document invalidates the old proposal. AI cannot approve, post, pay, reconcile, or close a period, and the normal workflow remains available when AI is unavailable.</p></section>
    </FinanceWorkspace>
  </FeaturePage>;
}
