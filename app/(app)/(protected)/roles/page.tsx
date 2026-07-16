import 'server-only';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission, parseRoleId, effectOf } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { T } from '@/components/i18n/TServer';
import { query } from '@/db';
import { getSecondaryLocale } from '@/server/locale';

export const dynamic = 'force-dynamic';

interface RoleRow {
  id: string;
  display_name: string;
  is_system: boolean;
  sort_order: number;
  allow_count: number;
  deny_count: number;
  user_count: number;
}

async function loadAll(): Promise<{ roles: RoleRow[]; permCount: number }> {
  const [rolesRes, grantsRes, permsRes, userCountRes] = await Promise.all([
    query<{ id: string; display_name: string; is_system: boolean; sort_order: number }>(
      `SELECT id, display_name, is_system, sort_order
         FROM perm.roles ORDER BY is_system DESC, sort_order, id`,
    ),
    query<{ role_id: string; permission_id: string }>(
      `SELECT role_id, permission_id FROM perm.role_permissions`,
    ),
    query<{ count: string }>(`SELECT COUNT(*)::text AS count FROM perm.permissions`),
    query<{ role_id: string; count: number }>(
      `SELECT role_id, COUNT(*)::int AS count FROM perm.user_roles GROUP BY role_id`,
    ),
  ]);
  const allowByRole = new Map<string, number>();
  const denyByRole = new Map<string, number>();
  for (const g of grantsRes.rows) {
    if (effectOf(g.permission_id) === 'deny') denyByRole.set(g.role_id, (denyByRole.get(g.role_id) ?? 0) + 1);
    else allowByRole.set(g.role_id, (allowByRole.get(g.role_id) ?? 0) + 1);
  }
  const userCountByRole = new Map<string, number>();
  for (const r of userCountRes.rows) userCountByRole.set(r.role_id, r.count);
  return {
    roles: rolesRes.rows.map((r) => ({
      ...r,
      allow_count: allowByRole.get(r.id) ?? 0,
      deny_count: denyByRole.get(r.id) ?? 0,
      user_count: userCountByRole.get(r.id) ?? 0,
    })),
    permCount: permsRes.rows[0] ? Number(permsRes.rows[0].count) : 0,
  };
}

export default async function RolesPage() {
  const h = await headers();
  const req = new Request('http://internal/roles', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);
  const session = out?.session;
  const locale = await getSecondaryLocale();
  if (!session) {
    return (
        <PageLayout title={<T id="auth.actorRequired" locale={locale} />}>
         <T id="auth.actorRedirect" locale={locale} />
      </PageLayout>
    );
  }
  if (!hasPermission(session, 'rbac:matrix:view::allow')) {
    return <NoPermissionView kind="locked" tile={null} attemptedPath="/roles" />;
  }
  const { roles, permCount } = await loadAll();
  return (
    <PageLayout title={<T id="nav.roles" locale={locale} />} subtitle={<T id="roles.subtitle" locale={locale} values={{ roles: roles.length, perms: permCount }} />}>
      <BreadcrumbSetter crumbs={[{ label: <T id="nav.roles" locale={locale} /> }]} />
      <div className="space-y-3">
        <table className="w-full text-sm border-separate border-spacing-y-1">
          <thead>
            <tr className="text-xs uppercase tracking-wider text-slate-400">
               <th className="text-left px-2 py-1"><T id="roles.role" locale={locale} /></th>
               <th className="text-left px-2 py-1"><T id="roles.level" locale={locale} /></th>
               <th className="text-right px-2 py-1"><T id="roles.allow" locale={locale} /></th>
               <th className="text-right px-2 py-1"><T id="roles.deny" locale={locale} /></th>
               <th className="text-right px-2 py-1"><T id="roles.users" locale={locale} /></th>
               <th className="text-right px-2 py-1"><T id="roles.system" locale={locale} /></th>
            </tr>
          </thead>
          <tbody>
            {roles.map((r) => {
              const parsed = parseRoleId(r.id);
              return (
                <tr key={r.id} className="bg-slate-900/50 border border-slate-700">
                  <td className="px-2 py-1.5 text-slate-100">
                    {r.display_name}
                    <span className="ml-2 font-mono text-xs text-slate-500">{r.id}</span>
                  </td>
                  <td className="px-2 py-1.5 text-slate-400 text-xs">{parsed?.level ?? '—'}</td>
                  <td className="px-2 py-1.5 text-right text-emerald-300">{r.allow_count}</td>
                  <td className="px-2 py-1.5 text-right text-rose-300">{r.deny_count}</td>
                  <td className="px-2 py-1.5 text-right text-cyan-300">{r.user_count}</td>
                  <td className="px-2 py-1.5 text-right text-xs text-slate-400">
                     {r.is_system ? <T id="roles.systemValue" locale={locale} /> : <T id="roles.customValue" locale={locale} />}
                  </td>
                </tr>
              );
            })}
          </tbody>
        </table>
        <div className="text-xs text-slate-500">
           <T id="roles.editHint" locale={locale} />
        </div>
      </div>
    </PageLayout>
  );
}