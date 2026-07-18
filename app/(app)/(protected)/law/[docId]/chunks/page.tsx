import { headers } from 'next/headers';
import { notFound } from 'next/navigation';
import { Layers } from 'lucide-react';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { trail } from '@/components/breadcrumbs/routes';
import { NoPermissionView } from '@/components/NoPermissionView';
import { Tabs, Panel, Empty } from '@/components/ui';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { getContract, listChunks } from '@/law/server';

export const dynamic = 'force-dynamic';

interface Props {
  params: Promise<{ docId: string }>;
}

export default async function LawClausesPage({ params }: Props) {
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
            { label: 'Clauses' },
          )}
        />
        <PageLayout title="Clauses">
          <NoPermissionView
            kind="locked"
            actor={out ? (out.session.user as any) : null}
            attemptedPath={`/law/${docId}/chunks`}
            reason={out ? 'law:contract:read required.' : 'Sign in to view this page.'}
          />
        </PageLayout>
      </>
    );
  }

  const contract = await getContract(docId);
  if (!contract) notFound();
  const chunks = await listChunks(docId);

  return (
    <>
      <BreadcrumbSetter
        crumbs={trail(
          locale,
          { label: <T id="law.title" locale={locale} />, href: '/law' },
          { label: contract.docNo || contract.fileName, href: `/law/${docId}` },
          { label: 'Clauses' },
        )}
      />
      <PageLayout
        title={contract.fileName}
        subtitle="Clauses"
        category={{ label: <T id="law.title" locale={locale} />, icon: 'Scale', href: '/law' }}
      >
        <Tabs
          value="chunks"
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
        {chunks.length === 0 ? (
          <Empty icon={Layers} title="No clauses" body="This contract has no indexed chunks yet." />
        ) : (
          <section className="space-y-4">
            {chunks.map((chunk) => (
              <Panel key={chunk.id}>
                <div className="mb-2 flex justify-between gap-3 font-mono text-xs text-mute">
                  <span>Clause {chunk.chunkIndex}</span>
                  <span>{chunk.tokenCount ?? 0} tokens</span>
                </div>
                <p className="whitespace-pre-wrap text-sm leading-6 text-ink-2">{chunk.content}</p>
              </Panel>
            ))}
          </section>
        )}
      </PageLayout>
    </>
  );
}
