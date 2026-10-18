import { canonicalUiHref } from "@/lib/work_projection/destination-registry";

import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { FeaturePage } from "@/components/shared/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { formatMoneyUnits } from "@/lib/finance/company-money";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { financeWorkbench } from "@/server/modules/finance/public/finance-workspace";
import { RecordLink as Link } from "@/components/records/record-link";
import { pageError } from "@/server/modules/work_projection/public/page-error";
import { FinanceToday } from "@/components/finance/finance-today";
import { financeWorkspaceV2Enabled } from "@/server/modules/finance/public/finance-presentation";
import { workSnapshot } from "@/server/modules/work_projection/public/work-items";

export const dynamic = "force-dynamic";

const roleDescription = (data: Awaited<ReturnType<typeof financeWorkbench>>) => {
  if (data.canApprove) return "Decisions and exceptions blocking posting or period close";
  if (data.canPay) return "Payment obligations, bank results, and unreconciled items";
  if (data.canPrepare) return "Documents and accounting entries to prepare and submit for approval";
  return "Financial status and records available to review";
};

export default async function FinanceWorkbenchPage({ params, searchParams }: { params: Promise<{ holdingSlug: string }>; searchParams: Promise<{ view?: string; q?: string; page?: string }> }) {
  const holdingSlug = (await params).holdingSlug;
  const actor = await requireWorkContext(holdingSlug);
  const base = workspaceBase(publicScope(actor));
  const data = await financeWorkbench(actor).catch(error => pageError(error, holdingSlug));
  if (financeWorkspaceV2Enabled()) {
    const query = await searchParams;
    const view = query.view === "waiting" || query.view === "team" ? query.view : "actions";
    const work = await workSnapshot(actor).catch(error => pageError(error, holdingSlug));
    return <FeaturePage className="ui-page-content" data-source="live" data-module="finance"><FinanceWorkspace actor={actor} base={base} active="work" title="Finance today" description={roleDescription(data)} actionsPurpose="new-work" actions={data.canPrepare ? <UiActionLink appearance="standard" variant="primary" href={canonicalUiHref(`${base}/finance/invoices?kind=receivable&new=1`)}>Add document</UiActionLink> : undefined}><FinanceToday base={base} attention={data.attention} work={work} view={view} query={query.q ?? ""} page={Number(query.page ?? 1)}/></FinanceWorkspace></FeaturePage>;
  }
  return <FeaturePage className="ui-page-content" data-source="live" data-module="finance">
    <FinanceWorkspace actor={actor} base={base} active="work" title="My finance work" description={roleDescription(data)} actionsPurpose="new-work" actions={data.canPrepare ? <UiActionLink appearance="standard" variant="primary" href={canonicalUiHref(`${base}/finance/invoices?new=1`)}>Add document</UiActionLink> : undefined}>
      <section className={styles.hero}>
        <div><p className="eyebrow">Needs attention</p><strong>{data.totalAttention}</strong><p>Current queue ordered by cash impact and period-close impact</p></div>
        <aside><span className="muted">Current role</span><b>{actor.role}</b><span className="muted">Company {actor.companyId} · {data.currency ?? "Currency not configured"}</span></aside>
      </section>
      <section className={styles.section} aria-labelledby="attention-title">
        <div className={styles.sectionHead}><div><h2 id="attention-title">Next actions</h2><p className="muted">Select a card to open the records that need attention</p></div></div>
        <div className={styles.grid}>
          {data.attention.map(item => <Link className={styles.queue} data-tone={item.tone} href={canonicalUiHref(`${base}/${item.href}`)} key={item.id}>
            <div><span className="eyebrow">{item.label}</span><p>{item.description}</p></div>
            <footer><strong>{item.count}</strong>{item.amounts?.length?<small>{item.amounts.map(amount=>formatMoneyUnits(String(amount.amountMinor),amount.currency,2)).join(" · ")}</small>:item.amountMinor !== null && item.amountMinor > 0 && item.currency ? <small>{formatMoneyUnits(String(item.amountMinor), item.currency, 2)}</small> : <small>Open records →</small>}</footer>
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
      <section className={styles.panel} aria-labelledby="ai-title"><div className={styles.sectionHead}><div><p className="eyebrow">Auditable AI</p><h2 id="ai-title">Let AI prepare; let people decide</h2></div><Link href={canonicalUiHref(`${base}/expenses`)}>Open document proposals →</Link></div><p className="muted">Receipt proposals pin the source version and evidence that was actually read. A changed document invalidates the old proposal. AI cannot approve, post, pay, reconcile, or close a period, and the normal workflow remains available when AI is unavailable.</p></section>
    </FinanceWorkspace>
  </FeaturePage>;
}
