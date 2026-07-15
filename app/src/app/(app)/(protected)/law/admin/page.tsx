import Link from 'next/link';
import { headers } from 'next/headers';
import { getContractStats, listContracts } from '@folio-lib/law/server';
import { loadActivePermSession, hasPermission, PERM } from '@folio-lib/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';

export const dynamic = 'force-dynamic';

export default async function LawAdminPage() {
  const h = await headers();
  const req = new Request('http://internal/law/admin', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);

  if (!out) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: 'Law', href: '/law' }, { label: 'Admin' }]} />
        <PageLayout title="Law admin" subtitle="Registry health and recent ingestion activity">
          <NoPermissionView kind="locked" actor={null} attemptedPath="/law/admin" reason="Sign in to view this page." />
        </PageLayout>
      </>
    );
  }

  if (!hasPermission(out.session, PERM.law.admin.stats)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: 'Law', href: '/law' }, { label: 'Admin' }]} />
        <PageLayout title="Law admin" subtitle="Registry health and recent ingestion activity">
          <NoPermissionView
            kind="locked"
            actor={out.session.user as any}
            attemptedPath="/law/admin"
            reason="law:admin:stats required."
          />
        </PageLayout>
      </>
    );
  }

  const [stats, recent] = await Promise.all([
    getContractStats(),
    listContracts({ limit: 10 }),
  ]);
  const cards = [
    ['Contracts', stats.total],
    ['Ready', stats.ready],
    ['Pending', stats.pending],
    ['Failed', stats.failed],
    ['Chunks', stats.chunks],
    ['Storage', `${(stats.bytes / 1024 / 1024).toFixed(2)} MB`],
  ] as const;

  return (
    <>
      <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: 'Law', href: '/law' }, { label: 'Admin' }]} />
      <PageLayout
        title="Law admin"
        subtitle="Registry health and recent ingestion activity"
        category={{ label: 'Law', icon: 'scale', href: '/law' }}
        actions={<Link href="/law" className="rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:border-slate-500">Contracts</Link>}
      >
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          {cards.map(([label, value]) => (
            <section key={label} className="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
              <p className="text-xs uppercase tracking-widest text-slate-500">{label}</p>
              <p className="mt-2 text-2xl font-semibold text-slate-100">{value}</p>
            </section>
          ))}
        </div>

        <section className="mt-6 rounded-2xl border border-slate-800 bg-slate-950/55">
          <div className="border-b border-slate-800 px-4 py-3">
            <h2 className="text-sm font-semibold text-slate-100">Recent activity</h2>
          </div>
          <ul className="divide-y divide-slate-800/80">
            {recent.map((contract) => (
              <li key={contract.id} className="flex flex-wrap items-center justify-between gap-3 px-4 py-3">
                <div>
                  <Link href={`/law/${contract.id}`} className="text-sm font-medium text-cyan-300 hover:text-cyan-200">
                    {contract.fileName}
                  </Link>
                  <p className="mt-1 font-mono text-xs text-slate-500">
                    {contract.docNo || contract.id.slice(0, 8)} · {new Date(contract.uploadedAt).toLocaleString()}
                  </p>
                </div>
                <span className="rounded-full border border-slate-700 px-2 py-1 font-mono text-xs text-slate-400">{contract.status}</span>
              </li>
            ))}
            {recent.length === 0 && <li className="px-4 py-10 text-center text-sm text-slate-500">No activity yet.</li>}
          </ul>
        </section>
      </PageLayout>
    </>
  );
}