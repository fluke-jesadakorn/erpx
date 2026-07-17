import { headers } from 'next/headers';
import { notFound } from 'next/navigation';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { trail } from '@/components/breadcrumbs';
import { NoPermissionView } from '@/components/NoPermissionView';
import { Tabs, Panel, Empty } from '@/components/ui';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { getContract } from '@/law/server';

export const dynamic = 'force-dynamic';

interface Props {
  params: Promise<{ docId: string }>;
}

export default async function LawPartiesPage({ params }: Props) {
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
            { label: 'Parties' },
          )}
        />
        <PageLayout title="Parties">
          <NoPermissionView
            kind="locked"
            actor={out ? (out.session.user as any) : null}
            attemptedPath={`/law/${docId}/parties`}
            reason={out ? 'law:contract:read required.' : 'Sign in to view this page.'}
          />
        </PageLayout>
      </>
    );
  }

  const contract = await getContract(docId);
  if (!contract) notFound();
  const parties = (contract.metadata as { parties?: Array<{ name?: string; role?: string }> } | null)?.parties ?? [];

  return (
    <>
      <BreadcrumbSetter
        crumbs={trail(
          locale,
          { label: <T id="law.title" locale={locale} />, href: '/law' },
          { label: contract.docNo || contract.fileName, href: `/law/${docId}` },
          { label: 'Parties' },
        )}
      />
      <PageLayout
        title={contract.fileName}
        subtitle="Parties"
        category={{ label: <T id="law.title" locale={locale} />, icon: 'scale', href: '/law' }}
      >
        <Tabs
          value="parties"
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
          <h2 className="text-sm font-semibold text-ink">Parties</h2>
          {parties.length === 0 ? (
            <Empty icon="users" title="No party metadata" body="This contract has no party metadata recorded." />
          ) : (
            <ul className="mt-6 divide-y divide-rule">
              {parties.map((party, index) => (
                <li key={index} className="flex items-center justify-between py-2 text-sm">
                  <span className="font-medium text-ink">{party.name ?? '—'}</span>
                  <span className="text-mute">{party.role ?? '—'}</span>
                </li>
              ))}
            </ul>
          )}
        </Panel>
      </PageLayout>
    </>
  );
}
