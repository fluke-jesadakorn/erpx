import 'server-only';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission } from '@folio-lib/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { ROOT_CRUMB } from '@/components/breadcrumbs';
import { NoPermissionView } from '@/components/NoPermissionView';
import { query } from '@folio-lib/db';
import { TilesClient } from './TilesClient';
import { T } from '@/components/i18n/T';
import { getTextServer } from '@/components/i18n/server';
import type { BilingualText } from '@folio-lib/i18n/types';
import { getSecondaryLocale } from '@folio-lib/server/locale';
import tilesDict from '@folio-lib/i18n/tiles';
import permissionsDict from '@folio-lib/i18n/permissions';

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

  const labelTiles: BilingualText = getTextServer(tilesDict, 'tile.page.label', locale);
  const titleTiles: BilingualText = getTextServer(tilesDict, 'tile.page.tiles.title', locale);
  const subtitleTiles: BilingualText = getTextServer(tilesDict, 'tile.page.tiles.subtitle', locale);
  const signInReason: BilingualText = getTextServer(permissionsDict, 'permissions.page.tiles.sign_in', locale);
  const viewRequiredReason: BilingualText = getTextServer(permissionsDict, 'permissions.page.tiles.view_required', locale);

  if (!out) {
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T value={labelTiles} /> }]} />
        <PageLayout title={<T value={titleTiles} />} subtitle={<T value={subtitleTiles} />}>
          <NoPermissionView kind="locked" actor={null} attemptedPath="/tiles" reason={<T value={signInReason} />} />
        </PageLayout>
      </>
    );
  }

  if (!hasPermission(out.session, 'rbac:matrix:view::allow')) {
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T value={labelTiles} /> }]} />
        <PageLayout title={<T value={titleTiles} />} subtitle={<T value={subtitleTiles} />}>
          <NoPermissionView
            kind="locked"
            actor={out.session.user as { id: number; fullname?: string; name?: string }}
            attemptedPath="/tiles"
            reason={<T value={viewRequiredReason} />}
          />
        </PageLayout>
      </>
    );
  }

  const { tiles, departments } = await loadAll();
  const canEdit = hasPermission(out.session, 'rbac:matrix:edit::allow');

  return (
    <>
      <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T value={labelTiles} /> }]} />
      <PageLayout
        title={<T value={titleTiles} />}
        subtitle={<T value={subtitleTiles} />}
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