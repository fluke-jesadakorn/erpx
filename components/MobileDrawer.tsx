'use client';

import React, { useEffect, useRef, type ReactNode } from 'react';
import { Menu, Search } from 'lucide-react';

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
    const overflow = document.body.style.overflow;
    document.body.style.overflow = 'hidden';
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
      document.body.style.overflow = overflow;
      previous.current?.focus();
    };
  }, [open, onClose]);

  if (!open) return null;

  return (
    <div className="fixed inset-0 z-modal">
      <div className="absolute inset-0 bg-canvas/70 backdrop-blur-sm" onClick={onClose} aria-hidden />
      <div
        ref={ref}
        role="dialog"
        aria-modal="true"
        aria-label="Navigation"
        tabIndex={-1}
        onClick={(event) => event.stopPropagation()}
        className="panel-floating absolute inset-y-3 left-3 z-modal flex w-72 max-w-[calc(100vw-1.5rem)] flex-col overflow-hidden rounded-2xl animate-slide-in-left"
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
        'inline-flex h-9 w-9 items-center justify-center rounded-lg text-ink-2 transition-colors hover:bg-paper-3/55 hover:text-ink',
        className,
      ].join(' ')}
    >
      <Menu size={20} />
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
        'glass-input inline-flex h-9 w-9 items-center justify-center text-ink-2 hover:border-rule-strong hover:text-ink',
        className,
      ].join(' ')}
    >
      <Search size={16} />
    </button>
  );
}

export default MobileDrawer;
