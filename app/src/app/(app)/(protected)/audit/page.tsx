// /audit — Audit tile page. Renders the audit log feed lifted from the old HR Console.

import 'server-only';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { tileCrumbs } from '@/components/breadcrumbs';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { headers } from 'next/headers';
import { T } from '@/components/i18n/T';
import { getTextServer } from '@/components/i18n/server';
import hrDict from '@folio-lib/i18n/hr';
import { tileFromRow, GROUP_LABEL, groupLabelBi } from '@/components/tile-config';
import { NoPermissionView } from '@/components/NoPermissionView';
import { query } from '@folio-lib/db';

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
            label: <T value={groupLabelBi(tile.group)} />,
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
    const auditTitle = getTextServer(hrDict, 'hr.audit', 'th');
    return (
      <PageLayout title={<T value={auditTitle} />} subtitle="Audit log feed">
        <div className="text-rose-400 text-sm">Audit tile is not configured.</div>
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
          label: <T value={groupLabelBi(tile.group)} />,
          icon: GROUP_LABEL[tile.group].icon,
          href: `/group/${tile.group}`,
        }}
      >
        <div className="text-slate-400 text-sm p-4">
          Audit log feed. Use the <code>/api/perm/audit</code> endpoint or
          query <code>perm.audit</code> directly.
        </div>
      </PageLayout>
    </>
  );
}
