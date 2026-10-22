import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { SectionWorkspace } from "@erpx/workplace/components/section-workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@erpx/feature-work_projection/lib/section-workspaces";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";

import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace } from "@erpx/feature-finance/components/finance-workspace";

import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { financeSetupWorkspace } from "@erpx/server-finance/public/finance-workspace";
import { ledgerWorkspace } from "@erpx/server-finance/public/ledger";
import { financeBookWorkspace } from "@erpx/server-finance/public/finance-books";
import { FinanceBookControls } from "@erpx/feature-finance/components/finance-book-controls";

export default async function FinanceSetupPage({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams?: Promise<Record<string, string | string[] | undefined>>;
}) {
    const { holdingSlug } = await params, query = searchParams ? await searchParams : {}, requestedChart = typeof query.chart === "string" ? query.chart : null;
    const actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
    
    const setup = await financeSetupWorkspace(actor), ledger = setup.charts.length ? await ledgerWorkspace(actor, requestedChart ?? undefined) : null;
    const step = "books";
    const books = await financeBookWorkspace(actor);
    
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
    {books && books.rules.some(r => r.status === "approved") && <UiActionLink appearance="standard" href={canonicalUiHref(`${base}/finance/book-adjustments`)}>Secondary book policy adjustments</UiActionLink>}
    {books && <FinanceBookControls base={base} data={books} canWrite={hasPermission(actor, "finance.write")} canApprove={hasPermission(actor, "finance.approve")} userId={actor.userId}/>}
    {false}
    {false}
  </SectionWorkspace></FinanceWorkspace></FeaturePage>;
}
