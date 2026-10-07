
import { ActionLink as UiActionLink } from "@/components/ui/foundation";
import { FinanceAiPolicyControls } from "@/components/finance/finance-ai-panel";
import { AccountingProfileControls } from "@/components/finance/accounting-profile-controls";
import { accountingProfiles } from "@/server/modules/finance/public/accounting-profiles";
import { RecordLink as Link } from "@/components/records/record-link";
import { FeaturePage } from "@/components/features/feature-page";
import { FinanceWorkspace, financeStyles as styles } from "@/components/finance/finance-workspace";
import { FinanceChartControls, type FinanceChart, type FinanceAccount } from "@/components/modules/finance-chart-controls";
import { OpeningBalanceImport } from "@/components/modules/ledger-controls";
import { hasPermission } from "@/lib/permissions";
import { workspaceBase } from "@/lib/workspace";
import { publicScope, requireWorkContext } from "@/server/modules/identity_access/public/authorization";
import { financeSetupWorkspace } from "@/server/modules/finance/public/finance-workspace";
import { ledgerWorkspace } from "@/server/modules/finance/public/ledger";
import { financeBookWorkspace } from "@/server/modules/finance/public/finance-books";
import { FinanceBookControls } from "@/components/finance/finance-book-controls";
import { financeCompanyTools } from "@/server/modules/finance/public/finance-company-tools";
import { FinanceToolControls } from "@/components/finance/finance-tool-controls";
import { FinanceStarterControls } from "@/components/finance/finance-starter-controls";

export const dynamic="force-dynamic";
export default async function FinanceSetupPage({params,searchParams}:{params:Promise<{holdingSlug:string}>;searchParams?:Promise<Record<string,string|string[]|undefined>>}) {
  const {holdingSlug}=await params,query=searchParams?await searchParams:{},requestedChart=typeof query.chart==="string"?query.chart:null;
  const actor=await requireWorkContext(holdingSlug),base=workspaceBase(publicScope(actor));
  const profiles=await accountingProfiles(actor);
  const setup=await financeSetupWorkspace(actor),ledger=setup.charts.length?await ledgerWorkspace(actor,requestedChart??undefined):null;
  const step=typeof query.step==="string"&&["chart","profile","opening","policy","ai","books","tools"].includes(query.step)?query.step:"checklist";
  const books=step==="books"?await financeBookWorkspace(actor):null;
  const tools=step==="tools"?await financeCompanyTools(actor):null;
  const steps=[["checklist","Readiness"],["chart","Charts & accounts"],["profile","Company accounting policy"],["opening","Opening balances"],["books","Books & dimensions"],["tools","Company tools"],["policy","Go-live review"],["ai","AI preparation"]];
  const stepHref=(value:string)=>`${base}/finance/setup?${new URLSearchParams({step:value,...(ledger?{chart:String(ledger.chart.id)}:{})})}`;
  const complete=setup.checks.filter(check=>check.complete).length;
  const selectedChart=ledger?.chart as FinanceChart | undefined;
  const chartAccounts=ledger?.accounts as FinanceAccount[] | undefined;
  return <FeaturePage className="ui-page-content" data-source="live"><FinanceWorkspace base={base} context={ledger?{chartId:String(ledger.chart.id),chartName:String(ledger.chart.name),currency:String(ledger.chart.base_currency)}:undefined} active="setup" title="Finance setup" description="Prepare the company before receiving documents and posting">
    <nav className={styles.setupSteps} aria-label="Finance setup steps">{steps.map(([key,label],index)=><Link key={key} href={stepHref(key)} aria-current={step===key?"step":undefined}><span>{index+1}</span>{label}</Link>)}</nav>
    {step==="checklist"&&<><section className={styles.hero}><div><p className="eyebrow">Readiness</p><strong>{complete}/{setup.checks.length}</strong><p>Key setup checks found by the system</p></div><aside><b>{complete===setup.checks.length?"Ready to test the workflow":"Setup still required"}</b><span className="muted">Go-live requires a close simulation and accountable data review.</span></aside></section>
    <section className={styles.panel}><h2>Getting-started checklist</h2>{setup.checks.map(check=><div className={styles.check} data-complete={check.complete} key={check.id}><span className={styles.checkMark}>{check.complete?"✓":"·"}</span><b>{check.label}</b><span className="muted">{check.detail}</span></div>)}</section></>}
    {["chart","opening"].includes(step)&&setup.charts.length>1&&<section className={styles.panel} aria-label="Finance chart selector"><h2>{step==="chart"?"Chart & account selection":"Opening balance chart"}</h2><p className="muted">The selected chart controls the accounts and periods shown below.</p><div className="ui-form-actions">{setup.charts.map(chart=><UiActionLink appearance="standard" variant="secondary" data-active={ledger?.chart.id===chart.id||undefined} href={`${base}/finance/setup?step=${step}&chart=${encodeURIComponent(String(chart.id))}`} key={String(chart.id)}>{String(chart.name)} · {String(chart.base_currency)}</UiActionLink>)}</div></section>}
    {["checklist","chart"].includes(step)&&!setup.charts.length&&actor.scope.kind==="holding"&&hasPermission(actor,"finance.write")&&<FinanceStarterControls base={base}/>}
    {step==="chart"&&hasPermission(actor,"finance.write")&&<FinanceChartControls base={base} chart={selectedChart} charts={setup.charts as FinanceChart[]} accounts={chartAccounts??[]}/>}
    {step==="profile"&&<AccountingProfileControls base={base} charts={setup.charts as FinanceChart[]} profiles={profiles} canPrepare={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")} userId={actor.userId}/>}
    {step==="opening"&&hasPermission(actor,"finance.write")&&ledger&&<OpeningBalanceImport base={base} chart={ledger.chart} accounts={ledger.accounts} periods={ledger.periods} parties={setup.parties}/>}
    {step==="opening"&&!ledger&&<p className={styles.empty}>Create a chart of accounts before importing opening balances.</p>}
    {step==="ai"&&<FinanceAiPolicyControls base={base}/>}
    {books&&books.rules.some(r=>r.status==="approved")&&<UiActionLink appearance="standard" href={`${base}/finance/book-adjustments`}>Secondary book policy adjustments</UiActionLink>}
    {books&&<FinanceBookControls base={base} data={books} canWrite={hasPermission(actor,"finance.write")} canApprove={hasPermission(actor,"finance.approve")} userId={actor.userId}/>}
    {tools&&<FinanceToolControls base={base} rows={tools} canWrite={hasPermission(actor,"finance.write")}/>}
    {step==="policy"&&<section className={styles.panel}><h2>Go-live policy</h2><p className="muted">Define independent preparers and approvers, Thai tax settings, bank accounts, mappings from PMS, POS, service, and manufacturing systems, and close owners. A chart of accounts alone does not mean the company is ready for production.</p>{hasPermission(actor,'iam.read')&&<UiActionLink appearance="standard" href={`${base}/technology`}>Review Finance assignments in Organization &amp; Access</UiActionLink>}</section>}
  </FinanceWorkspace></FeaturePage>;
}
