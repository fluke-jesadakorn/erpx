'use client';

import React, { useState } from 'react';
import Link from 'next/link';
import { TileTooltip } from '../TileTooltip';
import { RequestAccessModal } from '../RequestAccessModal';
import type { TileState } from '../tileAccess';
import { T } from '@/components/i18n/T';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import thDict from '../../messages/th.json';
import deDict from '../../messages/de.json';
import enDict from '../../messages/en.json';
import type { SecondaryLocale } from '@/i18n/config';
import { ArrowUpRight, Lock, Mail } from 'lucide-react';
import { tileIcon } from '../tile-config';

const accentMap: Record<string, { bg: string; bar: string; text: string; glow: string; ring: string }> = {
  emerald: { bg: 'bg-positive-soft', bar: 'bg-positive', text: 'text-positive', glow: 'shadow-positive', ring: 'ring-positive' },
  indigo:  { bg: 'bg-accent-soft',   bar: 'bg-accent',   text: 'text-accent',   glow: 'shadow-accent',   ring: 'ring-accent' },
  amber:   { bg: 'bg-caution-soft',  bar: 'bg-caution',  text: 'text-caution',  glow: 'shadow-caution',  ring: 'ring-caution' },
  purple:  { bg: 'bg-accent-soft',   bar: 'bg-accent',   text: 'text-accent',   glow: 'shadow-accent',   ring: 'ring-accent' },
  rose:    { bg: 'bg-critical-soft', bar: 'bg-critical', text: 'text-critical', glow: 'shadow-critical', ring: 'ring-critical' },
  cyan:    { bg: 'bg-info-soft',     bar: 'bg-info',     text: 'text-info',     glow: 'shadow-info',     ring: 'ring-info' },
  slate:   { bg: 'bg-paper-2',       bar: 'bg-paper-3',  text: 'text-ink-2',    glow: 'shadow-paper-3',  ring: 'ring-rule' },
};

const SEC: Record<SecondaryLocale, Record<string, unknown>> = {
  th: thDict as Record<string, unknown>,
  de: deDict as Record<string, unknown>,
};
const EN_DICT: Record<string, unknown> = enDict as Record<string, unknown>;

function lookup(dict: Record<string, unknown>, path: string): string | undefined {
  const parts = path.split('.');
  let cur: unknown = dict;
  for (const p of parts) {
    if (cur && typeof cur === 'object' && p in (cur as Record<string, unknown>)) {
      cur = (cur as Record<string, unknown>)[p];
    } else {
      return undefined;
    }
  }
  return typeof cur === 'string' ? cur : undefined;
}

export interface TileProps {
  tile: any;
  active?: boolean;
  href?: string;
  state?: TileState;
  reason?: string;
  requiredRoles?: string[];
  onClick?: () => void;
  onRequestAccess?: () => void;
  actorId?: number;
  targetLabel?: string;
}

function TileText({
  tileId,
  fallbackEn,
  fallbackSub,
  nameClass,
  subClass,
  stack = false,
}: {
  tileId: string;
  fallbackEn: string;
  fallbackSub?: string;
  nameClass: string;
  subClass?: string;
  stack?: boolean;
}) {
  const loc = useSecondaryLocale();
  const nameEn = lookup(EN_DICT, `tiles.tile.${tileId}.name`) ?? fallbackEn;
  const nameSec = lookup(SEC[loc], `tiles.tile.${tileId}.name`);
  const subEn = fallbackSub != null ? lookup(EN_DICT, `tiles.tile.${tileId}.subtitle`) ?? fallbackSub : undefined;
  const subSec = fallbackSub != null ? lookup(SEC[loc], `tiles.tile.${tileId}.subtitle`) : undefined;

  const showSec = (sec?: string, baseEn?: string) => !!sec && sec !== baseEn;
  const secName = showSec(nameSec, nameEn) ? nameSec : undefined;
  const secSub  = subSec !== undefined && showSec(subSec, subEn) ? subSec : undefined;

  if (!stack) {
    return (
      <>
        <span className={nameClass}>
          {nameEn}
          {secName ? (
            <span className="ml-1 text-xs font-normal text-ink-2">· {secName}</span>
          ) : null}
        </span>
        {subEn ? (
          <p className={subClass}>
            {subEn}
            {secSub ? (
              <span className="ml-1 text-xs font-normal text-mute">· {secSub}</span>
            ) : null}
          </p>
        ) : null}
      </>
    );
  }

  return (
    <>
      <span className={nameClass}>
        <span className="block break-words">{nameEn}</span>
        {secName ? (
          <span className="block mt-1 text-xs font-normal text-ink-2 break-words">{secName}</span>
        ) : null}
      </span>
      {subEn ? (
        <p className={subClass}>
          <span className="block break-words">{subEn}</span>
          {secSub ? (
            <span className="block mt-1 text-xs font-normal text-mute break-words">{secSub}</span>
          ) : null}
        </p>
      ) : null}
    </>
  );
}

export const Tile: React.FC<TileProps> = ({
  tile, active, href,
  state = 'open', reason, requiredRoles,
  onClick, onRequestAccess, actorId, targetLabel,
}) => {
  const c = accentMap[tile.accent] || accentMap.slate;
  const live = typeof tile.count === 'number' && (tile.count as number) > 0;
  const locked = state === 'locked';

  const [requestOpen, setRequestOpen] = useState(false);

  const viewPermId = tile.access_meta?.viewPermId ?? tile.view_perm_id ?? null;

  const content = (
    <>
      <div
        className={`absolute top-0 left-0 right-0 ${
          active ? 'h-1.5' : 'h-1'
        } ${c.bar} ${active ? 'opacity-100 shadow-[0_0_12px_currentColor]' : locked ? 'opacity-25' : 'opacity-50'}`}
        style={active ? { color: 'currentcolor' } : undefined}
      />

      {active && !locked && (
        <div className="absolute top-2.5 right-2.5 flex items-center gap-1.5 z-dropdown">
          <span className={`w-1.5 h-1.5 rounded-full ${c.bar} shadow-[0_0_8px_currentColor] animate-pulse`} />
        </div>
      )}

      {locked && (
        <div className="glass-chip absolute right-3 top-3 z-dropdown inline-flex items-center gap-1 px-2 py-1 text-[10px] font-medium uppercase tracking-[0.12em] text-ink-2">
          <Lock size={11} aria-hidden /> <span><T id="tiles.locked" hideSecondary /></span>
        </div>
      )}

      {React.createElement(tileIcon(tile), {
        'aria-hidden': true,
        className: `absolute -bottom-7 -right-5 h-28 w-28 select-none stroke-[0.7] ${locked ? 'opacity-[0.035]' : 'opacity-[0.055]'}`,
      })}

      {active && !locked && (
        <div className={`pointer-events-none absolute inset-0 rounded-2xl ${c.bg}`} />
      )}

      {!locked && (
        <div
          aria-hidden
          className="absolute inset-y-0 -left-1/3 w-1/3 bg-paper-2 pointer-events-none opacity-0 group-hover:opacity-30"
        />
      )}

      <div className="relative z-10 flex flex-col h-full">
        <div className="flex items-start justify-between gap-3">
          <span className={`inline-flex h-10 w-10 items-center justify-center rounded-xl border border-rule bg-paper-2/60 shadow-inner ${c.text} ${locked ? 'opacity-60' : ''}`}>
            {React.createElement(tileIcon(tile), { size: 19, 'aria-hidden': true })}
          </span>
          {tile.count !== undefined && (
            <div className={`flex flex-col items-end ${live && !locked ? '' : 'opacity-70'}`}>
              <span className={`font-black font-mono leading-none ${locked ? 'text-mute' : c.text} text-3xl`}>
                {typeof tile.count === 'number' && live ? tile.count.toLocaleString() : tile.count}
              </span>
              {tile.countLabel && (
                <span className="text-xs text-mute font-mono uppercase tracking-wider mt-0.5 text-right max-w-[80px] leading-tight">
                  {tile.countLabel}
                </span>
              )}
            </div>
          )}
        </div>

        <div className="mt-auto min-w-0 pt-3">
          <TileText
            tileId={tile.id}
            fallbackEn={tile.display_name}
            fallbackSub={tile.subtitle}
            nameClass={`block text-[14px] font-semibold leading-tight ${locked ? 'text-ink-2' : 'text-ink'}`}
            subClass="mt-1.5 line-clamp-2 font-sans text-xs leading-relaxed text-mute"
            stack
          />
          {locked && (
            <span className="mt-2 inline-flex items-center gap-1 text-[11px] font-medium text-info">
              <Mail size={11} aria-hidden />
              <T id="tilesUi.request" hideSecondary />
            </span>
          )}
        </div>
      </div>
    </>
  );

  const className = [
    'panel-interactive group relative block h-full min-h-[180px] w-full overflow-hidden rounded-2xl p-4 text-left',
    locked
      ? `cursor-pointer opacity-70 saturate-50 ring-1 ring-rule/60 border border-rule/80 hover:opacity-100`
      : active
        ? `ring-2 ${c.ring} shadow-xl ${c.glow}`
        : 'ring-1 ring-rule/50 hover:ring-rule-strong hover:-translate-y-px',
    'transition-all duration-200',
  ].join(' ');

  const tooltipBody = (
    <div className="space-y-1.5">
      <div className="text-sm font-mono text-ink">
        <TileText
          tileId={tile.id}
          fallbackEn={reason || tile.display_name}
          nameClass="font-mono text-ink"
        />
      </div>
      {viewPermId && (
        <div className="text-xs font-mono text-ink-2">
          <span className="text-mute"><T id="tilesUi.permLabel" hideSecondary />:</span> {viewPermId}
        </div>
      )}
      {requiredRoles && requiredRoles.length > 0 && (
        <div className="text-xs font-mono text-ink-2">
          <span className="text-mute"><T id="tilesUi.rolesWithAccess" hideSecondary />: </span>
          {requiredRoles.join(', ')}
        </div>
      )}
      {locked && (
        <button
          type="button"
          onClick={(e) => {
            e.preventDefault();
            e.stopPropagation();
            setRequestOpen(true);
            onRequestAccess?.();
          }}
          className="mt-1 inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-info border border-info text-xs font-mono uppercase tracking-wider text-info-soft hover:bg-info"
        >
          <Mail size={12} aria-hidden /> <T id="tilesUi.request" hideSecondary />
        </button>
      )}
    </div>
  );

  if (locked) {
    return (
      <>
        <TileTooltip content={tooltipBody}>
          <button
            type="button"
            onClick={() => {
              setRequestOpen(true);
              onRequestAccess?.();
            }}
            className={className}
          >
            {content}
          </button>
        </TileTooltip>
        {actorId && (
          <RequestAccessModal
            open={requestOpen}
            onClose={() => setRequestOpen(false)}
            tile={tile}
            actorId={actorId}
            targetLabel={targetLabel || 'HR Manager'}
          />
        )}
      </>
    );
  }

  if (href) {
    return (
      <TileTooltip content={tooltipBody}>
        <Link href={href} onClick={onClick} className={className}>
          {content}
          <ArrowUpRight aria-hidden size={14} className="absolute bottom-4 right-4 z-10 text-mute transition-colors group-hover:text-ink" />
        </Link>
      </TileTooltip>
    );
  }

  return (
    <TileTooltip content={tooltipBody}>
      <button type="button" onClick={onClick} className={className}>
        {content}
      </button>
    </TileTooltip>
  );
};

export default Tile;
