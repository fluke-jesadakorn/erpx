'use client';

import React from 'react';
import { useFormatMoney } from '@/components/i18n/formatters';
import { T } from '@/components/i18n/T';

interface Props {
  actorCanSeeLines: boolean;
  totalDebit: number;
  totalCredit: number;
  balanced: boolean;
  lineCount: number;
  children: React.ReactNode;
}

export function GlVisibilityGate({
  actorCanSeeLines,
  totalDebit,
  totalCredit,
  balanced,
  lineCount,
  children,
}: Props) {
  const fmt = useFormatMoney();
  const [show, setShow] = React.useState(false);
  if (actorCanSeeLines) return <>{children}</>;

  return (
    <div className="space-y-2">
      <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-slate-800/60 bg-slate-950/40 p-2 font-mono text-sm">
        <div className="flex flex-wrap items-center gap-3">
          <span className="text-slate-500">
            <T id="waybill.gl.totalDebit" />
            <span className="ml-1 font-bold text-emerald-200">{fmt(totalDebit, '').trim()}</span>
          </span>
          <span className="text-slate-500">
            <T id="waybill.gl.totalCredit" />
            <span className="ml-1 font-bold text-amber-200">{fmt(totalCredit, '').trim()}</span>
          </span>
          <span className="text-slate-500">
            <T id="waybill.gl.glLinesN" />
            <span className="ml-1 font-bold text-cyan-300">{lineCount}</span>
          </span>
        </div>
        <div className="flex items-center gap-2">
          {balanced ? (
            <span className="inline-flex items-center gap-1 rounded-full border border-emerald-500/40 bg-emerald-500/15 px-2 py-0.5 text-emerald-200">
              <span aria-hidden>✓</span>
              <span>
                <T id="waybill.gl.balanced" />
              </span>
            </span>
          ) : (
            <span className="inline-flex items-center gap-1 rounded-full border border-rose-500/40 bg-rose-500/15 px-2 py-0.5 text-rose-200">
              <span aria-hidden>⚠</span>
              <span>
                <T id="waybill.gl.unbalanced" />
              </span>
            </span>
          )}
          <button
            type="button"
            onClick={() => setShow((v) => !v)}
            aria-expanded={show}
            className="rounded-md border border-slate-700 bg-slate-900/60 px-2 py-0.5 text-xs font-mono uppercase tracking-wider text-slate-300 hover:bg-slate-800"
          >
            <T id={show ? 'waybill.gl.hideLines' : 'waybill.gl.showLines'} />
          </button>
        </div>
      </div>
      {show && <>{children}</>}
    </div>
  );
}
