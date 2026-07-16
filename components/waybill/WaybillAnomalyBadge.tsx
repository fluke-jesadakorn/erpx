import 'server-only';
import { getSecondaryLocale } from '@/server/locale';
import { T } from '@/components/i18n/TServer';

interface FlaggedReason {
  score?: number;
  flags?: Array<{ code: string; severity: string; message: string }>;
  generatedAt?: string;
}

interface Props {
  flagged: unknown;
}

export async function WaybillAnomalyBadge({ flagged }: Props) {
  const locale = await getSecondaryLocale();
  const f = typeof flagged === 'string' ? safeParse(flagged) : (flagged as FlaggedReason);
  if (!f || !Array.isArray(f.flags) || f.flags.length === 0) return null;

  const severity = f.flags.some(x => x.severity === 'error') ? 'error' : 'warning';
  const color = severity === 'error'
    ? 'border-rose-500/50 bg-rose-500/15 text-rose-200'
    : 'border-amber-500/50 bg-amber-500/15 text-amber-200';

  return (
    <details className={`inline-flex flex-col rounded-full border ${color}`}>
      <summary className="cursor-pointer list-none px-2 py-0.5 text-xs font-mono">
        <span aria-hidden>🚩</span>
        <span className="ml-1">
          <T id="waybill.anomaly.label" values={{ n: f.flags.length }} locale={locale} />
        </span>
      </summary>
      <div className="mt-1 rounded border border-slate-800 bg-slate-950 p-2 text-xs">
        <ul className="space-y-1">
          {f.flags.map((flag, i) => (
            <li key={i} className="flex items-start gap-2">
              <span className={`inline-flex shrink-0 rounded-full border px-1.5 py-0.5 text-[10px] font-mono uppercase tracking-wide ${
                flag.severity === 'error' ? 'border-rose-500/40 bg-rose-500/10 text-rose-300' :
                                              'border-amber-500/40 bg-amber-500/10 text-amber-300'
              }`}>{flag.severity}</span>
              <span className="font-mono text-slate-500">{flag.code}</span>
              <span className="text-slate-200">{flag.message}</span>
            </li>
          ))}
        </ul>
      </div>
    </details>
  );
}

function safeParse(s: string): FlaggedReason | null {
  try { return JSON.parse(s); } catch { return null; }
}
