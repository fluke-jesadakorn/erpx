import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { FxRevaluationControls } from "@/components/finance/fx-revaluation-controls";
import { PeriodControls } from "@/components/modules/ledger-controls";
import { StatusBadge } from "@/components/ui/primitives";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/authorization";
import { closeWorkspace } from "@/server/finance-workspace";
import { fxRevaluationWorkspace } from "@/server/finance";

export const dynamic="force-dynamic";
export default async function ClosePage({params}:{params:Promise<{holdingSlug:string}>}) {
  const actor=await requireWorkContext((await params).holdingSlug),base=workspaceBase(publicScope(actor)),periods=await closeWorkspace(actor);
  const revaluations=new Map((await Promise.all(periods.map(async period=>[period.id,await fxRevaluationWorkspace(actor,period.id)] as const))));
  return <FeaturePage className="content-stack" data-source="live"><FinanceWorkspace base={base} active="close" title="Period close center" description="See blockers, evidence, and close decisions in one place">
    <section className={styles.process}><article><b>Review drafts</b><p className="muted">Documents and journals must be reviewed</p></article><article><b>Reconcile</b><p className="muted">Bank and subledger differences must be explained</p></article><article><b>Review reports</b><p className="muted">Check the trial balance and reports before locking</p></article><article><b>Approve period close</b><p className="muted">An independent approver confirms the evidence</p></article></section>
    <section className={styles.section}><div className={styles.sectionHead}><div><h2>Accounting periods</h2><p className="muted">Blocker counts come from the company’s current data</p></div></div>
      {periods.map(period=><article className={styles.panel} key={String(period.id)}><div className={styles.sectionHead}><div><h3>{String(period.name)}</h3><p className="muted">{String(period.start_date)} – {String(period.end_date)}</p></div><StatusBadge status={String(period.state)}/></div>
        <div className={styles.metrics}><div className="card"><span className="eyebrow">Draft journals</span><strong>{Number(period.draft_journals)}</strong></div><div className="card"><span className="eyebrow">Unreconciled bank</span><strong>{Number(period.unmatched_bank)}</strong></div><div className="card"><span className="eyebrow">Draft documents</span><strong>{Number(period.draft_invoices)}</strong></div><div className="card"><span className="eyebrow">Open manufacturing</span><strong>{Number(period.open_manufacturing)}</strong></div><div className="card"><span className="eyebrow">Calculations awaiting review</span><strong>{Number(period.pending_calculations)}</strong></div></div>
        {period.reconciliations.length>0&&<div className="business-table-wrap"><table className="business-table"><thead><tr><th>Reconciliation</th><th>Account</th><th>GL</th><th>Source</th><th>Difference</th><th>Evidence</th></tr></thead><tbody>{period.reconciliations.map(row=><tr key={`${row.kind}:${row.accountId}`}><td>{row.kind}</td><td>{row.chartName}<br/>{row.accountCode} · {row.accountName}</td><td>{row.currency} {(row.ledgerMinor/100).toFixed(2)}</td><td>{row.currency} {(row.sourceMinor/100).toFixed(2)}</td><td>{row.currency} {(row.differenceMinor/100).toFixed(2)}</td><td>{row.kind==="bank"?(row.source.evidenceComplete?`Statement ${String(row.source.statementNumber)} at period end`:`Statement required at ${String(row.source.periodEnd)}`):`${Number(row.source.invoiceCount??0)} document(s) · ${Number(row.source.foreignOpenCount??0)} foreign item(s) awaiting revaluation`}</td></tr>)}</tbody></table></div>}
        {revaluations.get(period.id)&&<FxRevaluationControls base={base} data={revaluations.get(period.id)!} canPrepare={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")} userId={actor.userId}/>} 
        <p>{Number(period.blockers)===0?"Automated blocker checks pass; ready for evidence review":"Period cannot close yet: resolve the items above"}</p>
        {hasPermission(actor,"finance.write")&&<PeriodControls base={base} period={period} canApprove={hasPermission(actor,"finance.approve")}/>}</article>)}
      {!periods.length&&<p className={styles.empty}>No accounting periods yet. Configure a period before posting.</p>}
    </section>
  </FinanceWorkspace></FeaturePage>;
}
