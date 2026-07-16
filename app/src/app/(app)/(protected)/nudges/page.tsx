import 'server-only';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission, PERM } from '@folio-lib/perm/server';
import { listRecentNudgesForUser } from '@folio-lib/waybill/nudge';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { NudgesPanel } from '@/components/waybill/NudgesPanel';

export const dynamic = 'force-dynamic';

export default async function NudgesPage() {
  const h = await headers();
  const out = await loadActivePermSession(
    new Request('http://internal/nudges', { headers: h as unknown as HeadersInit }),
  );
  if (!out || !hasPermission(out.session, PERM.tile.inbox.view)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Hub', href: '/' }, { label: 'Nudges' }]} />
        <PageLayout title="Approver nudges" subtitle={out?.session.user.name ?? undefined}>
          <NoPermissionView
            kind="locked"
            actor={out ? (out.session.user as any) : null}
            attemptedPath="/nudges"
            reason={out ? 'tile:inbox:view required.' : 'Sign in to view this page.'}
          />
        </PageLayout>
      </>
    );
  }

  const actorId = (out.session.user as any).id;
  const nudges = await listRecentNudgesForUser(actorId, 20);

  return (
    <>
      <BreadcrumbSetter crumbs={[{ label: 'Hub', href: '/' }, { label: 'Nudges' }]} />
      <PageLayout title="Approver nudges" subtitle="AI one-liners for waybills idle > 8h">
        <NudgesPanel initial={nudges} lang="en" />
      </PageLayout>
    </>
  );
}