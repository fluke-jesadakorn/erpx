'use client';

import { useState } from 'react';
import { T } from '@/components/i18n/T';

interface CoaSuggestion {
  code: string;
  name: string | null;
  name_th: string | null;
  similarity: number;
}

interface VendorSuggestion {
  id: number;
  code: string;
  name: string;
}

interface HelperHint {
  description: string;
  descriptionTh: string | null;
  coaSuggestions: CoaSuggestion[];
  vendorSuggestion: VendorSuggestion | null;
  amountHypothesis: number | null;
}

interface Props {
  currentUserId: number;
  lang?: 'en' | 'th' | 'de';
  onUse?: (hint: HelperHint) => void;
}

export function StaffSubmitHelper({ currentUserId, lang = 'en', onUse }: Props) {
  const [text, setText] = useState('');
  const [busy, setBusy] = useState(false);
  const [hint, setHint] = useState<HelperHint | null>(null);
  const [error, setError] = useState<string | null>(null);

  const onAsk = async () => {
    if (!text.trim() || busy) return;
    setBusy(true);
    setError(null);
    setHint(null);
    try {
      const res = await fetch('/api/expense/helper', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ text, submitterId: currentUserId, lang }),
      });
      const json = await res.json();
      if (!res.ok || !json.ok) {
        setError(json.error ?? `HTTP ${res.status}`);
      } else if (json.hint) {
        setHint(json.hint);
      } else {
        setError('AI returned no hint.');
      }
    } catch (e: any) {
      setError(e?.message ?? String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="rounded-md border border-info bg-info-soft p-4">
      <h3 className="mb-1 text-xs font-mono uppercase tracking-widest text-info-strong">
        <T id="waybill.staffHelper.title" />
      </h3>
      <p className="mb-2 text-xs text-ink-2">
        <T id="waybill.staffHelper.subtitle" />
      </p>
      <textarea
        value={text}
        onChange={(e) => setText(e.target.value)}
        rows={2}
        placeholder="e.g. กาแฟตอนประชุมลูกค้า ABC ที่ร้านกาแฟ"
        className="w-full rounded-md border border-rule bg-paper-2 px-3 py-2 text-sm text-ink placeholder:text-mute focus:border-info focus:outline-none"
      />
      <div className="mt-2 flex gap-2">
        <button
          type="button"
          onClick={onAsk}
          disabled={busy || !text.trim()}
          className="rounded-md border border-info bg-info px-3 py-1.5 text-xs font-mono uppercase tracking-wider text-paper hover:bg-info-strong disabled:opacity-50 disabled:pointer-events-none"
        >
          {busy ? <T id="waybill.review.thinking" /> : <T id="waybill.staffHelper.suggest" />}
        </button>
        {hint && onUse && (
          <button
            type="button"
            onClick={() => onUse(hint)}
            className="rounded-md border border-positive bg-positive px-3 py-1.5 text-xs font-mono uppercase tracking-wider text-paper hover:bg-positive-strong"
          >
            <T id="waybill.staffHelper.apply" />
          </button>
        )}
      </div>
      {error && <div className="mt-3 rounded-md border border-critical bg-critical-soft px-3 py-2 text-xs text-critical-strong">{error}</div>}
      {hint && (
        <div className="mt-3 space-y-2 text-xs">
          <div>
            <div className="font-mono text-mute">
              <T id="waybill.staffHelper.suggestedDesc" />
            </div>
            <div className="text-ink">{hint.description}</div>
            {hint.descriptionTh && <div className="text-ink-2">{hint.descriptionTh}</div>}
          </div>
          {hint.amountHypothesis != null && (
            <div>
              <span className="font-mono text-mute">
                <T id="waybill.staffHelper.estimatedAmount" />
              </span>{' '}
              <span className="font-mono text-positive">
                {hint.amountHypothesis.toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB
              </span>
            </div>
          )}
          {hint.vendorSuggestion && (
            <div>
              <span className="font-mono text-mute">
                <T id="waybill.staffHelper.vendor" />
              </span>{' '}
              <span className="font-mono text-info">{hint.vendorSuggestion.code}</span>{' '}
              <span className="text-ink-2">{hint.vendorSuggestion.name}</span>
            </div>
          )}
          {Array.isArray(hint.coaSuggestions) && hint.coaSuggestions.length > 0 && (
            <div>
              <div className="mb-1 font-mono text-mute">
                <T id="waybill.staffHelper.coaCandidates" />
              </div>
              <ul className="space-y-0.5">
                {hint.coaSuggestions.slice(0, 3).map((s, i) => (
                  <li key={i} className="flex items-center gap-2 rounded border border-rule bg-paper px-2 py-1">
                    <span className="font-mono text-info">{s.code}</span>
                    <span className="text-ink-2">{s.name ?? '?'}</span>
                    {s.name_th && <span className="text-mute">({s.name_th})</span>}
                    <span className="ml-auto font-mono text-positive">{(s.similarity ?? 0).toFixed(0)}%</span>
                  </li>
                ))}
              </ul>
            </div>
          )}
        </div>
      )}
    </section>
  );
}

export default StaffSubmitHelper;
