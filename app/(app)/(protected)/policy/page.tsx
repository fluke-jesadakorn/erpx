// /policy — full permission matrix (departments + specific roles × system perms).
import 'server-only';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { ROOT_CRUMB } from '@/components/breadcrumbs';
import { PermissionMatrix } from '@/components/policy/PermissionMatrix';
import { PolicyAdmin } from '@/components/policy/PolicyAdmin';
import { NoPermissionView } from '@/components/NoPermissionView';
import { T } from '@/components/i18n/TServer';
import { loadMatrixCells, loadMatrixColumns, loadMatrixTargets } from '@/policy/matrixRepo';
import { getSecondaryLocale } from '@/server/locale';

export const dynamic = 'force-dynamic';

function parseFlash(sp: URLSearchParams): { kind: 'ok' | 'err'; code: string; meta: Record<string, string> } | null {
  const ok = sp.get('ok');
  const err = sp.get('error');
  if (ok) {
    const meta: Record<string, string> = {};
    for (const [k, v] of sp.entries()) if (k !== 'ok' && k !== 'error') meta[k] = v;
    return { kind: 'ok', code: ok, meta };
  }
  if (err) {
    const meta: Record<string, string> = {};
    for (const [k, v] of sp.entries()) if (k !== 'ok' && k !== 'error') meta[k] = v;
    return { kind: 'err', code: err, meta };
  }
  return null;
}

export default async function PolicyPage({
  searchParams,
}: {
  searchParams?: Promise<Record<string, string | string[] | undefined>>;
}) {
  const h = await headers();
  const req = new Request('http://internal/policy', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);
  const locale = await getSecondaryLocale();
  const sp = (await (searchParams ?? Promise.resolve({}))) as Record<string, string | string[] | undefined>;
  const search = new URLSearchParams();
  for (const [k, v] of Object.entries(sp)) {
    if (typeof v === 'string') search.set(k, v);
  }
  const flash = parseFlash(search);

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

  const [columns, targets, cells, usersRes] = await Promise.all([
    loadMatrixColumns(),
    loadMatrixTargets(),
    loadMatrixCells(),
    fetchUserDirectory(),
  ]);
  const cellsObj: Record<string, string[]> = {};
  for (const [tid, set] of cells) cellsObj[tid] = Array.from(set);
  const canEdit = hasPermission(out.session, 'rbac:matrix:edit::allow');
  const canAssign =
    hasPermission(out.session, 'rbac:role:assign::allow') ||
    hasPermission(out.session, 'user:role:assign::allow');
  const actor = out.session.user;

  return (
    <>
      <BreadcrumbSetter crumbs={[ROOT_CRUMB, { label: <T id="nav.policy" locale={locale} /> }]} />
      <PageLayout
        title={<T id="policy.title" locale={locale} />}
        subtitle={<T id="policy.subtitle" locale={locale} />}
        density="compact"
        width="full"
      >
        <PermissionMatrix
          columns={columns}
          targets={targets}
          initialCells={cellsObj}
          canEdit={canEdit}
          actorName={(actor as any)?.fullname ?? (actor as any)?.id?.toString() ?? ''}
        />
        <div className="mt-4">
          <PolicyAdmin
            targets={targets}
            users={usersRes}
            canEdit={canEdit}
            canAssign={canAssign}
            flash={flash}
          />
        </div>
      </PageLayout>
    </>
  );
}

interface UserLite {
  id: number;
  fullname: string;
  employee_code: string;
  department: string | null;
  perm_role_ids: string[];
  perm_role_names: string[];
}

async function fetchUserDirectory(): Promise<UserLite[]> {
  const { query } = await import('@/db');
  const res = await query<UserLite>(
    `SELECT u.id, u.fullname, u.employee_code,
            (SELECT split_part(up.permission_id, ':', 3)
               FROM perm.user_permissions up
              WHERE up.user_id = u.id
                AND up.permission_id LIKE 'user:dept:%::allow'
                AND up.revoked_at IS NULL
                AND (up.ends_at IS NULL OR up.ends_at > now())
              ORDER BY up.permission_id LIMIT 1) AS department,
            COALESCE((SELECT array_agg(ur.role_id ORDER BY ur.role_id)
                        FROM perm.user_roles ur WHERE ur.user_id = u.id),
                      ARRAY[]::text[]) AS perm_role_ids,
            COALESCE((SELECT array_agg(pr.display_name ORDER BY pr.display_name)
                        FROM perm.user_roles ur JOIN perm.roles pr ON pr.id = ur.role_id
                       WHERE ur.user_id = u.id),
                      ARRAY[]::text[]) AS perm_role_names
       FROM users u
      WHERE u.is_active = true
      ORDER BY u.fullname`,
  );
  return res.rows;
}
