import Link from 'next/link';
import { notFound } from 'next/navigation';
import { getContract, listChunks, previewContract } from '@/law/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ docId: string }>;
  searchParams: Promise<{ tab?: string | string[] }>;
}

export default async function ContractDetailPage({ params, searchParams }: PageProps) {
  const { docId } = await params;
  const contract = await getContract(docId);
  if (!contract) notFound();

  const rawTab = (await searchParams).tab;
  const tabValue = Array.isArray(rawTab) ? rawTab[0] : rawTab;
  const tab = ['pdf', 'chunks', 'metadata'].includes(tabValue || '') ? tabValue : 'pdf';
  const [preview, chunks] = await Promise.all([
    previewContract(docId).catch(() => ({ pdfUrl: '', pages: [] as string[] })),
    listChunks(docId),
  ]);
  const tabs = [
    ['pdf', 'PDF preview'],
    ['chunks', `Chunks (${chunks.length})`],
    ['metadata', 'Metadata'],
  ] as const;

  return (
    <>
      <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: 'Law', href: '/law' }, { label: contract.docNo || contract.fileName }]} />
      <PageLayout
        title={contract.fileName}
        subtitle={`${contract.docNo || contract.id} · ${contract.status}`}
        category={{ label: 'Law', icon: 'scale', href: '/law' }}
        actions={<Link href="/law" className="rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:border-slate-500">Back to contracts</Link>}
      >
        <nav className="mb-4 flex flex-wrap gap-2" aria-label="Contract detail tabs">
          {tabs.map(([key, label]) => (
            <Link
              key={key}
              href={`/law/${docId}?tab=${key}`}
              aria-current={tab === key ? 'page' : undefined}
              className={`rounded-lg border px-3 py-2 text-xs ${
                tab === key
                  ? 'border-cyan-500 bg-cyan-500/15 text-cyan-100'
                  : 'border-slate-700 text-slate-400 hover:border-slate-500'
              }`}
            >
              {label}
            </Link>
          ))}
        </nav>

        {tab === 'pdf' && (
          <section className="overflow-hidden rounded-2xl border border-slate-800 bg-slate-950/55">
            {preview.pdfUrl ? (
              <iframe
                src={preview.pdfUrl}
                title={contract.fileName}
                className="h-[72vh] w-full bg-white"
              />
            ) : preview.pages.length > 0 ? (
              <div className="space-y-4 p-4">
                {preview.pages.map((url, index) => (
                  <img key={url} src={url} alt={`Page ${index + 1}`} className="mx-auto max-w-full rounded-lg bg-white" />
                ))}
              </div>
            ) : (
              <div className="px-6 py-20 text-center text-sm text-slate-500">No MinIO preview is available for this contract.</div>
            )}
          </section>
        )}

        {tab === 'chunks' && (
          <section className="space-y-3">
            {chunks.map((chunk) => (
              <article key={chunk.id} className="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
                <div className="mb-2 flex justify-between gap-3 font-mono text-xs text-slate-500">
                  <span>Chunk {chunk.chunkIndex}</span>
                  <span>{chunk.tokenCount ?? 0} tokens</span>
                </div>
                <p className="whitespace-pre-wrap text-sm leading-6 text-slate-300">{chunk.content}</p>
              </article>
            ))}
            {chunks.length === 0 && (
              <div className="rounded-2xl border border-dashed border-slate-800 px-6 py-16 text-center text-sm text-slate-500">
                This contract has no indexed chunks yet.
              </div>
            )}
          </section>
        )}

        {tab === 'metadata' && (
          <section className="rounded-2xl border border-slate-800 bg-slate-950/55 p-5">
            <dl className="grid gap-4 sm:grid-cols-2">
              {[
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
              ].map(([label, value]) => (
                <div key={label}>
                  <dt className="text-xs uppercase tracking-wider text-slate-500">{label}</dt>
                  <dd className="mt-1 break-all text-sm text-slate-300">{value}</dd>
                </div>
              ))}
            </dl>
            {contract.metadata != null && (
              <pre className="mt-5 overflow-auto rounded-xl border border-slate-800 bg-slate-950 p-4 text-xs text-slate-400">
                {JSON.stringify(contract.metadata, null, 2)}
              </pre>
            )}
          </section>
        )}
      </PageLayout>
    </>
  );
}
