import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import { SectionWorkspace } from "@erpx/workplace/components/section-workspace";
import { sectionWorkspaceTopics, workspaceTopicLinks } from "@erpx/feature-work_projection/lib/section-workspaces";
import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";

import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@erpx/feature-finance/components/finance-workspace";
import { FinanceChartControls, type FinanceChart, type FinanceAccount } from "@erpx/feature-finance/components/finance-chart-controls";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { readFinanceSetup } from "../_native-setup";
import { nativeChartAccounts } from "@erpx/feature-finance/lib/native-finance-setup";
import { ledgerWorkspace } from "@erpx/server-finance/public/ledger";
import { FinanceStarterControls } from "@erpx/feature-finance/components/finance-starter-controls";

export default async function FinanceSetupPage({ params, searchParams }: {
    params: Promise<{
        holdingSlug: string;
    }>;
    searchParams?: Promise<Record<string, string | string[] | undefined>>;
}) {
    const { holdingSlug } = await params, query = searchParams ? await searchParams : {}, requestedChart = typeof query.chart === "string" ? query.chart : null;
    const actor = await requireWorkContext(holdingSlug), base = workspaceBase(publicScope(actor));
    
    const setup = (await readFinanceSetup(actor, holdingSlug)).setup, ledger = setup.charts.length ? await ledgerWorkspace(actor, requestedChart ?? undefined) : null;
    const nativeActor = actor.authority && actor.iam
      ? {
          userId: actor.userId,
          holdingId: actor.holdingId,
          holdingSlug,
          companyId: actor.companyId,
          branchId: actor.branchId,
          departmentId: actor.departmentId,
          assignmentId: actor.assignmentId,
          role: actor.role,
          assignmentScope: actor.assignmentScope,
          scope:
            actor.scope.kind === "holding"
              ? { kind: "holding" as const }
              : actor.scope.kind === "branch"
                ? { kind: "branch" as const, branchSlug: actor.scope.branchSlug ?? "" }
                : {
                    kind: "department" as const,
                    branchSlug: actor.scope.branchSlug ?? undefined,
                    departmentSlug: actor.scope.departmentSlug ?? "",
                  },
          authority: { revision: actor.authority.revision, assignmentVersion: actor.authority.assignmentVersion },
          modules: actor.modules,
          iam: { permissions: actor.iam.permissions },
        }
      : undefined;
    const nativeChart = await nativeChartAccounts(nativeActor, holdingSlug, requestedChart ?? (ledger?.chart.id as string | undefined) ?? (setup.charts[0]?.id as string | undefined));
    const step = "chart";
    
    
    const complete = setup.checks.filter(check => check.complete).length;
    const selectedChart = (nativeChart?.chart ?? ledger?.chart) as FinanceChart | undefined;
    const chartAccounts = (nativeChart?.accounts ?? ledger?.accounts) as FinanceAccount[] | undefined;
    return <FeaturePage className="ui-page-content" data-source="live"><FinanceWorkspace base={base} context={ledger ? { chartId: String(ledger.chart.id), chartName: String(ledger.chart.name), currency: String(ledger.chart.base_currency) } : undefined} active="setup" title="Finance setup" description="Prepare the company before receiving documents and posting">
    <SectionWorkspace nested showHeading={false} title="Finance setup" items={workspaceTopicLinks(base, query, sectionWorkspaceTopics.finance)} selected={step} context={ledger ? `${ledger.chart.name} · ${ledger.chart.base_currency}` : "Company setup"}>
    {false}
    {setup.charts.length > 1 && <section className={styles.panel} aria-label="Finance chart selector"><h2>{"Chart & account selection"}</h2><p className="muted">The selected chart controls the accounts and periods shown below.</p><div className="ui-form-actions">{setup.charts.map(chart => <UiActionLink appearance="standard" variant="secondary" data-active={ledger?.chart.id === chart.id || undefined} href={canonicalUiHref(`${base}/finance/setup?step=${step}&chart=${encodeURIComponent(String(chart.id))}`)} key={String(chart.id)}>{String(chart.name)} · {String(chart.base_currency)}</UiActionLink>)}</div></section>}
    {!setup.charts.length && actor.scope.kind === "holding" && hasPermission(actor, "finance.write") && <FinanceStarterControls base={base}/>}
    {hasPermission(actor, "finance.write") && <FinanceChartControls base={base} chart={selectedChart} charts={setup.charts as FinanceChart[]} accounts={chartAccounts ?? []}/>}
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
