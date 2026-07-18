import 'server-only';
import { headers } from 'next/headers';
import { redirect } from 'next/navigation';
import Link from 'next/link';
import { Briefcase, Calendar, CircleAlert, type LucideIcon } from 'lucide-react';
import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { getEmployeeQuota, listLeave, type LeaveType } from '@/hr/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { crumbsForPath } from '@/components/breadcrumbs/routes';
import { NoPermissionView } from '@/components/NoPermissionView';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { LeaveRequestForm } from './LeaveRequestForm';
import { Panel, Badge, ListRow, Empty } from '@/components/ui';

export const dynamic = 'force-dynamic';

const LEAVE_META: Record<LeaveType, { icon: LucideIcon; labelId: string; tone: 'positive' | 'info' | 'caution' }> = {
  sick:     { icon: CircleAlert, labelId: 'me.leave.typeSick',     tone: 'positive' },
  annual:   { icon: Calendar,    labelId: 'me.leave.typeAnnual',   tone: 'info' },
  personal: { icon: Briefcase,   labelId: 'me.leave.typePersonal', tone: 'caution' },
};

interface StatusView {
  tone: 'positive' | 'critical' | 'caution';
  labelId: string;
}

const STATUS_META: Record<string, StatusView> = {
  open:      { tone: 'caution',  labelId: 'me.leave.statusPending' },
  completed: { tone: 'positive', labelId: 'me.leave.statusApproved' },
  rejected:  { tone: 'critical', labelId: 'me.leave.statusRejected' },
};

function statusView(status: string): StatusView {
  return STATUS_META[status] ?? STATUS_META.open;
}

export default async function MyLeavePage() {
  const h = await headers();
  const session = await loadActivePermSession(
    new Request('http://internal/me/leave', { headers: h as unknown as HeadersInit }),
  );
  const actor = await loadActor();
  if (!session || !actor) redirect('/login');

  const locale = await getSecondaryLocale();
  const allowed = hasPermission(session.session, 'tile:me_leave:view::allow');

  if (!allowed) {
    return (
      <>
        <BreadcrumbSetter crumbs={crumbsForPath('/me/leave', locale)} />
        <PageLayout title={<T id="me.leave.title" locale={locale} />}>
          <NoPermissionView
            kind="locked"
            actor={actor}
            attemptedPath="/me/leave"
            reason={<T id="me.leave.permissionRequired" locale={locale} />}
          />
        </PageLayout>
      </>
    );
  }

  const [quota, history] = await Promise.all([
    getEmployeeQuota(actor.id),
    listLeave({ employeeId: actor.id, limit: 50 }),
  ]);

  const subtitle = (
    <T id="me.leave.subtitle" locale={locale} values={{ name: actor.fullname }} />
  );

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
          { label: <T id="nav.home" locale={locale} />, href: '/' },
          { label: <T id="me.leave.title" locale={locale} /> },
        ]}
      />
      <PageLayout title={<T id="me.leave.title" locale={locale} />} subtitle={subtitle}>
        <div className="space-y-6">
          <Panel>
            <h2 className="mb-3 text-sm font-mono uppercase tracking-wider text-mute">
              <T id="me.leave.quotaTitle" locale={locale} />
            </h2>
            {quota ? (
              <div className="grid grid-cols-1 gap-4 md:grid-cols-3">
                <QuotaCard
                  locale={locale}
                  labelId="me.leave.typeSick"
                  icon={LEAVE_META.sick.icon}
                  tone={LEAVE_META.sick.tone}
                  total={quota.sick.total}
                  used={quota.sick.used}
                  remaining={quota.sick.remaining}
                />
                <QuotaCard
                  locale={locale}
                  labelId="me.leave.typeAnnual"
                  icon={LEAVE_META.annual.icon}
                  tone={LEAVE_META.annual.tone}
                  total={quota.annual.total}
                  used={quota.annual.used}
                  remaining={quota.annual.remaining}
                />
                <QuotaCard
                  locale={locale}
                  labelId="me.leave.typePersonal"
                  icon={LEAVE_META.personal.icon}
                  tone={LEAVE_META.personal.tone}
                  total={quota.personal.total}
                  used={quota.personal.used}
                  remaining={quota.personal.remaining}
                />
              </div>
            ) : (
              <p className="text-sm italic text-mute">
                <T id="me.leave.noQuota" locale={locale} />
              </p>
            )}
          </Panel>

          <Panel className="space-y-4">
            <h2 className="text-sm font-mono uppercase tracking-wider text-mute">
              <T id="me.leave.requestTitle" locale={locale} />
            </h2>
            <LeaveRequestForm />
          </Panel>

          <Panel className="space-y-4">
            <h2 className="text-sm font-mono uppercase tracking-wider text-mute">
              <T id="me.leave.historyTitle" locale={locale} />
            </h2>
            {history.length === 0 ? (
              <Empty
                title={<T id="me.leave.historyEmpty" locale={locale} />}
                body="Submitted leave requests will appear here."
              />
            ) : (
              <ul>
                {history.map((row: any) => {
                  const meta: { icon: LucideIcon; labelId: string; tone: 'positive' | 'info' | 'caution' } = LEAVE_META[row.leave_type as LeaveType] ?? LEAVE_META.annual;
                  const sv = statusView(row.status);
                  const d = typeof row.days === 'string' ? parseFloat(row.days) : row.days;
                  return (
                    <ListRow key={row.waybill_id}>
                      <meta.icon size={20} className="shrink-0 text-mute" />
                      <div className="min-w-0 flex-1">
                        <div className="flex flex-wrap items-baseline gap-2">
                          <T id={meta.labelId} locale={locale} />
                          <span className="font-mono text-xs text-ink-2">
                            {row.start_date} → {row.end_date}
                          </span>
                          <span className="text-xs text-mute">· {d} <T id="me.leave.days" locale={locale} /></span>
                        </div>
                        {row.reason && (
                          <p className="mt-0.5 truncate text-xs text-ink-2">{row.reason}</p>
                        )}
                      </div>
                      <Badge tone={sv.tone}>
                        <T id={sv.labelId} locale={locale} />
                      </Badge>
                      <Link
                        href={`/hr/leave/${row.waybill_id}`}
                        className="shrink-0 text-xs text-accent hover:underline"
                      >
                        <T id="me.leave.viewDetail" locale={locale} />
                      </Link>
                    </ListRow>
                  );
                })}
              </ul>
            )}
          </Panel>
        </div>
      </PageLayout>
    </>
  );
}

function QuotaCard({
  locale,
  labelId,
  icon: IconCmp,
  tone,
  total,
  used,
  remaining,
}: {
  locale: 'th' | 'de';
  labelId: string;
  icon: LucideIcon;
  tone: 'positive' | 'info' | 'caution';
  total: number;
  used: number;
  remaining: number;
}) {
  const pct = total > 0 ? (remaining / total) * 100 : 0;
  const barTone = tone === 'positive' ? 'bg-positive' : tone === 'info' ? 'bg-info' : 'bg-caution';
  return (
    <div className="space-y-2 rounded-md border border-rule bg-paper-2 p-3.5">
      <div className="flex items-center justify-between gap-2 text-sm">
        <span className="flex items-center gap-1.5 font-semibold text-ink-2">
          <IconCmp size={15} className="text-mute" />
          <T id={labelId} locale={locale} />
        </span>
        <span className="font-mono text-ink-2">
          {remaining} / {total} <T id="me.leave.days" locale={locale} />
        </span>
      </div>
      <div className="h-2 w-full overflow-hidden rounded-full bg-rule">
        <div className={`h-full ${barTone} rounded-full`} style={{ width: `${Math.max(0, Math.min(100, pct))}%` }} />
      </div>
      <div className="text-xs text-mute">
        <T id="me.leave.used" locale={locale} values={{ used }} />
      </div>
    </div>
  );
}
