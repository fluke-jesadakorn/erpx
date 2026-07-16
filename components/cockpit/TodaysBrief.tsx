import 'server-only';
import { query } from '@/db';
import { loadExecutiveBrief } from '@/server/execBrief';
import { getExecutiveReport } from '@/dashboard/queries';
import { getSecondaryLocale } from '@/server/locale';
import { T } from '@/components/i18n/TServer';
import { ExecutiveNarrative } from '@/components/ai/ExecutiveNarrative';
import { DeptBudgetStrip } from './DeptBudgetStrip';
import { SparklineTrends } from './SparklineTrends';
import { TodaysBriefPinnedCharts } from './TodaysBriefPinnedCharts';
import { AiSummaryCharts } from './AiSummaryCharts';
import { ProjectionCard } from './ProjectionCard';
import { CockpitAiNarrative } from './CockpitAiNarrative';
import { CockpitSqlPanel } from './CockpitSqlPanel';
import { FinanceRagPanel } from './FinanceRagPanel';
import { GlCommentaryCard } from './GlCommentaryCard';
import Link from 'next/link';

export interface TodaysBriefActor {
  id: number;
  fullname: string;
  role_name: string;
  effective_level?: number;
}

export interface TodaysBriefProps {
  actor: TodaysBriefActor;
}

function formatTHB(n: number): string {
  return (Number(n) || 0).toLocaleString('th-TH', { maximumFractionDigits: 0 });
}

function timeOfDay(d: Date): string {
  return d.toLocaleString('th-TH', { hour: '2-digit', minute: '2-digit' });
}

interface StuckRow {
  waybill_id: string;
  origin_id: number;
  vendor_name: string;
  total_amount: string;
  current_stage: string;
  age_hours: string;
}

interface StuckSalesRow {
  waybill_id: string;
  so_id: number;
  customer_code: string | null;
  customer_name: string | null;
  total_amount: string;
  current_stage: string;
  age_hours: string;
}

interface AnomalyRow {
  id: number;
  message: string;
  severity: string;
  created_at: string;
}

async function loadStuck(): Promise<StuckRow[]> {
  const r = await query<StuckRow>(
    `SELECT w.id AS waybill_id,
            e.id AS origin_id,
            COALESCE(e.vendor_name, '—') AS vendor_name,
            COALESCE(e.total_amount, 0)::text AS total_amount,
            w.current_stage,
            EXTRACT(EPOCH FROM (NOW() - e.created_at))/3600 AS age_hours
       FROM expenses e
       JOIN waybills w ON w.origin='expense' AND w.origin_id = e.id
      WHERE w.current_stage NOT IN ('disbursed','rejected','gl_confirmed')
        AND e.created_at < NOW() - INTERVAL '24 hours'
      ORDER BY e.created_at ASC
      LIMIT 3`,
  );
  return r.rows;
}

async function loadStuckSales(): Promise<StuckSalesRow[]> {
  const r = await query<StuckSalesRow>(
    `SELECT w.id AS waybill_id,
            so.id AS so_id,
            c.code AS customer_code,
            c.name AS customer_name,
            COALESCE(so.total_amount, 0)::text AS total_amount,
            w.current_stage,
            EXTRACT(EPOCH FROM (NOW() - so.created_at))/3600 AS age_hours
       FROM sales_orders so
       JOIN waybills w ON w.origin='so' AND w.origin_id = so.id
       LEFT JOIN customers c ON c.id = so.customer_id
      WHERE w.current_stage NOT IN ('so_paid','rejected')
        AND so.created_at < NOW() - INTERVAL '24 hours'
   ORDER BY so.created_at ASC
      LIMIT 3`,
  );
  return r.rows;
}

async function loadSalesSparkline(): Promise<{ day: string; total: number }[]> {
  const r = await query<{ day: string; total: string }>(
    `SELECT to_char(date_trunc('day', so.created_at), 'YYYY-MM-DD') AS day,
            COALESCE(SUM(so.total_amount), 0)::text AS total
       FROM sales_orders so
      WHERE so.created_at >= CURRENT_DATE - INTERVAL '6 days'
        AND so.status NOT IN ('so_draft','rejected')
   GROUP BY 1
   ORDER BY 1 ASC`,
  );
  return r.rows.map((row) => ({ day: row.day, total: Number(row.total ?? 0) }));
}

async function loadAnomalies(): Promise<AnomalyRow[]> {
  const r = await query<{ id: number; message: string; severity: string; created_at: string | Date }>(
    `SELECT id,
            COALESCE(payload_json->>'message', type) AS message,
            COALESCE(payload_json->>'severity', 'warning') AS severity,
            created_at::text AS created_at
       FROM notifications
      WHERE type = 'anomaly'
        AND cleared_at IS NULL
      ORDER BY created_at DESC
      LIMIT 3`,
  );
  return r.rows.map((row) => ({
    id: row.id,
    message: row.message,
    severity: row.severity,
    created_at: typeof row.created_at === 'string' ? row.created_at : String(row.created_at),
  }));
}

export async function TodaysBrief({ actor }: TodaysBriefProps) {
  const [brief, execR, stuck, stuckSales, salesTrend, anomalies, locale] = await Promise.all([
    loadExecutiveBrief(actor),
    getExecutiveReport(actor.id),
    loadStuck(),
    loadStuckSales(),
    loadSalesSparkline(),
    loadAnomalies(),
    getSecondaryLocale(),
  ]);

  const audience: 'ceo' | 'cfo' =
    actor.effective_level != null && actor.effective_level <= 1 ? 'ceo' : 'cfo';

  const execReport = execR.success ? execR.report ?? null : null;

  return (
    <section className="glass-panel rounded-3xl border-purple-500/30 bg-gradient-to-br from-purple-950/30 via-slate-950 to-indigo-950/30 p-6 sm:p-8 shadow-2xl">
      <header className="flex flex-col md:flex-row md:items-center md:justify-between gap-4 mb-6">
        <div className="flex items-center gap-3">
          <span className="text-3xl">👑</span>
          <div>
            <div className="text-xs font-mono font-black uppercase text-purple-300 tracking-wider">
              <T id="cockpit.todaysBriefHeading" values={{ audience: audience.toUpperCase() }} locale={locale} />
            </div>
            <h2 className="text-xl sm:text-2xl md:text-3xl font-black text-white tracking-tight">
              <T id="cockpit.todaysBriefGreeting" values={{ name: actor.fullname }} locale={locale} />
            </h2>
          </div>
        </div>
        <div className="flex items-center gap-3 flex-shrink-0">
          <span className="text-xs font-mono text-slate-400">
            <T id="cockpit.todaysBriefLastUpdated" locale={locale} />{' '}
            {timeOfDay(new Date(brief.generatedAt))}
          </span>
          <Link
            href="/cockpit"
            className="inline-flex items-center gap-1.5 rounded-xl border border-purple-500/40 bg-purple-500/20 px-3 py-1.5 text-sm font-bold text-purple-200 hover:bg-purple-500/30 transition-all"
          >
            <span>↻</span>
            <span>
              <T id="cockpit.todaysBriefRefresh" locale={locale} />
            </span>
          </Link>
        </div>
      </header>

      <div className="mb-6">
        <ExecutiveNarrative execReport={execReport} audience={audience} />
      </div>

      <div className="mb-6">
        <CockpitAiNarrative actorId={actor.id} />
      </div>

      <div className="grid grid-cols-1 sm:grid-cols-2 lg:grid-cols-3 gap-4 2xl:gap-6 mb-6">
        <div className="rounded-2xl border border-indigo-500/30 bg-indigo-950/20 p-4">
          <div className="text-xs font-mono uppercase tracking-wider text-indigo-300 mb-1">
            <T id="cockpit.kpiCashPosition" locale={locale} />
          </div>
          <div className="text-xl sm:text-2xl lg:text-3xl font-black text-white font-mono">
            {formatTHB(brief.kpis.totalCash)}
          </div>
          <div className="text-xs text-slate-500 font-mono">
            <T id="cockpit.kpiCashSub" locale={locale} />
          </div>
        </div>
        <div className="rounded-2xl border border-amber-500/30 bg-amber-950/20 p-4">
          <div className="text-xs font-mono uppercase tracking-wider text-amber-300 mb-1">
            <T id="cockpit.kpiUnpaidLiabilities" locale={locale} />
          </div>
          <div className="text-xl sm:text-2xl lg:text-3xl font-black text-white font-mono">
            {formatTHB(brief.kpis.outstandingLiabilities)}
          </div>
          <div className="text-xs text-slate-500 font-mono">
            <T id="cockpit.kpiUnpaidSub" locale={locale} />
          </div>
        </div>
        <div className="rounded-2xl border border-purple-500/30 bg-purple-950/20 p-4">
          <div className="text-xs font-mono uppercase tracking-wider text-purple-300 mb-1">
            <T id="cockpit.kpiMtdExpenses" locale={locale} />
          </div>
          <div className="text-xl sm:text-2xl lg:text-3xl font-black text-white font-mono">
            {formatTHB(brief.kpis.mtdExpenses)}
          </div>
          <div className="text-xs text-slate-500 font-mono">
            <T id="cockpit.kpiMtdExpensesSub" locale={locale} />
          </div>
        </div>
        <div className="rounded-2xl border border-emerald-500/30 bg-emerald-950/20 p-4">
          <div className="text-xs font-mono uppercase tracking-wider text-emerald-300 mb-1">
            <T id="cockpit.kpiNetIncome" locale={locale} />
          </div>
          <div
            className={`text-xl sm:text-2xl lg:text-3xl font-black font-mono ${brief.kpis.netIncome >= 0 ? 'text-emerald-400' : 'text-rose-400'}`}
          >
            {formatTHB(brief.kpis.netIncome)}
          </div>
          <div className="text-xs text-slate-500 font-mono">
            <T id="cockpit.kpiNetIncomeSub" locale={locale} />
          </div>
        </div>
        <div className="rounded-2xl border border-fuchsia-500/30 bg-fuchsia-950/20 p-4">
          <div className="text-xs font-mono uppercase tracking-wider text-fuchsia-300 mb-1">
            <T id="cockpit.kpiMtdSalesRevenue" locale={locale} />
          </div>
          <div className="text-xl sm:text-2xl lg:text-3xl font-black text-white font-mono">
            {formatTHB(brief.kpis.salesRevenueMtd ?? 0)}
          </div>
          <div className="text-xs text-slate-500 font-mono">
            <T id="cockpit.kpiMtdSalesRevenueSub" locale={locale} />
          </div>
        </div>
        <div className="rounded-2xl border border-cyan-500/30 bg-cyan-950/20 p-4">
          <div className="text-xs font-mono uppercase tracking-wider text-cyan-300 mb-1">
            <T id="cockpit.kpiOpenSalesOrders" locale={locale} />
          </div>
          <div className="text-xl sm:text-2xl lg:text-3xl font-black text-white font-mono">
            {brief.kpis.openSalesOrders ?? 0}
          </div>
          <div className="text-xs text-slate-500 font-mono">
            <T id="cockpit.kpiOpenSalesOrdersSub" locale={locale} />
          </div>
        </div>
      </div>

      <div className="mb-6 grid grid-cols-1 lg:grid-cols-2 gap-4">
        <div className="rounded-2xl border border-amber-500/30 bg-amber-950/20 p-4">
          <div className="flex items-center justify-between mb-2">
            <div className="text-xs font-mono uppercase tracking-wider text-amber-300">
              <T id="cockpit.arAgingHeading" locale={locale} />
            </div>
            <div className="text-xs font-mono text-slate-500">
              {brief.arAging?.length ?? 0}{' '}
              <T id="cockpit.bucketCount" locale={locale} />
            </div>
          </div>
          <div className="space-y-1.5 mt-2">
            {(brief.arAging ?? []).length === 0 && (
              <p className="text-xs font-mono text-slate-500">
                <T id="cockpit.arNoOverdue" locale={locale} />
              </p>
            )}
            {(brief.arAging ?? []).map((b: { bucket: string; amount_thb: number; so_count: number }) => {
              const tone =
                b.bucket === '0-30'
                  ? 'text-emerald-300'
                  : b.bucket === '31-60'
                  ? 'text-cyan-300'
                  : b.bucket === '61-90'
                  ? 'text-amber-300'
                  : 'text-rose-300';
              return (
                <div
                  key={b.bucket}
                  className="flex items-center justify-between text-xs font-mono"
                >
                  <span className="text-slate-400">
                    {b.bucket}{' '}
                    <T id="cockpit.daysUnit" locale={locale} />
                  </span>
                  <span className="flex items-center gap-2">
                    <span className="text-slate-500">
                      {b.so_count ?? 0} <T id="cockpit.sosUnit" locale={locale} />
                    </span>
                    <span className={`font-bold tabular-nums ${tone}`}>
                      {formatTHB(b.amount_thb)}
                    </span>
                  </span>
                </div>
              );
            })}
          </div>
          <div className="mt-2 pt-2 border-t border-amber-900/30 flex items-center justify-between text-xs font-mono">
            <span className="text-rose-300">
              <T id="cockpit.totalOverdue" locale={locale} />
            </span>
            <span className="text-rose-200 font-bold tabular-nums">
              {formatTHB(brief.kpis.overdueArAmount ?? 0)}
            </span>
          </div>
        </div>
        <div className="rounded-2xl border border-fuchsia-500/30 bg-fuchsia-950/20 p-4">
          <div className="flex items-center justify-between mb-2">
            <div className="text-xs font-mono uppercase tracking-wider text-fuchsia-300">
              <T id="cockpit.salesRolling7Day" locale={locale} />
            </div>
            <div className="text-xs font-mono text-slate-500">
              {salesTrend.length}{' '}
              <T id={salesTrend.length === 1 ? 'cockpit.sparklineDay' : 'cockpit.sparklineDays'} locale={locale} />
            </div>
          </div>
          {salesTrend.length === 0 ? (
            <p className="text-xs font-mono text-slate-500">
              <T id="cockpit.noRevenue" locale={locale} />
            </p>
          ) : (
            <div className="flex items-end gap-1 h-20">
              {salesTrend.map((d: { day: string; total: number }, i: number) => {
                const max = Math.max(1, ...salesTrend.map((x: { total: number }) => x.total));
                const h = Math.max(4, Math.round((d.total / max) * 80));
                return (
                  <div key={i} className="flex-1 flex flex-col items-center gap-1">
                    <span className="text-xs font-mono text-slate-400 tabular-nums">
                      {(d.total / 1000).toFixed(0)}k
                    </span>
                    <div
                      className="w-full rounded-t bg-gradient-to-b from-fuchsia-400 to-cyan-500/60"
                      style={{ height: `${h}px` }}
                      aria-hidden
                      title={`${d.day} · ${formatTHB(d.total)} THB`}
                    />
                    <span className="text-xs font-mono text-slate-500">
                      {d.day.slice(5)}
                    </span>
                  </div>
                );
              })}
            </div>
          )}
        </div>
      </div>

      <div className="mb-6">
        <ProjectionCard />
      </div>

      <div className="mb-6">
        <div className="text-xs font-mono font-bold uppercase text-slate-400 tracking-wider mb-2">
          <T id="cockpit.trend7Day" locale={locale} />
        </div>
        <SparklineTrends cash={brief.kpis.cashTrend} mtd={brief.kpis.mtdTrend} />
      </div>

      <div className="mb-6">
        <AiSummaryCharts actor={actor} brief={brief} stuck={stuck} locale={locale} />
      </div>

      <div className="grid grid-cols-1 lg:grid-cols-3 gap-6 mb-6">
        <div className="lg:col-span-2 rounded-2xl border border-slate-800 bg-slate-950/40 p-4">
          <div className="flex items-center justify-between mb-3">
            <div className="text-xs font-mono font-bold uppercase text-slate-400 tracking-wider">
              <T id="cockpit.deptBudgetBurn" locale={locale} />
            </div>
            <div className="text-xs font-mono text-slate-500">
              {brief.deptBudgets.filter((d: { is_over_threshold: boolean }) => d.is_over_threshold).length}{' '}
              <T id="cockpit.over90" locale={locale} />
            </div>
          </div>
          <DeptBudgetStrip />
        </div>
        <div className="min-w-0 rounded-2xl border border-slate-800 bg-slate-950/40 p-4">
          <div className="flex items-center justify-between mb-3">
            <div className="text-xs font-mono font-bold uppercase text-rose-300 tracking-wider">
              <T id="cockpit.stuckAnomalies" locale={locale} />
            </div>
            <div className="text-xs font-mono text-slate-500">
              {brief.stuckCount}{' '}
              <T id="cockpit.stuckOver24h" locale={locale} />
            </div>
          </div>
          <div className="space-y-2">
            {stuck.length === 0 ? (
              <p className="text-xs font-mono text-slate-500">
                <T id="cockpit.noStuck" locale={locale} />
              </p>
            ) : (
              stuck.map((s: StuckRow) => (
                <Link
                  key={s.waybill_id}
                  href={`/waybill/${s.waybill_id}`}
                  className="block min-w-0 rounded-xl border border-amber-500/30 bg-amber-950/20 px-3 py-2 hover:bg-amber-950/40 transition-all"
                >
                  <div className="flex items-center justify-between text-sm font-mono gap-2 min-w-0">
                    <span className="text-amber-200 font-bold truncate min-w-0">{s.waybill_id}</span>
                    <span className="text-amber-300 flex-shrink-0 whitespace-nowrap">{formatTHB(Number(s.total_amount))} THB</span>
                  </div>
                  <div className="flex items-center justify-between text-xs text-slate-400 mt-0.5 gap-2 min-w-0">
                    <span className="truncate min-w-0">{s.vendor_name}</span>
                    <span className="flex-shrink-0 whitespace-nowrap">{Number(s.age_hours).toFixed(0)}{' '}
                      <T id="cockpit.hoursOld" locale={locale} />
                    </span>
                  </div>
                </Link>
              ))
            )}
            {anomalies.length > 0 && (
              <div className="pt-2 mt-2 border-t border-slate-900 space-y-1.5">
                {anomalies.map((a: AnomalyRow) => (
                  <div
                    key={a.id}
                    className="rounded-xl border border-slate-800 bg-slate-950/60 px-3 py-2 text-sm text-slate-300"
                  >
                    <span className="font-mono text-xs text-slate-500 mr-1">#{a.id}</span>
                    {a.message}
                  </div>
                ))}
              </div>
            )}
            {stuckSales.length > 0 && (
              <div className="pt-2 mt-2 border-t border-slate-900 space-y-1.5">
                <div className="text-xs font-mono font-bold uppercase tracking-wider text-fuchsia-300">
                  🛒 <T id="hr.cockpit.stuckSales" locale={locale} />
                </div>
                {stuckSales.map((s: StuckSalesRow) => (
                  <Link
                    key={s.waybill_id}
                    href={`/waybill/${s.waybill_id}`}
                    className="block min-w-0 rounded-xl border border-fuchsia-500/30 bg-fuchsia-950/20 px-3 py-2 hover:bg-fuchsia-950/40 transition-all"
                  >
                    <div className="flex items-center justify-between text-sm font-mono gap-2 min-w-0">
                      <span className="text-fuchsia-200 font-bold truncate min-w-0">
                        {s.waybill_id}
                      </span>
                      <span className="text-fuchsia-300 flex-shrink-0 whitespace-nowrap">
                        {formatTHB(Number(s.total_amount))} THB
                      </span>
                    </div>
                    <div className="flex items-center justify-between text-xs text-slate-400 mt-0.5 gap-2 min-w-0">
                      <span className="truncate min-w-0">
                        {s.customer_code ? `${s.customer_code} · ` : ''}
                        {s.customer_name ?? '—'}
                      </span>
                      <span className="flex-shrink-0 whitespace-nowrap">
                        {Number(s.age_hours).toFixed(0)}h · {s.current_stage}
                      </span>
                    </div>
                  </Link>
                ))}
              </div>
            )}
          </div>
        </div>
      </div>

      <TodaysBriefPinnedCharts />

      <section className="mt-6 grid grid-cols-1 gap-4 lg:grid-cols-2">
        <CockpitSqlPanel />
        <FinanceRagPanel />
        <div className="lg:col-span-2">
          <GlCommentaryCard />
        </div>
      </section>
    </section>
  );
}

export default TodaysBrief;