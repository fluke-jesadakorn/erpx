import React from 'react';
import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import { listAllOpenWaybills, listActiveWaybills, listMyWaybills, listAwaitingForActor, type WaybillInboxRow } from '@/waybill/queries';
import { WaybillChip } from '@/components/waybill/WaybillChip';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

export const dynamic = 'force-dynamic';

interface PageProps {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}

function asScope(v: string | string[] | undefined, fallback: string): string {
  if (Array.isArray(v)) return v[0] ?? fallback;
  return v ?? fallback;
}

export default async function MyWaybillsPage({ searchParams }: PageProps) {
  const sp = await searchParams;
  const actor = await loadActor();
  if (!actor) redirect('/login');

  if (!sp.scope) {
    redirect('/my-waybills?scope=active');
  }
  const scope = asScope(sp.scope, 'active');
   const role = actor.role_name ?? 'staff';
   const locale = await getSecondaryLocale();

  let rows: WaybillInboxRow[] = [];
  if (scope === 'all') {
    rows = await listAllOpenWaybills();
  } else if (scope === 'active') {
    rows = await listActiveWaybills();
  } else if (scope === 'queue') {
    rows = await listAwaitingForActor(actor.id, role);
  } else {
    rows = await listMyWaybills(actor.id);
  }

  const tabHref = (s: string) =>
    `/my-waybills?scope=${s}`;

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
           { label: <T id="nav.home" locale={locale} />, href: '/' },
           { label: <T id="waybill.inbox.title" locale={locale} />, href: '/my-waybills' },
        ]}
      />
      <PageLayout
         title={<T id="waybill.inbox.title" locale={locale} />}
         subtitle={<T id="waybill.inbox.subtitle" locale={locale} values={{ role, scope }} />}
      >
        <nav className="mb-4 flex flex-wrap gap-2 text-xs font-mono">
          <a
            href={tabHref('active')}
            aria-current={scope === 'active' ? 'page' : undefined}
            className={
              'rounded-lg border px-3 py-1.5 ' +
              (scope === 'active'
                ? 'border-cyan-500 bg-cyan-500/15 text-cyan-200'
                : 'border-slate-700 text-slate-400 hover:border-slate-500')
            }
          >
             ⚡ <T id="waybill.inbox.active" locale={locale} />
          </a>
          <a
            href={tabHref('mine')}
            aria-current={scope === 'mine' ? 'page' : undefined}
            className={
              'rounded-lg border px-3 py-1.5 ' +
              (scope === 'mine'
                ? 'border-cyan-500 bg-cyan-500/15 text-cyan-200'
                : 'border-slate-700 text-slate-400 hover:border-slate-500')
            }
          >
             📤 <T id="waybill.inbox.mine" locale={locale} />
          </a>
          <a
            href={tabHref('queue')}
            aria-current={scope === 'queue' ? 'page' : undefined}
            className={
              'rounded-lg border px-3 py-1.5 ' +
              (scope === 'queue'
                ? 'border-cyan-500 bg-cyan-500/15 text-cyan-200'
                : 'border-slate-700 text-slate-400 hover:border-slate-500')
            }
          >
             ✅ <T id="waybill.inbox.queue" locale={locale} />
          </a>
          <a
            href={tabHref('all')}
            aria-current={scope === 'all' ? 'page' : undefined}
            className={
              'rounded-lg border px-3 py-1.5 ' +
              (scope === 'all'
                ? 'border-cyan-500 bg-cyan-500/15 text-cyan-200'
                : 'border-slate-700 text-slate-500 hover:border-slate-500')
            }
          >
             🗂 <T id="waybill.inbox.all" locale={locale} />
          </a>
        </nav>

        <ul className="space-y-2">
          {rows.length === 0 && (
            <li className="rounded-2xl border border-slate-800 bg-slate-950/60 p-4 text-xs text-slate-500">
               <T id="waybill.inbox.empty" locale={locale} />
            </li>
          )}
          {rows.map((row) => {
            const domain = row.origin === 'expense' ? 'expense' : 'procurement';
            const amount = row.total_amount ? parseFloat(row.total_amount) : null;
            const originLabel =
              row.origin === 'expense'
                ? `EXP-${row.origin_id}`
                : row.origin === 'pr'
                ? `PR-${row.origin_id}`
                : `PO-${row.origin_id}`;
            return (
              <li
                key={row.id}
                className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-slate-800 bg-slate-950/60 p-4"
              >
                <div className="min-w-0 flex-1 space-y-1">
                  <div className="flex items-center gap-2">
                    <span className="font-mono text-sm text-cyan-300">{row.id}</span>
                    <WaybillChip
                      domain={domain}
                      currentStage={row.current_stage}
                      amountTHB={amount}
                    />
                  </div>
                  <div className="text-sm text-slate-400">
                    {originLabel} · {row.vendor_name ?? '—'} ·{' '}
                    {(amount ?? 0).toLocaleString('th-TH', { minimumFractionDigits: 2 })} {row.currency}
                  </div>
                  <div className="text-xs font-mono text-slate-500">
                     <T id="waybill.inbox.submitter" locale={locale} values={{ name: row.submitter_name ?? '—' }} />{' · '}
                     <T id="waybill.inbox.age" locale={locale} values={{ h: Math.max(0, Math.floor(row.age_hours)) }} />
                  </div>
                </div>
                <a
                  href={`/waybill/${row.id}`}
                  className="rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-3 py-1.5 text-xs font-mono text-cyan-200 hover:bg-cyan-500/20"
                >
                   <T id="common.open" locale={locale} /> →
                </a>
              </li>
            );
          })}
        </ul>
      </PageLayout>
    </>
  );
}
