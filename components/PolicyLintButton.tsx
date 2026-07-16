'use client';

import { useState } from 'react';
import { T } from '@/components/i18n/T';
import { useTranslations } from 'next-intl';

interface PolicyRow {
  id: string;
  name: string;
}

interface Finding {
  code: string;
  severity: 'error' | 'warning' | 'info';
  message: string;
}

interface Props {
  policies: PolicyRow[];
}

export function PolicyLintButton({ policies }: Props) {
  const t = useTranslations();
  const [policyId, setPolicyId] = useState<string>('');
  const [busy, setBusy] = useState(false);
  const [findings, setFindings] = useState<Finding[]>([]);
  const [policyName, setPolicyName] = useState<string>('');
  const [error, setError] = useState<string | null>(null);

  const onLint = async () => {
    if (!policyId || busy) return;
    setBusy(true);
    setError(null);
    setFindings([]);
    try {
      const res = await fetch('/api/policy/lint', {
        method: 'POST',
        headers: { 'content-type': 'application/json' },
        body: JSON.stringify({ policyId }),
      });
      const json = await res.json();
      if (!res.ok || !json.ok) {
        setError(json.error ?? t('policy.lint.httpError', { status: res.status }));
      } else {
        setFindings(Array.isArray(json.lint?.findings) ? json.lint.findings : []);
        setPolicyName(json.lint?.policyName ?? '');
      }
    } catch (e: any) {
      setError(e?.message ?? String(e));
    } finally {
      setBusy(false);
    }
  };

  return (
    <section className="mt-4 rounded-2xl border border-amber-500/30 bg-amber-950/15 p-4">
      <h3 className="mb-1 text-xs font-mono uppercase tracking-widest text-amber-300"><T id="policy.lint.title" hideSecondary /></h3>
      <p className="mb-3 text-xs text-slate-500"><T id="policy.lint.subtitle" hideSecondary /></p>
      <div className="flex flex-wrap items-center gap-2">
        <select
          value={policyId}
          onChange={(e) => setPolicyId(e.target.value)}
          className="flex-1 rounded-lg border border-slate-700 bg-slate-950 px-3 py-2 text-sm font-mono text-white focus:border-amber-500 focus:outline-none"
        >
          <option value=""><T id="policy.lint.selectPlaceholder" hideSecondary /></option>
          {policies.map((p) => (
            <option key={p.id} value={p.id}>{p.name}</option>
          ))}
        </select>
        <button
          type="button"
          onClick={onLint}
          disabled={busy || !policyId}
          className="rounded-lg border border-amber-500/40 bg-amber-500/10 px-4 py-1.5 text-xs font-mono text-amber-200 hover:bg-amber-500/20 disabled:opacity-50"
        >
          {busy ? <T id="policy.lint.linting" hideSecondary /> : <T id="policy.lint.lint" hideSecondary />}
        </button>
      </div>
      {error && <div className="mt-3 rounded border border-rose-500/40 bg-rose-500/10 px-3 py-2 text-xs text-rose-200">{error}</div>}
      {findings.length > 0 && (
        <div className="mt-3 space-y-2">
          <div className="text-xs text-slate-400">
            <T id="policy.lint.findingsFor" hideSecondary values={{ name: policyName }} />:
          </div>
          <ul className="space-y-1.5">
            {findings.map((f, i) => (
              <li key={i} className="flex items-start gap-2 rounded border border-slate-800 bg-slate-950 px-3 py-2 text-xs">
                <span className={`mt-0.5 inline-flex shrink-0 rounded-full border px-1.5 py-0.5 text-[10px] font-mono uppercase tracking-wide ${
                  f.severity === 'error' ? 'border-rose-500/40 bg-rose-500/10 text-rose-300' :
                  f.severity === 'warning' ? 'border-amber-500/40 bg-amber-500/10 text-amber-300' :
                  'border-slate-500/40 bg-slate-500/10 text-slate-300'
                }`}>{f.severity}</span>
                <span className="font-mono text-slate-500">{f.code}</span>
                <span className="text-slate-200">{f.message}</span>
              </li>
            ))}
          </ul>
        </div>
      )}
      {!error && findings.length === 0 && policyName && (
        <div className="mt-3 rounded border border-emerald-500/40 bg-emerald-500/10 px-3 py-2 text-xs text-emerald-200"><T id="policy.lint.noIssues" hideSecondary /></div>
      )}
    </section>
  );
}
