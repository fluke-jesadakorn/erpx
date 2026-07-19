import 'server-only';
import { redirect } from 'next/navigation';
import { headers } from 'next/headers';
import { loadActor } from '@/server/guard';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { listActionCount, listUnreadCount, listUserNotifications, type NotificationReadFilter, type NotificationView } from '@/notifications/queries';
import { reconcileOpenActionsForUser } from '@/notifications/waybill';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { GROUP_LABEL } from '@/components/tile-config';
import { tileCrumbs } from '@/components/breadcrumbs';
import { NotificationInboxList } from '@/components/inbox/NotificationInboxList';
import { InboxFilters } from '@/components/inbox/InboxFilters';
import { getSecondaryLocale } from '@/server/locale';
import { T } from '@/components/i18n/TServer';

export const dynamic = 'force-dynamic';

interface Props {
  searchParams: Promise<{ view?: string; scope?: string; read?: string; domain?: string }>;
}

function viewOf(raw: string | undefined, scope: string | undefined): NotificationView {
  if (raw === 'actions' || raw === 'notifications') return raw;
  if (scope === 'waiting') return 'actions';
  if (scope === 'watching') return 'notifications';
  return 'all';
}

function readOf(raw: string | undefined): NotificationReadFilter {
  if (raw === 'unread' || raw === 'read') return raw;
  return 'all';
}

export default async function InboxPage({ searchParams }: Props) {
  const h = await headers();
  const out = await loadActivePermSession(new Request('http://internal', { headers: h as unknown as HeadersInit }));
  const locale = await getSecondaryLocale();
  if (!out || !hasPermission(out.session, PERM.tile.inbox.view)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Hub', href: '/' }, { label: 'Inbox', href: '/inbox' }]} />
        <PageLayout title={<T id="inbox.title" locale={locale} />} subtitle={out?.session.user.name ?? undefined}>
          <NoPermissionView kind="locked" actor={out ? out.session.user as any : null} attemptedPath="/inbox" reason={out ? 'tile:inbox:view required.' : 'Sign in to view this page.'} />
        </PageLayout>
      </>
    );
  }

  const actor = await loadActor();
  if (!actor) redirect('/login');
  await reconcileOpenActionsForUser(actor.id);
  const sp = await searchParams;
  const view = viewOf(sp.view, sp.scope);
  const read = readOf(sp.read);
  const domain = sp.domain === 'expense' || sp.domain === 'so' ? sp.domain : 'all';
  const [items, actions, unread] = await Promise.all([
    listUserNotifications(actor.id, 100, { view, read, domain, watchingOnly: sp.scope === 'watching' }),
    listActionCount(actor.id),
    listUnreadCount(actor.id),
  ]);

  return (
    <>
      <BreadcrumbSetter crumbs={tileCrumbs({ id: 'inbox', display_name: 'Inbox', sub_view: null, group_name: 'work' })} />
      <PageLayout
        title={<T id="inbox.title" locale={locale} />}
        subtitle={<T id="inbox.subtitle" locale={locale} values={{ name: actor.fullname, role: actor.role_name, waiting: actions, watching: unread }} />}
        category={{ label: GROUP_LABEL.work.label, icon: GROUP_LABEL.work.icon, href: '/group/work' }}
        width="wide"
      >
        <InboxFilters current={view} read={read} domain={domain} counts={{ actions, unread }} />
        <NotificationInboxList initialItems={items} />
      </PageLayout>
    </>
  );
}
