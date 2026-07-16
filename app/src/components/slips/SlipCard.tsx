import React from 'react';
import { Star } from 'lucide-react';
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
  if (value == null) return <span className="text-mute text-[10px]">—</span>;
  const filled = Math.max(0, Math.min(5, Math.round(value)));
  return (
    <span
      className="inline-flex items-center gap-px leading-none"
      aria-label={`${filled} of 5`}
      title={`${filled} of 5`}
    >
      {Array.from({ length: 5 }, (_, i) => (
        <Star
          key={i}
          className={i < filled ? 'size-2.5 text-caution fill-caution' : 'size-2.5 text-mute'}
          strokeWidth={1.5}
          aria-hidden
        />
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
      className={`w-full h-full text-left rounded-xl border p-3 transition-colors disabled:opacity-50 disabled:cursor-not-allowed focus:outline-none focus:ring-2 focus:ring-positive/40 ${
        selected
          ? 'border-positive/60 bg-positive-soft'
          : 'border-rule bg-paper-3/40 hover:border-rule-strong hover:bg-paper-3/70'
      }`}
    >
      <div className="flex items-start gap-2">
        <span
          className={`mt-1 inline-block w-3 h-3 rounded-full border shrink-0 ${
            selected ? 'border-positive bg-positive' : 'border-mute'
          }`}
          aria-hidden
        />
        <div className="flex-1 min-w-0 space-y-1">
          <div className="flex items-center gap-1.5 flex-wrap min-w-0">
            <span className="text-xs font-bold text-ink font-mono truncate min-w-0">{m.name}</span>
            {m.provider_name && (
              <span className="px-1.5 py-0.5 rounded text-[10px] font-mono text-ink-2 bg-paper-3 border border-rule shrink-0">
                {m.provider_name}
              </span>
            )}
          </div>
          {m.description && (
            <p
              className="text-xs text-ink-2 leading-snug break-words line-clamp-2"
              title={m.description}
            >
              {m.description}
            </p>
          )}
          {(m.speed_rating != null || m.accuracy_rating != null) && (
            <div className="flex flex-wrap items-center gap-x-3 gap-y-0.5 pt-1 border-t border-rule">
              <span className="inline-flex items-center gap-1.5 text-[10px] font-mono text-mute uppercase tracking-wider">
                <span>Spd</span>
                <Stars value={m.speed_rating} />
              </span>
              <span className="inline-flex items-center gap-1.5 text-[10px] font-mono text-mute uppercase tracking-wider">
                <span>Acc</span>
                <Stars value={m.accuracy_rating} />
              </span>
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
      className="absolute right-2.5 top-1/2 -translate-y-1/2 flex items-center justify-center w-5 h-5 rounded-full text-positive ring-1 ring-positive/40 pointer-events-none bg-positive-soft"
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

export const Field: React.FC<{
  label: string;
  hint?: string;
  icon?: React.ReactNode;
  children: React.ReactNode;
}> = ({ label, hint, icon, children }) => (
  <label className="block space-y-1.5">
    <span className="text-xs font-mono text-ink uppercase tracking-wider font-bold inline-flex items-center gap-1.5">
      {icon ? <span aria-hidden className="text-ink-2">{icon}</span> : null}
      {label}
      {hint && <span className="ml-1 text-ink-2/70 normal-case tracking-normal font-medium">({hint})</span>}
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

const SECTION_TONE_BG: Record<'info' | 'caution' | 'positive', string> = {
  info: 'bg-info/15 border-info/60 ring-1 ring-info/25',
  caution: 'bg-caution/15 border-caution/60 ring-1 ring-caution/25',
  positive: 'bg-positive/15 border-positive/60 ring-1 ring-positive/25',
};

const SECTION_TONE_TEXT: Record<'info' | 'caution' | 'positive', string> = {
  info: 'text-info-strong',
  caution: 'text-caution-strong',
  positive: 'text-positive-strong',
};

const SECTION_TONE_ICON: Record<'info' | 'caution' | 'positive', string> = {
  info: 'text-info',
  caution: 'text-caution',
  positive: 'text-positive',
};

export function SectionHeader({
  icon,
  label,
  tone,
  trailing,
}: {
  icon: React.ReactNode;
  label: string;
  tone: 'info' | 'caution' | 'positive';
  trailing?: React.ReactNode;
}) {
  return (
    <div
      className={`text-sm font-mono uppercase tracking-wider font-extrabold flex items-center gap-2 px-3 py-1.5 rounded-lg border-2 self-start shadow-sm ${SECTION_TONE_BG[tone]} ${SECTION_TONE_TEXT[tone]}`}
    >
      <span aria-hidden className={SECTION_TONE_ICON[tone]}>{icon}</span>
      <span>{label}</span>
      {trailing}
    </div>
  );
}
