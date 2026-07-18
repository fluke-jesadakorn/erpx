import React from 'react';
import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import { listCustomers, getCustomerArHistory } from '@/customer/queries';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { getSecondaryLocale } from '@/server/locale';
import { T } from '@/components/i18n/TServer';
import { Empty } from '@/components/ui/Empty';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission } from '@/perm/server';
import { NoPermissionView } from '@/components/NoPermissionView';
import { getTranslations } from 'next-intl/server';

export const dynamic = 'force-dynamic';

interface PageProps {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}

function asString(v: string | string[] | undefined): string {
  if (Array.isArray(v)) return v[0] ?? '';
  return v ?? '';
}

function formatTHB(n: number): string {
  return (Number(n) || 0).toLocaleString('th-TH', { maximumFractionDigits: 0 });
}

export default async function CustomersPage({ searchParams }: PageProps) {
  const h = await headers();
  const out = await loadActivePermSession(
    new Request('http://internal', { headers: h as unknown as HeadersInit }),
  );
  const locale = await getSecondaryLocale();
  const t = await getTranslations();
  if (!out || !hasPermission(out.session, 'tile:customers:view::allow')) {
    return (
      <>
        <BreadcrumbSetter
          crumbs={[
            { label: <T id="breadcrumbs.home" locale={locale} />, href: '/' },
            { label: <T id="nav.customers" locale={locale} />, href: '/customers' },
          ]}
        />
        <PageLayout
          title={<T id="customers.title" locale={locale} />}
          subtitle={out?.session.user.name ?? undefined}
        >
          <NoPermissionView
            kind="locked"
            actor={out ? (out.session.user as any) : null}
            attemptedPath="/customers"
            reason={out ? 'tile:customers:view required.' : 'Sign in to view this page.'}
          />
        </PageLayout>
      </>
    );
  }

  const actor = await loadActor();
  if (!actor) redirect('/login');
  const sp = await searchParams;
  const q = asString(sp.q).trim();

  const customers = await listCustomers({ search: q, activeOnly: false, limit: 100 });

  const rows = await Promise.all(customers.map(async (c) => {
    const ar = await getCustomerArHistory(c.id).catch(() => null);
    return { customer: c, ar };
  }));

  const searchPlaceholder = t('customers.searchPlaceholder');
  const searchButton = t('customers.searchButton');
  const emptyTitle = t('customers.empty');

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
          { label: <T id="breadcrumbs.home" locale={locale} />, href: '/' },
          { label: <T id="nav.customers" locale={locale} />, href: '/customers' },
        ]}
      />
      <PageLayout
        title={<T id="customers.title" locale={locale} />}
        subtitle={<T id="customers.subtitle" locale={locale} values={{ role: actor.role_name ?? 'unknown' }} />}
      >
        <form className="mb-4 flex gap-2" method="get">
          <input
            type="text"
            name="q"
            defaultValue={q}
            placeholder={searchPlaceholder}
            className="flex-1 rounded-lg bg-paper border border-rule px-3 py-2 text-sm text-ink placeholder-mute focus:outline-none focus:border-accent/40"
          />
          <button
            type="submit"
            className="rounded-lg border border-info/40 bg-info px-4 py-2 text-sm font-mono text-info hover:bg-info"
          >
            {searchButton}
          </button>
        </form>

        <div className="overflow-x-auto rounded-md border border-rule bg-paper-2/60">
          <table className="w-full text-sm">
            <thead className="border-b border-rule text-xs font-mono uppercase tracking-wider text-mute">
              <tr>
                <th className="px-3 py-3 text-left"><T id="customers.colCode" locale={locale} /></th>
                <th className="px-3 py-3 text-left"><T id="customers.colName" locale={locale} /></th>
                <th className="px-3 py-3 text-right"><T id="customers.colCreditLimit" locale={locale} /></th>
                <th className="px-3 py-3 text-right"><T id="customers.colOutstanding" locale={locale} /></th>
                <th className="px-3 py-3 text-right"><T id="customers.colInvoiced" locale={locale} /></th>
                <th className="px-3 py-3 text-right"><T id="customers.colPaid" locale={locale} /></th>
                <th className="px-3 py-3 text-center"><T id="customers.colStatus" locale={locale} /></th>
                <th className="px-3 py-3 text-right"><T id="customers.colSos" locale={locale} /></th>
              </tr>
            </thead>
            <tbody>
              {rows.length === 0 ? (
                <tr>
                  <td colSpan={8}>
                    <Empty
                      title={emptyTitle}
                      body={q ? 'Try a different search term.' : 'Customers will appear here once added.'}
                    />
                  </td>
                </tr>
              ) : rows.map(({ customer: c, ar }) => (
                <tr key={c.id} className="border-b border-rule hover:bg-paper-2/40">
                  <td className="px-3 py-2 font-mono text-info">
                    <a href={`/customers/${c.id}`} className="hover:underline">{c.code}</a>
                  </td>
                  <td className="px-3 py-2 text-ink">{c.name}{c.name_th ? <span className="ml-1 text-mute">({c.name_th})</span> : null}</td>
                  <td className="px-3 py-2 text-right font-mono text-ink-2">{formatTHB(c.credit_limit_thb)}</td>
                  <td className="px-3 py-2 text-right font-mono text-caution">{formatTHB(ar?.outstanding_ar ?? 0)}</td>
                  <td className="px-3 py-2 text-right font-mono text-ink-2">{formatTHB(ar?.total_invoiced ?? 0)}</td>
                  <td className="px-3 py-2 text-right font-mono text-positive">{formatTHB(ar?.total_paid ?? 0)}</td>
                  <td className="px-3 py-2 text-center">
                    {c.blacklist ? <span className="rounded-full border border-critical/40 bg-critical px-2 py-0.5 text-xs font-mono text-critical"><T id="customers.statusBlacklist" locale={locale} /></span>
                      : c.is_active ? <span className="rounded-full border border-positive/40 bg-positive px-2 py-0.5 text-xs font-mono text-positive"><T id="customers.statusActive" locale={locale} /></span>
                        : <span className="rounded-full border border-rule bg-paper-2 px-2 py-0.5 text-xs font-mono text-ink-2"><T id="customers.statusInactive" locale={locale} /></span>}
                  </td>
                  <td className="px-3 py-2 text-right font-mono text-mute">{ar?.so_count ?? 0}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </PageLayout>
    </>
  );
}