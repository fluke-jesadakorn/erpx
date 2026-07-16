import React from 'react';
import type { SecondaryLocale } from '@/server/locale';
import type { ApproverRow } from '@/waybill/queries';
import { roleDisplayBi } from './ui';
import { ApproverStack } from './ApproverStack';
import { T } from '@/components/i18n/T';

export interface ActedUserLite {
  user_id: number;
  fullname: string;
  role_name: string | null;
  kind: string;
  sequence: number;
  occurred_at: Date;
}

interface Props {
  approvers: ApproverRow[];
  actedUsers?: ActedUserLite[];
  currentUserId: number | null;
  locale?: SecondaryLocale;
  tone?: 'cyan' | 'emerald' | 'rose' | 'indigo';
  title?: string | null;
}

export function ApproversList({
  approvers,
  actedUsers = [],
  currentUserId: _currentUserId,
  locale,
  tone = 'cyan',
  title = null,
}: Props) {
  const localeSafe: SecondaryLocale = locale ?? 'th';
  if (approvers.length === 0) {
    return (
      <div>
        {title !== null && (
          <div className="text-xs font-mono uppercase tracking-widest text-slate-500">
            {title}
          </div>
        )}
        <p className="mt-1 text-sm italic text-slate-500">
          <T id="waybill.approver.noEligible" />
        </p>
      </div>
    );
  }

  void _currentUserId;

  const gradientText = {
    cyan: 'from-cyan-200 to-indigo-200',
    emerald: 'from-emerald-200 to-cyan-200',
    rose: 'from-rose-200 to-rose-100',
    indigo: 'from-indigo-200 to-violet-200',
  }[tone];

  const headerTone = {
    cyan: 'text-cyan-300',
    emerald: 'text-emerald-300',
    rose: 'text-rose-300',
    indigo: 'text-indigo-300',
  }[tone];

  const visible = approvers.slice(0, 5);
  const first = visible[0];
  const second = visible[1];
  const distinctRoles = new Set(approvers.map((a) => a.role_id).filter(Boolean));
  const multiRole = distinctRoles.size > 1;

  return (
    <div>
      {title !== null && (
        <div className="flex items-baseline justify-between gap-2">
          <div className={`text-xs font-mono uppercase tracking-widest ${headerTone}`}>
            {title}
          </div>
          <div className="text-xs font-mono text-slate-500">
            {approvers.length}{' '}
            <T id={approvers.length === 1 ? 'waybill.approver.personOne' : 'waybill.approver.personMany'} />
          </div>
        </div>
      )}

      <div className="mt-2 flex items-center gap-3">
        <ApproverStack approvers={approvers} />
        <div className="min-w-0 flex-1 text-xs leading-tight text-slate-300">
          <p>
            <span className={`bg-gradient-to-r bg-clip-text font-semibold text-transparent ${gradientText}`}>
              {first?.fullname}
            </span>
            {second && (
              <>
                {', '}
                <span className="font-semibold text-slate-100">{second.fullname}</span>
              </>
            )}
            {approvers.length > 2 && (
              <>
                {' '}
                <span className="text-slate-400">
                  and {approvers.length - 2} other{approvers.length - 2 === 1 ? '' : 's'}
                </span>
              </>
            )}
          </p>
          <p className="mt-0.5 text-xs font-mono uppercase tracking-widest text-slate-500">
            {first
              ? multiRole
                ? (first.dept_group_name ?? '')
                : `${roleDisplayBi(first.role_id, localeSafe)}${first.dept_group_name ? ' · ' + first.dept_group_name : ''}`
              : ''}
          </p>
        </div>
      </div>

      {actedUsers.length > 0 && (
        <div className="mt-3 text-xs font-mono uppercase tracking-widest text-emerald-300/80">
          <span aria-hidden>✓</span>{' '}
          <T id="waybill.approver.nSigned" values={{ n: actedUsers.length }} />
        </div>
      )}
    </div>
  );
}
