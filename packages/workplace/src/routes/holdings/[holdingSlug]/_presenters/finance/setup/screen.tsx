import { canonicalUiHref } from "@erpx/feature-work_projection/lib/destination-registry";
import {SectionWorkspace} from "@erpx/workplace/components/section-workspace";
import {sectionWorkspaceTopics,workspaceTopicLinks} from "@erpx/feature-work_projection/lib/section-workspaces";

import { ActionLink as UiActionLink } from "@erpx/ui/components/foundation";
import { FinanceAiPolicyControls } from "@erpx/feature-finance/components/finance-ai-panel";
import { AccountingProfileControls } from "@erpx/feature-finance/components/accounting-profile-controls";
import { accountingProfiles } from "@erpx/server-finance/public/accounting-profiles";
import { FeaturePage } from "@erpx/feature-shared/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@erpx/feature-finance/components/finance-workspace";
import { FinanceChartControls, type FinanceChart, type FinanceAccount } from "@erpx/feature-finance/components/finance-chart-controls";
import { OpeningBalanceImport } from "@erpx/feature-finance/components/ledger-controls";
import { hasPermission } from "@erpx/feature-identity_access/lib/permissions";
import { workspaceBase } from "@erpx/feature-work_projection/lib/workspace";
import { publicScope, requireWorkContext } from "@erpx/server-identity_access/public/authorization";
import { financeSetupWorkspace } from "@erpx/server-finance/public/finance-workspace";
import { ledgerWorkspace } from "@erpx/server-finance/public/ledger";
import { financeBookWorkspace } from "@erpx/server-finance/public/finance-books";
import { FinanceBookControls } from "@erpx/feature-finance/components/finance-book-controls";
import { financeCompanyTools } from "@erpx/server-finance/public/finance-company-tools";
import { FinanceToolControls } from "@erpx/feature-finance/components/finance-tool-controls";
import { FinanceStarterControls } from "@erpx/feature-finance/components/finance-starter-controls";

export const dynamic="force-dynamic";
export default async function FinanceSetupPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams?:Promise<Record<string,string|string[]|undefined>>}) {
  const {holdingSlug}=await params,query=searchParams?await searchParams:{},requestedChart=typeof query.chart==="string"?query.chart:null;
  const actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
  const profiles=await accountingProfiles(actor);
  const setup=await financeSetupWorkspace(actor),ledger=setup.charts.length?await ledgerWorkspace(actor,requestedChart??undefined):null;
  const step=typeof query.step==="string"&&["chart","profile","opening","policy","ai","books","tools"].includes(query.step)?query.step:"checklist";
  const books=step==="books"?await financeBookWorkspace(actor):null;
  const tools=step==="tools"?await financeCompanyTools(actor):null;
  const complete=setup.checks.filter(check=>check.complete).length;
  const selectedChart=ledger?.chart as FinanceChart | undefined;
  const chartAccounts=ledger?.accounts as FinanceAccount[] | undefined;
  return <FeaturePage className="ui-page-content" data-source="live"><FinanceWorkspace actor={actor} base={base} context={ledger?{chartId:String(ledger.chart.id),chartName:String(ledger.chart.name),currency:String(ledger.chart.base_currency)}:undefined} active="setup" title="Finance setup" description="Prepare the company before receiving documents and posting">
    <SectionWorkspace nested showHeading={false} title="Finance setup" items={workspaceTopicLinks(base,query,sectionWorkspaceTopics.finance)} selected={step} context={ledger?`${ledger.chart.name} · ${ledger.chart.base_currency}`:"Company setup"}>
    {step==="checklist"&&<><section className={styles.hero}><div><p className="eyebrow">Readiness</p><strong>{complete}/{setup.checks.length}</strong><p>Key setup checks found by the system</p></div><aside><b>{complete===setup.checks.length?"Ready to test the workflow":"Setup still required"}</b><span className="muted">Go-live requires a close simulation and accountable data review.</span></aside></section>
    <section className={styles.panel}><h2>Getting-started checklist</h2>{setup.checks.map(check=><div className={styles.check} data-complete={check.complete} key={check.id}><span className={styles.checkMark}>{check.complete?"✓":"·"}</span><b>{check.label}</b><span className="muted">{check.detail}</span></div>)}</section></>}
    {["chart","opening"].includes(step)&&setup.charts.length>1&&<section className={styles.panel} aria-label="Finance chart selector"><h2>{step==="chart"?"Chart & account selection":"Opening balance chart"}</h2><p className="muted">The selected chart controls the accounts and periods shown below.</p><div className="ui-form-actions">{setup.charts.map(chart=><UiActionLink appearance="standard" variant="secondary" data-active={ledger?.chart.id===chart.id||undefined} href={canonicalUiHref(`${base}/finance/setup?step=${step}&chart=${encodeURIComponent(String(chart.id))}`)} key={String(chart.id)}>{String(chart.name)} · {String(chart.base_currency)}</UiActionLink>)}</div></section>}
    {["checklist","chart"].includes(step)&&!setup.charts.length&&actor.scope.kind==="holding"&&hasPermission(actor,"finance.write")&&<FinanceStarterControls base={base}/>}
    {step==="chart"&&hasPermission(actor,"finance.write")&&<FinanceChartControls base={base} chart={selectedChart} charts={setup.charts as FinanceChart[]} accounts={chartAccounts??[]}/>}
    {step==="profile"&&<AccountingProfileControls base={base} charts={setup.charts as FinanceChart[]} profiles={profiles} canPrepare={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")} userId={actor.userId}/>}
    {step==="opening"&&hasPermission(actor,"finance.write")&&ledger&&<OpeningBalanceImport base={base} chart={ledger.chart} accounts={ledger.accounts} periods={ledger.periods} parties={setup.parties}/>}
    {step==="opening"&&!ledger&&<p className={styles.empty}>Create a chart of accounts before importing opening balances.</p>}
    {step==="ai"&&<FinanceAiPolicyControls base={base}/>}
    {books&&books.rules.some(r=>r.status==="approved")&&<UiActionLink appearance="standard" href={canonicalUiHref(`${base}/finance/book-adjustments`)}>Secondary book policy adjustments</UiActionLink>}
    {books&&<FinanceBookControls base={base} data={books} canWrite={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")} userId={actor.userId}/>}
    {tools&&<FinanceToolControls base={base} rows={tools} canWrite={hasPermission(actor,"finance.write")}/>}
    {step==="policy"&&<section className={styles.panel}><h2>Go-live policy</h2><p className="muted">Define independent preparers and approvers, Thai tax settings, bank accounts, mappings from PMS, POS, service, and manufacturing systems, and close owners. A chart of accounts alone does not mean the company is ready for production.</p>{hasPermission(actor,'iam.read')&&<UiActionLink appearance="standard" href={canonicalUiHref(`${base}/technology`)}>Review Finance assignments in Organization &amp; Access</UiActionLink>}</section>}
  </SectionWorkspace></FinanceWorkspace></FeaturePage>;
}
