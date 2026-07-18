// /audit — Audit tile page. Renders the audit log feed lifted from the old HR Console.

import 'server-only';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { tileCrumbs } from '@/components/breadcrumbs';
import { hasPermission, loadActivePermSession } from '@/perm/server';
import { headers } from 'next/headers';
import { T } from '@/components/i18n/TServer';
import { tileFromRow, GROUP_LABEL, groupLabelId } from '@/components/tile-config';
import { NoPermissionView } from '@/components/NoPermissionView';
import { query } from '@/db';
import { getSecondaryLocale } from '@/server/locale';

export const dynamic = 'force-dynamic';

async function loadTileRow(id: string) {
  const { rows } = await query<{
    id: string; display_name: string; subtitle: string; icon: string; accent: string;
    group_name: string; sub_view: string | null; href: string;
    view_perm_id: string;
    request_target: string | null; sort_order: number;
  }>(
    `SELECT id, display_name, COALESCE(subtitle,'') AS subtitle,
            COALESCE(icon,'square') AS icon, COALESCE(accent,'slate') AS accent,
            group_name, COALESCE(sub_view,'') AS sub_view, COALESCE(href,'') AS href,
            view_perm_id,
            COALESCE(request_target,'') AS request_target,
            sort_order
       FROM perm.tiles
       WHERE id = $1 OR href = $1 OR sub_view = $1
       LIMIT 1`,
    [id],
  );
  return rows[0] ?? null;
}

export default async function AuditPage() {
  const hdrs = await headers();
  const out = await loadActivePermSession(
    new Request('http://internal', { headers: hdrs as unknown as HeadersInit }),
  );
  const locale = await getSecondaryLocale();

  const tileRow = await loadTileRow('audit');
  const tile = tileRow ? tileFromRow(tileRow as never) : null;

  const actorName = out?.session.user.name ?? null;
  void actorName;

  if (!out || !hasPermission(out.session, 'tile:audit:view::allow')) {
    return (
      <>
        <BreadcrumbSetter crumbs={tile ? tileCrumbs(tile) : [{ label: 'Audit' }]} />
        <PageLayout
          title={tile?.display_name ?? 'Audit'}
          subtitle={tile?.subtitle ?? 'Audit log feed'}
          category={tile ? {
label: <T id={groupLabelId(tile.group)} locale={locale} />,
            icon: GROUP_LABEL[tile.group].icon,
            href: `/group/${tile.group}`,
          } : undefined}
        >
          <NoPermissionView
            kind="locked"
            actor={out ? (out.session.user as any) : null}
            tile={(tile as any) ?? null}
            attemptedPath="/audit"
            reason={out ? 'tile:audit:view required.' : 'Sign in to view this page.'}
          />
        </PageLayout>
      </>
    );
  }

  if (!tileRow || !tile) {
    return (
      <PageLayout title={<T id="hr.audit" locale={locale} />} subtitle={<T id="audit.subtitle" locale={locale} />}>
        <div className="text-critical text-sm">
          <T id="audit.notConfigured" locale={locale} />
        </div>
      </PageLayout>
    );
  }

  return (
    <>
      <BreadcrumbSetter crumbs={tileCrumbs(tile)} />
      <PageLayout
        title={tile.display_name}
        subtitle={tile.subtitle}
        category={{
          label: <T id={groupLabelId(tile.group)} locale={locale} />,
          icon: GROUP_LABEL[tile.group].icon,
          href: `/group/${tile.group}`,
        }}
      >
        <div className="text-ink-2 text-sm p-4">
          <T id="audit.feedHelp" locale={locale} values={{}} />
        </div>
      </PageLayout>
    </>
  );
}