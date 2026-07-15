'use client';

import { LineChart, Line, ResponsiveContainer, YAxis, Tooltip } from 'recharts';
import { T } from '@/components/i18n/T';

export interface SparklineTrendsProps {
  cash: number[];
  mtd: number[];
}

interface Point {
  idx: number;
  cash: number;
  mtd: number;
}

export function SparklineTrends({ cash, mtd }: SparklineTrendsProps) {
  const maxLen = Math.max(cash.length, mtd.length);
  const data: Point[] = Array.from({ length: maxLen }, (_, i) => ({
    idx: i,
    cash: cash[i] ?? 0,
    mtd: mtd[i] ?? 0,
  }));

  if (data.length === 0) {
    return (
      <div className="rounded-2xl border border-slate-800 bg-slate-950/40 p-3">
        <p className="text-xs font-mono text-slate-500">
          <T
            value={{
              en: 'No snapshots yet — run bun run snapshot-exec to seed.',
              th: 'ยังไม่มีสแนปชอต — รัน bun run snapshot-exec เพื่อสร้างข้อมูล',
              de: 'Noch keine Snapshots — führe bun run snapshot-exec aus, um Daten zu erzeugen.',
            }}
          />
        </p>
      </div>
    );
  }

  const lastCash = cash[cash.length - 1] ?? 0;
  const firstCash = cash[0] ?? 0;
  const cashDelta = lastCash - firstCash;

  return (
    <div className="rounded-2xl border border-slate-800 bg-slate-950/40 p-3">
      <div className="flex items-center justify-between mb-2">
        <div className="text-xs font-mono text-slate-400">
          {data.length}{' '}
          <T value={data.length === 1 ? { en: 'day', th: 'วัน', de: 'Tag' } : { en: 'days', th: 'วัน', de: 'Tage' }} />{' '}
          · last {data.length - 1} delta
        </div>
        <div
          className={`text-sm font-mono font-bold ${cashDelta >= 0 ? 'text-emerald-400' : 'text-rose-400'}`}
        >
          {cashDelta >= 0 ? '↑' : '↓'} {Math.abs(cashDelta).toLocaleString()} THB
        </div>
      </div>
      <div className="h-[60px] sm:h-[72px] w-full">
        <ResponsiveContainer width="100%" height="100%">
          <LineChart data={data} margin={{ top: 4, right: 8, left: 0, bottom: 0 }}>
            <YAxis hide domain={['dataMin', 'dataMax']} />
            <Tooltip
              contentStyle={{
                background: '#0f172a',
                border: '1px solid #334155',
                borderRadius: 8,
                fontSize: 11,
              }}
              formatter={((value: unknown, name: unknown) => [
                `${Number(value).toLocaleString()} THB`,
                name === 'cash'
                  ? <T value={{ en: 'Cash', th: 'เงินสด', de: 'Bargeld' }} />
                  : <T value={{ en: 'MTD Exp', th: 'ค่าใช้จ่าย MTD', de: 'MTD Ausgaben' }} />,
              ]) as any}
              labelFormatter={() => <T value={{ en: 'Day', th: 'วัน', de: 'Tag' }} />}
            />
            <Line
              type="monotone"
              dataKey="cash"
              stroke="#818cf8"
              strokeWidth={2}
              dot={false}
              isAnimationActive={false}
            />
            <Line
              type="monotone"
              dataKey="mtd"
              stroke="#f472b6"
              strokeWidth={2}
              dot={false}
              isAnimationActive={false}
            />
          </LineChart>
        </ResponsiveContainer>
      </div>
      <div className="flex items-center gap-4 mt-2 text-xs font-mono text-slate-400">
        <span className="flex items-center gap-1.5">
          <span className="inline-block w-3 h-0.5 bg-indigo-400" />
          <T value={{ en: 'Cash', th: 'เงินสด', de: 'Bargeld' }} />
        </span>
        <span className="flex items-center gap-1.5">
          <span className="inline-block w-3 h-0.5 bg-pink-400" />
          <T value={{ en: 'MTD Expenses', th: 'ค่าใช้จ่าย MTD', de: 'MTD-Ausgaben' }} />
        </span>
      </div>
    </div>
  );
}

export default SparklineTrends;
