import 'server-only';
import { headers } from 'next/headers';
import {
  listEmployees,
  listHRUsers,
} from '@folio-lib/hr/server';
import {
  listLeaveRequests,
  listLeaveStats,
  listDeptStats,
} from '@folio-lib/hr/server';
import { loadActor } from '@folio-lib/server/guard';
import { loadActivePermSession, hasPermission, PERM } from '@folio-lib/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { HRDashboardClient } from './_components/HRDashboardClient';
import { NoPermissionView } from '@/components/NoPermissionView';

export const dynamic = 'force-dynamic';

export default async function HRPage() {
  const h = await headers();
  const req = new Request('http://internal/hr', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);

  if (!out) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: 'HR' }]} />
        <PageLayout title="HR Dashboard" subtitle="Employees · Leave · Analytics">
          <NoPermissionView kind="locked" actor={null} attemptedPath="/hr" reason="Sign in to view this page." />
        </PageLayout>
      </>
    );
  }

  if (!hasPermission(out.session, PERM.hr.employee.read)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: 'HR' }]} />
        <PageLayout title="HR Dashboard" subtitle="Employees · Leave · Analytics">
          <NoPermissionView
            kind="locked"
            actor={out.session.user as any}
            attemptedPath="/hr"
            reason="hr:employee:read required."
          />
        </PageLayout>
      </>
    );
  }

  const actor = await loadActor();

  const [employees, hrUsers, requests, stats, deptStats] = await Promise.all([
    listEmployees(),
    listHRUsers(),
    listLeaveRequests(),
    listLeaveStats(),
    listDeptStats(),
  ]);

  return (
    <HRDashboardClient
      actorName={actor?.fullname ?? 'HR'}
      hrUsers={hrUsers}
      requests={requests}
      employees={employees}
      stats={stats}
      deptStats={deptStats}
    />
  );
}