'use client';

import React, { useEffect, useState } from 'react';
import { Icon, type IconName } from '@/components/icons';
import { T } from '@/components/i18n/T';
import { getStoredTheme, resolveTheme, setStoredTheme, type ThemeMode } from '@/theme';

const OPTIONS: Array<{ mode: ThemeMode; icon: IconName; labelId: string; ariaLabelId: string }> = [
  { mode: 'light',  icon: 'sun',     labelId: 'theme.option.light',  ariaLabelId: 'theme.option.lightAria' },
  { mode: 'dark',   icon: 'moon',    labelId: 'theme.option.dark',   ariaLabelId: 'theme.option.darkAria' },
  { mode: 'system', icon: 'monitor', labelId: 'theme.option.system', ariaLabelId: 'theme.option.systemAria' },
];

export const ThemeToggle: React.FC<{ className?: string }> = ({ className }) => {
  const [mode, setMode] = useState<ThemeMode>('dark');
  const [mounted, setMounted] = useState(false);

  useEffect(() => { const m = getStoredTheme(); setMode(m); setMounted(true); }, []);

  useEffect(() => {
    if (!mounted) return;
    setStoredTheme(mode);
    document.documentElement.setAttribute('data-theme', resolveTheme(mode));
  }, [mode, mounted]);

  useEffect(() => {
    if (mode !== 'system') return;
    const mq = window.matchMedia('(prefers-color-scheme: dark)');
    const on = () => {
      document.documentElement.setAttribute('data-theme', mq.matches ? 'dark' : 'light');
    };
    on();
    mq.addEventListener('change', on);
    return () => mq.removeEventListener('change', on);
  }, [mode]);

  if (!mounted) {
    return (
      <div
        role="radiogroup" aria-label="Color theme"
        className={['flex items-center gap-0.5 rounded-lg border h-9 p-0.5 border-glass-border bg-surface-glass-heavy', className || ''].join(' ')}
      >

        {OPTIONS.map((o) => (
          <span key={o.mode} className="inline-flex items-center justify-center w-6 h-6 rounded-sm" aria-hidden>
            <Icon name={o.icon} size={14} />
          </span>
        ))}
      </div>
    );
  }

  return (
    <div role="radiogroup" aria-label="Color theme"
      className={['border border-glass-border bg-surface-glass-heavy flex items-center gap-0.5 rounded-lg h-9 p-0.5', className || ''].join(' ')}>
      {OPTIONS.map((o) => {
        const active = o.mode === mode;
        return (
          <button
            key={o.mode}
            type="button"
            role="radio"
            aria-checked={active}
            aria-label={o.ariaLabelId}
            title={o.labelId}
            onClick={() => setMode(o.mode)}
            className={[
              'inline-flex items-center justify-center w-6 h-6 rounded-md border-0 transition-all',
              active
                ? 'bg-accent/25 text-accent shadow-[inset_0_0_0_1px_rgba(132,179,147,0.45)]'
                : 'text-mute hover:text-ink-2 hover:bg-surface-glass-strong',
            ].join(' ')}
          >
            <Icon name={o.icon} size={14} />
          </button>
        );
      })}
    </div>
  );
};

export default ThemeToggle;

export const FIRST_VISIT_KEY = 'folio.theme.gateShown';

export function markThemeGateShown() {
  if (typeof window === 'undefined') return;
  try { window.localStorage.setItem(FIRST_VISIT_KEY, '1'); } catch {}
}
export function wasThemeGateShown(): boolean {
  if (typeof window === 'undefined') return false;
  try { return window.localStorage.getItem(FIRST_VISIT_KEY) === '1'; } catch { return false; }
}

export const ThemeGate: React.FC = () => {
  const [visible, setVisible] = useState(false);
  useEffect(() => {
    if (wasThemeGateShown()) return;
    if (typeof window === 'undefined') return;
    const t = setTimeout(() => setVisible(true), 800);
    return () => clearTimeout(t);
  }, []);
  const choose = (m: ThemeMode) => {
    setStoredTheme(m);
    document.documentElement.setAttribute('data-theme', resolveTheme(m));
    markThemeGateShown();
    setTimeout(() => setVisible(false), 300);
  };
  if (!visible) return null;
  return (
    <div role="dialog" aria-label="Choose a color theme"
      className="glass-panel fixed bottom-4 right-4 z-[300] max-w-xs w-[calc(100vw-2rem)] rounded-sm p-4 animate-fade-in">
      <div className="font-display text-base text-ink mb-1"><T id="theme.gateTitle" hideSecondary /></div>
      <p className="text-xs text-mute mb-3 leading-relaxed font-sans"><T id="theme.gateSubtitle" hideSecondary /></p>
      <div className="grid grid-cols-3 gap-1.5">
        {OPTIONS.map((o) => (
          <button key={o.mode} type="button" onClick={() => choose(o.mode)}
            className="flex flex-col items-center gap-1 py-2 rounded-sm border border-rule text-mute hover:text-ink-2 hover:border-accent transition-colors">
            <Icon name={o.icon} size={18} />
            <span className="text-sm font-mono uppercase tracking-wider"><T id={o.labelId} hideSecondary /></span>
          </button>
        ))}
      </div>
      <button type="button" onClick={() => { markThemeGateShown(); setVisible(false); }}
        className="mt-3 text-sm font-mono uppercase tracking-wider text-mute hover:text-ink-2">
        <T id="theme.gateSkip" hideSecondary />
      </button>
    </div>
  );
};
