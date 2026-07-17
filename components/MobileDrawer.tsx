'use client';

import React, { useEffect, useRef, type ReactNode } from 'react';
import { Icon } from '@/components/icons';

interface MobileDrawerProps {
  open: boolean;
  onClose: () => void;
  children: ReactNode;
}

const FOCUSABLE = [
  'a[href]',
  'button:not([disabled])',
  'input:not([disabled])',
  'select:not([disabled])',
  'textarea:not([disabled])',
  '[tabindex]:not([tabindex="-1"])',
].join(', ');

export function MobileDrawer({ open, onClose, children }: MobileDrawerProps) {
  const ref = useRef<HTMLDivElement>(null);
  const previous = useRef<HTMLElement | null>(null);

  useEffect(() => {
    if (!open) return;
    previous.current = document.activeElement as HTMLElement | null;
    const frame = window.requestAnimationFrame(() => {
      const first = ref.current?.querySelector<HTMLElement>(FOCUSABLE);
      first?.focus();
      if (!first) ref.current?.focus();
    });

    const onKey = (event: KeyboardEvent) => {
      if (event.key === 'Escape') {
        event.preventDefault();
        onClose();
        return;
      }
      if (event.key !== 'Tab') return;
      const items = Array.from(ref.current?.querySelectorAll<HTMLElement>(FOCUSABLE) ?? []);
      if (items.length === 0) {
        event.preventDefault();
        ref.current?.focus();
        return;
      }
      const first = items[0];
      const last = items[items.length - 1];
      if (event.shiftKey && document.activeElement === first) {
        event.preventDefault();
        last.focus();
      } else if (!event.shiftKey && document.activeElement === last) {
        event.preventDefault();
        first.focus();
      }
    };

    window.addEventListener('keydown', onKey);
    return () => {
      window.cancelAnimationFrame(frame);
      window.removeEventListener('keydown', onKey);
      previous.current?.focus();
    };
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-50">
      <div className="absolute inset-0 bg-black/40" onClick={onClose} aria-hidden />
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label="Navigation"
        tabIndex={-1}
        onClick={(event) => event.stopPropagation()}
        className="absolute inset-y-0 left-0 z-50 flex w-72 max-w-[80vw] flex-col border-r border-rule bg-paper-2 shadow-modal animate-slide-in-left"
      >
        {children}
      </div>
    </div>
  );
}

export function MobileMenuButton({ className = '' }: { className?: string }) {
  return (
    <button
      type="button"
      aria-label="Open navigation"
      onClick={() => window.dispatchEvent(new Event('folio:open-sidebar'))}
      className={[
        'inline-flex h-9 w-9 items-center justify-center rounded-md text-ink-2 hover:bg-paper-3 hover:text-ink',
        className,
      ].join(' ')}
    >
      <Icon name="menu" size={20} />
    </button>
  );
}

export function SearchButton({ className = '' }: { className?: string }) {
  return (
    <button
      type="button"
      aria-label="Search"
      onClick={() => window.dispatchEvent(new Event('folio:open-command'))}
      className={[
        'inline-flex h-9 w-9 items-center justify-center rounded-md border border-rule bg-paper-2 text-ink-2 hover:border-rule-strong hover:bg-paper-3 hover:text-ink',
        className,
      ].join(' ')}
    >
      <Icon name="search" size={16} />
    </button>
  );
}

export default MobileDrawer;
