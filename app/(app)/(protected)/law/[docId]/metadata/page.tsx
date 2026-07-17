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

export default async function LawMetadataPage({ params }: Props) {
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
            { label: 'Metadata' },
          )}
        />
        <PageLayout title="Metadata">
          <NoPermissionView
            kind="locked"
            actor={out ? (out.session.user as any) : null}
            attemptedPath={`/law/${docId}/metadata`}
            reason={out ? 'law:contract:read required.' : 'Sign in to view this page.'}
          />
        </PageLayout>
      </>
    );
  }

  const contract = await getContract(docId);
  if (!contract) notFound();

  const rows: ReadonlyArray<readonly [string, string]> = [
    ['Document number', contract.docNo || 'Not assigned'],
    ['Status', contract.status],
    ['Category', contract.category || 'Uncategorized'],
    ['Source', contract.source || 'Unknown'],
    ['MIME type', contract.fileMime || 'Unknown'],
    ['Size', contract.sizeBytes == null ? 'Unknown' : `${contract.sizeBytes.toLocaleString()} bytes`],
    ['LINE user', contract.lineUserId || 'Not linked'],
    ['Uploaded', new Date(contract.uploadedAt).toLocaleString()],
    ['Storage bucket', contract.storageBucket || 'Not stored'],
    ['Storage key', contract.storagePath || 'Not stored'],
  ];

  return (
    <>
      <BreadcrumbSetter
        crumbs={trail(
          locale,
          { label: <T id="law.title" locale={locale} />, href: '/law' },
          { label: contract.docNo || contract.fileName, href: `/law/${docId}` },
          { label: 'Metadata' },
        )}
      />
      <PageLayout
        title={contract.fileName}
        subtitle="Metadata"
        category={{ label: <T id="law.title" locale={locale} />, icon: 'scale', href: '/law' }}
      >
        <Tabs
          value="metadata"
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
          <dl className="grid gap-4 sm:grid-cols-2">
            {rows.map(([label, value]) => (
              <div key={label}>
                <dt className="text-xs uppercase tracking-wider text-mute">{label}</dt>
                <dd className="mt-1 break-all text-sm text-ink-2">{value}</dd>
              </div>
            ))}
          </dl>
          {contract.metadata != null && (
            <pre className="mt-6 overflow-auto rounded-md border border-rule bg-paper p-4 text-xs text-mute">
              {JSON.stringify(contract.metadata, null, 2)}
            </pre>
          )}
        </Panel>
      </PageLayout>
    </>
  );
}
