'use client';

import { useMemo, useState, type ReactNode } from 'react';
import Link from 'next/link';
import {
  Activity,
  ArrowRight,
  Banknote,
  BarChart3,
  Boxes,
  CheckCircle2,
  CircleAlert,
  CircleDollarSign,
  FileSearch,
  Gauge,
  Landmark,
  ReceiptText,
  Search,
  ShieldCheck,
  SlidersHorizontal,
  Sparkles,
  TrendingUp,
  WalletCards,
} from 'lucide-react';
import {
  Area,
  AreaChart,
  CartesianGrid,
  ResponsiveContainer,
  Tooltip,
  XAxis,
  YAxis,
} from 'recharts';
import type { ExecutiveFinance } from '@/finance/reporting';
import { ReportAiAsk } from './ReportAiAsk';

const money = (value: number, compact = false) => value.toLocaleString('en-US', {
  notation: compact ? 'compact' : 'standard',
  minimumFractionDigits: compact ? 0 : 2,
  maximumFractionDigits: compact ? 1 : 2,
});

const pct = (value: number) => `${value.toFixed(1)}%`;

function Metric({ label, value, caption, href, icon, tone = 'text-ink' }: { label: string; value: string; caption: string; href: string; icon: ReactNode; tone?: string }) {
  return <Link href={href} className="group relative overflow-hidden rounded-2xl border border-rule bg-paper/75 p-4 shadow-[inset_0_1px_0_var(--glass-highlight)] transition duration-200 hover:-translate-y-1 hover:border-accent/65 hover:bg-paper-2">
    <span aria-hidden className="absolute inset-x-8 top-0 h-px bg-gradient-to-r from-transparent via-accent/55 to-transparent opacity-0 transition group-hover:opacity-100" />
    <div className="flex items-start justify-between gap-3">
      <span className="grid h-9 w-9 place-items-center rounded-xl border border-rule bg-paper-2 text-accent">{icon}</span>
      <ArrowRight size={15} className="mt-1 text-mute transition group-hover:translate-x-0.5 group-hover:text-accent" />
    </div>
    <div className="mt-5 text-[11px] font-semibold uppercase tracking-[0.13em] text-mute">{label}</div>
    <div className={`mt-1.5 font-mono text-[clamp(1.3rem,2vw,1.75rem)] font-semibold tracking-[-0.045em] tabular-nums ${tone}`}>{value}</div>
    <p className="mt-1 min-h-8 text-xs leading-4 text-ink-2">{caption}</p>
  </Link>;
}

function Head({ eyebrow, title, detail, children }: { eyebrow: string; title: string; detail: string; children?: ReactNode }) {
  return <div className="flex flex-wrap items-start justify-between gap-4">
    <div className="max-w-2xl">
      <p className="text-[11px] font-semibold uppercase tracking-[0.16em] text-accent">{eyebrow}</p>
      <h2 className="mt-1 text-xl font-semibold tracking-[-0.035em] text-ink sm:text-2xl">{title}</h2>
      <p className="mt-1 text-sm leading-6 text-ink-2">{detail}</p>
    </div>
    {children}
  </div>;
}

function futurePeriods(last: string | undefined, fallback: string) {
  const match = last?.match(/^(\d{4})-(\d{2})$/);
  const fallbackMatch = fallback.match(/^(\d{4})-(\d{2})/);
  const start = match
    ? new Date(Date.UTC(Number(match[1]), Number(match[2]) - 1, 1))
    : new Date(Date.UTC(Number(fallbackMatch?.[1] ?? 2000), Number(fallbackMatch?.[2] ?? 1) - 1, 1));
  return Array.from({ length: 6 }, (_, index) => {
    const date = new Date(Date.UTC(start.getUTCFullYear(), start.getUTCMonth() + index + 1, 1));
    return `${date.getUTCFullYear()}-${String(date.getUTCMonth() + 1).padStart(2, '0')}`;
  });
}

export function ExecutiveFinanceDashboard({ data, canUseAi = false }: { data: ExecutiveFinance; canUseAi?: boolean }) {
  const query = `from=${data.filters.dateFrom}&to=${data.filters.dateTo}${data.filters.branchId ? `&branch=${data.filters.branchId}` : ''}`;
  const [growth, setGrowth] = useState(8);
  const [margin, setMargin] = useState(2);
  const [opex, setOpex] = useState(3);
  const [collection, setCollection] = useState(85);
  const scenario = useMemo(() => {
    const baseMargin = data.actual.revenue ? data.actual.grossMargin / data.actual.revenue : 0;
    const revenue = data.actual.revenue * (1 + growth / 100);
    const marginRate = Math.max(-1, Math.min(1, baseMargin + margin / 100));
    const grossMargin = revenue * marginRate;
    const operatingExpense = data.actual.operatingExpense * (1 + opex / 100);
    const netIncome = grossMargin - operatingExpense;
    const collectionImpact = data.actual.ar * (collection / 100 - 1);
    const cash = data.actual.forecastCash + (netIncome - data.actual.netIncome) + collectionImpact;
    return { revenue, marginRate, grossMargin, operatingExpense, netIncome, collectionImpact, cash };
  }, [collection, data, growth, margin, opex]);
  const chart = useMemo(() => {
    const actual = data.revenueTrend.map((item) => ({ period: item.period, actual: item.revenue, projected: null as number | null }));
    const monthly = data.revenueTrend.at(-1)?.revenue ?? (data.revenueTrend.length ? data.actual.revenue / data.revenueTrend.length : data.actual.revenue);
    const rate = Math.pow(Math.max(0, 1 + growth / 100), 1 / 6) - 1;
    const future = futurePeriods(data.revenueTrend.at(-1)?.period, data.filters.dateTo).map((period, index) => ({ period, actual: null as number | null, projected: monthly * Math.pow(1 + rate, index + 1) }));
    if (actual.length && future.length) future.unshift({ period: actual.at(-1)!.period, actual: null, projected: actual.at(-1)!.actual });
    return [...actual, ...future];
  }, [data, growth]);
  const tied = Object.values({
    journals: data.tieOuts.journalsBalanced,
    balance: data.tieOuts.balanceSheetTied,
    ar: data.tieOuts.arTied,
    ap: data.tieOuts.apTied,
    inventory: data.tieOuts.inventoryTied,
  }).filter(Boolean).length;
  const marginRate = data.actual.revenue ? data.actual.grossMargin / data.actual.revenue * 100 : 0;
  const reportContext = {
    period: data.filters,
    posted_actuals: data.actual,
    operational_pipeline: data.pipeline,
    ar_aging: data.arAging,
    ap_aging: data.apAging,
    inventory: data.stock,
    control_tie_outs: data.tieOuts,
    generated_at: data.generatedAt,
  };
  const actions = [
    { href: '#financial-projection', label: 'Financial projection', detail: 'Six-month outlook', icon: <TrendingUp size={17} /> },
    { href: '#executive-summary', label: 'Summarize', detail: 'Board-ready brief', icon: <Sparkles size={17} /> },
    { href: '#audit', label: 'Audit', detail: 'Control tie-outs', icon: <ShieldCheck size={17} /> },
    { href: '#account-details', label: 'Account detail', detail: 'Statements & ledger', icon: <Search size={17} /> },
    { href: '#simulation', label: 'Simulate data', detail: 'Model assumptions', icon: <SlidersHorizontal size={17} /> },
  ];

  return <div className="space-y-8">
    <section className="panel-elevated overflow-hidden p-2">
      <div className="grid gap-2 md:grid-cols-5">
        {actions.map((item, index) => <a href={item.href} key={item.href} className="group flex items-center gap-3 rounded-xl border border-transparent px-3 py-3 transition hover:border-rule hover:bg-paper-2">
          <span className={`grid h-9 w-9 shrink-0 place-items-center rounded-xl ${index === 0 ? 'bg-accent text-action-ink' : 'border border-rule bg-paper text-accent'}`}>{item.icon}</span>
          <span className="min-w-0"><span className="block text-sm font-semibold text-ink">{item.label}</span><span className="block truncate text-[11px] text-mute">{item.detail}</span></span>
        </a>)}
      </div>
    </section>

    <section id="executive-summary" className="scroll-mt-24">
      <Head eyebrow="01 · Executive summary" title="The business, in one decision frame" detail="Posted performance, liquidity, and working-capital exposure for the selected reporting scope.">
        <ReportAiAsk title="Executive summary" scope="posted performance and liquidity" context={reportContext} canUseAi={canUseAi} prompts={['Write a board-ready summary with the three most important decisions.', 'What changed materially and what should leadership watch next?', 'Separate performance, liquidity, and operating pipeline risk.']} />
      </Head>
      <div className="mt-5 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <Metric label="Revenue" value={`฿${money(data.actual.revenue, true)}`} caption="Posted revenue in the selected period" href={`/reports?report=profit_and_loss&${query}`} icon={<CircleDollarSign size={18} />} tone="text-positive" />
        <Metric label="Gross margin" value={pct(marginRate)} caption={`฿${money(data.actual.grossMargin, true)} contribution`} href={`/reports?report=gross_margin&${query}`} icon={<BarChart3 size={18} />} tone={data.actual.grossMargin >= 0 ? 'text-positive' : 'text-critical'} />
        <Metric label="Net income" value={`฿${money(data.actual.netIncome, true)}`} caption={`Operating expense ฿${money(data.actual.operatingExpense, true)}`} href={`/reports?report=profit_and_loss&${query}`} icon={<Activity size={18} />} tone={data.actual.netIncome >= 0 ? 'text-positive' : 'text-critical'} />
        <Metric label="Available cash" value={`฿${money(data.actual.cash, true)}`} caption={`${pct(data.actual.reconciledPercent)} of bank rows reconciled`} href={`/reports?report=cash_flow&${query}`} icon={<WalletCards size={18} />} />
      </div>
      <div className="mt-3 grid gap-3 sm:grid-cols-2 xl:grid-cols-4">
        <Metric label="Receivables" value={`฿${money(data.actual.ar, true)}`} caption={`${data.arAging.reduce((sum, row) => sum + row.count, 0)} open documents`} href={`/reports?report=ar_aging&${query}`} icon={<ReceiptText size={18} />} />
        <Metric label="Payables" value={`฿${money(data.actual.ap, true)}`} caption={`${data.apAging.reduce((sum, row) => sum + row.count, 0)} open documents`} href={`/reports?report=ap_aging&${query}`} icon={<Landmark size={18} />} tone="text-caution" />
        <Metric label="Inventory" value={`฿${money(data.actual.inventory, true)}`} caption={`${data.stock.turnover.toFixed(2)}× annualized turnover`} href={`/reports?report=inventory&${query}`} icon={<Boxes size={18} />} />
        <Metric label="30-day cash" value={`฿${money(data.actual.forecastCash, true)}`} caption="Due AR less due AP and commitments" href="#financial-projection" icon={<TrendingUp size={18} />} tone={data.actual.forecastCash >= 0 ? 'text-positive' : 'text-critical'} />
      </div>
    </section>

    <section id="financial-projection" className="panel-elevated scroll-mt-24 overflow-hidden p-5 sm:p-6">
      <Head eyebrow="02 · Financial projection" title="Actual trajectory into a six-month outlook" detail="The solid line is posted monthly revenue. The dotted projection applies the current revenue-growth scenario to the latest observed month.">
        <ReportAiAsk title="Financial projection" scope="actual trend and forward outlook" context={{ actual: data.actual, revenue_trend: data.revenueTrend, scenario }} canUseAi={canUseAi} prompts={['Interpret this projection for the CFO.', 'Where is the downside risk in this outlook?', 'Which assumption has the highest cash sensitivity?']} />
      </Head>
      <div className="mt-6 grid gap-5 xl:grid-cols-[minmax(0,1.7fr)_minmax(18rem,0.8fr)]">
        <div className="min-h-[300px] rounded-2xl border border-rule bg-paper/60 p-3 sm:p-5">
          {chart.some((point) => point.actual || point.projected) ? <ResponsiveContainer width="100%" height={280}>
            <AreaChart data={chart} margin={{ top: 12, right: 8, left: -8, bottom: 0 }}>
              <defs><linearGradient id="actualFill" x1="0" y1="0" x2="0" y2="1"><stop offset="0%" stopColor="var(--accent)" stopOpacity={0.28} /><stop offset="100%" stopColor="var(--accent)" stopOpacity={0} /></linearGradient></defs>
              <CartesianGrid vertical={false} stroke="var(--rule)" strokeDasharray="3 5" opacity={0.5} />
              <XAxis dataKey="period" tick={{ fill: 'var(--mute)', fontSize: 10 }} axisLine={false} tickLine={false} />
              <YAxis tickFormatter={(value) => `฿${money(Number(value), true)}`} tick={{ fill: 'var(--mute)', fontSize: 10 }} axisLine={false} tickLine={false} width={62} />
              <Tooltip formatter={(value) => [`฿${money(Number(value))}`, 'Revenue']} contentStyle={{ border: '1px solid var(--rule)', borderRadius: 12, background: 'var(--glass-floating)', color: 'var(--ink)' }} />
              <Area type="monotone" dataKey="actual" stroke="var(--accent)" strokeWidth={2.5} fill="url(#actualFill)" connectNulls={false} isAnimationActive={false} />
              <Area type="monotone" dataKey="projected" stroke="var(--info)" strokeWidth={2.5} strokeDasharray="6 5" fill="transparent" connectNulls isAnimationActive={false} />
            </AreaChart>
          </ResponsiveContainer> : <div className="grid h-[280px] place-items-center text-center text-sm text-mute">Projection begins when posted revenue history is available.</div>}
        </div>
        <div className="grid gap-3 sm:grid-cols-3 xl:grid-cols-1">
          <ProjectionStat label="Current cash" value={`฿${money(data.actual.cash, true)}`} detail="Posted balance" />
          <ProjectionStat label="Base 30-day forecast" value={`฿${money(data.actual.forecastCash, true)}`} detail="Due items + commitments" tone={data.actual.forecastCash >= 0 ? 'text-positive' : 'text-critical'} />
          <ProjectionStat label="Scenario cash" value={`฿${money(scenario.cash, true)}`} detail="Current simulation" tone={scenario.cash >= 0 ? 'text-positive' : 'text-critical'} />
        </div>
      </div>
    </section>

    <section id="simulation" className="panel-elevated scroll-mt-24 overflow-hidden p-5 sm:p-6">
      <Head eyebrow="03 · Scenario studio" title="Change the assumptions. See the financial consequence." detail="A transparent sensitivity model anchored to posted revenue, gross margin, operating expense, open AR, and the current 30-day cash forecast.">
        <ReportAiAsk title="Scenario interpretation" scope="the active financial simulation" context={{ assumptions: { revenue_growth_percent: growth, gross_margin_shift_points: margin, operating_expense_change_percent: opex, ar_collection_percent: collection }, baseline: data.actual, result: scenario, formula: 'scenario cash = base 30-day cash forecast + change in net income + open AR collection sensitivity versus 100%' }} canUseAi={canUseAi} prompts={['Explain this scenario and recommend one management action.', 'Compare this scenario to the posted baseline.', 'What would need to change to protect cash?']} />
      </Head>
      <div className="mt-6 grid gap-6 xl:grid-cols-[minmax(18rem,0.85fr)_minmax(0,1.4fr)]">
        <div className="space-y-5 rounded-2xl border border-rule bg-paper/65 p-5">
          <Range label="Revenue growth" value={growth} min={-30} max={50} suffix="%" onChange={setGrowth} />
          <Range label="Gross-margin shift" value={margin} min={-10} max={10} suffix=" pts" onChange={setMargin} />
          <Range label="Operating-expense change" value={opex} min={-20} max={30} suffix="%" onChange={setOpex} />
          <Range label="Open AR collected" value={collection} min={50} max={100} suffix="%" onChange={setCollection} />
          <button type="button" onClick={() => { setGrowth(8); setMargin(2); setOpex(3); setCollection(85); }} className="text-xs font-medium text-mute underline-offset-4 hover:text-ink hover:underline">Reset assumptions</button>
        </div>
        <div>
          <div className="grid gap-3 sm:grid-cols-2">
            <ScenarioStat label="Projected revenue" value={`฿${money(scenario.revenue)}`} delta={scenario.revenue - data.actual.revenue} />
            <ScenarioStat label="Projected gross margin" value={pct(scenario.marginRate * 100)} delta={scenario.grossMargin - data.actual.grossMargin} />
            <ScenarioStat label="Projected net income" value={`฿${money(scenario.netIncome)}`} delta={scenario.netIncome - data.actual.netIncome} />
            <ScenarioStat label="Scenario cash" value={`฿${money(scenario.cash)}`} delta={scenario.cash - data.actual.forecastCash} />
          </div>
          <div className="mt-3 flex gap-3 rounded-2xl border border-info/50 bg-info-soft p-4 text-sm text-ink-2">
            <Gauge className="mt-0.5 shrink-0 text-info" size={18} />
            <p><span className="font-semibold text-ink">Model note.</span> Scenario cash starts from the live 30-day forecast, then applies the change in net income and collection sensitivity on open AR. It does not post or alter accounting data.</p>
          </div>
        </div>
      </div>
    </section>

    <section id="audit" className="scroll-mt-24">
      <Head eyebrow="04 · Audit and controls" title="Every headline number should tie back" detail={`${tied} of 5 core financial controls are tied for this reporting scope.`}>
        <ReportAiAsk title="Audit review" scope="journal, statement, subledger, and inventory controls" context={{ period: data.filters, tie_outs: data.tieOuts, reconciled_percent: data.actual.reconciledPercent }} canUseAi={canUseAi} prompts={['Rank the control exceptions by financial risk.', 'Draft an audit follow-up plan for the failed tie-outs.', 'Summarize the evidence available for this close.']} />
      </Head>
      <div className="mt-5 grid gap-4 lg:grid-cols-[0.7fr_1.3fr]">
        <div className="panel-elevated flex min-h-56 flex-col items-center justify-center p-6 text-center">
          <div className="relative grid h-32 w-32 place-items-center rounded-full" style={{ background: `conic-gradient(var(--positive) ${tied / 5 * 360}deg, var(--paper-3) 0deg)` }}>
            <div className="grid h-24 w-24 place-items-center rounded-full bg-paper"><div><div className="font-mono text-3xl font-semibold text-ink">{tied}/5</div><div className="text-[10px] uppercase tracking-widest text-mute">controls tied</div></div></div>
          </div>
          <p className={`mt-4 text-sm font-semibold ${tied === 5 ? 'text-positive' : 'text-caution'}`}>{tied === 5 ? 'Control posture is clean' : `${5 - tied} exception${5 - tied === 1 ? '' : 's'} need review`}</p>
        </div>
        <div className="panel-elevated divide-y divide-rule overflow-hidden">
          <Control label="Debits equal credits" ok={data.tieOuts.journalsBalanced} detail={`฿${money(data.tieOuts.debits)} debit · ฿${money(data.tieOuts.credits)} credit`} />
          <Control label="Balance-sheet equation" ok={data.tieOuts.balanceSheetTied} detail={`฿${money(data.tieOuts.assets)} assets · ฿${money(data.tieOuts.liabilitiesEquity)} liabilities + equity`} />
          <Control label="AR control equals subledger" ok={data.tieOuts.arTied} detail={`Open receivables ฿${money(data.actual.ar)}`} />
          <Control label="AP control equals subledger" ok={data.tieOuts.apTied} detail={`Open payables ฿${money(data.actual.ap)}`} />
          <Control label="Inventory GL equals valuation" ok={data.tieOuts.inventoryTied} detail={`Valuation ฿${money(data.actual.inventory)}`} />
        </div>
      </div>
    </section>

    <section id="account-details" className="scroll-mt-24">
      <Head eyebrow="05 · Accounting deep dive" title="Move from the signal to the source record" detail="Open the statement, subledger, tax register, or general ledger behind each executive signal.">
        <ReportAiAsk title="Accounting detail" scope="accounts, working capital, inventory, budget, and FX exposure" context={reportContext} canUseAi={canUseAi} prompts={['Which account area deserves the deepest review?', 'Explain the working-capital position in accounting terms.', 'Give me a drill-down plan from statements to ledger evidence.']} />
      </Head>
      <div className="mt-5 grid gap-4 xl:grid-cols-3">
        <DetailCard title="Financial statements" icon={<FileSearch size={19} />} links={[[`/reports?report=profit_and_loss&${query}`, 'Profit & loss', `฿${money(data.actual.netIncome, true)} net income`], [`/reports?report=balance_sheet&${query}`, 'Balance sheet', `฿${money(data.tieOuts.assets, true)} assets`], [`/reports?report=cash_flow&${query}`, 'Cash flow', `฿${money(data.actual.cash, true)} cash`], [`/reports?report=trial_balance&${query}`, 'Trial balance', data.tieOuts.journalsBalanced ? 'Balanced' : 'Review required']]} />
        <DetailCard title="Working capital" icon={<Banknote size={19} />} links={[[`/reports?report=ar_aging&${query}`, 'AR aging', `฿${money(data.actual.ar, true)} open`], [`/reports?report=ap_aging&${query}`, 'AP aging', `฿${money(data.actual.ap, true)} open`], [`/reports?report=inventory&${query}`, 'Inventory valuation', `฿${money(data.actual.inventory, true)}`], [`/reports?report=fx_exposure&${query}`, 'FX exposure', `฿${money(data.actual.fxExposureThb, true)}`]]} />
        <DetailCard title="Control and compliance" icon={<ShieldCheck size={19} />} links={[[`/reports?report=budget_vs_actual&${query}`, 'Budget vs actual', `฿${money(data.actual.budgetVariance, true)} variance`], [`/reports?report=vat_register&${query}`, 'VAT register', 'Posted tax records'], [`/reports?report=wht_register&${query}`, 'WHT register', 'Posted withholding records'], [`/ledger?${query}`, 'General ledger', 'Account-level evidence']]} />
      </div>
    </section>

    <section className="rounded-2xl border border-caution/55 bg-caution-soft/70 p-5 sm:p-6">
      <div className="flex flex-wrap items-start justify-between gap-4">
        <div className="flex gap-3"><CircleAlert className="mt-0.5 shrink-0 text-caution" size={20} /><div><h2 className="font-semibold text-ink">Operational pipeline is forecast context</h2><p className="mt-1 text-sm text-ink-2">Open orders and unposted expenses are deliberately excluded from financial statements.</p></div></div>
        <span className="rounded-full border border-caution/60 bg-paper/45 px-3 py-1 text-[10px] font-semibold uppercase tracking-widest text-caution">Unposted</span>
      </div>
      <div className="mt-5 grid gap-3 sm:grid-cols-3"><Pipeline label="Open sales orders" value={data.pipeline.openSales} href="/sales" /><Pipeline label="Open procurement" value={data.pipeline.openProcurement} href="/po" /><Pipeline label="Unposted expenses" value={data.pipeline.unpostedExpenses} href="/expense" /></div>
    </section>
  </div>;
}

function ProjectionStat({ label, value, detail, tone = 'text-ink' }: { label: string; value: string; detail: string; tone?: string }) {
  return <div className="rounded-2xl border border-rule bg-paper/70 p-4"><div className="text-[10px] font-semibold uppercase tracking-[0.14em] text-mute">{label}</div><div className={`mt-2 font-mono text-2xl font-semibold tabular-nums ${tone}`}>{value}</div><div className="mt-1 text-xs text-ink-2">{detail}</div></div>;
}

function Range({ label, value, min, max, suffix, onChange }: { label: string; value: number; min: number; max: number; suffix: string; onChange: (value: number) => void }) {
  return <label className="block"><span className="flex items-center justify-between gap-3 text-sm"><span className="font-medium text-ink">{label}</span><span className="rounded-lg border border-rule bg-paper-2 px-2 py-1 font-mono text-xs text-accent">{value > 0 && min < 0 ? '+' : ''}{value}{suffix}</span></span><input type="range" min={min} max={max} value={value} onChange={(event) => onChange(Number(event.target.value))} className="mt-3 h-1.5 w-full cursor-pointer accent-[var(--accent)]" /></label>;
}

function ScenarioStat({ label, value, delta }: { label: string; value: string; delta: number }) {
  return <div className="rounded-2xl border border-rule bg-paper/70 p-4"><div className="text-xs font-semibold uppercase tracking-[0.12em] text-mute">{label}</div><div className="mt-2 font-mono text-xl font-semibold tabular-nums text-ink">{value}</div><div className={`mt-1 text-xs font-medium ${delta >= 0 ? 'text-positive' : 'text-critical'}`}>{delta >= 0 ? '+' : '−'}฿{money(Math.abs(delta), true)} vs baseline</div></div>;
}

function Control({ label, ok, detail }: { label: string; ok: boolean; detail: string }) {
  return <div className="flex items-center justify-between gap-4 px-5 py-4"><div className="flex min-w-0 items-start gap-3">{ok ? <CheckCircle2 className="mt-0.5 shrink-0 text-positive" size={18} /> : <CircleAlert className="mt-0.5 shrink-0 text-critical" size={18} />}<div><div className="text-sm font-medium text-ink">{label}</div><div className="mt-0.5 text-xs text-mute">{detail}</div></div></div><span className={`shrink-0 rounded-full border px-2.5 py-1 text-[10px] font-semibold uppercase tracking-wider ${ok ? 'border-positive/50 bg-positive-soft text-positive' : 'border-critical/50 bg-critical-soft text-critical'}`}>{ok ? 'Tied' : 'Review'}</span></div>;
}

function DetailCard({ title, icon, links }: { title: string; icon: ReactNode; links: string[][] }) {
  return <section className="panel-elevated overflow-hidden"><div className="flex items-center gap-3 border-b border-rule p-4"><span className="grid h-9 w-9 place-items-center rounded-xl border border-rule bg-paper-2 text-accent">{icon}</span><h3 className="font-semibold text-ink">{title}</h3></div><div className="divide-y divide-rule">{links.map(([href, label, value]) => <Link href={href} key={href} className="group flex items-center justify-between gap-4 px-4 py-3.5 transition hover:bg-paper-2"><span className="text-sm text-ink-2 group-hover:text-ink">{label}</span><span className="flex items-center gap-2 text-right font-mono text-xs text-mute"><span>{value}</span><ArrowRight size={13} className="transition group-hover:translate-x-0.5 group-hover:text-accent" /></span></Link>)}</div></section>;
}

function Pipeline({ label, value, href }: { label: string; value: number; href: string }) {
  return <Link href={href} className="group rounded-xl border border-caution/40 bg-paper/50 p-4 transition hover:border-caution"><div className="flex items-center justify-between gap-3 text-xs font-medium text-ink-2"><span>{label}</span><ArrowRight size={13} className="transition group-hover:translate-x-0.5" /></div><div className="mt-2 font-mono text-xl font-semibold text-ink">฿{money(value, true)}</div></Link>;
}
