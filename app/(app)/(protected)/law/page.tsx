import Link from 'next/link';
import { headers } from 'next/headers';
import { listContracts, getContractStats } from '@/law/server';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { ContractList } from './_components/ContractList';
import { NoPermissionView } from '@/components/NoPermissionView';

export const dynamic = 'force-dynamic';

export default async function LawPage() {
  const h = await headers();
  const req = new Request('http://internal/law', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);

  if (!out) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: 'Law' }]} />
        <PageLayout title="Law documents" subtitle="Contract registry">
          <NoPermissionView kind="locked" actor={null} attemptedPath="/law" reason="Sign in to view this page." />
        </PageLayout>
      </>
    );
  }

  if (!hasPermission(out.session, PERM.law.contract.read)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: 'Law' }]} />
        <PageLayout title="Law documents" subtitle="Contract registry">
          <NoPermissionView
            kind="locked"
            actor={out.session.user as any}
            attemptedPath="/law"
            reason="law:contract:read required."
          />
        </PageLayout>
      </>
    );
  }

  const [contracts, stats] = await Promise.all([
    listContracts({ limit: 200 }),
    getContractStats(),
  ]);
  const cards = [
    ['Total', stats.total],
    ['Ready', stats.ready],
    ['Pending', stats.pending],
    ['Failed', stats.failed],
  ] as const;

  return (
    <>
      <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: 'Law' }]} />
      <PageLayout
        title="Law documents"
        subtitle="Contract registry, semantic retrieval, and LINE ingestion"
        category={{ label: 'Law', icon: 'scale', href: '/law' }}
        actions={
          <div className="flex gap-2">
            <Link href="/law/admin" className="rounded-lg border border-slate-700 px-3 py-2 text-xs text-slate-300 hover:border-slate-500">
              Admin
            </Link>
            <Link href="/law/upload" className="rounded-lg border border-cyan-500/50 bg-cyan-500/15 px-3 py-2 text-xs text-cyan-100 hover:bg-cyan-500/25">
              Upload
            </Link>
          </div>
        }
      >
        <div className="mb-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          {cards.map(([label, value]) => (
            <section key={label} className="rounded-2xl border border-slate-800 bg-slate-950/55 p-4">
              <p className="text-xs uppercase tracking-widest text-slate-500">{label}</p>
              <p className="mt-2 text-3xl font-semibold text-slate-100">{value}</p>
            </section>
          ))}
        </div>
        <ContractList contracts={contracts} />
      </PageLayout>
    </>
  );
}