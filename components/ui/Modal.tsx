import React, { useEffect, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { T } from '@/components/i18n/T';

export type ModalTone = 'indigo' | 'rose' | 'amber' | 'emerald' | 'cyan' | 'purple' | 'slate';

export interface ModalProps {
  open: boolean;
  onClose: () => void;
  title?: React.ReactNode;
  subtitle?: React.ReactNode;
  /** Custom header content that fully replaces the title+subtitle row. */
  header?: React.ReactNode;
  tone?: ModalTone;
  width?: 'sm' | 'md' | 'lg' | 'xl' | '2xl';
  hideCloseButton?: boolean;
  closeOnBackdrop?: boolean;
  closeOnEsc?: boolean;
  initialFocusRef?: React.RefObject<HTMLElement | null>;
  children?: React.ReactNode;
  footer?: React.ReactNode;
  /** Hide the header band entirely (when `header` is null). */
  bareHeader?: boolean;
}

const TONE_RING: Record<ModalTone, string> = {
  indigo:  'border-indigo-500/30',
  rose:    'border-rose-500/40',
  amber:   'border-amber-500/40',
  emerald: 'border-emerald-500/40',
  cyan:    'border-cyan-500/40',
  purple:  'border-purple-500/30',
  slate:   'border-slate-800',
};

const TONE_GLYPH: Record<ModalTone, string> = {
  indigo: '🪟', rose: '⚡', amber: '⚠️', emerald: '✓', cyan: '🧊', purple: '✨', slate: '•',
};

const WIDTH: Record<NonNullable<ModalProps['width']>, string> = {
  sm:   'max-w-sm',
  md:   'max-w-md',
  lg:   'max-w-lg',
  xl:   'max-w-xl',
  '2xl':'max-w-2xl',
};

export const Modal: React.FC<ModalProps> = ({
  open,
  onClose,
  title,
  subtitle,
  header,
  tone = 'slate',
  width = 'md',
  hideCloseButton,
  closeOnBackdrop = true,
  closeOnEsc = true,
  initialFocusRef,
  children,
  footer,
  bareHeader,
}) => {
  const cardRef = useRef<HTMLDivElement | null>(null);
  const [mounted, setMounted] = useState(false);

  useEffect(() => {
    setMounted(true);
    return () => setMounted(false);
  }, []);

  // ESC to close
  useEffect(() => {
    if (!open || !closeOnEsc) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.stopPropagation();
        onClose();
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [open, closeOnEsc, onClose]);

  // Body scroll lock + initial focus
  useEffect(() => {
    if (!open) return;
    const prevOverflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
    const t = setTimeout(() => {
      const target = initialFocusRef?.current || cardRef.current?.querySelector<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])'
      );
      target?.focus();
    }, 30);
    return () => {
      document.body.style.overflow = prevOverflow;
      clearTimeout(t);
    };
  }, [open, initialFocusRef]);

  if (!open) return null;
  if (!mounted) return null;

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      className="fixed inset-0 z-[100] flex items-center justify-center p-4 animate-fade-in"
    >
      {/* Backdrop */}
      <div
        className="absolute inset-0 bg-slate-950/80 backdrop-blur-sm"
        onClick={closeOnBackdrop ? onClose : undefined}
        aria-hidden
      />
      {/* Card */}
      <div
        ref={cardRef}
        className={`relative w-full ${WIDTH[width]} glass-panel rounded-3xl border ${TONE_RING[tone]} overflow-hidden animate-fade-in`}
      >
        {/* Header */}
        {(title || subtitle || header || !hideCloseButton) && !bareHeader && (
          <header className="flex items-start justify-between gap-3 p-5 border-b border-slate-800/80">
            <div className="min-w-0 flex-1">
              {header ?? (
                <>
                  {title && (
                    <h3 className="text-base font-bold text-white flex items-center gap-2">
                      <span aria-hidden>{TONE_GLYPH[tone]}</span>
                      <span className="truncate">{title}</span>
                    </h3>
                  )}
                  {subtitle && (
                    <p className="text-sm text-slate-400 mt-1 font-sans">{subtitle}</p>
                  )}
                </>
              )}
            </div>
            {!hideCloseButton && (
              <button
                type="button"
                onClick={onClose}
                aria-label="Close"
                className="w-7 h-7 inline-flex items-center justify-center rounded-lg text-slate-400 hover:text-white hover:bg-slate-800 text-sm shrink-0 transition-colors"
              >
                <svg className="w-4 h-4" fill="none" viewBox="0 0 24 24" stroke="currentColor" strokeWidth={2} aria-hidden>
                  <path strokeLinecap="round" strokeLinejoin="round" d="M6 18L18 6M6 6l12 12" />
                </svg>
                <span className="sr-only"><T id="common.close" hideSecondary /></span>
              </button>
            )}
          </header>
        )}
        {/* Body */}
        <div className="p-5 max-h-[70vh] overflow-y-auto">{children}</div>
        {/* Footer */}
        {footer && (
          <footer className="flex items-center justify-end gap-2 p-4 border-t border-slate-800/80 bg-slate-950/40">
            {footer}
          </footer>
        )}
      </div>
    </div>,
    document.body
  );
};
