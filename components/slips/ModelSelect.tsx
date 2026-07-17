'use client';

import React, { useEffect, useId, useRef, useState } from 'react';
import ReactDOM from 'react-dom';
import { Eye, ChevronDown, Search } from 'lucide-react';
import type { VisionModel } from '@/ai/loadVisionModels';
import { ModelCard } from './SlipCard';

export interface ModelSelectProps {
  models: VisionModel[];
  value: string;
  onChange: (name: string) => void;
  disabled?: boolean;
  testId?: string;
  buttonTestId?: string;
}

export function ModelSelect({
  models,
  value,
  onChange,
  disabled,
  testId,
  buttonTestId,
}: ModelSelectProps) {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState('');
  const triggerRef = useRef<HTMLButtonElement | null>(null);
  const panelRef = useRef<HTMLDivElement | null>(null);
  const [pos, setPos] = useState<{ left: number; top: number; width: number; placement: 'down' | 'up' } | null>(null);
  const listboxId = useId();

  useEffect(() => {
    if (!open) return;
    const onDown = (e: MouseEvent) => {
      const t = e.target as Node | null;
      if (!t) return;
      if (panelRef.current?.contains(t)) return;
      if (triggerRef.current?.contains(t)) return;
      setOpen(false);
    };
    const onKey = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        setOpen(false);
        triggerRef.current?.focus();
      }
    };
    document.addEventListener('mousedown', onDown);
    document.addEventListener('keydown', onKey);
    return () => {
      document.removeEventListener('mousedown', onDown);
      document.removeEventListener('keydown', onKey);
    };
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const btn = triggerRef.current;
    if (!btn) return;
    const rect = btn.getBoundingClientRect();
    const panelW = Math.min(560, Math.max(320, window.innerWidth - 32));
    const margin = 8;
    let left = rect.right - panelW;
    left = Math.max(margin, Math.min(left, window.innerWidth - panelW - margin));
    const spaceBelow = window.innerHeight - rect.bottom - margin;
    const spaceAbove = rect.top - margin;
    const need = 360;
    const placement: 'down' | 'up' = spaceBelow < need && spaceAbove > spaceBelow ? 'up' : 'down';
    const top = placement === 'down' ? rect.bottom + 6 : rect.top - 6;
    setPos({ left, top, width: panelW, placement });
    setQuery('');
  }, [open]);

  useEffect(() => {
    if (!open) return;
    const onScroll = () => setOpen(false);
    window.addEventListener('scroll', onScroll, true);
    window.addEventListener('resize', onScroll);
    return () => {
      window.removeEventListener('scroll', onScroll, true);
      window.removeEventListener('resize', onScroll);
    };
  }, [open]);

  const filtered = React.useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return models;
    return models.filter((m) => {
      const name = (m.name ?? '').toLowerCase();
      const provider = (m.provider_name ?? '').toLowerCase();
      const desc = (m.description ?? '').toLowerCase();
      return name.includes(q) || provider.includes(q) || desc.includes(q);
    });
  }, [models, query]);

  return (
    <>
      <button
        ref={triggerRef}
        type="button"
        onClick={() => !disabled && setOpen((v) => !v)}
        disabled={disabled}
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={listboxId}
        title="Choose OCR model"
        data-testid={buttonTestId ?? 'slip-vision-model-trigger'}
        className="inline-flex items-center gap-1.5 px-2.5 py-1.5 rounded-lg border border-rule bg-paper-2 text-xs font-mono text-ink-2 hover:border-rule-strong hover:text-ink disabled:opacity-50 disabled:cursor-not-allowed transition-colors"
      >
        <Eye className="size-3.5" strokeWidth={2} aria-hidden />
        <span className="text-ink truncate max-w-[140px]">{value || 'auto'}</span>
        <ChevronDown className={['size-3 text-mute transition-transform', open ? 'rotate-180' : 'rotate-0'].join(' ')} />
      </button>

      {open && pos && typeof document !== 'undefined' && (() => {
        const node = (
          <div
            ref={panelRef}
            role="dialog"
            aria-modal="false"
            aria-label="OCR model"
            data-testid={testId ?? 'slip-vision-model-popover'}
            style={{
              position: 'fixed',
              left: pos.left,
              top: pos.top,
              width: pos.width,
              transform: pos.placement === 'up' ? 'translateY(-100%)' : undefined,
              zIndex: 1000,
            }}
            className="rounded-xl bg-paper-2 border border-rule-strong shadow-popover animate-fade-scale"
          >
            <div className="p-2 space-y-2">
              <div className="relative">
                <Search className="size-3.5 absolute left-2 top-1/2 -translate-y-1/2 text-mute pointer-events-none" aria-hidden />
                <input
                  type="text"
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  placeholder="Search model…"
                  aria-label="Search OCR model"
                  className="w-full pl-7 pr-2 py-1.5 rounded-md border border-rule bg-paper text-xs text-ink placeholder:text-mute focus:border-accent focus:ring-2 focus:ring-accent/20 focus:outline-none"
                />
              </div>
              <div
                id={listboxId}
                role="listbox"
                aria-label="OCR models"
                className="grid grid-cols-1 md:grid-cols-2 gap-2 max-h-[60vh] overflow-y-auto pr-1"
              >
                {filtered.length === 0 ? (
                  <div className="col-span-full text-xs text-mute italic px-3 py-6 text-center">
                    No models match.
                  </div>
                ) : (
                  filtered.map((m) => (
                    <ModelCard
                      key={m.id}
                      m={m}
                      selected={m.name === value}
                      onSelect={(name) => {
                        onChange(name);
                        setOpen(false);
                        triggerRef.current?.focus();
                      }}
                      testId={`slip-vision-model-option-${m.id}`}
                      disabled={disabled}
                    />
                  ))
                )}
              </div>
            </div>
          </div>
        );
        return ReactDOM.createPortal(node, document.body);
      })()}
    </>
  );
}

export default ModelSelect;
