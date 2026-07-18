import React from 'react';
import Link from 'next/link';
import { redirect } from 'next/navigation';
import { headers } from 'next/headers';
import { Briefcase, Calendar, CircleAlert, FileText, type LucideIcon } from 'lucide-react';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { crumbsForPath } from '@/components/breadcrumbs/routes';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { listLeaveRequests } from '@/hr/server';
import type { LeaveRequestRow, LeaveStatus } from '@/hr/server';
import { Badge, Panel } from '@/components/ui';

export const dynamic = 'force-dynamic';

const TYPE_ICON: Record<string, LucideIcon> = {
  sick: CircleAlert,
  annual: Calendar,
  personal: Briefcase,
};

const TYPE_LABEL: Record<string, string> = {
  sick: 'hr.leave.typeSick',
  annual: 'hr.leave.typeAnnual',
  personal: 'hr.leave.typePersonal',
};

function statusTone(s: LeaveStatus): 'positive' | 'critical' | 'caution' {
  if (s === 'approved') return 'positive';
  if (s === 'rejected') return 'critical';
  return 'caution';
}

interface Group {
  key: LeaveStatus;
  labelKey: string;
  rows: LeaveRequestRow[];
}

export default async function HRLeavePage() {
  const h = await headers();
  const req = new Request('http://internal/hr/leave', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);
  const locale = await getSecondaryLocale();

  if (!out) {
    redirect('/login');
  }

  if (!hasPermission(out.session, PERM.hr.leave.read)) {
    redirect('/forbidden?path=/hr/leave&reason=hr:leave:read');
  }

  const requests = await listLeaveRequests();
  const grouped: Group[] = [
    {
      key: 'pending',
      labelKey: 'hr.leave.statusPending',
      rows: requests.filter((r) => r.status === 'pending'),
    },
    {
      key: 'approved',
      labelKey: 'hr.leave.statusApproved',
      rows: requests.filter((r) => r.status === 'approved'),
    },
    {
      key: 'rejected',
      labelKey: 'hr.leave.statusRejected',
      rows: requests.filter((r) => r.status === 'rejected'),
    },
  ];

  return (
    <>
      <BreadcrumbSetter crumbs={crumbsForPath('/hr/leave', locale)} />
      <PageLayout
        title={<T id="hr.leave.title" locale={locale} />}
        subtitle={<T id="hr.leave.subtitle" locale={locale} values={{ n: requests.length }} />}
      >
        <div className="space-y-6">
          {grouped.map((g) => (
            <section key={g.key} className="space-y-4">
              <h2 className="text-xs font-mono uppercase tracking-widest text-ink-2 flex items-center gap-2">
<Badge tone={statusTone(g.key)}>
                   <T id={g.labelKey} locale={locale} /> ({g.rows.length})
                 </Badge>
              </h2>
              {g.rows.length === 0 ? (
                <Panel padding="sm" className="text-sm text-mute italic">
                   <T id="hr.leave.empty" locale={locale} />
                 </Panel>
              ) : (
                <ul className="space-y-4">
                  {g.rows.map((r) => {
                     const IconCmp = TYPE_ICON[r.leave_type] ?? FileText;
                     const typeKey = TYPE_LABEL[r.leave_type] ?? 'hr.leave.typeOther';
                    return (
                      <li key={r.id}>
                        <Link
                          href={`/hr/leave/${r.id}`}
                          className="block rounded-md border border-rule bg-paper-2 p-4 transition-colors hover:bg-paper-3"
                        >
                          <div className="flex flex-wrap items-center justify-between gap-3">
                            <div className="min-w-0 flex-1 space-y-1">
                              <div className="flex flex-wrap items-center gap-2">
                                 <IconCmp size={17} className="text-accent" />
                                 <span className="font-semibold text-ink">{r.employee_name}</span>
                                 <span className="text-xs font-mono text-mute">· {r.employee_code}</span>
                              </div>
                              <div className="text-xs text-ink-2">
                                <T id={typeKey} locale={locale} /> ·{' '}
                                <span className="font-mono">{r.start_date} → {r.end_date}</span>{' '}
                                · <span className="text-accent">{r.days} <T id="hr.leave.days" locale={locale} /></span>
                              </div>
                              {r.reason && (
                                 <div className="text-xs text-mute italic truncate max-w-xl">
                                   {r.reason}
                                 </div>
                              )}
                            </div>
                             <Badge tone={statusTone(r.status)}>
                               <T id={g.labelKey} locale={locale} />
                             </Badge>
                          </div>
                        </Link>
                      </li>
                    );
                  })}
                </ul>
              )}
            </section>
          ))}
        </div>
      </PageLayout>
    </>
  );
}