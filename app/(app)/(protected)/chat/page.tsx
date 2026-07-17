import 'server-only';
import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import { matchPerm } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { FullChat } from '@/components/chat/FullChat';
import { listSessions } from '@/chat/history';

export const dynamic = 'force-dynamic';

export default async function ChatPage() {
  const actor = await loadActor();
  if (!actor) redirect('/login');

  const allowed = matchPerm(actor.permissions, 'tile:chat:view::allow');

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
          subtitle="Full AI assistant"
          category={{ label: 'AI Chat', icon: 'zap', href: '/chat' }}
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
        title="AI Chat"
        subtitle={`${actor.fullname} · ${actor.role_name}`}
        category={{ label: 'AI Chat', icon: 'zap', href: '/chat' }}
        className="max-w-none px-3 sm:px-4 pt-2 pb-2"
        contentClassName="mt-2"
      >
        <FullChat initialSessions={sessions} />
      </PageLayout>
    </>
  );
}