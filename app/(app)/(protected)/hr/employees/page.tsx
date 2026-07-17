import Link from 'next/link';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { crumbsForPath } from '@/components/breadcrumbs/routes';
import { NoPermissionView } from '@/components/NoPermissionView';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { listEmployees } from '@/hr/server';
import type { EmployeeRow } from '@/hr/server';
import { Panel, Empty } from '@/components/ui';

export const dynamic = 'force-dynamic';

interface QuotaCellProps {
  total: number;
  used: number;
  color: string;
}

function QuotaCell({ total, used, color }: QuotaCellProps) {
  const remaining = Math.max(0, total - used);
  const pct = total > 0 ? Math.max(0, Math.min(100, (remaining / total) * 100)) : 0;
  return (
    <div className="space-y-1 min-w-[120px]">
      <div className="flex justify-between text-sm font-mono">
        <span className="text-mute">{used}/{total}</span>
        <span className="text-ink-2">{remaining}</span>
      </div>
      <div className="h-1.5 w-full bg-paper-3 rounded-full overflow-hidden">
        <div className={'h-full rounded-full ' + color} style={{ width: `${pct}%` }} />
      </div>
    </div>
  );
}

export default async function HREmployeesPage() {
  const h = await headers();
  const req = new Request('http://internal/hr/employees', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);
  const locale = await getSecondaryLocale();

  if (!out) {
    return (
      <>
        <BreadcrumbSetter crumbs={crumbsForPath('/hr/employees', locale)} />
        <PageLayout title={<T id="hr.employees.title" locale={locale} />}>
          <NoPermissionView kind="locked" actor={null} attemptedPath="/hr/employees" reason="Sign in to view this page." />
        </PageLayout>
      </>
    );
  }

  if (!hasPermission(out.session, PERM.hr.employee.read)) {
    return (
      <>
        <BreadcrumbSetter crumbs={crumbsForPath('/hr/employees', locale)} />
        <PageLayout title={<T id="hr.employees.title" locale={locale} />}>
          <NoPermissionView
            kind="locked"
            actor={out.session.user as never}
            attemptedPath="/hr/employees"
            reason="hr:employee:read required."
          />
        </PageLayout>
      </>
    );
  }

  const employees = await listEmployees();

  const rows: EmployeeRow[] = employees ?? [];

  return (
    <>
      <BreadcrumbSetter crumbs={crumbsForPath('/hr/employees', locale)} />
      <PageLayout
        title={<T id="hr.employees.title" locale={locale} />}
        subtitle={<T id="hr.employees.subtitle" locale={locale} values={{ n: rows.length }} />}
      >
        <Panel padding="none" className="overflow-x-auto">
          <table className="w-full text-left border-collapse text-sm">
            <thead>
              <tr className="bg-paper-3 text-xs font-mono uppercase tracking-wider text-mute border-b border-rule">
                <th className="px-4 py-3"><T id="hr.employees.colCode" locale={locale} /></th>
                <th className="px-4 py-3"><T id="hr.employees.colName" locale={locale} /></th>
                <th className="px-4 py-3"><T id="hr.employees.colDept" locale={locale} /></th>
                <th className="px-4 py-3"><T id="hr.employees.colPosition" locale={locale} /></th>
                <th className="px-4 py-3"><T id="hr.employees.colSick" locale={locale} /></th>
                <th className="px-4 py-3"><T id="hr.employees.colAnnual" locale={locale} /></th>
                <th className="px-4 py-3"><T id="hr.employees.colPersonal" locale={locale} /></th>
              </tr>
            </thead>
            <tbody className="divide-y divide-rule text-ink">
              {rows.length === 0 ? (
                <tr>
                  <td colSpan={7}>
                    <Empty icon="users" title={<T id="hr.employees.empty" locale={locale} />} />
                  </td>
                </tr>
              ) : (
                rows.map((e) => (
                  <tr key={e.id} className="hover:bg-paper-3">
                    <td className="px-4 py-3 font-mono text-xs text-accent">{e.employee_code}</td>
                    <td className="px-4 py-3"><Link href={`/hr/employees/${e.id}`} className="font-semibold text-ink hover:text-accent">{e.fullname}</Link></td>
                    <td className="px-4 py-3 text-mute">{e.dept_label ?? '—'}</td>
                    <td className="px-4 py-3 text-mute">{e.position}</td>
                    <td className="px-4 py-3">
                      <QuotaCell
                        total={e.total_sick_leave}
                        used={e.used_sick_leave}
                        color="bg-positive"
                      />
                    </td>
                    <td className="px-4 py-3">
                      <QuotaCell
                        total={e.total_annual_leave}
                        used={e.used_annual_leave}
                        color="bg-info"
                      />
                    </td>
                    <td className="px-4 py-3">
                      <QuotaCell
                        total={e.total_personal_leave}
                        used={e.used_personal_leave}
                        color="bg-caution"
                      />
                    </td>
                  </tr>
                ))
              )}
            </tbody>
          </table>
        </Panel>
      </PageLayout>
    </>
  );
}