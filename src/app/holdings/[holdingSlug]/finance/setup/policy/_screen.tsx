import { canonicalUiHref } from "@/lib/work_projection/destination-registry";
import { SectionWorkspace } from "@/components/workplace/section-workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@/lib/work_projection/section-workspaces";
import { ActionLink as UiActionLink } from "@/components/ui/foundation";

import { FeaturePage } from "@/components/shared/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";

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
    const step = "policy";
    
    
    const complete = setup.checks.filter(check => check.complete).length;
    
    
    return <FeaturePage className="ui-page-content" data-source="live"><FinanceWorkspace base={base} context={ledger ? { chartId: String(ledger.chart.id), chartName: String(ledger.chart.name), currency: String(ledger.chart.base_currency) } : undefined} active="setup" title="Finance setup" description="Prepare the company before receiving documents and posting">
    <SectionWorkspace nested showHeading={false} title="Finance setup" items={workspaceTopicLinks(base, query, sectionWorkspaceTopics.finance)} selected={step} context={ledger ? `${ledger.chart.name} · ${ledger.chart.base_currency}` : "Company setup"}>
    {false}
    {false}
    {false}
    {false}
    {false}
    {false}
    {false}
    {false}
    {false}
    {false}
    {false}
    {<section className={styles.panel}><h2>Go-live policy</h2><p className="muted">Define independent preparers and approvers, Thai tax settings, bank accounts, mappings from PMS, POS, service, and manufacturing systems, and close owners. A chart of accounts alone does not mean the company is ready for production.</p>{hasPermission(actor, 'iam.read') && <UiActionLink appearance="standard" href={canonicalUiHref(`${base}/technology`)}>Review Finance assignments in Organization &amp; Access</UiActionLink>}</section>}
  </SectionWorkspace></FinanceWorkspace></FeaturePage>;
}
