import 'server-only';
import { computeRiskScore } from '@folio-lib/waybill/risk';

export async function WaybillRiskBadge({ waybillId }: { waybillId: string }) {
  const score = await computeRiskScore(waybillId);
  const color =
    score >= 80 ? 'border-rose-500/50 bg-rose-500/15 text-rose-200' :
    score >= 50 ? 'border-orange-500/50 bg-orange-500/15 text-orange-200' :
    score >= 20 ? 'border-amber-500/50 bg-amber-500/15 text-amber-200' :
                  'border-slate-700 bg-slate-800 text-slate-400';

  return (
    <span
      title={`Composite risk score (vendor frequency, amount vs. avg, OCR confidence, prior rejections)`}
      className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-mono ${color}`}
    >
      <span aria-hidden>⚠</span>
      <span>Risk {score}</span>
    </span>
  );
}
