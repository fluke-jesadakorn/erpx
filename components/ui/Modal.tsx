'use client';
import React, { useEffect, useId, useRef, useState } from 'react';
import { createPortal } from 'react-dom';
import { Check, CircleAlert, Info, Sparkles, TriangleAlert, X, type LucideIcon } from 'lucide-react';
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
  ariaLabel?: string;
  panelClassName?: string;
  contentClassName?: string;
  children?: React.ReactNode;
  footer?: React.ReactNode;
  /** Hide the header band entirely (when `header` is null). */
  bareHeader?: boolean;
}

const TONE_RING: Record<ModalTone, string> = {
  indigo:  'border-accent/40',
  rose:    'border-critical/40',
  amber:   'border-caution/40',
  emerald: 'border-positive/40',
  cyan:    'border-info/40',
  purple:  'border-accent/40',
  slate:   'border-rule',
};

const TONE_ICON: Record<ModalTone, LucideIcon> = {
  indigo: Sparkles,
  rose: CircleAlert,
  amber: TriangleAlert,
  emerald: Check,
  cyan: Info,
  purple: Sparkles,
  slate: Info,
};

const TONE_TEXT: Record<ModalTone, string> = {
  indigo: 'text-accent',
  rose: 'text-critical',
  amber: 'text-caution',
  emerald: 'text-positive',
  cyan: 'text-info',
  purple: 'text-accent',
  slate: 'text-mute',
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
  ariaLabel,
  panelClassName = '',
  contentClassName = '',
  children,
  footer,
  bareHeader,
}) => {
  const cardRef = useRef<HTMLDivElement | null>(null);
  const previousRef = useRef<HTMLElement | null>(null);
  const [mounted, setMounted] = useState(false);
  const titleId = useId();
  const ToneIcon = TONE_ICON[tone];

  useEffect(() => {
    setMounted(true);
    return () => setMounted(false);
  }, []);

  useEffect(() => {
    if (!open) return;
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape' && closeOnEsc) {
        e.stopPropagation();
        onClose();
      }
      if (e.key !== 'Tab' || !cardRef.current) return;
      const items = Array.from(cardRef.current.querySelectorAll<HTMLElement>(
        'button:not([disabled]), [href], input:not([disabled]), textarea:not([disabled]), select:not([disabled]), [tabindex]:not([tabindex="-1"])'
      )).filter((item) => item.offsetParent !== null);
      if (items.length === 0) {
        e.preventDefault();
        cardRef.current.focus();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      if (e.shiftKey && document.activeElement === first) {
        e.preventDefault();
        last.focus();
      } else if (!e.shiftKey && document.activeElement === last) {
        e.preventDefault();
        first.focus();
      }
    };
    window.addEventListener('keydown', onKey, true);
    return () => window.removeEventListener('keydown', onKey, true);
  }, [open, closeOnEsc, onClose]);

  useEffect(() => {
    if (!open) return;
    previousRef.current = document.activeElement as HTMLElement | null;
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
      previousRef.current?.focus();
    };
  }, [open, initialFocusRef]);

  if (!open) return null;
  if (!mounted) return null;

  return createPortal(
    <div
      role="dialog"
      aria-modal="true"
      aria-labelledby={title ? titleId : undefined}
      aria-label={title ? undefined : ariaLabel ?? 'Dialog'}
      className="fixed inset-0 z-modal flex items-center justify-center p-4 animate-fade-in sm:p-6"
    >
      <div
        className="absolute inset-0 bg-canvas/70 backdrop-blur-md"
        onClick={closeOnBackdrop ? onClose : undefined}
        aria-hidden
      />
      <div
        ref={cardRef}
        tabIndex={-1}
        className={`panel-floating relative w-full ${WIDTH[width]} ${TONE_RING[tone]} overflow-hidden animate-fade-scale ${panelClassName}`}
      >
        {(title || subtitle || header || !hideCloseButton) && !bareHeader && (
          <header className="flex items-start justify-between gap-3 border-b border-rule/80 px-5 py-4">
            <div className="min-w-0 flex-1">
              {header ?? (
                <>
                  {title && (
                    <h3 id={titleId} className="flex items-center gap-2 text-base font-semibold tracking-tight text-ink">
                      <ToneIcon size={17} className={TONE_TEXT[tone]} aria-hidden />
                      <span className="truncate">{title}</span>
                    </h3>
                  )}
                  {subtitle && (
                    <p className="text-sm text-ink-2 mt-1 font-sans">{subtitle}</p>
                  )}
                </>
              )}
            </div>
            {!hideCloseButton && (
              <button
                type="button"
                onClick={onClose}
                aria-label="Close"
                className="inline-flex h-8 w-8 shrink-0 items-center justify-center rounded-lg border border-transparent text-mute transition-colors hover:border-rule hover:bg-paper-3/50 hover:text-ink"
              >
                <X size={16} aria-hidden />
                <span className="sr-only"><T id="common.close" hideSecondary /></span>
              </button>
            )}
          </header>
        )}
        <div className={`max-h-[72vh] overflow-y-auto p-5 ${contentClassName}`}>{children}</div>
        {footer && (
          <footer className="flex items-center justify-end gap-2 border-t border-rule/80 bg-paper-2/35 p-4">
            {footer}
          </footer>
        )}
      </div>
    </div>,
    document.body
  );
};
