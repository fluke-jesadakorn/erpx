import { SectionWorkspace } from "@erpx/platform-workplace/components/section-workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@erpx/work-projection-ui/lib/section-workspaces";

import { FeaturePage } from "@erpx/platform-shared-ui/components/features/feature-page";
import { FinanceWorkspace } from "@erpx/finance-ui/components/finance-workspace";

import { hasPermission } from "@erpx/identity-access-ui/lib/permissions";
import { workspaceBase } from "@erpx/work-projection-ui/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/identity-access-server/public/authorization";
import { readFinanceSetup } from "../_native-setup";
import { ledgerWorkspace } from "@erpx/finance-server/public/ledger";
import { financeCompanyTools } from "@erpx/finance-server/public/finance-company-tools";
import { FinanceToolControls } from "@erpx/finance-ui/components/finance-tool-controls";

export default async function FinanceSetupPage({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams?: Promise<Record<string, string | string[] | undefined>>;
}) {
    const { holdingSlug } = await params, query = searchParams ? await searchParams : {}, requestedChart = typeof query.chart === "string" ? query.chart : null;
    const actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
    
    const setup = (await readFinanceSetup(actor, holdingSlug)).setup, ledger = setup.charts.length ? await ledgerWorkspace(actor, requestedChart ?? undefined) : null;
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
