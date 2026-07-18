'use client';

import { useMemo, useState } from 'react';
import { Briefcase, Calendar, CircleAlert, Send, type LucideIcon } from 'lucide-react';
import { T } from '@/components/i18n/T';
import { submitMyLeaveAction } from './_actions';

const TYPE_OPTIONS: ReadonlyArray<{ value: 'sick' | 'annual' | 'personal'; labelId: string; icon: LucideIcon }> = [
  { value: 'sick', labelId: 'me.leave.typeSick', icon: CircleAlert },
  { value: 'annual', labelId: 'me.leave.typeAnnual', icon: Calendar },
  { value: 'personal', labelId: 'me.leave.typePersonal', icon: Briefcase },
];

function calcDays(start: string, end: string): number {
  if (!start || !end) return 0;
  const s = new Date(start);
  const e = new Date(end);
  if (Number.isNaN(s.getTime()) || Number.isNaN(e.getTime())) return 0;
  if (e < s) return 0;
  const ms = e.getTime() - s.getTime();
  return Math.round(ms / 86400000) + 1;
}

export function LeaveRequestForm() {
  const [leaveType, setLeaveType] = useState<'sick' | 'annual' | 'personal'>('annual');
  const [startDate, setStartDate] = useState('');
  const [endDate, setEndDate] = useState('');
  const [reason, setReason] = useState('');
  const days = useMemo(() => calcDays(startDate, endDate), [startDate, endDate]);

  return (
    <form action={submitMyLeaveAction} className="space-y-4">
      <div className="grid grid-cols-1 gap-4 md:grid-cols-2">
        <Field label={<T id="me.leave.formLeaveType" />}>
          <select
            name="leaveType"
            value={leaveType}
            onChange={(e) => setLeaveType(e.target.value as 'sick' | 'annual' | 'personal')}
            className="w-full rounded-md border border-rule bg-paper px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none"
          >
            {TYPE_OPTIONS.map((option) => (
              <option key={option.value} value={option.value}>
                {option.labelId}
              </option>
            ))}
          </select>
        </Field>

        <Field label={<T id="me.leave.formStartDate" />}>
          <input
            type="date"
            name="startDate"
            value={startDate}
            onChange={(e) => setStartDate(e.target.value)}
            required
            className="w-full rounded-md border border-rule bg-paper px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none"
          />
        </Field>

        <Field label={<T id="me.leave.formEndDate" />}>
          <input
            type="date"
            name="endDate"
            value={endDate}
            min={startDate || undefined}
            onChange={(e) => setEndDate(e.target.value)}
            required
            className="w-full rounded-md border border-rule bg-paper px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none"
          />
        </Field>

        <Field label={<T id="me.leave.formDays" />}>
          <input
            type="number"
            name="days"
            value={days || ''}
            readOnly
            min={1}
            className="w-full rounded-md border border-rule bg-paper-2 px-3 py-2 font-mono text-sm text-ink-2"
          />
        </Field>
      </div>

      <Field label={<T id="me.leave.formReason" />}>
        <textarea
          name="reason"
          value={reason}
          onChange={(e) => setReason(e.target.value)}
          rows={3}
          className="w-full resize-none rounded-md border border-rule bg-paper px-3 py-2 text-sm text-ink focus:border-accent focus:outline-none"
        />
      </Field>

      <div className="flex items-center justify-end gap-3 pt-1">
        <button
          type="submit"
          disabled={!days}
          className="inline-flex items-center gap-2 rounded-md border border-accent/40 bg-accent px-4 py-2 text-sm font-medium text-accent-ink disabled:cursor-not-allowed disabled:opacity-40 hover:bg-accent-strong"
        >
          <Send size={15} />
          <T id="me.leave.formSubmit" />
        </button>
      </div>
    </form>
  );
}

function Field({ label, children }: { label: React.ReactNode; children: React.ReactNode }) {
  return (
    <label className="block space-y-1.5">
      <span className="text-xs font-mono uppercase tracking-wider text-mute">
        {label}
      </span>
      {children}
    </label>
  );
}
