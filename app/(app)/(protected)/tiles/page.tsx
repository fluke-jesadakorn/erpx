import 'server-only';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { ROOT_CRUMB } from '@/components/breadcrumbs';
import { NoPermissionView } from '@/components/NoPermissionView';
import { query } from '@/db';
import { TilesClient } from './TilesClient';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

export const dynamic = 'force-dynamic';

interface TileRow {
  id: string;
  display_name: string;
  subtitle: string;
  icon: string;
  accent: string;
  group_name: string;
  href: string;
  view_perm_id: string;
  request_target: string | null;
  sort_order: number;
}

interface DeptRow {
  id: string;
  display_name: string;
  display_name_th: string | null;
  display_name_de: string | null;
}

async function loadAll(): Promise<{ tiles: TileRow[]; departments: DeptRow[] }> {
  const [tilesRes, deptsRes] = await Promise.all([
    query<TileRow>(
      `SELECT t.id, t.display_name, COALESCE(t.subtitle,'') AS subtitle,
              COALESCE(t.icon,'🧾') AS icon, COALESCE(t.accent,'slate') AS accent,
              t.group_name, COALESCE(t.href,'') AS href,
              t.view_perm_id,
              COALESCE(t.request_target,'') AS request_target,
              t.sort_order
         FROM perm.tiles t
         ORDER BY t.sort_order, t.id`,
    ),
    query<DeptRow>(
      `SELECT DISTINCT split_part(id, ':', 3) AS id, split_part(id, ':', 3) AS display_name,
              NULL::text AS display_name_th, NULL::text AS display_name_de
         FROM perm.permissions
         WHERE id LIKE 'user:dept:%::allow'
         ORDER BY id`,
    ),
  ]);
  return { tiles: tilesRes.rows, departments: deptsRes.rows };
}

export default async function TilesPage() {
  const h = await headers();
  const req = new Request('http://internal/tiles', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);
  const locale = await getSecondaryLocale();

  if (!out) {
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T id="tiles.pageLabel" locale={locale} /> }]} />
        <PageLayout title={<T id="tiles.pageTilesTitle" locale={locale} />} subtitle={<T id="tiles.pageTilesSubtitle" locale={locale} />}>
          <NoPermissionView kind="locked" actor={null} attemptedPath="/tiles" reason={<T id="permissions.pageTilesSignIn" locale={locale} />} />
        </PageLayout>
      </>
    );
  }

  if (!hasPermission(out.session, 'rbac:matrix:view::allow')) {
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T id="tiles.pageLabel" locale={locale} /> }]} />
        <PageLayout title={<T id="tiles.pageTilesTitle" locale={locale} />} subtitle={<T id="tiles.pageTilesSubtitle" locale={locale} />}>
          <NoPermissionView
            kind="locked"
            actor={out.session.user as { id: number; fullname?: string; name?: string }}
            attemptedPath="/tiles"
            reason={<T id="permissions.pageTilesViewRequired" locale={locale} />}
          />
        </PageLayout>
      </>
    );
  }

  const { tiles, departments } = await loadAll();
  const canEdit = hasPermission(out.session, 'rbac:matrix:edit::allow');

  return (
    <>
      <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T id="tiles.pageLabel" locale={locale} /> }]} />
      <PageLayout
        title={<T id="tiles.pageTilesTitle" locale={locale} />}
        subtitle={<T id="tiles.pageTilesSubtitle" locale={locale} />}
      >
        <TilesClient
          initialTiles={tiles}
          departments={departments}
          canEdit={canEdit}
        />
      </PageLayout>
    </>
  );
}