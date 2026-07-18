import Link from 'next/link';
import { headers } from 'next/headers';
import { Users } from 'lucide-react';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { crumbsForPath } from '@/components/breadcrumbs/routes';
import { NoPermissionView } from '@/components/NoPermissionView';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { listEmployees } from '@/hr/server';
import { Empty, Panel } from '@/components/ui';

export const dynamic = 'force-dynamic';

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

  return (
    <>
      <BreadcrumbSetter crumbs={crumbsForPath('/hr/employees', locale)} />
      <PageLayout
        title={<T id="hr.employees.title" locale={locale} />}
        subtitle={<T id="hr.employees.subtitle" locale={locale} values={{ n: employees.length }} />}
      >
        {employees.length === 0 ? (
          <Panel padding="md">
            <Empty icon={Users} title={<T id="hr.employees.empty" locale={locale} />} />
          </Panel>
        ) : (
          <div className="overflow-x-auto rounded-md border border-rule bg-paper-2">
            <table className="w-full text-left text-sm">
              <thead className="bg-paper-3 text-ink-2 text-xs font-mono uppercase tracking-wider">
                <tr>
                  <th className="px-4 py-3"><T id="hr.employees.colCode" locale={locale} /></th>
                  <th className="px-4 py-3"><T id="hr.employees.colName" locale={locale} /></th>
                  <th className="px-4 py-3"><T id="hr.employees.colDept" locale={locale} /></th>
                  <th className="px-4 py-3"><T id="hr.employees.colPosition" locale={locale} /></th>
                </tr>
              </thead>
              <tbody className="divide-y divide-rule">
                {employees.map((e) => (
                  <tr key={e.id} className="hover:bg-paper-3/40">
                    <td className="px-4 py-3 font-mono text-xs text-accent">{e.employee_code}</td>
                    <td className="px-4 py-3">
                      <Link href={`/hr/employees/${e.id}`} className="font-semibold text-ink hover:text-accent">
                        {e.name}
                      </Link>
                    </td>
                    <td className="px-4 py-3 text-mute">{e.department || '—'}</td>
                    <td className="px-4 py-3 text-mute">{e.position || '—'}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        )}
      </PageLayout>
    </>
  );
}