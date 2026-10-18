import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { SectionWorkspace } from "@/components/workplace/section-workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@/lib/work_projection/section-workspaces";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { FeaturePage } from "@/components/shared/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";

import { OpeningBalanceImport } from "@/components/finance/ledger-controls";
import { hasPermission } from "@/lib/identity_access/permissions";
import { workspaceBase } from "@/lib/work_projection/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { financeSetupWorkspace } from "@/server/modules/finance/public/finance-workspace";
import { ledgerWorkspace } from "@/server/modules/finance/public/ledger";

export default async function FinanceSetupPage({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams?: Promise<Record<string, string | string[] | undefined>>;
}) {
    const { holdingSlug } = await params, query = searchParams ? await searchParams : {}, requestedChart = typeof query.chart === "string" ? query.chart : null;
    const actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
    
    const setup = await financeSetupWorkspace(actor), ledger = setup.charts.length ? await ledgerWorkspace(actor, requestedChart ?? undefined) : null;
    const step = "opening";
    
    
    const complete = setup.checks.filter(check => check.complete).length;
    
    
    return <FeaturePage className="ui-page-content" data-source="live"><FinanceWorkspace base={base} context={ledger ? { chartId: String(ledger.chart.id), chartName: String(ledger.chart.name), currency: String(ledger.chart.base_currency) } : undefined} active="setup" title="Finance setup" description="Prepare the company before receiving documents and posting">
    <SectionWorkspace nested showHeading={false} title="Finance setup" items={workspaceTopicLinks(base, query, sectionWorkspaceTopics.finance)} selected={step} context={ledger ? `${ledger.chart.name} · ${ledger.chart.base_currency}` : "Company setup"}>
    {false}
    {setup.charts.length > 1 && <section className={styles.panel} aria-label="Finance chart selector"><h2>{"Opening balance chart"}</h2><p className="muted">The selected chart controls the accounts and periods shown below.</p><div className="ui-form-actions">{setup.charts.map(chart => <UiActionLink appearance="standard" variant="secondary" data-active={ledger?.chart.id === chart.id || undefined} href={canonicalUiHref(`${base}/finance/setup?step=${step}&chart=${encodeURIComponent(String(chart.id))}`)} key={String(chart.id)}>{String(chart.name)} · {String(chart.base_currency)}</UiActionLink>)}</div></section>}
    {false}
    {false}
    {false}
    {hasPermission(actor, "finance.write") && ledger && <OpeningBalanceImport base={base} chart={ledger.chart} accounts={ledger.accounts} periods={ledger.periods} parties={setup.parties}/>}
    {!ledger && <p className={styles.empty}>Create a chart of accounts before importing opening balances.</p>}
    {false}
    {false}
    {false}
    {false}
    {false}
  </SectionWorkspace></FinanceWorkspace></FeaturePage>;
}
