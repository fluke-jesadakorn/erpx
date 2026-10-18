import { SectionWorkspace } from "@/components/workplace/section-workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@/lib/work_projection/section-workspaces";

import { FeaturePage } from "@/components/shared/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";

import { hasPermission } from "@/lib/identity_access/permissions";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { financeSetupWorkspace } from "@/server/modules/finance/public/finance-workspace";
import { ledgerWorkspace } from "@/server/modules/finance/public/ledger";
import { FinanceStarterControls } from "@/components/finance/finance-starter-controls";

export default async function FinanceSetupPage({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams?: Promise<Record<string, string | string[] | undefined>>;
}) {
    const { holdingSlug } = await params, query = searchParams ? await searchParams : {}, requestedChart = typeof query.chart === "string" ? query.chart : null;
    const actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
    
    const setup = await financeSetupWorkspace(actor), ledger = setup.charts.length ? await ledgerWorkspace(actor, requestedChart ?? undefined) : null;
    const step = "checklist";
    
    
    const complete = setup.checks.filter(check => check.complete).length;
    
    
    return <FeaturePage className="ui-page-content" data-source="live"><FinanceWorkspace base={base} context={ledger ? { chartId: String(ledger.chart.id), chartName: String(ledger.chart.name), currency: String(ledger.chart.base_currency) } : undefined} active="setup" title="Finance setup" description="Prepare the company before receiving documents and posting">
    <SectionWorkspace nested showHeading={false} title="Finance setup" items={workspaceTopicLinks(base, query, sectionWorkspaceTopics.finance)} selected={step} context={ledger ? `${ledger.chart.name} · ${ledger.chart.base_currency}` : "Company setup"}>
    {<><section className={styles.hero}><div><p className="eyebrow">Readiness</p><strong>{complete}/{setup.checks.length}</strong><p>Key setup checks found by the system</p></div><aside><b>{complete === setup.checks.length ? "Ready to test the workflow" : "Setup still required"}</b><span className="muted">Go-live requires a close simulation and accountable data review.</span></aside></section>
    <section className={styles.panel}><h2>Getting-started checklist</h2>{setup.checks.map(check => <div className={styles.check} data-complete={check.complete} key={check.id}><span className={styles.checkMark}>{check.complete ? "✓" : "·"}</span><b>{check.label}</b><span className="muted">{check.detail}</span></div>)}</section></>}
    {false}
    {!setup.charts.length && actor.scope.kind === "holding" && hasPermission(actor, "finance.write") && <FinanceStarterControls base={base}/>}
    {false}
    {false}
    {false}
    {false}
    {false}
    {false}
    {false}
    {false}
    {false}
  </SectionWorkspace></FinanceWorkspace></FeaturePage>;
}
