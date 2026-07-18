import React from 'react';
import Link from 'next/link';
import { Bell } from 'lucide-react';
import { getTranslations } from 'next-intl/server';
import { loadActor } from '@/server/guard';
import { listRecentNudgesForUser } from '@/waybill/nudge';

export async function NudgesLink() {
  const actor = await loadActor();
  if (!actor) return null;
  const nudges = await listRecentNudgesForUser(actor.id, 5);
  const count = nudges.length;
  const t = await getTranslations();

  const label = t('hub.nudgesLink');
  const titleText = t('hub.nudgesTitle');
  const ariaLabel = count > 0 ? `${label} (${count} ${t('hub.nudgesNew')})` : label;

  return (
    <Link
      href="/nudges"
      title={titleText}
      aria-label={ariaLabel}
      className="relative inline-flex h-9 w-9 items-center justify-center rounded-lg border border-rule text-ink-2 transition-colors hover:border-info hover:text-info"
    >
      <Bell className="h-4 w-4" aria-hidden />
      {count > 0 && (
        <span
          aria-hidden
          className="absolute -right-1 -top-1 inline-flex min-w-[1.1rem] items-center justify-center rounded-full border border-critical bg-critical px-1 text-[10px] font-mono leading-none text-ink"
        >
          {count > 9 ? '9+' : count}
        </span>
      )}
    </Link>
  );
}
