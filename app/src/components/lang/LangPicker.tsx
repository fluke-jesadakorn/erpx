'use client';

import React, { useEffect, useState } from 'react';

export const STORAGE_KEY = 'folio.lang';
export const LANG_EVENT = 'folio:lang';
export type Lang = 'de' | 'th';

export function useLang(): Lang {
  const [lang, setLangState] = useState<Lang>('th');
  useEffect(() => {
    const stored = (typeof window !== 'undefined' ? localStorage.getItem(STORAGE_KEY) : null) as Lang | null;
    if (stored === 'de' || stored === 'th') setLangState(stored);
    else setLangState('th');
    const handler = (e: Event) => {
      const d = (e as CustomEvent<Lang>).detail;
      if (d === 'de' || d === 'th') setLangState(d);
    };
    window.addEventListener(LANG_EVENT, handler);
    const onStorage = (e: StorageEvent) => {
      if (e.key !== STORAGE_KEY) return;
      const v = e.newValue;
      if (v === 'de' || v === 'th') setLangState(v);
    };
    window.addEventListener('storage', onStorage);
    return () => {
      window.removeEventListener(LANG_EVENT, handler);
      window.removeEventListener('storage', onStorage);
    };
  }, []);
  return lang;
}

export function setLang(lang: Lang): void {
  if (typeof window === 'undefined') return;
  localStorage.setItem(STORAGE_KEY, lang);
  document.documentElement.lang = lang;
  window.dispatchEvent(new CustomEvent(LANG_EVENT, { detail: lang }));
}

export function toggleLang(): Lang {
  const cur = (typeof window !== 'undefined'
    ? localStorage.getItem(STORAGE_KEY)
    : null) as Lang | null;
  const next: Lang = cur === 'de' ? 'th' : 'de';
  setLang(next);
  return next;
}

interface Props {
  open: boolean;
  onPick: (lang: Lang) => void;
}

export function LangPicker({ open, onPick }: Props) {
  if (!open) return null;
  return (
    <div role="dialog" aria-modal="true" aria-label="Pick language"
      className="fixed inset-0 z-[400] flex items-center justify-center bg-ink/40 animate-fade-in">
      <div className="glass-panel rounded-sm p-5 w-[calc(100vw-2rem)] max-w-sm">
        <h2 className="font-display text-lg text-ink">Pick language</h2>
        <p className="text-xs text-mute font-sans mb-4 mt-1">เลือกภาษาที่ต้องการใช้ · Sprache wählen</p>
        <div className="grid grid-cols-2 gap-2">
          <button type="button" onClick={() => onPick('th')}
            className="glass-panel flex flex-col items-center gap-1 rounded-sm p-4 hover:border-accent text-ink">
            <span className="font-display text-2xl">TH</span>
            <span className="text-sm font-medium">ไทย</span>
          </button>
          <button type="button" onClick={() => onPick('de')}
            className="glass-panel flex flex-col items-center gap-1 rounded-sm p-4 hover:border-accent text-ink">
            <span className="font-display text-2xl">DE</span>
            <span className="text-sm font-medium">Deutsch</span>
          </button>
        </div>
      </div>
    </div>
  );
}

interface TriggerProps {
  className?: string;
}

export function LangPickerTrigger({ className }: TriggerProps) {
  const lang = useLang();

  const swap = () => {
    toggleLang();
  };

  return (
    <button type="button" onClick={swap} title="Toggle language (TH ↔ DE)" aria-label="Toggle language"
      className={['inline-flex h-9 min-w-11 items-center justify-center rounded-lg border border-glass-border bg-surface-glass-heavy px-2.5 text-sm font-mono uppercase tracking-wider text-ink-2 transition-colors hover:border-glass-border-strong hover:bg-surface-glass-strong hover:text-ink',
        className ?? '',
      ].join(' ')}>
      {lang === 'th' ? 'TH' : 'DE'}
    </button>
  );
}