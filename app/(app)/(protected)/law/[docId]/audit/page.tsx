import { headers } from 'next/headers';
import { notFound } from 'next/navigation';
import { History } from 'lucide-react';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { trail } from '@/components/breadcrumbs/routes';
import { NoPermissionView } from '@/components/NoPermissionView';
import { Tabs, Panel, Empty } from '@/components/ui';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { getContract } from '@/law/server';

export const dynamic = 'force-dynamic';

interface Props {
  params: Promise<{ docId: string }>;
}

export default async function LawAuditPage({ params }: Props) {
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
            { label: 'Audit' },
          )}
        />
        <PageLayout title="Audit">
          <NoPermissionView
            kind="locked"
            actor={out ? (out.session.user as any) : null}
            attemptedPath={`/law/${docId}/audit`}
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
          { label: 'Audit' },
        )}
      />
      <PageLayout
        title={contract.fileName}
        subtitle="Audit"
        category={{ label: <T id="law.title" locale={locale} />, icon: 'Scale', href: '/law' }}
      >
        <Tabs
          value="audit"
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
          <Empty
            icon={History}
            title="Audit trail"
            body="Contract-level audit events will surface here once the indexer is wired up."
          />
        </Panel>
      </PageLayout>
    </>
  );
}
