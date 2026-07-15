import React from 'react';
import type { VisionModel } from '@folio-lib/ai/loadVisionModels';

export function formatBytes(n: number): string {
  if (n < 1024) return `${n} B`;
  if (n < 1024 * 1024) return `${(n / 1024).toFixed(1)} KB`;
  return `${(n / (1024 * 1024)).toFixed(2)} MB`;
}

export function fileKind(mime: string, name?: string): string {
  const m = (mime || '').toLowerCase();
  if (m.startsWith('image/jpeg') || /\.jpe?g$/i.test(name || '')) return 'JPG';
  if (m.startsWith('image/png') || /\.png$/i.test(name || '')) return 'PNG';
  if (m.startsWith('image/webp') || /\.webp$/i.test(name || '')) return 'WEBP';
  if (m === 'application/pdf' || /\.pdf$/i.test(name || '')) return 'PDF';
  return (m.split('/')[1] || 'FILE').toUpperCase();
}

export function Stars({ value }: { value: number | null }) {
  if (value == null) return <span className="text-mute text-xs">—</span>;
  const filled = Math.max(0, Math.min(5, Math.round(value)));
  return (
    <span className="inline-flex items-center gap-px text-sm leading-none font-mono" aria-label={`${filled} of 5`}>
      {Array.from({ length: 5 }, (_, i) => (
        <span key={i} className={i < filled ? 'text-caution' : 'text-mute'}>
          {i < filled ? '★' : '☆'}
        </span>
      ))}
    </span>
  );
}

export function ModelCard({
  m,
  selected,
  onSelect,
  testId,
  disabled,
}: {
  m: VisionModel;
  selected: boolean;
  onSelect: (name: string) => void;
  testId: string;
  disabled?: boolean;
}) {
  return (
    <button
      type="button"
      onClick={() => onSelect(m.name)}
      disabled={disabled}
      data-testid={testId}
      className={`glass-tint-positive w-full h-full text-left rounded-xl border p-3 transition-colors disabled:opacity-50 disabled:cursor-not-allowed focus:outline-none focus:ring-2 focus:ring-positive/40 ${
        selected
          ? 'border-positive/60 '
          : 'border-rule bg-paper-3/40 hover:border-rule-strong hover:bg-paper-3/70'
      }`}
    >
      <div className="flex items-start gap-2">
        <span
          className={`mt-1 inline-block w-3 h-3 rounded-full border shrink-0 ${
            selected ? 'border-positive bg-positive ' : 'border-mute'
          }`}
          aria-hidden
        />
        <div className="flex-1 min-w-0 space-y-1">
          <div className="flex items-center gap-1.5 flex-wrap">
            <span className="text-xs font-bold text-white font-mono truncate">{m.name}</span>
            <span className="glass-panel px-1.5 py-0.5 rounded text-sm font-mono text-ink-2 uppercase tracking-wider">
              {m.provider_name}
            </span>
          </div>
          {m.description && (
            <p className="text-xs text-ink-2 leading-snug break-words">{m.description}</p>
          )}
          {(m.speed_rating != null || m.accuracy_rating != null) && (
            <div className="grid grid-cols-2 gap-x-3 gap-y-0.5 pt-1 border-t border-rule">
              <div className="flex items-center justify-between gap-1.5">
                <span className="text-sm font-mono text-mute uppercase tracking-wider">Speed</span>
                <Stars value={m.speed_rating} />
              </div>
              <div className="flex items-center justify-between gap-1.5">
                <span className="text-sm font-mono text-mute uppercase tracking-wider">Accuracy</span>
                <Stars value={m.accuracy_rating} />
              </div>
            </div>
          )}
        </div>
      </div>
    </button>
  );
}

export function FilledTick({ filled }: { filled: boolean }) {
  if (!filled) return null;
  return (
    <span
      className="glass-tint-positive absolute right-2.5 top-1/2 -translate-y-1/2 flex items-center justify-center w-5 h-5 rounded-full text-positive ring-1 ring-positive/40 pointer-events-none"
      aria-label="filled"
      data-testid="field-filled"
      title="Filled"
    >
      <svg className="w-2.5 h-2.5 stroke-[3.5] stroke-current fill-none" viewBox="0 0 24 24">
        <path strokeLinecap="round" strokeLinejoin="round" d="M4.5 12.75l6 6 9-13.5" />
      </svg>
    </span>
  );
}

export const Field: React.FC<{ label: string; hint?: string; children: React.ReactNode }> = ({
  label,
  hint,
  children,
}) => (
  <label className="block space-y-1.5">
    <span className="text-xs font-mono text-ink-2 uppercase tracking-widest font-semibold">
      {label}
      {hint && <span className="ml-1 text-mute normal-case tracking-normal">({hint})</span>}
    </span>
    {children}
  </label>
);

export const FieldSpinner: React.FC = () => (
  <span
    className="absolute right-2 top-1/2 -translate-y-1/2 inline-block w-3.5 h-3.5 border-2 border-positive/40 border-t-positive rounded-full animate-spin"
    aria-label="extracting"
  />
);

export function SectionHeader({
  icon,
  label,
  tone,
}: {
  icon: string;
  label: string;
  tone: 'info' | 'caution' | 'positive';
}) {
  return (
    <div className={`text-xs font-mono text-${tone} uppercase tracking-widest font-bold flex items-center gap-1.5`}>
      <span>{icon}</span> {label}
    </div>
  );
}