import Link from 'next/link';
import { redirect } from 'next/navigation';
import { headers } from 'next/headers';
import { getContractStats, listContracts } from '@/law/server';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

export const dynamic = 'force-dynamic';

export default async function LawAdminPage() {
  const h = await headers();
  const req = new Request('http://internal/law/admin', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);
  const locale = await getSecondaryLocale();

  if (!out) {
    redirect('/?login=1&next=/law/admin');
  }

  if (!hasPermission(out.session, PERM.law.admin.stats)) {
    redirect('/forbidden?path=/law/admin&reason=law:admin:stats');
  }

  const [stats, recent] = await Promise.all([
    getContractStats(),
    listContracts({ limit: 10 }),
  ]);

  return (
    <>
      <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: <T id="law.title" locale={locale} />, href: '/law' }, { label: <T id="law.admin" locale={locale} /> }]} />
      <PageLayout
        title={<T id="law.adminTitle" locale={locale} />}
        subtitle={<T id="law.adminSubtitle" locale={locale} />}
        category={{ label: <T id="law.title" locale={locale} />, icon: 'Scale', href: '/law' }}
        actions={<Link href="/law" className="rounded-lg border border-rule px-3 py-2 text-xs text-ink-2 hover:border-rule"><T id="law.contracts" locale={locale} /></Link>}
      >
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-3">
          <section className="rounded-md border border-rule bg-paper-2/55 p-4">
            <p className="text-xs uppercase tracking-widest text-mute"><T id="law.adminCards.contracts" locale={locale} /></p>
            <p className="mt-2 text-2xl font-semibold text-ink">{stats.total}</p>
          </section>
          <section className="rounded-md border border-rule bg-paper-2/55 p-4">
            <p className="text-xs uppercase tracking-widest text-mute"><T id="law.stats.ready" locale={locale} /></p>
            <p className="mt-2 text-2xl font-semibold text-ink">{stats.ready}</p>
          </section>
          <section className="rounded-md border border-rule bg-paper-2/55 p-4">
            <p className="text-xs uppercase tracking-widest text-mute"><T id="law.stats.pending" locale={locale} /></p>
            <p className="mt-2 text-2xl font-semibold text-ink">{stats.pending}</p>
          </section>
          <section className="rounded-md border border-rule bg-paper-2/55 p-4">
            <p className="text-xs uppercase tracking-widest text-mute"><T id="law.stats.failed" locale={locale} /></p>
            <p className="mt-2 text-2xl font-semibold text-ink">{stats.failed}</p>
          </section>
          <section className="rounded-md border border-rule bg-paper-2/55 p-4">
            <p className="text-xs uppercase tracking-widest text-mute"><T id="law.chunks" locale={locale} /></p>
            <p className="mt-2 text-2xl font-semibold text-ink">{stats.chunks}</p>
          </section>
          <section className="rounded-md border border-rule bg-paper-2/55 p-4">
            <p className="text-xs uppercase tracking-widest text-mute"><T id="law.storage" locale={locale} /></p>
            <p className="mt-2 text-2xl font-semibold text-ink">{(stats.bytes / 1024 / 1024).toFixed(2)} MB</p>
          </section>
        </div>

        <section className="mt-6 rounded-md border border-rule bg-paper-2/55">
          <div className="border-b border-rule px-4 py-3">
            <h2 className="text-sm font-semibold text-ink"><T id="law.recentActivity" locale={locale} /></h2>
          </div>
          <ul className="divide-y divide-rule">
            {recent.map((contract) => (
              <li key={contract.id} className="flex flex-wrap items-center justify-between gap-3 px-4 py-3">
                <div>
                  <Link href={`/law/${contract.id}`} className="text-sm font-medium text-info hover:text-info">
                    {contract.fileName}
                  </Link>
                  <p className="mt-1 font-mono text-xs text-mute">
                    {contract.docNo || contract.id.slice(0, 8)} · {new Date(contract.uploadedAt).toLocaleString()}
                  </p>
                </div>
                <span className="rounded-full border border-rule px-2 py-1 font-mono text-xs text-ink-2">{contract.status}</span>
              </li>
            ))}
            {recent.length === 0 && <li className="px-4 py-10 text-center text-sm text-mute"><T id="law.noActivity" locale={locale} /></li>}
          </ul>
        </section>
      </PageLayout>
    </>
  );
}