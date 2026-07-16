import 'server-only';
import { notFound, redirect } from 'next/navigation';

const DEPRECATED_SLUGS = new Set(['my-waybills', 'my_waybills']);
import { PageLayout } from '@/components/PageLayout';
import {
  evaluateTile,
  loadActorAsSession,
  tileFlags,
  stageAllowMap,
} from '@/components/tileAccess.server';
import { tileFromRow, GROUP_LABEL, groupLabelId, type TileRow } from '@/components/tile-config';
import { T } from '@/components/i18n/TServer';
import { query } from '@/db';
import { hasPermission, loadActivePermSession } from '@/perm/server';
import { headers } from 'next/headers';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { ROOT_CRUMB, tileCrumbs } from '@/components/breadcrumbs';
import { getSecondaryLocale } from '@/server/locale';

interface Props {
  slug: string;
  actor: {
    id: number;
    role_name: string;
    fullname?: string;
    role_id?: string | null;
    [k: string]: unknown;
  };
}

async function getTileBySlugPerm(slug: string): Promise<TileRow | null> {
  const { rows } = await query<TileRow>(
    `SELECT id, display_name, COALESCE(subtitle,'') AS subtitle,
            COALESCE(icon,'square') AS icon, COALESCE(accent,'slate') AS accent,
            group_name,
            COALESCE(sub_view,'') AS sub_view, COALESCE(href,'') AS href,
            view_perm_id,
            COALESCE(request_target,'') AS request_target,
            sort_order
       FROM perm.tiles
       WHERE id = $1 OR href = $1 OR sub_view = $1
       LIMIT 1`,
    [slug],
  );
  return rows[0] || null;
}

const DIRECT: Record<string, string> = {
  org_chart: '/org-chart',
  roles: '/roles',
  directory: '/directory',
  departments: '/departments',
  access_requests: '/access-requests',
  audit: '/audit',
  policy: '/policy',
  tiles: '/tiles',
  settings: '/settings',
  hook: '/hook',
  expense: '/expense',
  inbox: '/inbox',
  pr: '/expense?scope=mine',
  po: '/expense?scope=queue',
  my_prs: '/inbox?scope=watching',
  sales: '/sales',
  customers: '/customers',
  reconciliation: '/reconciliation',
  team_manage: '/team-manage',
  cockpit: '/cockpit',
  ledger: '/ledger',
  search_coa: '/search-coa',
};

export async function SlugTile({ slug, actor }: Props) {
  if (DEPRECATED_SLUGS.has(slug)) notFound();
  const directKey = slug.replace(/[-_]/g, '_');
  if (directKey in DIRECT) redirect(DIRECT[directKey]);

  const tileRow = await getTileBySlugPerm(slug);
  if (!tileRow) {
    const locale = await getSecondaryLocale();
    return (
      <>
        <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: `/${slug}` }]} />
        <PageLayout
          title={<T id="chrome.notFoundTitle" locale={locale} />}
          subtitle={<T id="chrome.notFoundSubtitle" locale={locale} />}
        >
          <NoPermissionView
            kind="not_found"
            actor={actor as any}
            attemptedPath={`/${slug}`}
            reason={<T id="chrome.notFoundSubtitle" locale={locale} />}
          />
        </PageLayout>
      </>
    );
  }

  const tile = tileFromRow(tileRow);

  const permSession = await loadActorAsSession();
  const flags = tileFlags(permSession);
  const stageAllow = stageAllowMap(permSession);

  const hdrs = await headers();
  const out = await loadActivePermSession(
    new Request('http://internal', { headers: hdrs as unknown as HeadersInit }),
  );

  const viewPerm = tileRow.view_perm_id;
  const access = out && hasPermission(out.session, viewPerm)
    ? { state: 'open' as const, reason: 'Allowed by your role.' }
    : await evaluateTile(tile, actor);

  if (access.state === 'locked') {
    const locale = await getSecondaryLocale();
    return (
      <>
        <BreadcrumbSetter crumbs={[...tileCrumbs(tile), { label: <T id="chrome.lockedBadge" locale={locale} /> }]} />
        <PageLayout
          title={tile.display_name}
          subtitle={<T id="chrome.accessRestricted" locale={locale} />}
          category={{
            label: <T id={groupLabelId(tile.group)} locale={locale} />,
            icon: GROUP_LABEL[tile.group].icon,
            href: `/group/${tile.group}`,
          }}
        >
          <NoPermissionView
            kind="locked"
            actor={actor as any}
            tile={tile as any}
            access={access as any}
            attemptedPath={`/${slug}`}
          />
        </PageLayout>
      </>
    );
  }

  void flags;
  void stageAllow;
  const detailHref = tileRow.href || `/${slug}`;
  redirect(detailHref);
}