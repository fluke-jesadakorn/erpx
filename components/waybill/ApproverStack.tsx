'use client';

import React from 'react';
import { TileTooltip, TileTooltipProvider } from '@/components/TileTooltip';
import type { ApproverRow } from '@/waybill/queries';
import { initialsOf, roleAccent, roleDisplay } from './ui';
import { ApproverAvatar } from './ApproverAvatar';
import { useSecondaryLocale } from '@/components/i18n/SecondaryLocaleProvider';
import { T } from '@/components/i18n/T';

const AVATAR_OVERFLOW = 5;

interface Props {
  approvers: ApproverRow[];
}

export function ApproverStack({ approvers }: Props) {
  const locale = useSecondaryLocale();
  const visible = approvers.slice(0, AVATAR_OVERFLOW);
  const overflow = approvers.slice(AVATAR_OVERFLOW);

  return (
    <TileTooltipProvider>
      <div className="flex -space-x-2.5">
        {visible.map((a) => (
          <ApproverAvatar key={`v-${a.user_id}`} user={a} />
        ))}
        {overflow.length > 0 && (
          <TileTooltip
            side="top"
            align="end"
            content={
              <div className="min-w-[220px] space-y-1.5">
                <div className="border-b border-slate-700/60 pb-1 text-xs font-mono uppercase tracking-widest text-slate-400">
                  <T id="waybill.approver.more" values={{ n: overflow.length }} />
                </div>
                <ul className="space-y-1.5">
                  {overflow.map((a) => {
                    const position = roleDisplay(a.role_id, locale);
                    const dept = a.dept_group_name ?? null;
                    const line = dept ? `${position} · ${dept}` : position;
                    return (
                      <li key={`o-${a.user_id}`} className="flex items-center gap-2">
                        <span
                          className={`grid h-6 w-6 shrink-0 place-items-center rounded-full text-xs font-bold ring-1 ring-slate-700/60 ${roleAccent(a.role_id)}`}
                        >
                          {initialsOf(a.fullname)}
                        </span>
                        <div className="min-w-0 flex-1 leading-tight">
                          <div className="truncate text-sm font-semibold text-slate-100">
                            {a.fullname}
                          </div>
                          <div className="truncate text-xs text-slate-400">
                            {line}
                          </div>
                        </div>
                      </li>
                    );
                  })}
                </ul>
              </div>
            }
          >
            <span
              tabIndex={0}
              aria-label={`+${overflow.length} more approvers`}
              className="grid h-10 w-10 shrink-0 cursor-default place-items-center rounded-full bg-slate-800 text-sm font-bold text-slate-300 ring-2 ring-slate-950 transition hover:bg-slate-700 hover:ring-slate-700 focus:outline-none focus:ring-slate-700"
            >
              +{overflow.length}
            </span>
          </TileTooltip>
        )}
      </div>
    </TileTooltipProvider>
  );
}