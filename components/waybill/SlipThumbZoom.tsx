'use client';

import { useEffect, useState } from 'react';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { T } from '@/components/i18n/T';

interface Props {
  href: string;
  alt: string;
  title?: string;
  subtitle?: string;
  className?: string;
  imgClassName?: string;
}

export function SlipThumbZoom({ href, alt, title, subtitle, className, imgClassName }: Props) {
  useSecondaryLocale();
  const [open, setOpen] = useState(false);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') setOpen(false);
    };
    window.addEventListener('keydown', onKey);
    const prev = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    return () => {
      window.removeEventListener('keydown', onKey);
      document.body.style.overflow = prev;
    };
  }, [open]);

  return (
    <>
      <button
        type="button"
        onClick={(e) => {
          e.preventDefault();
          e.stopPropagation();
          setOpen(true);
        }}
        title={title ?? 'Click to enlarge · คลิกเพื่อขยาย'}
        aria-label={title ?? 'Enlarge image'}
        className={
          'relative group shrink-0 overflow-hidden rounded-lg border border-slate-700/70 bg-slate-950/60 hover:border-emerald-500/50 transition-colors cursor-zoom-in ' +
          (className ?? 'h-16 w-12')
        }
      >
        <img src={href} alt={alt} className={'h-full w-full object-cover ' + (imgClassName ?? '')} />
        <span
          aria-hidden
          className="absolute bottom-1 right-1 grid place-items-center w-5 h-5 rounded-full bg-slate-950/85 ring-1 ring-slate-700/80 text-xs text-slate-100 opacity-0 group-hover:opacity-100 transition-opacity"
        >
          🔍
        </span>
      </button>
      {open && (
        <div
          role="dialog"
          aria-modal="true"
          aria-label={title ?? alt}
          onClick={() => setOpen(false)}
          className="fixed inset-0 z-[200] flex items-center justify-center p-4 bg-slate-950/85 backdrop-blur-sm animate-fade-in"
        >
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              setOpen(false);
            }}
            aria-label={<T id="waybill.slip.close" /> as unknown as string}
            className="absolute top-4 right-4 w-9 h-9 inline-flex items-center justify-center rounded-lg bg-slate-900/80 ring-1 ring-slate-700 text-slate-200 hover:text-white hover:bg-slate-800 text-sm"
          >
            ✕
          </button>
          <div
            onClick={(e) => e.stopPropagation()}
            className="relative w-full max-w-4xl"
          >
            {title && (
              <div className="mb-2 flex items-baseline justify-between gap-3 px-1">
                <h3 className="text-sm font-bold text-white truncate">{title}</h3>
                {subtitle && <span className="text-sm font-mono text-slate-400 shrink-0">{subtitle}</span>}
              </div>
            )}
            <div className="flex items-center justify-center bg-slate-950 rounded-2xl border border-slate-800 p-2 shadow-2xl shadow-black">
              <img
                src={href}
                alt={alt}
                className="max-h-[80vh] w-auto max-w-full object-contain rounded-lg"
              />
            </div>
          </div>
        </div>
      )}
    </>
  );
}
