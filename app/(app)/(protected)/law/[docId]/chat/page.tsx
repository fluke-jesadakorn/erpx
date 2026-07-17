import { headers } from 'next/headers';
import { notFound } from 'next/navigation';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { trail } from '@/components/breadcrumbs';
import { NoPermissionView } from '@/components/NoPermissionView';
import { Tabs, Panel } from '@/components/ui';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { getContract } from '@/law/server';

export const dynamic = 'force-dynamic';

interface Props {
  params: Promise<{ docId: string }>;
}

export default async function LawChatPage({ params }: Props) {
  const { docId } = await params;
  const h = await headers();
  const out = await loadActivePermSession(new Request('http://internal', { headers: h as unknown as HeadersInit }));
  const locale = await getSecondaryLocale();

  if (!out || !hasPermission(out.session, PERM.law.contract.read)) {
    return (
      <>
        <BreadcrumbSetter
          crumbs={trail(
            locale,
            { label: <T id="law.title" locale={locale} />, href: '/law' },
            { label: docId, href: `/law/${docId}` },
            { label: 'Chat' },
          )}
        />
        <PageLayout title="Chat">
          <NoPermissionView
            kind="locked"
            actor={out ? (out.session.user as any) : null}
            attemptedPath={`/law/${docId}/chat`}
            reason={out ? 'law:contract:read required.' : 'Sign in to view this page.'}
          />
        </PageLayout>
      </>
    );
  }

  const contract = await getContract(docId);
  if (!contract) notFound();

  return (
    <>
      <BreadcrumbSetter
        crumbs={trail(
          locale,
          { label: <T id="law.title" locale={locale} />, href: '/law' },
          { label: contract.docNo || contract.fileName, href: `/law/${docId}` },
          { label: 'Chat' },
        )}
      />
      <PageLayout
        title={contract.fileName}
        subtitle="Chat"
        category={{ label: <T id="law.title" locale={locale} />, icon: 'scale', href: '/law' }}
      >
        <Tabs
          value="chat"
          variant="page"
          className="mb-6"
          items={[
            { value: 'pdf', label: 'Document', href: `/law/${docId}` },
            { value: 'parties', label: 'Parties', href: `/law/${docId}/parties` },
            { value: 'chunks', label: 'Clauses', href: `/law/${docId}/chunks` },
            { value: 'metadata', label: 'Metadata', href: `/law/${docId}/metadata` },
            { value: 'audit', label: 'Audit', href: `/law/${docId}/audit` },
            { value: 'chat', label: 'Chat', href: `/law/${docId}/chat` },
          ]}
        />
        <Panel>
          <h2 className="text-sm font-semibold text-ink">Document chat</h2>
          <p className="mt-1 text-sm text-ink-2">
            Ask follow-up questions about contract <span className="font-mono">{docId}</span>.
          </p>
          <p className="mt-6 text-xs text-mute">
            Chat history and RAG search are wired through the existing AI contract endpoints.
          </p>
        </Panel>
      </PageLayout>
    </>
  );
}
