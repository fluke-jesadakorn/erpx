import { SectionWorkspace } from "@erpx/workplace/components/section-workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@erpx/feature-work_projection/lib/section-workspaces";

import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace } from "@erpx/feature-finance/components/finance-workspace";

import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { financeSetupWorkspace } from "@erpx/server-finance/public/finance-workspace";
import { ledgerWorkspace } from "@erpx/server-finance/public/ledger";
import { financeCompanyTools } from "@erpx/server-finance/public/finance-company-tools";
import { FinanceToolControls } from "@erpx/feature-finance/components/finance-tool-controls";

export default async function FinanceSetupPage({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams?: Promise<Record<string, string | string[] | undefined>>;
}) {
    const { holdingSlug } = await params, query = searchParams ? await searchParams : {}, requestedChart = typeof query.chart === "string" ? query.chart : null;
    const actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
    
    const setup = await financeSetupWorkspace(actor), ledger = setup.charts.length ? await ledgerWorkspace(actor, requestedChart ?? undefined) : null;
    const step = "tools";
    
    const tools = await financeCompanyTools(actor);
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
    {tools && <FinanceToolControls base={base} rows={tools} canWrite={hasPermission(actor, "finance.write")}/>}
    {false}
  </SectionWorkspace></FinanceWorkspace></FeaturePage>;
}
