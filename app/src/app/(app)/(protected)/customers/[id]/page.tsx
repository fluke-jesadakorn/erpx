import React from 'react';
import Link from 'next/link';
import { redirect, notFound } from 'next/navigation';
import { headers } from 'next/headers';
import { loadActor } from '@folio-lib/server/guard';
import { loadActivePermSession, hasPermission, PERM } from '@folio-lib/perm/server';
import { getCustomer, getCustomerArHistory, listCustomerContacts } from '@folio-lib/customer/queries';
import { query } from '@folio-lib/db';
import { CustomerArHistory } from '@/components/customer/CustomerArHistory';
import { CustomerAdvisoryCard } from '@/components/customer/CustomerAdvisoryCard';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NoPermissionView } from '@/components/NoPermissionView';
import { getSecondaryLocale, type SecondaryLocale } from '@folio-lib/server/locale';
import custDict from '@folio-lib/i18n/customers';
import { UpdateCustomerForm } from './UpdateCustomerForm';

export const dynamic = 'force-dynamic';

interface PageProps {
  params: Promise<{ id: string }>;
}

async function loadSalesOrdersForCustomer(customerId: number) {
  const r = await query<{
    id: number;
    so_number: string;
    status: string;
    total_amount: string;
    due_date: string | null;
    created_at: string;
    invoice_number: string | null;
  }>(
    `SELECT id, so_number, status, total_amount::text, due_date::text, created_at::text, invoice_number
       FROM sales_orders
      WHERE customer_id = $1
      ORDER BY created_at DESC
      LIMIT 30`,
    [customerId],
  );
  return r.rows;
}

function formatTHB(n: number): string {
  return (Number(n) || 0).toLocaleString('th-TH', { maximumFractionDigits: 0 });
}

function t(key: string, locale: SecondaryLocale): string {
  const en = custDict.en[key] ?? key;
  if (locale === 'de') return custDict.de?.[key] ?? en;
  return custDict.th?.[key] ?? en;
}

export default async function CustomerDetailPage({ params }: PageProps) {
  const h = await headers();
  const out = await loadActivePermSession(
    new Request('http://internal', { headers: h as unknown as HeadersInit }),
  );
  if (!out || !hasPermission(out.session, PERM.tile.customers.view)) {
    return (
      <>
        <BreadcrumbSetter crumbs={[{ label: 'Hub', href: '/' }, { label: 'Customers', href: '/customers' }, { label: 'Detail', href: '/customers' }]} />
        <PageLayout title="Customer" subtitle={out?.session.user.name ?? undefined}>
          <NoPermissionView
            kind="locked"
            actor={out ? (out.session.user as any) : null}
            attemptedPath="/customers/[id]"
            reason={out ? 'tile:customers:view required.' : 'Sign in to view this page.'}
          />
        </PageLayout>
      </>
    );
  }

  const actor = await loadActor();
  if (!actor) redirect('/login');
  const { id } = await params;
  const customerId = parseInt(id, 10);
  if (!Number.isFinite(customerId) || customerId <= 0) notFound();

  const customer = await getCustomer(customerId);
  if (!customer) notFound();
  const [ar, contacts, salesOrders] = await Promise.all([
    getCustomerArHistory(customerId),
    listCustomerContacts(customerId),
    loadSalesOrdersForCustomer(customerId),
  ]);
  const locale = await getSecondaryLocale();

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
          { label: 'Hub', href: '/' },
          { label: 'Customers', href: '/customers' },
          { label: customer.code, href: `/customers/${customer.id}` },
        ]}
      />
      <PageLayout
        title={customer.name}
        subtitle={`${customer.code} · ${customer.name_th ?? ''} · ${customer.payment_terms}`}
      >
        <div className="grid grid-cols-1 lg:grid-cols-3 gap-6">
          <div className="lg:col-span-2 space-y-6">
            <section className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4">
              <h3 className="text-xs font-mono uppercase tracking-wider text-slate-400 mb-3">
                {t('customers.detail.fields', locale)}
              </h3>
              <UpdateCustomerForm customer={customer} />
            </section>

            <CustomerAdvisoryCard customerId={customerId} lang={locale} />

            <section className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4">
              <h3 className="text-xs font-mono uppercase tracking-wider text-slate-400 mb-3">
                {t('customers.detail.ar_aging', locale)}
              </h3>
              <CustomerArHistory
                data={
                  ar
                    ? [
                        { bucket: '0-30',  days_from: 0,  days_to: 30, amount_thb: 0,       so_count: 0 },
                        { bucket: '31-60', days_from: 31, days_to: 60, amount_thb: 0,       so_count: 0 },
                        { bucket: '61-90', days_from: 61, days_to: 90, amount_thb: 0,       so_count: 0 },
                        { bucket: '90+',   days_from: 91, days_to: 9999, amount_thb: 0,     so_count: 0 },
                      ]
                    : null
                }
                totalInvoiced={ar?.total_invoiced ?? 0}
                totalPaid={ar?.total_paid ?? 0}
                creditLimit={customer.credit_limit_thb}
                customerName={customer.name}
                locale={locale}
              />
            </section>

            <section className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4">
              <h3 className="text-xs font-mono uppercase tracking-wider text-slate-400 mb-3">
                {t('customers.detail.recent_sos', locale)}
              </h3>
              <ul className="space-y-1">
                {salesOrders.length === 0 ? (
                  <li className="text-sm text-slate-500">{t('customers.detail.no_sos', locale)}</li>
                ) : salesOrders.map((so) => {
                  const soWl = 'WB';
                  void soWl;
                  return (
                    <li key={so.id} className="flex items-center justify-between text-sm">
                      <Link href={`/sales/SO-${so.so_number}`} className="font-mono text-cyan-300 hover:underline">
                        {so.so_number}
                      </Link>
                      <span className="font-mono text-slate-300">{formatTHB(parseFloat(so.total_amount))} THB</span>
                      <span className="text-xs font-mono text-slate-500">{so.status}</span>
                    </li>
                  );
                })}
              </ul>
            </section>
          </div>

          <div className="space-y-6">
            <section className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4">
              <h3 className="text-xs font-mono uppercase tracking-wider text-slate-400 mb-3">
                {t('customers.detail.contacts', locale)}
              </h3>
              {contacts.length === 0 ? (
                <p className="text-sm text-slate-500">{t('customers.detail.no_contacts', locale)}</p>
              ) : (
                <ul className="space-y-2">
                  {contacts.map((c) => (
                    <li key={c.id} className="rounded-xl border border-slate-800 bg-slate-900/40 p-2 text-sm">
                      <div className="font-bold text-white">{c.fullname}</div>
                      {c.role ? <div className="text-sm text-slate-400">{c.role}</div> : null}
                      <div className="text-sm font-mono text-slate-500">
                        {c.email ? <a href={`mailto:${c.email}`} className="hover:underline">{c.email}</a> : null}
                        {c.phone ? <span className="ml-2">{c.phone}</span> : null}
                      </div>
                    </li>
                  ))}
                </ul>
              )}
            </section>
          </div>
        </div>
      </PageLayout>
    </>
  );
}
