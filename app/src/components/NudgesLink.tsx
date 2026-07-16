import React from 'react';
import Link from 'next/link';
import { Bell } from 'lucide-react';
import { loadActor } from '@folio-lib/server/guard';
import { listRecentNudgesForUser } from '@folio-lib/waybill/nudge';

export async function NudgesLink() {
  const actor = await loadActor();
  if (!actor) return null;
  const nudges = await listRecentNudgesForUser(actor.id, 5);
  const count = nudges.length;

  return (
    <Link
      href="/nudges"
      title="Approver nudges"
      aria-label={`Approver nudges${count > 0 ? ` (${count} new)` : ''}`}
      className="relative inline-flex h-9 w-9 items-center justify-center rounded-lg border border-slate-700 text-slate-400 transition-colors hover:border-cyan-500 hover:text-cyan-200"
    >
      <Bell className="h-4 w-4" aria-hidden />
      {count > 0 && (
        <span
          aria-hidden
          className="absolute -right-1 -top-1 inline-flex min-w-[1.1rem] items-center justify-center rounded-full border border-rose-500 bg-rose-500 px-1 text-[10px] font-mono leading-none text-white"
        >
          {count > 9 ? '9+' : count}
        </span>
      )}
    </Link>
  );
}
