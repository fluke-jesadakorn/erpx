'use client';

import React from 'react';
import { TileTooltip } from '@/components/TileTooltip';
import type { ApproverRow } from '@/waybill/queries';
import { initialsOf, roleAccent, roleDisplay } from './ui';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';

interface Props {
  user: ApproverRow;
  size?: 'sm' | 'md';
}

export function ApproverAvatar({ user, size = 'md' }: Props) {
  const locale = useSecondaryLocale();
  const dim = size === 'sm' ? 'h-8 w-8 text-xs' : 'h-10 w-10 text-sm';
  const accent = roleAccent(user.role_id);
  const position = roleDisplay(user.role_id, locale);
  const dept = user.dept_group_name ?? null;
  const positionLine = dept ? `${position} · ${dept}` : position;
  const levelLabel = user.level != null ? `Level ${user.level}` : null;
  void locale;

  return (
    <TileTooltip
      side="top"
      align="center"
      content={
        <div className="min-w-[180px] space-y-1">
          <div className="flex items-center gap-2">
            <span
              className={`grid ${dim} shrink-0 place-items-center rounded-full font-bold ring-1 ring-slate-700/60 ${accent}`}
            >
              {initialsOf(user.fullname)}
            </span>
            <div className="min-w-0 flex-1">
              <div className="text-[13px] font-bold leading-tight text-slate-50">
                {user.fullname}
              </div>
              <div className="text-xs font-mono uppercase tracking-wider text-slate-400">
                #{user.user_id}
              </div>
            </div>
          </div>
          <div className="border-t border-slate-700/60 pt-1.5">
            <div className="text-sm leading-tight text-slate-200">
              {positionLine}
            </div>
            {levelLabel && (
              <div className="mt-1 inline-flex items-center rounded-full border border-slate-700/60 bg-slate-800/60 px-1.5 py-0.5 text-xs font-mono uppercase tracking-widest text-slate-300">
                {levelLabel}
              </div>
            )}
          </div>
        </div>
      }
    >
      <span
        tabIndex={0}
        aria-label={`${user.fullname} · ${positionLine}`}
        className={`grid ${dim} cursor-default shrink-0 place-items-center rounded-full font-bold ring-2 ring-slate-950 transition hover:ring-slate-700 focus:outline-none focus:ring-slate-700 ${accent}`}
      >
        {initialsOf(user.fullname)}
      </span>
    </TileTooltip>
  );
}