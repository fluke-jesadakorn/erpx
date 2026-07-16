import 'server-only';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { listRecentNudgesForUser } from '@/waybill/nudge';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { NudgesPanel } from '@/components/waybill/NudgesPanel';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

export const dynamic = 'force-dynamic';

export default async function NudgesPage() {
  const h = await headers();
  const out = await loadActivePermSession(
    new Request('http://internal/nudges', { headers: h as unknown as HeadersInit }),
  );
  const locale = await getSecondaryLocale();
  if (!out || !hasPermission(out.session, PERM.tile.inbox.view)) {
    return (
      <>
         <BreadcrumbSetter crumbs={[{ label: <T id="nav.home" locale={locale} />, href: '/' }, { label: <T id="waybill.nudges.title" locale={locale} /> }]} />
         <PageLayout title={<T id="waybill.nudges.title" locale={locale} />} subtitle={out?.session.user.name ?? undefined}>
          <NoPermissionView
            kind="locked"
            actor={out ? (out.session.user as any) : null}
            attemptedPath="/nudges"
             reason={out ? <T id="permissions.inboxRequired" locale={locale} /> : <T id="access.signInBody" locale={locale} />}
          />
        </PageLayout>
      </>
    );
  }

  const actorId = (out.session.user as any).id;
  const nudges = await listRecentNudgesForUser(actorId, 20);

  return (
    <>
       <BreadcrumbSetter crumbs={[{ label: <T id="nav.home" locale={locale} />, href: '/' }, { label: <T id="waybill.nudges.title" locale={locale} /> }]} />
       <PageLayout title={<T id="waybill.nudges.title" locale={locale} />} subtitle={<T id="waybill.nudges.subtitle" locale={locale} />}>
        <NudgesPanel initial={nudges} lang="en" />
      </PageLayout>
    </>
  );
}