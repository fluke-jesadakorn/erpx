'use client';

import React, { useEffect, useRef, useState } from 'react';
import {
  SECONDARIES,
  STORAGE_KEY,
  LANG_EVENT,
  LOCALE_META,
  type SecondaryLocale,
} from '@/i18n/config';

export const LANG_STORAGE_KEY = STORAGE_KEY;
export { LANG_EVENT };

export function useLang(): SecondaryLocale {
  const [lang, setLangState] = useState<SecondaryLocale>('th');
  useEffect(() => {
    const stored = (typeof window !== 'undefined' ? localStorage.getItem(STORAGE_KEY) : null) as SecondaryLocale | null;
    if (stored && (SECONDARIES as readonly string[]).includes(stored)) setLangState(stored);
    const handler = (e: Event) => {
      const d = (e as CustomEvent<SecondaryLocale>).detail;
      if (d && (SECONDARIES as readonly string[]).includes(d)) setLangState(d);
    };
    window.addEventListener(LANG_EVENT, handler);
    const onStorage = (e: StorageEvent) => {
      if (e.key !== STORAGE_KEY) return;
      const v = e.newValue;
      if (v && (SECONDARIES as readonly string[]).includes(v)) setLangState(v as SecondaryLocale);
    };
    window.addEventListener('storage', onStorage);
    return () => {
      window.removeEventListener(LANG_EVENT, handler);
      window.removeEventListener('storage', onStorage);
    };
  }, []);
  return lang;
}

export function setLang(lang: SecondaryLocale): void {
  if (typeof window === 'undefined') return;
  localStorage.setItem(STORAGE_KEY, lang);
  document.documentElement.lang = lang;
  window.dispatchEvent(new CustomEvent(LANG_EVENT, { detail: lang }));
}

export function toggleLang(): SecondaryLocale {
  const cur = (typeof window !== 'undefined'
    ? localStorage.getItem(STORAGE_KEY)
    : null) as SecondaryLocale | null;
  const idx = cur && (SECONDARIES as readonly string[]).includes(cur) ? SECONDARIES.indexOf(cur) : 0;
  const next = SECONDARIES[(idx + 1) % SECONDARIES.length];
  setLang(next);
  return next;
}

interface ListProps {
  current: SecondaryLocale;
  onPick: (lang: SecondaryLocale) => void;
  className?: string;
}

function LangList({ current, onPick, className }: ListProps) {
  return (
    <div
      className={[
        'glass-panel-heavy rounded-xl border border-glass-border-strong shadow-xl shadow-black/40 overflow-hidden',
        className ?? '',
      ].join(' ')}
    >
      <ul role="listbox" className="py-1">
        {SECONDARIES.map((loc) => {
          const meta = LOCALE_META[loc];
          const selected = loc === current;
          return (
            <li key={loc}>
              <button
                type="button"
                role="option"
                aria-selected={selected}
                lang={meta.bcp47}
                onClick={() => onPick(loc)}
                className={[
                  'w-full flex items-center gap-2.5 px-3 py-2 text-left transition-colors',
                  selected
                    ? 'bg-accent/15 text-ink'
                    : 'text-ink-2 hover:bg-surface-glass-strong hover:text-ink',
                ].join(' ')}
              >
                <span
                  aria-hidden
                  className="text-lg leading-none shrink-0"
                  style={{ fontFamily: '"Apple Color Emoji","Segoe UI Emoji","Noto Color Emoji",sans-serif' }}
                >
                  {meta.flag}
                </span>
                <span className="flex-1 text-sm font-medium truncate">
                  {meta.native}
                </span>
                <span className="text-[10px] font-mono uppercase tracking-wider text-mute">
                  {meta.code}
                </span>
                {selected ? (
                  <span aria-hidden className="text-accent text-xs">✓</span>
                ) : null}
              </button>
            </li>
          );
        })}
      </ul>
    </div>
  );
}

interface PickerProps {
  open: boolean;
  onPick: (lang: SecondaryLocale) => void;
  onClose?: () => void;
}

export function LangPicker({ open, onPick, onClose }: PickerProps) {
  const ref = useRef<HTMLDivElement>(null);
  const current = useLang();

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') onClose?.(); };
    const onClick = (e: MouseEvent) => {
      if (ref.current && !ref.current.contains(e.target as Node)) onClose?.();
    };
    window.addEventListener('keydown', onKey);
    window.addEventListener('mousedown', onClick);
    return () => {
      window.removeEventListener('keydown', onKey);
      window.removeEventListener('mousedown', onClick);
    };
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div
      ref={ref}
      role="dialog"
      aria-modal="true"
      aria-label="Pick language"
      className="w-[18rem] max-w-[calc(100vw-2rem)] animate-fade-in"
    >
      <LangList current={current} onPick={(l) => { onPick(l); onClose?.(); }} />
    </div>
  );
}

interface TriggerProps {
  className?: string;
}

export function LangPickerTrigger({ className }: TriggerProps) {
  const lang = useLang();
  const [open, setOpen] = useState(false);
  const wrapRef = useRef<HTMLDivElement>(null);

  const meta = LOCALE_META[lang];

  useEffect(() => {
    if (!open) return;
    const onClick = (e: MouseEvent) => {
      if (wrapRef.current && !wrapRef.current.contains(e.target as Node)) setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => { if (e.key === 'Escape') setOpen(false); };
    window.addEventListener('mousedown', onClick);
    window.addEventListener('keydown', onKey);
    return () => {
      window.removeEventListener('mousedown', onClick);
      window.removeEventListener('keydown', onKey);
    };
  }, [open]);

  return (
    <div ref={wrapRef} className="relative">
      <button
        type="button"
        onClick={() => setOpen((v) => !v)}
        title={`Language: ${meta.english}`}
        aria-label={`Language: ${meta.english}`}
        aria-haspopup="dialog"
        aria-expanded={open}
        className={[
          'inline-flex h-9 items-center gap-1.5 rounded-lg border px-2 transition-colors',
          'font-mono uppercase tracking-wider text-ink-2',
          open
            ? 'border-accent bg-accent/10 text-ink ring-2 ring-accent/25'
            : 'border-glass-border bg-surface-glass-heavy hover:border-glass-border-strong hover:bg-surface-glass-strong hover:text-ink',
          className ?? '',
        ].join(' ')}
      >
        <span
          aria-hidden
          className="text-base leading-none"
          style={{ fontFamily: '"Apple Color Emoji","Segoe UI Emoji","Noto Color Emoji",sans-serif' }}
        >
          {meta.flag}
        </span>
        <span className="text-[11px] font-bold">{meta.code}</span>
        <svg
          aria-hidden
          viewBox="0 0 20 20"
          fill="currentColor"
          className={['h-3 w-3 transition-transform', open ? 'rotate-180' : ''].join(' ')}
        >
          <path
            fillRule="evenodd"
            d="M5.23 7.21a.75.75 0 011.06.02L10 11.06l3.71-3.83a.75.75 0 111.08 1.04l-4.25 4.39a.75.75 0 01-1.08 0L5.21 8.27a.75.75 0 01.02-1.06z"
            clipRule="evenodd"
          />
        </svg>
      </button>

      {open ? (
        <div className="absolute right-0 top-full mt-2 z-[400]">
          <LangPicker
            open={open}
            onPick={setLang}
            onClose={() => setOpen(false)}
          />
        </div>
      ) : null}
    </div>
  );
}
