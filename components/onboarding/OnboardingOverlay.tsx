'use client';

import React, { useEffect, useState } from 'react';
import { useActor } from '@/components/ActorProvider';
import { T } from '@/components/i18n/T';
import { Button } from '@/components/ui/Button';
import { Modal } from '@/components/ui/Modal';

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
  expense: 'border-caution bg-caution-soft text-caution-strong border border-caution',
  ledger:  'border-accent bg-accent text-paper',
  cockpit: 'border-accent bg-accent text-paper',
  policy:  'border-info bg-info text-paper',
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
    <Modal
      open={open}
      onClose={dismiss}
      title={<T id="onboarding.title" hideSecondary />}
      subtitle={<T id="onboarding.subtitle" hideSecondary />}
      tone="indigo"
      width="xl"
      hideCloseButton
      closeOnBackdrop={false}
      closeOnEsc={false}
      footer={
        <div className="flex w-full flex-wrap items-center justify-between gap-2">
          <Button variant="secondary" size="sm" onClick={back} disabled={step === 0}>
            <T id="common.back" hideSecondary />
          </Button>
          <Button variant="ghost" size="sm" onClick={dismiss}>
            <T id="onboarding.skip" hideSecondary />
          </Button>
          <Button variant="primary" size="sm" onClick={isLast ? dismiss : next}>
            {isLast ? <T id="onboarding.done" hideSecondary /> : <T id="onboarding.next" hideSecondary />}
          </Button>
        </div>
      }
    >
      <div className="space-y-5">
        <div className="flex items-center justify-end">
          <div className="flex items-center gap-1">
            {[0, 1, 2, 3].map((i) => (
              <span
                key={i}
                className={`h-1.5 w-8 rounded-full ${i === step ? 'bg-accent' : 'bg-paper-3'}`}
              />
            ))}
          </div>
        </div>

        {step === 0 && (
          <div className="space-y-3">
            <h3 className="text-base font-bold text-accent-soft"><T id="onboarding.s1.heading" hideSecondary /></h3>
            <p className="text-xs text-ink-2 leading-relaxed">
              <T id="onboarding.s1.body" hideSecondary />
            </p>
            <div className="grid grid-cols-2 gap-2">
              {focusedTiles.map((tt) => (
                <div
                  key={tt.nameId}
                  className={`rounded-md border ${tt.accent} px-3 py-2.5`}
                >
                  <div className="flex items-center gap-2">
                    <span className="text-lg">{tt.glyph}</span>
                    <span className="text-xs font-bold"><T id={tt.nameId} hideSecondary /></span>
                  </div>
                  <p className="text-xs text-ink-2 mt-1 leading-relaxed"><T id={tt.hintId} hideSecondary /></p>
                </div>
              ))}
            </div>
            <p className="text-sm text-accent-soft italic"><T id={rcId} hideSecondary /></p>
          </div>
        )}

        {step === 1 && (
          <div className="space-y-3">
            <h3 className="text-base font-bold text-accent-soft"><T id="onboarding.s2.heading" hideSecondary /></h3>
            <p className="text-xs text-ink-2 leading-relaxed">
              <T id="onboarding.s2.body" hideSecondary />
            </p>
            <div className="rounded-md border border-rule bg-paper-2/60 p-3 space-y-2">
              <div className="flex items-center justify-between">
                <div className="flex items-center gap-2">
                  <span className="text-base">🔔</span>
                  <span className="text-sm font-bold text-ink"><T id="onboarding.s2.waiting" hideSecondary /></span>
                </div>
                <span className="text-xs font-mono text-critical"><T id="onboarding.s2.urgent" hideSecondary /></span>
              </div>
              <div className="border-t border-rule pt-2 space-y-1.5">
                <div className="flex items-center justify-between text-sm">
                  <span className="text-ink">EXP-2026-0142 · Coffee Club ฿1,240</span>
                  <span className="text-caution font-mono"><T id="onboarding.s2.awaiting" hideSecondary /></span>
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-ink">PR-2026-0089 · Office supplies ฿4,800</span>
                  <span className="text-caution font-mono"><T id="onboarding.s2.awaiting" hideSecondary /></span>
                </div>
                <div className="flex items-center justify-between text-sm">
                  <span className="text-ink">Daily digest · 5 events</span>
                  <span className="text-positive font-mono"><T id="onboarding.s2.reviewed" hideSecondary /></span>
                </div>
              </div>
            </div>
            <p className="text-xs text-mute leading-relaxed">
              <T id="onboarding.s2.tip" hideSecondary />
            </p>
          </div>
        )}

        {step === 2 && (
          <div className="space-y-3">
            <h3 className="text-base font-bold text-accent-soft"><T id="onboarding.s3.heading" hideSecondary /></h3>
            <p className="text-xs text-ink-2 leading-relaxed">
              <T id="onboarding.s3.body" hideSecondary />
            </p>
            <div className="rounded-md border border-dashed border-positive bg-positive p-4 text-center">
              <div className="text-2xl mb-1">📸</div>
              <p className="text-sm text-positive-soft font-mono"><T id="onboarding.s3.dropHere" hideSecondary /></p>
              <p className="text-xs text-ink-2 mt-1"><T id="onboarding.s3.flow" hideSecondary /></p>
            </div>
            <div className="rounded-lg border border-rule bg-paper-2/50 p-2.5 text-sm text-ink-2 leading-relaxed">
              <span className="text-positive font-mono"><T id="onboarding.s3.ocr" hideSecondary />:</span>{' '}
              <span className="text-ink">Coffee Club</span> ·{' '}
              <span className="text-ink">12 Jul 2026</span> ·{' '}
              <span className="text-ink">฿1,240</span> ·{' '}
              <span className="text-ink">Meals & Entertainment</span>
            </div>
          </div>
        )}

        {step === 3 && (
          <div className="space-y-3">
            <h3 className="text-base font-bold text-accent-soft"><T id="onboarding.s4.heading" hideSecondary /></h3>
            <p className="text-xs text-ink-2 leading-relaxed">
              <T id="onboarding.s4.body" hideSecondary />
            </p>
            <div className="rounded-md border border-accent bg-paper-2/70 p-3">
              <div className="flex items-center gap-2 border border-rule rounded-lg px-3 py-2 bg-paper-2/60">
                <span className="text-mute">🔍</span>
                <span className="text-xs font-mono text-ink">cockpit</span>
                <span className="ml-auto h-3 w-1 bg-accent animate-pulse" />
              </div>
              <div className="mt-2 space-y-1">
                <div className="rounded-md bg-accent border border-accent px-2.5 py-1.5 flex items-center justify-between">
                  <span className="text-sm text-accent-soft"><T id="onboarding.s4.goCockpit" hideSecondary /></span>
                  <span className="text-xs font-mono text-accent">↵</span>
                </div>
                <div className="rounded-md px-2.5 py-1.5 flex items-center justify-between">
                  <span className="text-sm text-ink-2"><T id="onboarding.s4.cockpitAi" hideSecondary /></span>
                  <span className="text-xs font-mono text-mute">⌘2</span>
                </div>
              </div>
            </div>
            <p className="text-xs text-mute leading-relaxed">
              <T id="onboarding.s4.tip" hideSecondary />
            </p>
          </div>
        )}

      </div>
    </Modal>
  );
};

export default OnboardingOverlay;
