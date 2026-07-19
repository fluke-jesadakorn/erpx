import 'server-only';
import { headers } from 'next/headers';
import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { FullChat } from '@/components/chat/FullChat';
import { listSessions } from '@/chat/history';

export const dynamic = 'force-dynamic';

export default async function ChatPage() {
  const h = await headers();
  const session = await loadActivePermSession(
    new Request('http://internal/chat', { headers: h as unknown as HeadersInit }),
  );
  const actor = await loadActor();
  if (!session || !actor) redirect('/login');

  const allowed = hasPermission(session.session, 'tile:chat:view::allow');

  if (!allowed) {
    return (
      <>
        <BreadcrumbSetter
          crumbs={[
            { label: 'Folio', href: '/' },
            { label: 'AI Chat' },
          ]}
        />
        <PageLayout
          title="AI Chat"
        >
          <NoPermissionView
            kind="locked"
            actor={actor as any}
            attemptedPath="/chat"
            reason="tile:chat:view required."
          />
        </PageLayout>
      </>
    );
  }

  const sessions = await listSessions(actor.id);

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
          { label: 'Folio', href: '/' },
          { label: 'AI Chat' },
        ]}
      />
      <PageLayout
        density="compact"
        className="h-full flex flex-col"
        contentClassName="flex-1 min-h-0 flex flex-col"
        width="full"
      >
        <FullChat initialSessions={sessions} />
      </PageLayout>
    </>
  );
}
