import 'server-only';
import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import { loadInboxForUser, type InboxScope } from '@/waybill/queries';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { GROUP_LABEL } from '@/components/tile-config';
import { tileCrumbs } from '@/components/breadcrumbs';
import { InboxFilters } from '@/components/inbox/InboxFilters';
import { InboxList } from '@/components/inbox/InboxList';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { NoPermissionView } from '@/components/NoPermissionView';

export const dynamic = 'force-dynamic';

interface Props {
  searchParams: Promise<{ scope?: string }>;
}

function normalizeScope(raw: string | undefined): InboxScope {
  if (raw === 'waiting') return 'waiting';
  if (raw === 'watching') return 'watching';
  if (raw === 'all') return 'all';
  return 'waiting';
}

export default async function InboxPage({ searchParams }: Props) {
  const h = await headers();
  const out = await loadActivePermSession(
    new Request('http://internal', { headers: h as unknown as HeadersInit }),
  );
  if (!out || !hasPermission(out.session, PERM.tile.inbox.view)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Hub', href: '/' }, { label: 'Inbox', href: '/inbox' }]} />
        <PageLayout title="Inbox" subtitle={out?.session.user.name ?? undefined}>
          <NoPermissionView
            kind="locked"
            actor={out ? (out.session.user as any) : null}
            attemptedPath="/inbox"
            reason={out ? 'tile:inbox:view required.' : 'Sign in to view this page.'}
          />
        </PageLayout>
      </>
    );
  }

  const actor = await loadActor();
  if (!actor) redirect('/login');

  const sp = await searchParams;
  const scope: InboxScope = normalizeScope(sp.scope);

  const [items, waitingItems, watchingItems] = await Promise.all([
    loadInboxForUser(actor.id, scope),
    loadInboxForUser(actor.id, 'waiting'),
    loadInboxForUser(actor.id, 'watching'),
  ]);

  const waitingLen = waitingItems.length;
  const watchingLen = watchingItems.length;

  const counts = {
    waiting: waitingLen,
    watching: watchingLen,
    all: items.length,
  };

  const subtitle = `${actor.fullname} · ${actor.role_name} · ${waitingLen} waiting · ${watchingLen} watching`;

  return (
    <>
      <BreadcrumbSetter crumbs={tileCrumbs({ id: 'inbox', display_name: 'Inbox', sub_view: null, group_name: 'work' })} />
      <PageLayout
        title="Inbox"
        subtitle={subtitle}
        category={{
          label: GROUP_LABEL.work.label,
          icon: GROUP_LABEL.work.icon,
          href: '/group/work',
        }}
      >
        <InboxFilters current={scope} counts={counts} />
        <InboxList scope={scope} items={items} />
      </PageLayout>
    </>
  );
}
