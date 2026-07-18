import Link from 'next/link';
import { headers } from 'next/headers';
import { listContracts, getContractStats } from '@/law/server';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { ContractList } from './_components/ContractList';
import { NoPermissionView } from '@/components/NoPermissionView';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

export const dynamic = 'force-dynamic';

export default async function LawPage() {
  const h = await headers();
  const req = new Request('http://internal/law', { headers: h as unknown as HeadersInit });
  const out = await loadActivePermSession(req);
  const locale = await getSecondaryLocale();

  if (!out) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: <T id="law.title" locale={locale} /> }]} />
        <PageLayout title={<T id="law.page.title" locale={locale} />} subtitle={<T id="law.page.subtitle" locale={locale} />}>
          <NoPermissionView kind="locked" actor={null} attemptedPath="/law" reason={<T id="access.signInBody" locale={locale} />} />
        </PageLayout>
      </>
    );
  }

  if (!hasPermission(out.session, PERM.law.contract.read)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: <T id="law.title" locale={locale} /> }]} />
        <PageLayout title={<T id="law.page.title" locale={locale} />} subtitle={<T id="law.page.subtitle" locale={locale} />}>
          <NoPermissionView
            kind="locked"
            actor={out.session.user as any}
            attemptedPath="/law"
            reason={<T id="law.permission.contractReadRequired" locale={locale} />}
          />
        </PageLayout>
      </>
    );
  }

  const [contracts, stats] = await Promise.all([
    listContracts({ limit: 200 }),
    getContractStats(),
  ]);

  return (
    <>
      <BreadcrumbSetter crumbs={[{ label: 'Folio', href: '/' }, { label: <T id="law.title" locale={locale} /> }]} />
      <PageLayout
        title={<T id="law.page.title" locale={locale} />}
        subtitle={<T id="law.page.subtitle" locale={locale} />}
        category={{ label: <T id="law.title" locale={locale} />, icon: 'Scale', href: '/law' }}
        width="wide"
        actions={
          <div className="flex gap-2">
            <Link href="/law/admin" className="rounded-lg border border-rule px-3 py-2 text-xs text-ink-2 hover:border-rule">
              <T id="law.page.adminLink" locale={locale} />
            </Link>
            <Link href="/law/upload" className="rounded-lg border border-info/40 bg-info px-3 py-2 text-xs text-info hover:bg-info">
              <T id="law.page.uploadLink" locale={locale} />
            </Link>
          </div>
        }
      >
        <div className="mb-6 grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
          <section className="panel p-4">
            <p className="text-xs uppercase tracking-widest text-mute"><T id="law.stats.total" locale={locale} /></p>
            <p className="mt-2 text-3xl font-semibold text-ink">{stats.total}</p>
          </section>
          <section className="panel p-4">
            <p className="text-xs uppercase tracking-widest text-mute"><T id="law.stats.ready" locale={locale} /></p>
            <p className="mt-2 text-3xl font-semibold text-ink">{stats.ready}</p>
          </section>
          <section className="panel p-4">
            <p className="text-xs uppercase tracking-widest text-mute"><T id="law.stats.pending" locale={locale} /></p>
            <p className="mt-2 text-3xl font-semibold text-ink">{stats.pending}</p>
          </section>
          <section className="panel p-4">
            <p className="text-xs uppercase tracking-widest text-mute"><T id="law.stats.failed" locale={locale} /></p>
            <p className="mt-2 text-3xl font-semibold text-ink">{stats.failed}</p>
          </section>
        </div>
        <ContractList contracts={contracts} />
      </PageLayout>
    </>
  );
}
