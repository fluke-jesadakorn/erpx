import { query } from '@/db';
import { getSecondaryLocale } from '@/server/locale';
import { T } from '@/components/i18n/TServer';

export interface DeptBudgetRow {
  dept_id: string;
  dept_name: string;
  monthly_budget: number;
  mtd_spend: number;
  pct_used: number;
  is_over_threshold: boolean;
}

export interface DeptBudgetStripProps {
  fiscalYear?: number;
  month?: number;
  limit?: number;
}

function barClass(pct: number): string {
  if (pct > 100) return ' from-critical-strong to-critical-strong';
  if (pct > 90) return ' from-caution to-critical';
  return ' from-accent to-accent';
}

function currentYearMonth(): { year: number; month: number } {
  const now = new Date();
  return { year: now.getFullYear(), month: now.getMonth() + 1 };
}

async function fetchDeptBudgets(year: number, month: number, limit: number): Promise<DeptBudgetRow[]> {
  const r = await query<{
    dept_id: string;
    dept_name: string;
    monthly_budget: string;
    mtd_spend: string;
    pct_used: string;
    is_over_threshold: boolean;
  }>(`SELECT * FROM get_dept_budget_status($1::int, $2::int)`, [year, month]);
  return r.rows.slice(0, limit).map((row) => ({
    dept_id: row.dept_id,
    dept_name: row.dept_name,
    monthly_budget: Number(row.monthly_budget ?? 0),
    mtd_spend: Number(row.mtd_spend ?? 0),
    pct_used: Number(row.pct_used ?? 0),
    is_over_threshold: !!row.is_over_threshold,
  }));
}

function deptKeyFor(deptId: string): { key: string; fallback: string } {
  const family = deptId.replace(/^dept-/, '').split('-')[0];
  return { key: `departments.${family}`, fallback: deptId };
}

export async function DeptBudgetStrip({
  fiscalYear,
  month,
  limit = 12,
}: DeptBudgetStripProps) {
  const def = currentYearMonth();
  const year = fiscalYear ?? def.year;
  const mon = month ?? def.month;
   const rows = await fetchDeptBudgets(year, mon, limit);
   const locale = await getSecondaryLocale();

   if (rows.length === 0) {
    return (
      <div className="rounded-md border border-rule bg-paper-2/50 p-4">
        <p className="text-xs font-mono text-mute">
          <T id="cockpit.deptBudgetEmpty" locale={locale} />
        </p>
      </div>
    );
  }

  return (
    <div className="space-y-2.5">
      {rows.map((row) => {
        const pct = Math.min(Math.max(row.pct_used, 0), 100);
        const { key } = deptKeyFor(row.dept_id);
        return (
          <div key={row.dept_id} className="space-y-1">
            <div className="flex items-center justify-between gap-3 text-xs">
              <span className="font-bold text-ink">
                 <T id={key} locale={locale} />
                {row.is_over_threshold && <span className="ml-1.5">🚨</span>}
              </span>
              <span className="font-mono text-ink-2 flex-shrink-0 whitespace-nowrap ml-2">
                {row.mtd_spend.toLocaleString()} / {row.monthly_budget.toLocaleString()} THB
                <span className="ml-2 text-xs text-mute">({row.pct_used.toFixed(1)}%)</span>
              </span>
            </div>
            <div className="w-full h-2 rounded-full bg-paper border border-rule overflow-hidden">
              <div
                className={`h-full rounded-full ${barClass(row.pct_used)} transition-all duration-700`}
                style={{ width: `${pct}%` }}
              />
            </div>
          </div>
        );
      })}
    </div>
  );
}

export default DeptBudgetStrip;