import { SectionWorkspace } from "@/components/workplace/section-workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@/lib/work_projection/section-workspaces";
import { FinanceAiPolicyControls } from "@/components/finance/finance-ai-panel";

import { FeaturePage } from "@/components/shared/features/feature-page";
import { FinanceWorkspace } from "@/components/finance/finance-workspace";

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
    const step = "ai";
    
    
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
    {<FinanceAiPolicyControls base={base}/>}
    {false}
    {false}
    {false}
    {false}
  </SectionWorkspace></FinanceWorkspace></FeaturePage>;
}
