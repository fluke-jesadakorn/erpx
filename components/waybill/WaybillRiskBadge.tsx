import 'server-only';
import { computeRiskScore } from '@/waybill/risk';
import { getSecondaryLocale } from '@/server/locale';
import { T } from '@/components/i18n/TServer';

export async function WaybillRiskBadge({ waybillId }: { waybillId: string }) {
  const locale = await getSecondaryLocale();
  const score = await computeRiskScore(waybillId);
  const color =
    score >= 80 ? 'border-critical bg-critical-soft text-critical-strong border border-critical' :
    score >= 50 ? 'border-caution bg-caution-soft text-caution-strong border border-caution' :
    score >= 20 ? 'border-caution bg-caution-soft text-caution-strong border border-caution' :
                  'border-rule bg-paper-2 text-ink-2';

  return (
    <span
      title="Composite risk score (vendor frequency, amount vs. avg, OCR confidence, prior rejections)"
      className={`inline-flex items-center gap-1 rounded-full border px-2 py-0.5 text-xs font-mono ${color}`}
    >
      <span aria-hidden>⚠</span>
      <span>
        <T id="waybill.risk.label" values={{ score }} locale={locale} />
      </span>
    </span>
  );
}
