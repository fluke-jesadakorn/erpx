import React from 'react';
import { Check, CircleDot, Circle, Sparkles } from 'lucide-react';

type Tone = 'indigo' | 'cyan' | 'emerald' | 'amber' | 'slate' | 'accent';

const RING: Record<Tone, string> = {
  indigo:  'border-accent/40',
  accent:  'border-accent/40',
  cyan:    'border-info/40',
  emerald: 'border-positive/40',
  amber:   'border-caution/40',
  slate:   'border-rule',
};

const RING_SHADOW: Record<Tone, string> = {
  indigo:  'shadow-[0_0_0_1px_color-mix(in_oklab,var(--accent)_18%,transparent),0_18px_44px_-18px_color-mix(in_oklab,var(--accent)_45%,transparent)]',
  accent:  'shadow-[0_0_0_1px_color-mix(in_oklab,var(--accent)_18%,transparent),0_18px_44px_-18px_color-mix(in_oklab,var(--accent)_45%,transparent)]',
  cyan:    'shadow-[0_0_0_1px_color-mix(in_oklab,var(--info)_20%,transparent),0_18px_44px_-18px_color-mix(in_oklab,var(--info)_45%,transparent)]',
  emerald: 'shadow-[0_0_0_1px_color-mix(in_oklab,var(--positive)_20%,transparent),0_18px_44px_-18px_color-mix(in_oklab,var(--positive)_45%,transparent)]',
  amber:   'shadow-[0_0_0_1px_color-mix(in_oklab,var(--caution)_20%,transparent),0_18px_44px_-18px_color-mix(in_oklab,var(--caution)_45%,transparent)]',
  slate:   'shadow-[0_0_0_1px_color-mix(in_oklab,var(--rule)_40%,transparent),0_18px_44px_-22px_rgba(0,0,0,0.55)]',
};

const RING_SHADOW_DONE: Record<Tone, string> = {
  indigo:  'shadow-[0_0_0_1px_color-mix(in_oklab,var(--positive)_20%,transparent),0_16px_36px_-22px_color-mix(in_oklab,var(--positive)_40%,transparent)]',
  accent:  'shadow-[0_0_0_1px_color-mix(in_oklab,var(--positive)_20%,transparent),0_16px_36px_-22px_color-mix(in_oklab,var(--positive)_40%,transparent)]',
  cyan:    'shadow-[0_0_0_1px_color-mix(in_oklab,var(--positive)_20%,transparent),0_16px_36px_-22px_color-mix(in_oklab,var(--positive)_40%,transparent)]',
  emerald: 'shadow-[0_0_0_1px_color-mix(in_oklab,var(--positive)_20%,transparent),0_16px_36px_-22px_color-mix(in_oklab,var(--positive)_40%,transparent)]',
  amber:   'shadow-[0_0_0_1px_color-mix(in_oklab,var(--caution)_18%,transparent),0_16px_36px_-22px_color-mix(in_oklab,var(--caution)_40%,transparent)]',
  slate:   'shadow-[0_0_0_1px_color-mix(in_oklab,var(--positive)_20%,transparent),0_16px_36px_-22px_color-mix(in_oklab,var(--positive)_40%,transparent)]',
};

const HEAD_BG: Record<Tone, string> = {
  indigo:  'bg-[color-mix(in_oklab,var(--accent)_8%,transparent)]',
  accent:  'bg-[color-mix(in_oklab,var(--accent)_8%,transparent)]',
  cyan:    'bg-[color-mix(in_oklab,var(--info)_8%,transparent)]',
  emerald: 'bg-[color-mix(in_oklab,var(--positive)_8%,transparent)]',
  amber:   'bg-[color-mix(in_oklab,var(--caution)_8%,transparent)]',
  slate:   'bg-paper-2',
};

export function StepBadge({
  n,
  done,
  active,
  tone = 'indigo',
}: {
  n: number;
  done: boolean;
  active: boolean;
  tone?: Tone;
}) {
  const pingRing = {
    indigo:  'bg-accent',
    accent:  'bg-accent',
    cyan:    'bg-info',
    emerald: 'bg-positive',
    amber:   'bg-caution',
    slate:   'bg-mute',
  }[tone];

  if (done) {
    return (
      <span
        className="shrink-0 relative inline-flex items-center justify-center w-9 h-9 rounded-full border border-positive/50 bg-positive-soft text-positive shadow-[0_0_0_3px_color-mix(in_oklab,var(--positive)_18%,transparent)]"
        aria-current={undefined}
      >
        <Check className="size-4" strokeWidth={3} aria-hidden />
        <span className="sr-only">Done</span>
      </span>
    );
  }
  if (active) {
    return (
      <span
        className="shrink-0 relative inline-flex items-center justify-center w-9 h-9 rounded-full border border-accent bg-accent text-paper-2 shadow-[0_0_0_3px_color-mix(in_oklab,var(--accent)_30%,transparent)]"
        aria-current="step"
      >
        <span
          aria-hidden
          className={`absolute inset-0 rounded-full ${pingRing} opacity-40 animate-ping`}
        />
        <span className="relative font-mono font-bold text-sm">{n}</span>
      </span>
    );
  }
  return (
    <span
      className="shrink-0 relative inline-flex items-center justify-center w-9 h-9 rounded-full border border-rule bg-paper-3 text-mute"
      aria-current={undefined}
    >
      <span className="font-mono font-bold text-sm">{n}</span>
    </span>
  );
}

export function StepCard({
  n,
  icon,
  title,
  titleTh,
  hint,
  done,
  active,
  tone = 'indigo',
  accent,
  bodyTint = false,
  cardId,
  flat = false,
  children,
  badge,
  dimmed = false,
}: {
  n: number;
  icon: React.ReactNode;
  title: React.ReactNode;
  titleTh?: React.ReactNode;
  hint?: React.ReactNode;
  done: boolean;
  active: boolean;
  tone?: Tone;
  accent?: 'your-turn' | null;
  bodyTint?: boolean;
  cardId?: string;
  flat?: boolean;
  children: React.ReactNode;
  badge?: React.ReactNode;
  dimmed?: boolean;
}) {
  const statusRing = done ? RING.emerald : active ? RING[tone] : RING.slate;
  const statusShadow = done ? RING_SHADOW_DONE[tone] : active ? RING_SHADOW[tone] : RING_SHADOW.slate;
  const headBg = done
    ? 'bg-[color-mix(in_oklab,var(--positive)_6%,transparent)]'
    : active
    ? HEAD_BG[tone]
    : HEAD_BG.slate;

  const accentRing =
    accent === 'your-turn'
      ? 'border-cyan-400/70 shadow-[0_0_0_1px_rgba(6,182,212,0.25),0_16px_44px_-14px_rgba(6,182,212,0.4)]'
      : '';

  const accentOverlay =
    accent === 'your-turn'
      ? 'before:absolute before:inset-0 before:rounded-2xl before:bg-[radial-gradient(ellipse_at_top_left,rgba(6,182,212,0.12),transparent_55%)] before:pointer-events-none'
      : '';

  const bodySurface = bodyTint
    ? done
      ? 'bg-[color-mix(in_oklab,var(--positive)_4%,transparent)]'
      : active
      ? 'bg-[color-mix(in_oklab,var(--accent)_4%,transparent)]'
      : 'bg-paper-2'
    : flat
    ? 'bg-paper-2'
    : 'glass-panel';

  const titleAttr =
    titleTh && typeof titleTh === 'string' ? `${title} · ${titleTh}` : undefined;

  return (
    <article
      id={cardId}
      className={[
        'relative overflow-hidden rounded-2xl border transition-all duration-300 scroll-mt-28',
        'backdrop-blur-sm',
        bodySurface,
        statusRing,
        statusShadow,
        accentRing,
        accentOverlay,
        dimmed
          ? 'opacity-50 grayscale transition duration-200 hover:opacity-100 hover:grayscale-0'
          : '',
      ].join(' ')}
      data-step={n}
      data-dimmed={dimmed ? 'true' : undefined}
      data-accent={accent ?? undefined}
      data-status={done ? 'done' : active ? 'active' : 'locked'}
    >
      <div className={`h-px w-full ${done ? 'bg-positive/30' : active ? 'bg-accent/30' : 'bg-rule'}`} aria-hidden />
      <header
        className={[
          'relative grid grid-cols-[auto_minmax(0,1fr)_auto] items-start gap-3 px-5 py-4 border-b border-rule',
          headBg,
          accent === 'your-turn' ? 'border-info/25' : '',
        ].join(' ')}
      >
        <StepBadge n={n} done={done} active={active} tone={tone} />
        <div className="min-w-0">
          <div className="flex items-center gap-2 flex-wrap">
            <span aria-hidden className="text-base leading-none text-ink-2">
              {icon}
            </span>
            <h3
              className="text-sm font-semibold text-ink leading-tight"
              {...(titleAttr ? { title: titleAttr } : {})}
            >
              {title}
            </h3>
            {titleTh ? (
              <span className="sr-only" lang="th">{titleTh}</span>
            ) : null}
            {accent === 'your-turn' && (
              <span
                aria-hidden
                title="Your turn"
                className="relative inline-flex items-center gap-1 rounded-full border border-info/60 bg-info-soft px-2 py-0.5 text-info shadow-[0_0_10px_-2px_color-mix(in_oklab,var(--info)_50%,transparent)]"
              >
                <span aria-hidden className="relative inline-flex h-1.5 w-1.5">
                  <span className="absolute inset-0 animate-ping rounded-full bg-info opacity-75" />
                  <span className="relative inline-block h-1.5 w-1.5 rounded-full bg-info" />
                </span>
                <Sparkles className="size-3" strokeWidth={2.5} />
              </span>
            )}
          </div>
          {hint && (
            <p
              className="mt-1 text-xs leading-snug text-ink-2"
              {...(titleTh && typeof titleTh === 'string'
                ? { title: typeof hint === 'string' ? hint : undefined }
                : {})}
            >
              {hint}
            </p>
          )}
        </div>
        {badge ? <div className="items-start">{badge}</div> : null}
      </header>
      <div className="relative p-6">{children}</div>
    </article>
  );
}

export function fmtTime(d: Date | string | null | undefined): string {
  if (!d) return '—';
  const date = typeof d === 'string' ? new Date(d) : d;
  if (Number.isNaN(date.getTime())) return '—';
  return date.toLocaleTimeString('en-GB', {
    hour: '2-digit',
    minute: '2-digit',
    second: '2-digit',
    hour12: false,
  });
}
