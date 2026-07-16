import React from 'react';
import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import { listCustomers, getCustomerArHistory } from '@/customer/queries';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { getSecondaryLocale } from '@/server/locale';
import { T } from '@/components/i18n/TServer';
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
  const empty = t('customers.empty');

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
            className="flex-1 rounded-lg bg-slate-900 border border-slate-700 px-3 py-2 text-sm text-white placeholder-slate-500 focus:outline-none focus:border-indigo-500"
          />
          <button
            type="submit"
            className="rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-4 py-2 text-sm font-mono text-cyan-200 hover:bg-cyan-500/20"
          >
            {searchButton}
          </button>
        </form>

        <div className="overflow-x-auto rounded-2xl border border-slate-800 bg-slate-950/60">
          <table className="w-full text-sm">
            <thead className="border-b border-slate-800 text-xs font-mono uppercase tracking-wider text-slate-500">
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
                  <td colSpan={8} className="px-3 py-6 text-center text-slate-500">{empty}</td>
                </tr>
              ) : rows.map(({ customer: c, ar }) => (
                <tr key={c.id} className="border-b border-slate-900 hover:bg-slate-900/40">
                  <td className="px-3 py-2 font-mono text-cyan-300">
                    <a href={`/customers/${c.id}`} className="hover:underline">{c.code}</a>
                  </td>
                  <td className="px-3 py-2 text-white">{c.name}{c.name_th ? <span className="ml-1 text-slate-500">({c.name_th})</span> : null}</td>
                  <td className="px-3 py-2 text-right font-mono text-slate-300">{formatTHB(c.credit_limit_thb)}</td>
                  <td className="px-3 py-2 text-right font-mono text-amber-300">{formatTHB(ar?.outstanding_ar ?? 0)}</td>
                  <td className="px-3 py-2 text-right font-mono text-slate-300">{formatTHB(ar?.total_invoiced ?? 0)}</td>
                  <td className="px-3 py-2 text-right font-mono text-emerald-300">{formatTHB(ar?.total_paid ?? 0)}</td>
                  <td className="px-3 py-2 text-center">
                    {c.blacklist ? <span className="rounded-full border border-rose-500/40 bg-rose-500/15 px-2 py-0.5 text-xs font-mono text-rose-300"><T id="customers.statusBlacklist" locale={locale} /></span>
                      : c.is_active ? <span className="rounded-full border border-emerald-500/40 bg-emerald-500/15 px-2 py-0.5 text-xs font-mono text-emerald-300"><T id="customers.statusActive" locale={locale} /></span>
                        : <span className="rounded-full border border-slate-700 bg-slate-800 px-2 py-0.5 text-xs font-mono text-slate-400"><T id="customers.statusInactive" locale={locale} /></span>}
                  </td>
                  <td className="px-3 py-2 text-right font-mono text-slate-500">{ar?.so_count ?? 0}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </PageLayout>
    </>
  );
}