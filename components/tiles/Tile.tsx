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

const accentMap: Record<string, { bg: string; bar: string; text: string; glow: string; ring: string }> = {
  emerald: { bg: 'from-emerald-500/25 via-emerald-700/15 to-emerald-900/40', bar: 'bg-emerald-400', text: 'text-emerald-300', glow: 'shadow-emerald-500/40', ring: 'ring-emerald-400/40' },
  indigo:  { bg: 'from-indigo-500/25 via-indigo-700/15 to-indigo-900/40',   bar: 'bg-indigo-400',  text: 'text-indigo-300',  glow: 'shadow-indigo-500/40', ring: 'ring-indigo-400/40' },
  amber:   { bg: 'from-amber-400/25 via-amber-700/15 to-amber-900/40',     bar: 'bg-amber-400',   text: 'text-amber-300',   glow: 'shadow-amber-500/40', ring: 'ring-amber-400/40' },
  purple:  { bg: 'from-purple-500/25 via-purple-700/15 to-purple-900/40',   bar: 'bg-purple-400',  text: 'text-purple-300',  glow: 'shadow-purple-500/40', ring: 'ring-purple-400/40' },
  rose:    { bg: 'from-rose-500/25 via-rose-700/15 to-rose-900/40',         bar: 'bg-rose-400',    text: 'text-rose-300',    glow: 'shadow-rose-500/40', ring: 'ring-rose-400/40' },
  cyan:    { bg: 'from-cyan-500/25 via-cyan-700/15 to-cyan-900/40',         bar: 'bg-cyan-400',    text: 'text-cyan-300',    glow: 'shadow-cyan-500/40', ring: 'ring-cyan-400/40' },
  slate:   { bg: 'from-slate-500/25 via-slate-700/15 to-slate-900/40',      bar: 'bg-slate-400',   text: 'text-slate-300',   glow: 'shadow-slate-500/40', ring: 'ring-slate-400/40' },
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
            <span className="ml-1 text-xs font-normal text-slate-400">· {secName}</span>
          ) : null}
        </span>
        {subEn ? (
          <p className={subClass}>
            {subEn}
            {secSub ? (
              <span className="ml-1 text-xs font-normal text-slate-500">· {secSub}</span>
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
          <span className="block mt-1 text-xs font-normal text-slate-400 break-words">{secName}</span>
        ) : null}
      </span>
      {subEn ? (
        <p className={subClass}>
          <span className="block break-words">{subEn}</span>
          {secSub ? (
            <span className="block mt-1 text-xs font-normal text-slate-500 break-words">{secSub}</span>
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
  const hasMeta = !!viewPermId;

  const content = (
    <>
      <div
        className={`absolute top-0 left-0 right-0 ${
          active ? 'h-1.5' : 'h-1'
        } ${c.bar} ${active ? 'opacity-100 shadow-[0_0_12px_currentColor]' : locked ? 'opacity-25' : 'opacity-50'}`}
        style={active ? { color: 'currentcolor' } : undefined}
      />

      {active && !locked && (
        <div className="absolute top-2.5 right-2.5 flex items-center gap-1.5 z-20">
          <span className={`w-1.5 h-1.5 rounded-full ${c.bar} shadow-[0_0_8px_currentColor] animate-pulse`} />
        </div>
      )}

      {locked && (
        <div className="absolute top-2 right-2 inline-flex items-center gap-1 rounded-full bg-slate-900/80 border border-slate-700/80 px-1.5 py-0.5 text-xs font-mono uppercase tracking-wider text-slate-300 z-20">
          🔒 <span><T id="tiles.locked" hideSecondary /></span>
        </div>
      )}

      <div className={`absolute -right-6 -bottom-8 text-[120px] leading-none select-none pointer-events-none ${locked ? 'opacity-[0.03]' : 'opacity-[0.05]'}`}>
        {tile.icon}
      </div>

      {active && !locked && (
        <div className={`absolute inset-0 rounded-2xl pointer-events-none ${c.bg}`} />
      )}

      {!locked && (
        <div
          aria-hidden
          className="absolute inset-y-0 -left-1/3 w-1/3 bg-gradient-to-r from-transparent via-white/15 to-transparent pointer-events-none opacity-0 group-hover:opacity-100 group-hover:animate-shine-sweep"
        />
      )}

      <div className="relative z-10 flex flex-col h-full">
        <div className="flex items-start justify-between gap-2">
          <span className={`text-[52px] drop-shadow-lg leading-none ${locked ? 'grayscale opacity-60' : ''}`}>{tile.icon}</span>
          {tile.count !== undefined && (
            <div className={`flex flex-col items-end ${live && !locked ? '' : 'opacity-70'}`}>
              <span className={`font-black font-mono leading-none ${locked ? 'text-slate-500' : c.text} text-3xl`}>
                {typeof tile.count === 'number' && live ? tile.count.toLocaleString() : tile.count}
              </span>
              {tile.countLabel && (
                <span className="text-xs text-slate-500 font-mono uppercase tracking-wider mt-0.5 text-right max-w-[80px] leading-tight">
                  {tile.countLabel}
                </span>
              )}
            </div>
          )}
        </div>

        <div className="mt-auto pt-3">
          <h3 className={`font-black leading-tight text-[14px] ${locked ? 'text-slate-400' : 'text-white'}`}>
            <TileText
              tileId={tile.id}
              fallbackEn={tile.display_name}
              fallbackSub={tile.subtitle}
              nameClass=""
              subClass="mt-1.5 font-sans leading-snug text-xs text-slate-400"
              stack
            />
          </h3>
          {hasMeta && viewPermId && (
            <div className="mt-2 flex flex-wrap items-center gap-1">
              <span className={`inline-flex items-center gap-0.5 rounded-full border px-1.5 py-0.5 text-[10px] font-mono uppercase tracking-wider ${locked ? 'bg-slate-900/70 text-slate-500 border-slate-800' : 'bg-slate-900/70 text-slate-300 border-slate-700/70'}`}>
                <span aria-hidden>{locked ? '🔒' : '✓'}</span>
                <span className="truncate max-w-[220px]">{locked ? <T id="tiles.locked" hideSecondary /> : <T id="tiles.open" hideSecondary />}</span>
              </span>
            </div>
          )}
        </div>
      </div>
    </>
  );

  const className = [
    'group relative rounded-2xl overflow-hidden text-left block w-full',
    'bg-gradient-to-br',
    c.bg,
    'min-h-[280px] p-5',
    locked
      ? `opacity-50 grayscale saturate-50 ring-1 ring-slate-800/60 border border-slate-800/80 cursor-not-allowed`
      : active
        ? `ring-2 ${c.ring} shadow-xl ${c.glow} scale-[1.015]`
        : 'ring-1 ring-slate-800/60 hover:ring-slate-600 hover:scale-[1.02] hover:-translate-y-0.5 hover:shadow-xl hover:shadow-black/50 border border-slate-800/80',
    'transition-all duration-200',
  ].join(' ');

  const tooltipBody = (
    <div className="space-y-1.5">
      <div className="text-sm font-mono text-slate-100">
        <TileText
          tileId={tile.id}
          fallbackEn={reason || tile.display_name}
          nameClass="font-mono text-slate-100"
        />
      </div>
      {viewPermId && (
        <div className="text-xs font-mono text-slate-400">
          <span className="text-slate-500"><T id="tilesUi.permLabel" hideSecondary />:</span> {viewPermId}
        </div>
      )}
      {requiredRoles && requiredRoles.length > 0 && (
        <div className="text-xs font-mono text-slate-400">
          <span className="text-slate-500"><T id="tilesUi.rolesWithAccess" hideSecondary />: </span>
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
          className="mt-1 inline-flex items-center gap-1.5 px-2.5 py-1 rounded-lg bg-cyan-500/20 border border-cyan-500/40 text-xs font-mono uppercase tracking-wider text-cyan-100 hover:bg-cyan-500/30"
        >
          ✉ <T id="tilesUi.request" hideSecondary />
        </button>
      )}
    </div>
  );

  if (locked) {
    return (
      <>
        <TileTooltip content={tooltipBody}>
          <button type="button" aria-disabled className={className}>
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
