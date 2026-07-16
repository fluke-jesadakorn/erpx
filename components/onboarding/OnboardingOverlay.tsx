'use client';

import React, { useEffect, useState } from 'react';
import { useActor } from '@/components/ActorProvider';
import { T } from '@/components/i18n/T';

const STORAGE_KEY = 'onboarded_v1';

interface TileChip {
  glyph: string;
  nameId: string;
  hintId: string;
  accent: string;
}

const ROLE_TILE_FOCUS: Record<string, string[]> = {
  staff:        ['expense', 'policy', 'ledger', 'cockpit'],
  account_officer: ['ledger', 'expense', 'policy', 'cockpit'],
  account_supervisor: ['ledger', 'expense', 'cockpit', 'policy'],
  accounting_manager: ['ledger', 'expense', 'policy', 'cockpit'],
  finance:      ['ledger', 'cockpit', 'expense', 'policy'],
  cfo:          ['cockpit', 'ledger', 'expense', 'policy'],
  ceo:          ['cockpit', 'policy', 'ledger', 'expense'],
  hr_manager:   ['policy', 'expense', 'cockpit', 'ledger'],
  admin:        ['cockpit', 'ledger', 'policy', 'expense'],
};

const ACCENTS: Record<string, string> = {
  expense: 'border-amber-500/30 bg-amber-500/5 text-amber-200',
  ledger:  'border-indigo-500/30 bg-indigo-500/5 text-indigo-200',
  cockpit: 'border-purple-500/30 bg-purple-500/5 text-purple-200',
  policy:  'border-cyan-500/30 bg-cyan-500/5 text-cyan-200',
};

const TILE_CHIPS: TileChip[] = [
  { glyph: '💰', nameId: 'onboarding.tile.expense.name', hintId: 'onboarding.tile.expense.hint', accent: ACCENTS.expense },
  { glyph: '📒', nameId: 'onboarding.tile.ledger.name',  hintId: 'onboarding.tile.ledger.hint',  accent: ACCENTS.ledger },
  { glyph: '👑', nameId: 'onboarding.tile.cockpit.name', hintId: 'onboarding.tile.cockpit.hint', accent: ACCENTS.cockpit },
  { glyph: '⚙️', nameId: 'onboarding.tile.policy.name',  hintId: 'onboarding.tile.policy.hint',  accent: ACCENTS.policy },
];

const KNOWN_ROLES = ['staff', 'account_officer', 'account_supervisor', 'accounting_manager', 'finance', 'cfo', 'ceo', 'hr_manager', 'admin'] as const;

function roleCopyId(role: string): string {
  return KNOWN_ROLES.includes(role as any)
    ? `onboarding.role.${role}`
    : 'onboarding.role.staff';
}

function pickTileFocus(order: string[]): TileChip[] {
  return order
    .map((id) => TILE_CHIPS.find((c) => c.nameId === `onboarding.tile.${id}.name`) ?? null)
    .filter((c): c is TileChip => c != null);
}

export const OnboardingOverlay: React.FC = () => {
  const { actor } = useActor();
  const role = actor?.role_name ?? '';
  const [open, setOpen] = useState(false);
  const [step, setStep] = useState(0);

  useEffect(() => {
    try {
      if (typeof window === 'undefined') return;
      if (window.localStorage.getItem(STORAGE_KEY)) return;
    } catch {
      return;
    }
    setOpen(true);
  }, []);

  if (!open) return null;

  const dismiss = () => {
    try { window.localStorage.setItem(STORAGE_KEY, '1'); } catch {}
    setOpen(false);
  };

  const next = () => setStep((s) => Math.min(3, s + 1));
  const back = () => setStep((s) => Math.max(0, s - 1));
  const isLast = step === 3;
  const rcId = roleCopyId(role);
  const focusedTiles = pickTileFocus(ROLE_TILE_FOCUS[role] ?? ROLE_TILE_FOCUS.staff);

  return (
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-[200] bg-slate-950/80 backdrop-blur-sm overflow-y-auto"
    >
      <div className="glass-panel-heavy rounded-2xl border border-purple-500/30 p-6 max-w-xl mx-auto mt-[10vh] shadow-2xl shadow-purple-900/40">
        <div className="flex items-center justify-between mb-4">
          <div className="flex items-center gap-2">
            <span className="text-2xl">✨</span>
            <div>
              <h2 className="text-lg font-bold text-white"><T id="onboarding.title" hideSecondary /></h2>
              <p className="text-sm text-slate-400"><T id="onboarding.subtitle" hideSecondary /></p>
            </div>
          </div>
          <div className="flex items-center gap-1">
            {[0, 1, 2, 3].map((i) => (
              <span
                key={i}
                className={`h-1.5 w-6 rounded-full ${i === step ? 'bg-purple-400' : 'bg-slate-700'}`}
              />
            ))}
          </div>
        </div>

        {step === 0 && (
          <div className="space-y-3">
            <h3 className="text-base font-bold text-purple-200"><T id="onboarding.s1.heading" hideSecondary /></h3>
            <p className="text-xs text-slate-300 leading-relaxed">
              <T id="onboarding.s1.body" hideSecondary />
            </p>
            <div className="grid grid-cols-2 gap-2">
              {focusedTiles.map((tt) => (
                <div
                  key={tt.nameId}
                  className={`rounded-xl border ${tt.accent} px-3 py-2.5`}
                >
                  <div className="flex items-center gap-2">
                    <span className="text-lg">{tt.glyph}</span>
                    <span className="text-xs font-bold"><T id={tt.nameId} hideSecondary /></span>
                  </div>
                  <p className="text-xs text-slate-400 mt-1 leading-relaxed"><T id={tt.hintId} hideSecondary /></p>
                </div>
              ))}
            </div>
            <p className="text-sm text-purple-200/80 italic"><T id={rcId} hideSecondary /></p>
          </div>
        )}

        {step === 1 && (
          <div className="space-y-3">
            <h3 className="text-base font-bold text-purple-200"><T id="onboarding.s2.heading" hideSecondary /></h3>
            <p className="text-xs text-slate-300 leading-relaxed">
              <T id="onboarding.s2.body" hideSecondary />
            </p>
            <div className="rounded-xl border border-slate-800 bg-slate-950/60 p-3 space-y-2">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <span className="text-base">🔔</span>
                  <span className="text-sm font-bold text-white"><T id="onboarding.s2.waiting" hideSecondary /></span>
                </div>
                <span className="text-xs font-mono text-rose-300"><T id="onboarding.s2.urgent" hideSecondary /></span>
              </div>
              <div className="border-t border-slate-800 pt-2 space-y-1.5">
                <div className="flex items-center justify-between text-sm">
                  <span className="text-slate-200">EXP-2026-0142 · Coffee Club ฿1,240</span>
                  <span className="text-amber-300 font-mono"><T id="onboarding.s2.awaiting" hideSecondary /></span>
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-slate-200">PR-2026-0089 · Office supplies ฿4,800</span>
                  <span className="text-amber-300 font-mono"><T id="onboarding.s2.awaiting" hideSecondary /></span>
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-slate-200">Daily digest · 5 events</span>
                  <span className="text-emerald-300 font-mono"><T id="onboarding.s2.reviewed" hideSecondary /></span>
                </div>
              </div>
            </div>
            <p className="text-xs text-slate-500 leading-relaxed">
              <T id="onboarding.s2.tip" hideSecondary />
            </p>
          </div>
        )}

        {step === 2 && (
          <div className="space-y-3">
            <h3 className="text-base font-bold text-purple-200"><T id="onboarding.s3.heading" hideSecondary /></h3>
            <p className="text-xs text-slate-300 leading-relaxed">
              <T id="onboarding.s3.body" hideSecondary />
            </p>
            <div className="rounded-xl border border-dashed border-emerald-500/40 bg-emerald-500/5 p-4 text-center">
              <div className="text-2xl mb-1">📸</div>
              <p className="text-sm text-emerald-200 font-mono"><T id="onboarding.s3.dropHere" hideSecondary /></p>
              <p className="text-xs text-slate-400 mt-1"><T id="onboarding.s3.flow" hideSecondary /></p>
            </div>
            <div className="rounded-lg border border-slate-800 bg-slate-950/40 p-2.5 text-sm text-slate-300 leading-relaxed">
              <span className="text-emerald-300 font-mono"><T id="onboarding.s3.ocr" hideSecondary />:</span>{' '}
              <span className="text-slate-100">Coffee Club</span> ·{' '}
              <span className="text-slate-100">12 Jul 2026</span> ·{' '}
              <span className="text-slate-100">฿1,240</span> ·{' '}
              <span className="text-slate-100">Meals & Entertainment</span>
            </div>
          </div>
        )}

        {step === 3 && (
          <div className="space-y-3">
            <h3 className="text-base font-bold text-purple-200"><T id="onboarding.s4.heading" hideSecondary /></h3>
            <p className="text-xs text-slate-300 leading-relaxed">
              <T id="onboarding.s4.body" hideSecondary />
            </p>
            <div className="rounded-xl border border-indigo-500/40 bg-slate-950/70 p-3">
              <div className="flex items-center gap-2 border border-slate-700 rounded-lg px-3 py-2 bg-slate-900/60">
                <span className="text-slate-500">🔍</span>
                <span className="text-xs font-mono text-white">cockpit</span>
                <span className="ml-auto h-3 w-1 bg-indigo-300 animate-pulse" />
              </div>
              <div className="mt-2 space-y-1">
                <div className="rounded-md bg-indigo-500/15 border border-indigo-500/30 px-2.5 py-1.5 flex items-center justify-between">
                  <span className="text-sm text-indigo-100"><T id="onboarding.s4.goCockpit" hideSecondary /></span>
                  <span className="text-xs font-mono text-indigo-300">↵</span>
                </div>
                <div className="rounded-md px-2.5 py-1.5 flex items-center justify-between">
                  <span className="text-sm text-slate-300"><T id="onboarding.s4.cockpitAi" hideSecondary /></span>
                  <span className="text-xs font-mono text-slate-500">⌘2</span>
                </div>
              </div>
            </div>
            <p className="text-xs text-slate-500 leading-relaxed">
              <T id="onboarding.s4.tip" hideSecondary />
            </p>
          </div>
        )}

        <footer className="mt-5 flex items-center justify-between border-t border-slate-800/80 pt-4">
          <button
            type="button"
            onClick={back}
            disabled={step === 0}
            className="text-sm px-3 py-1.5 rounded-lg border border-slate-700 text-slate-300 hover:bg-slate-800 disabled:opacity-30 disabled:cursor-not-allowed font-mono font-bold"
          >
            <T id="common.back" hideSecondary />
          </button>
          <button
            type="button"
            onClick={dismiss}
            className="text-sm px-3 py-1.5 rounded-lg border border-slate-700 text-slate-300 hover:bg-rose-900/40 hover:border-rose-500/40 hover:text-rose-200 font-mono font-bold"
          >
            <T id="onboarding.skip" hideSecondary /> ✕
          </button>
          {isLast ? (
            <button
              type="button"
              onClick={dismiss}
              className="text-sm px-3 py-1.5 rounded-lg bg-purple-500/20 border border-purple-500/50 text-purple-100 hover:bg-purple-500/30 font-mono font-bold"
            >
              <T id="onboarding.done" hideSecondary />
            </button>
          ) : (
            <button
              type="button"
              onClick={next}
              className="text-sm px-3 py-1.5 rounded-lg bg-indigo-500/20 border border-indigo-500/50 text-indigo-100 hover:bg-indigo-500/30 font-mono font-bold"
            >
              <T id="onboarding.next" hideSecondary />
            </button>
          )}
        </footer>
      </div>
    </div>
  );
};

export default OnboardingOverlay;
