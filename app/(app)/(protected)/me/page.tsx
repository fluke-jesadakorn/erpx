import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { headers } from 'next/headers';
import Link from 'next/link';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { crumbsForPath } from '@/components/breadcrumbs/routes';
import { NoPermissionView } from '@/components/NoPermissionView';
import { query } from '@/db';
import { Panel, Badge, Avatar, Empty } from '@/components/ui';
import { ListRow } from '@/components/ui/ListRow';

export const dynamic = 'force-dynamic';

interface ProfileRow {
  id: number;
  fullname: string;
  employee_code: string | null;
  email: string | null;
  position: string | null;
  department: string | null;
  dept_label: string | null;
  hired_at: string | null;
  is_active: boolean | null;
}

const PERMS: ReadonlyArray<{ id: string; tone: 'positive' | 'critical' | 'caution' | 'info' | 'neutral' | 'accent' }> = [
  { id: 'profile.view', tone: 'info' },
  { id: 'profile.edit', tone: 'caution' },
  { id: 'me.leave.request', tone: 'positive' },
  { id: 'me.leave.view', tone: 'neutral' },
];

export default async function MePage() {
  const h = await headers();
  const req = new Request('http://internal/me', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);

  if (!out) {
    return (
      <>
        <BreadcrumbSetter crumbs={crumbsForPath('/me', 'th')} />
        <PageLayout title="Me">
          <NoPermissionView kind="locked" actor={null} attemptedPath="/me" reason="Sign in to view this page." />
        </PageLayout>
      </>
    );
  }

  const allowed = hasPermission(out.session, 'tile:me_leave:view::allow');
  if (!allowed) {
    return (
      <>
        <BreadcrumbSetter crumbs={crumbsForPath('/me', 'th')} />
        <PageLayout title="Me" subtitle={out.session.user.name ?? undefined}>
          <NoPermissionView
            kind="locked"
            actor={out.session.user as never}
            attemptedPath="/me"
            reason="tile:me_leave:view required."
          />
        </PageLayout>
      </>
    );
  }

  const actor = await loadActor();
  const profile = actor
    ? await query<ProfileRow>(
        `SELECT id, fullname, employee_code, NULLIF(LOWER(u.email),'') AS email, position,
                department, dept_label, hired_at, is_active
           FROM users u WHERE id = $1`,
        [actor.id],
      ).then((r) => r.rows[0] ?? null).catch(() => null)
    : null;
  const canSubmit = hasPermission(out.session, 'me.leave:submit::allow');
  const canSeeLeave = hasPermission(out.session, 'tile:me_leave:view::allow');
  const canEditProfile = hasPermission(out.session, 'user:self:edit::allow');

  return (
    <>
      <BreadcrumbSetter crumbs={crumbsForPath('/me', 'th')} />
      <PageLayout
        title="Me"
        subtitle={profile?.fullname ?? out.session.user.name ?? undefined}
      >
        <Panel>
          <div className="flex items-center gap-4">
            <Avatar name={profile?.fullname ?? 'You'} size={56} tone="accent" />
            <div className="min-w-0">
              <h2 className="text-base font-semibold text-ink">{profile?.fullname ?? '—'}</h2>
              <p className="text-sm text-ink-2">
                {profile?.position ?? '—'}
                {profile?.dept_label ? <> · {profile.dept_label}</> : null}
              </p>
              <p className="font-mono text-xs text-mute">{profile?.employee_code ?? '—'}</p>
            </div>
            <div className="ml-auto flex flex-wrap items-center gap-2">
              {profile?.is_active === false && <Badge tone="neutral">Inactive</Badge>}
              <Badge tone="accent">{actor?.role_name ?? '—'}</Badge>
              {canEditProfile && <Badge tone="info">Editable</Badge>}
            </div>
          </div>
          <dl className="mt-6 grid grid-cols-1 gap-4 text-sm sm:grid-cols-2">
            <div>
              <dt className="text-xs font-mono uppercase tracking-wider text-mute">Email</dt>
              <dd className="mt-1 text-ink">{profile?.email ?? '—'}</dd>
            </div>
            <div>
              <dt className="text-xs font-mono uppercase tracking-wider text-mute">Hired</dt>
              <dd className="mt-1 text-ink">{profile?.hired_at ?? '—'}</dd>
            </div>
            <div>
              <dt className="text-xs font-mono uppercase tracking-wider text-mute">Department</dt>
              <dd className="mt-1 text-ink">{profile?.dept_label ?? profile?.department ?? '—'}</dd>
            </div>
            <div>
              <dt className="text-xs font-mono uppercase tracking-wider text-mute">Position</dt>
              <dd className="mt-1 text-ink">{profile?.position ?? '—'}</dd>
            </div>
          </dl>
        </Panel>

        <div className="mt-6 grid gap-6 lg:grid-cols-3">
          <Panel>
            <h2 className="text-sm font-semibold text-ink">Permissions</h2>
            <p className="mt-1 text-sm text-ink-2">Self-service capabilities currently active for your account.</p>
            <ul className="mt-4">
              {PERMS.map((p) => {
                const has = hasPermission(out.session, `${p.id}::allow`);
                return (
                  <ListRow key={p.id} className="justify-between text-sm">
                    <span className="font-mono text-xs text-ink-2">{p.id}</span>
                    <Badge tone={has ? p.tone : 'neutral'}>{has ? 'allowed' : 'denied'}</Badge>
                  </ListRow>
                );
              })}
            </ul>
          </Panel>

          <Panel>
            <h2 className="text-sm font-semibold text-ink">Quick actions</h2>
            <p className="mt-1 text-sm text-ink-2">Open the actions available to you without leaving this page.</p>
            <div className="mt-4 space-y-4">
              {canSeeLeave ? (
                <Link href="/me/leave" className="flex items-center justify-between rounded-md border border-rule bg-paper-2 px-3 py-2 text-sm font-medium text-ink hover:bg-paper-3">
                  <span>My leave</span>
                  <span className="font-mono text-mute">{canSubmit ? 'submit + history' : 'history only'}</span>
                </Link>
              ) : (
                <Empty title="No self-service actions" />
              )}
              <Link href="/customers" className="flex items-center justify-between rounded-md border border-rule bg-paper-2 px-3 py-2 text-sm font-medium text-ink hover:bg-paper-3">
                <span>Customers</span>
                <span className="font-mono text-mute">directory</span>
              </Link>
            </div>
          </Panel>

          <Panel>
            <h2 className="text-sm font-semibold text-ink">Session</h2>
            <p className="mt-1 text-sm text-ink-2">Active session context for the current Folio role.</p>
            <dl className="mt-4 space-y-4 text-sm">
              <div>
                <dt className="text-xs font-mono uppercase tracking-wider text-mute">Role</dt>
                <dd className="mt-1 text-ink">{actor?.role_name ?? '—'}</dd>
              </div>
              <div>
                <dt className="text-xs font-mono uppercase tracking-wider text-mute">Level</dt>
                <dd className="mt-1 text-ink">{actor?.level ?? '—'}</dd>
              </div>
              <div>
                <dt className="text-xs font-mono uppercase tracking-wider text-mute">Department</dt>
                <dd className="mt-1 text-ink">{actor?.dept_group_name ?? '—'}</dd>
              </div>
            </dl>
          </Panel>
        </div>
      </PageLayout>
    </>
  );
}
