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
          'relative group shrink-0 overflow-hidden rounded-lg border border-rule/70 bg-paper-2/60 hover:border-positive transition-colors cursor-zoom-in ' +
          (className ?? 'h-16 w-12')
        }
      >
        <img src={href} alt={alt} className={'h-full w-full object-cover ' + (imgClassName ?? '')} />
        <span
          aria-hidden
          className="absolute bottom-1 right-1 grid place-items-center w-5 h-5 rounded-full bg-paper-2/85 ring-1 ring-rule/80 text-xs text-ink opacity-0 group-hover:opacity-100 transition-opacity"
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
          className="fixed inset-0 z-toast flex items-center justify-center p-4 bg-paper-2/85 backdrop-blur-sm animate-fade-in"
        >
          <button
            type="button"
            onClick={(e) => {
              e.stopPropagation();
              setOpen(false);
            }}
            aria-label={<T id="waybill.slip.close" /> as unknown as string}
            className="absolute top-4 right-4 w-9 h-9 inline-flex items-center justify-center rounded-lg bg-paper-2/80 ring-1 ring-rule text-ink hover:text-ink hover:bg-paper-2 text-sm"
          >
            ✕
          </button>
          <div
            onClick={(e) => e.stopPropagation()}
            className="relative w-full max-w-4xl"
          >
            {title && (
              <div className="mb-2 flex items-baseline justify-between gap-3 px-1">
                <h3 className="text-sm font-bold text-ink truncate">{title}</h3>
                {subtitle && <span className="text-sm font-mono text-ink-2 shrink-0">{subtitle}</span>}
              </div>
            )}
            <div className="flex items-center justify-center bg-paper rounded-md border border-rule p-2 shadow-modal">
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
