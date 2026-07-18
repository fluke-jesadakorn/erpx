import React from 'react';
import Link from 'next/link';
import { notFound } from 'next/navigation';
import { getContract, listChunks, previewContract } from '@/law/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ docId: string }>;
  searchParams: Promise<{ tab?: string | string[] }>;
}

export default async function ContractDetailPage({ params, searchParams }: PageProps) {
  const { docId } = await params;
  const locale = await getSecondaryLocale();
  const contract = await getContract(docId);
  if (!contract) notFound();

  const rawTab = (await searchParams).tab;
  const tabValue = Array.isArray(rawTab) ? rawTab[0] : rawTab;
  const tab = ['pdf', 'chunks', 'metadata'].includes(tabValue || '') ? tabValue : 'pdf';
  const [preview, chunks] = await Promise.all([
    previewContract(docId).catch(() => ({ pdfUrl: '', pages: [] as string[] })),
    listChunks(docId),
  ]);

  return (
    <>
      <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: <T id="law.title" locale={locale} />, href: '/law' }, { label: contract.docNo || contract.fileName }]} />
      <PageLayout
        title={contract.fileName}
        subtitle={`${contract.docNo || contract.id} · ${contract.status}`}
        category={{ label: <T id="law.title" locale={locale} />, icon: 'Scale', href: '/law' }}
        actions={<Link href="/law" className="rounded-lg border border-rule px-3 py-2 text-xs text-ink-2 hover:border-rule"><T id="law.backToContracts" locale={locale} /></Link>}
      >
        <nav className="mb-4 flex flex-wrap gap-2" aria-label="Contract detail tabs">
          <Link
            href={`/law/${docId}?tab=pdf`}
            aria-current={tab === 'pdf' ? 'page' : undefined}
            className={`rounded-lg border px-3 py-2 text-xs ${
              tab === 'pdf'
                ? 'border-info/40 bg-info text-info'
                : 'border-rule text-ink-2 hover:border-rule'
            }`}
          >
            <T id="law.detail.tabPdf" locale={locale} />
          </Link>
          <Link
            href={`/law/${docId}?tab=chunks`}
            aria-current={tab === 'chunks' ? 'page' : undefined}
            className={`rounded-lg border px-3 py-2 text-xs ${
              tab === 'chunks'
                ? 'border-info/40 bg-info text-info'
                : 'border-rule text-ink-2 hover:border-rule'
            }`}
          >
            <T id="law.detail.tabChunksWithCount" locale={locale} values={{ count: chunks.length }} />
          </Link>
          <Link
            href={`/law/${docId}?tab=metadata`}
            aria-current={tab === 'metadata' ? 'page' : undefined}
            className={`rounded-lg border px-3 py-2 text-xs ${
              tab === 'metadata'
                ? 'border-info/40 bg-info text-info'
                : 'border-rule text-ink-2 hover:border-rule'
            }`}
          >
            <T id="law.detail.tabMetadata" locale={locale} />
          </Link>
        </nav>

        {tab === 'pdf' && (
          <section className="overflow-hidden rounded-md border border-rule bg-paper-2/55">
            {preview.pdfUrl ? (
              <iframe
                src={preview.pdfUrl}
                title={contract.fileName}
                className="h-[72vh] w-full bg-paper"
              />
            ) : preview.pages.length > 0 ? (
              <div className="space-y-4 p-4">
                {preview.pages.map((url, index) => (
                  <img key={url} src={url} alt={`Page ${index + 1}`} className="mx-auto max-w-full rounded-lg bg-paper" />
                ))}
              </div>
            ) : (
              <div className="px-6 py-20 text-center text-sm text-mute"><T id="law.detail.noPdfPreview" locale={locale} /></div>
            )}
          </section>
        )}

        {tab === 'chunks' && (
          <section className="space-y-3">
            {chunks.map((chunk) => (
              <article key={chunk.id} className="rounded-md border border-rule bg-paper-2/55 p-4">
                <div className="mb-2 flex justify-between gap-3 font-mono text-xs text-mute">
                  <span><T id="law.detail.chunkIndex" locale={locale} values={{ index: chunk.chunkIndex }} /></span>
                  <span><T id="law.detail.tokenCount" locale={locale} values={{ count: chunk.tokenCount ?? 0 }} /></span>
                </div>
                <p className="whitespace-pre-wrap text-sm leading-6 text-ink-2">{chunk.content}</p>
              </article>
            ))}
            {chunks.length === 0 && (
              <div className="rounded-md border border-dashed border-rule px-6 py-16 text-center text-sm text-mute">
                <T id="law.detail.noChunks" locale={locale} />
              </div>
            )}
          </section>
        )}

        {tab === 'metadata' && (
          <section className="rounded-md border border-rule bg-paper-2/55 p-5">
            <dl className="grid gap-4 sm:grid-cols-2">
              <div>
                <dt className="text-xs uppercase tracking-wider text-mute"><T id="law.metadata.documentNumber" locale={locale} /></dt>
                <dd className="mt-1 break-all text-sm text-ink-2">{contract.docNo || <T id="law.metadata.notAssigned" locale={locale} />}</dd>
              </div>
              <div>
                <dt className="text-xs uppercase tracking-wider text-mute"><T id="law.metadata.status" locale={locale} /></dt>
                <dd className="mt-1 break-all text-sm text-ink-2">{contract.status}</dd>
              </div>
              <div>
                <dt className="text-xs uppercase tracking-wider text-mute"><T id="law.metadata.category" locale={locale} /></dt>
                <dd className="mt-1 break-all text-sm text-ink-2">{contract.category || <T id="law.metadata.uncategorized" locale={locale} />}</dd>
              </div>
              <div>
                <dt className="text-xs uppercase tracking-wider text-mute"><T id="law.metadata.source" locale={locale} /></dt>
                <dd className="mt-1 break-all text-sm text-ink-2">{contract.source || <T id="law.metadata.unknown" locale={locale} />}</dd>
              </div>
              <div>
                <dt className="text-xs uppercase tracking-wider text-mute"><T id="law.metadata.mimeType" locale={locale} /></dt>
                <dd className="mt-1 break-all text-sm text-ink-2">{contract.fileMime || <T id="law.metadata.unknown" locale={locale} />}</dd>
              </div>
              <div>
                <dt className="text-xs uppercase tracking-wider text-mute"><T id="law.metadata.size" locale={locale} /></dt>
                <dd className="mt-1 break-all text-sm text-ink-2">
                  {contract.sizeBytes == null ? <T id="law.metadata.unknown" locale={locale} /> : <T id="law.metadata.sizeBytes" locale={locale} values={{ bytes: contract.sizeBytes.toLocaleString() }} />}
                </dd>
              </div>
              <div>
                <dt className="text-xs uppercase tracking-wider text-mute"><T id="law.metadata.lineUser" locale={locale} /></dt>
                <dd className="mt-1 break-all text-sm text-ink-2">{contract.lineUserId || <T id="law.metadata.notLinked" locale={locale} />}</dd>
              </div>
              <div>
                <dt className="text-xs uppercase tracking-wider text-mute"><T id="law.metadata.uploaded" locale={locale} /></dt>
                <dd className="mt-1 break-all text-sm text-ink-2">{new Date(contract.uploadedAt).toLocaleString()}</dd>
              </div>
              <div>
                <dt className="text-xs uppercase tracking-wider text-mute"><T id="law.metadata.storageBucket" locale={locale} /></dt>
                <dd className="mt-1 break-all text-sm text-ink-2">{contract.storageBucket || <T id="law.metadata.notStored" locale={locale} />}</dd>
              </div>
              <div>
                <dt className="text-xs uppercase tracking-wider text-mute"><T id="law.metadata.storageKey" locale={locale} /></dt>
                <dd className="mt-1 break-all text-sm text-ink-2">{contract.storagePath || <T id="law.metadata.notStored" locale={locale} />}</dd>
              </div>
            </dl>
            {contract.metadata != null && (
              <pre className="mt-5 overflow-auto rounded-md border border-rule bg-paper p-4 text-xs text-ink-2">
                {JSON.stringify(contract.metadata, null, 2)}
              </pre>
            )}
          </section>
        )}
      </PageLayout>
    </>
  );
}