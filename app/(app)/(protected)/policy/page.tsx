// /policy — full permission matrix (departments + specific roles × system perms).
import 'server-only';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { ROOT_CRUMB } from '@/components/breadcrumbs';
import { PermissionMatrix } from '@/components/policy/PermissionMatrix';
import { NoPermissionView } from '@/components/NoPermissionView';
import { T } from '@/components/i18n/TServer';
import { loadMatrixCells, loadMatrixColumns, loadMatrixTargets } from '@/policy/matrixRepo';
import { getSecondaryLocale } from '@/server/locale';

export const dynamic = 'force-dynamic';

export default async function PolicyPage() {
  const h = await headers();
  const req = new Request('http://internal/policy', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);
  const locale = await getSecondaryLocale();

  if (!out) {
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T id="nav.policy" locale={locale} /> }]} />
        <PageLayout
          title={<T id="policy.title" locale={locale} />}
          subtitle={<T id="policy.subtitle" locale={locale} />}
        >
          <NoPermissionView kind="locked" actor={null} attemptedPath="/policy" reason="Sign in to view this page." />
        </PageLayout>
      </>
    );
  }

  if (!hasPermission(out.session, 'rbac:matrix:view::allow')) {
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T id="nav.policy" locale={locale} /> }]} />
        <PageLayout
          title={<T id="policy.title" locale={locale} />}
          subtitle={<T id="policy.subtitle" locale={locale} />}
        >
          <NoPermissionView
            kind="locked"
            actor={out.session.user as any}
            attemptedPath="/policy"
            reason="rbac:matrix:view required."
          />
        </PageLayout>
      </>
    );
  }

  const [columns, targets, cells] = await Promise.all([
    loadMatrixColumns(),
    loadMatrixTargets(),
    loadMatrixCells(),
  ]);
  const cellsObj: Record<string, string[]> = {};
  for (const [tid, set] of cells) cellsObj[tid] = Array.from(set);
  const canEdit = hasPermission(out.session, 'rbac:matrix:edit::allow');
  const actor = out.session.user;

  return (
    <>
      <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T id="nav.policy" locale={locale} /> }]} />
      <PageLayout
        title={<T id="policy.title" locale={locale} />}
        subtitle={<T id="policy.subtitle" locale={locale} />}
        className="max-w-none px-4 py-3 md:px-6 md:py-4"
      >
        <PermissionMatrix
          columns={columns}
          targets={targets}
          initialCells={cellsObj}
          canEdit={canEdit}
          actorName={(actor as any)?.fullname ?? (actor as any)?.id?.toString() ?? ''}
        />
      </PageLayout>
    </>
  );
}
