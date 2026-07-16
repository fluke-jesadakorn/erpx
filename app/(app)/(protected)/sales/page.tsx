import React from 'react';
import Link from 'next/link';
import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import {
  listAllOpenWaybills,
  listMyWaybills,
  listAwaitingForActor,
  activeStageOf,
  loadActiveSalesDraftForRep,
  loadWaybillEvents,
  loadApproverSummariesForRows,
  type WaybillInboxRow,
} from '@/waybill/queries';
import { query } from '@/db';
import { WaybillChip } from '@/components/waybill/WaybillChip';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NewSalesPanel } from '@/components/waybill/NewSalesPanel';
import { ApproverChip } from '@/components/waybill/ApproverChip';
import { stageRoles } from '@/waybill/derive';
import { getSecondaryLocale } from '@/server/locale';
import { T } from '@/components/i18n/TServer';
import { getTranslations } from 'next-intl/server';
import { headers } from 'next/headers';
import { loadActivePermSession, hasPermission } from '@/perm/server';
import { NoPermissionView } from '@/components/NoPermissionView';

export const dynamic = 'force-dynamic';

interface PageProps {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}

function asScope(v: string | string[] | undefined, fallback: string): string {
  if (Array.isArray(v)) return v[0] ?? fallback;
  return v ?? fallback;
}

function fmtAge(iso: string | null): string {
  if (!iso) return '—';
  const d = new Date(iso);
  if (Number.isNaN(d.getTime())) return '—';
  const mins = Math.max(0, Math.floor((Date.now() - d.getTime()) / 60000));
  if (mins < 1) return 'just now';
  if (mins < 60) return `${mins} min ago`;
  const hrs = Math.floor(mins / 60);
  if (hrs < 24) return `${hrs} h ago`;
  return `${Math.floor(hrs / 24)} d ago`;
}

interface SalesChipRow {
  so_id: number;
  so_number: string;
  customer_name: string | null;
  due_date: Date | null;
}

async function loadSalesChipsForRows(
  soIds: number[],
): Promise<Map<number, SalesChipRow>> {
  const out = new Map<number, SalesChipRow>();
  if (soIds.length === 0) return out;
  const r = await query<SalesChipRow>(
    `SELECT so.id AS so_id,
            so.so_number,
            c.name AS customer_name,
            so.due_date
       FROM sales_orders so
       LEFT JOIN customers c ON c.id = so.customer_id
       WHERE so.id = ANY($1::int[])`,
    [soIds],
  );
  for (const row of r.rows) out.set(row.so_id, row);
  return out;
}

export default async function SalesInboxPage({ searchParams }: PageProps) {
  const h = await headers();
  const out = await loadActivePermSession(
    new Request('http://internal', { headers: h as unknown as HeadersInit }),
  );
  const locale = await getSecondaryLocale();
  const t = await getTranslations();
  if (!out || !hasPermission(out.session, 'tile:sales:view::allow')) {
    return (
      <>
        <BreadcrumbSetter
          crumbs={[
            { label: <T id="breadcrumbs.home" locale={locale} />, href: '/' },
            { label: <T id="nav.sales" locale={locale} />, href: '/sales' },
          ]}
        />
        <PageLayout
          title={<T id="sales.title" locale={locale} />}
          subtitle={out?.session.user.name ?? undefined}
        >
          <NoPermissionView
            kind="locked"
            actor={out ? (out.session.user as any) : null}
            attemptedPath="/sales"
            reason={out ? 'tile:sales:view required.' : 'Sign in to view this page.'}
          />
        </PageLayout>
      </>
    );
  }

  const sp = await searchParams;
  const actor = await loadActor();
  if (!actor) redirect('/login');

  const scope = asScope(sp.scope, 'mine');
  const role = actor.role_name ?? 'sales_rep';

  let rows: WaybillInboxRow[] = [];
  if (scope === 'all') {
    rows = await listAllOpenWaybills();
  } else if (scope === 'queue') {
    rows = await listAwaitingForActor(actor.id, role);
  } else {
    rows = await listMyWaybills(actor.id);
  }

  const salesRows = rows.filter((r) => r.origin === 'so');
  const [chips, summariesMap] = await Promise.all([
    loadSalesChipsForRows(salesRows.map((r) => r.origin_id)),
    loadApproverSummariesForRows(
      rows,
      (r) => (r.total_amount ? parseFloat(r.total_amount) : null),
    ),
  ]);
  const summaries = summariesMap;

  const activeDraft = scope === 'mine' ? await loadActiveSalesDraftForRep(actor.id) : null;
  const draftEvents = activeDraft ? await loadWaybillEvents(activeDraft.waybill_id) : [];
  const draftEventCount = draftEvents.length;

  const tabHref = (s: string) => `/sales?scope=${s}`;

  const subtitle = t('sales.subtitle', { role, scope });

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
          { label: <T id="breadcrumbs.home" locale={locale} />, href: '/' },
          { label: <T id="nav.sales" locale={locale} />, href: '/sales' },
        ]}
      />
      <PageLayout
        title={<T id="sales.title" locale={locale} />}
        subtitle={subtitle}
      >
        <nav className="mb-4 flex flex-wrap gap-2 text-xs font-mono">
          <a
            href={tabHref('mine')}
            aria-current={scope === 'mine' ? 'page' : undefined}
            className={
              'inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 transition-colors ' +
              (scope === 'mine'
                ? 'border-cyan-500 bg-cyan-500/15 text-cyan-200'
                : 'border-slate-700 text-slate-400 hover:border-slate-500 hover:text-slate-200')
            }
          >
            <span aria-hidden>📤</span>
            <span><T id="sales.tabMine" locale={locale} /></span>
            {rows.length > 0 && <span className="rounded-full bg-slate-800 px-1.5 text-xs">{rows.length}</span>}
          </a>
          <a
            href={tabHref('queue')}
            aria-current={scope === 'queue' ? 'page' : undefined}
            className={
              'inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 transition-colors ' +
              (scope === 'queue'
                ? 'border-cyan-500 bg-cyan-500/15 text-cyan-200'
                : 'border-slate-700 text-slate-400 hover:border-slate-500 hover:text-slate-200')
            }
          >
            <span aria-hidden>✅</span>
            <span><T id="sales.tabQueue" locale={locale} /></span>
          </a>
          <a
            href={tabHref('all')}
            aria-current={scope === 'all' ? 'page' : undefined}
            className={
              'inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 transition-colors ' +
              (scope === 'all'
                ? 'border-cyan-500 bg-cyan-500/15 text-cyan-200'
                : 'border-slate-700 text-slate-400 hover:border-slate-500 hover:text-slate-200')
            }
          >
            <span aria-hidden>🌐</span>
            <span><T id="sales.tabAll" locale={locale} /></span>
          </a>
          <Link
            href="/customers"
            className="ml-auto inline-flex items-center gap-1.5 rounded-lg border border-slate-700 px-3 py-1.5 font-mono text-slate-400 transition-colors hover:border-slate-500 hover:text-slate-200"
          >
            <span aria-hidden>🏢</span>
            <span><T id="sales.tabCustomers" locale={locale} /></span>
          </Link>
        </nav>

        {scope === 'mine' && (
          <section
            aria-label="draft-status"
            className="mb-6 flex flex-wrap items-center gap-3 rounded-2xl border border-slate-800 bg-slate-950/55 px-4 py-3 text-xs"
          >
            <span aria-hidden className="text-base">📝</span>
            <span className="font-bold text-slate-200"><T id="sales.draftTitle" locale={locale} /></span>
            {activeDraft ? (
              <>
                <span className="text-slate-500">·</span>
                <span className="font-mono text-cyan-300">{activeDraft.so_number}</span>
                <span className="text-slate-500">·</span>
                <span className="text-slate-300">{activeDraft.customer_name ?? '—'}</span>
                {activeDraft.total_amount && (
                  <>
                    <span className="text-slate-500">·</span>
                    <span className="font-mono text-emerald-300">
                      {parseFloat(activeDraft.total_amount).toLocaleString('th-TH', { minimumFractionDigits: 2 })} THB
                    </span>
                  </>
                )}
                <span className="text-slate-500">·</span>
                <span className="text-slate-400">
                  <T id="sales.draftSaved" locale={locale} values={{ age: fmtAge(activeDraft.draft_updated_at ? activeDraft.draft_updated_at.toISOString() : null) }} />
                </span>
                <span className="text-slate-500">·</span>
                <span className="text-slate-500">
                  <T id="sales.draftEvents" locale={locale} values={{ n: draftEventCount }} />
                </span>
                <a
                  href={`/waybill/${activeDraft.waybill_id}`}
                  className="ml-auto rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-3 py-1.5 font-mono text-cyan-200 hover:bg-cyan-500/20"
                >
                  <T id="sales.draftOpen" locale={locale} />
                </a>
              </>
            ) : (
              <>
                <span className="text-slate-500">·</span>
                <span className="text-slate-400"><T id="sales.draftEmpty" locale={locale} /></span>
              </>
            )}
          </section>
        )}

        {scope === 'mine' && (
          <div className="mb-8">
            <NewSalesPanel
              currentUserId={actor.id}
              initialDraft={
                activeDraft
                  ? {
                      waybillId: activeDraft.waybill_id,
                      salesOrderId: activeDraft.sales_order_id,
                      customerId: activeDraft.customer_id ?? null,
                      customerName: activeDraft.customer_name ?? null,
                      savedAt: activeDraft.draft_updated_at
                        ? new Date(activeDraft.draft_updated_at).toISOString()
                        : null,
                    }
                  : null
              }
            />
          </div>
        )}

        {salesRows.length > 0 && (
          <>
            <div className="mb-3 flex items-baseline justify-between">
              <h2 className="text-xs font-mono uppercase tracking-widest text-slate-500">
                {scope === 'mine'
                  ? <T id="sales.sectionMine" locale={locale} />
                  : <T id="sales.sectionOpen" locale={locale} values={{ n: salesRows.length }} />}
              </h2>
              <span className="text-xs font-mono text-slate-500">
                <T id="sales.rowClick" locale={locale} />
              </span>
            </div>
            <ul className="space-y-2">
              {salesRows.map((row) => {
                const amount = row.total_amount ? parseFloat(row.total_amount) : null;
                const displayStage = activeStageOf(row.current_stage);
                const summary = summaries.get(row.id) ?? null;
                const canAct = !!role && stageRoles(displayStage).includes(role);
                const chip = chips.get(row.origin_id);
                return (
                  <li
                    key={row.id}
                    className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-slate-800 bg-slate-950/60 p-4"
                  >
                    <div className="min-w-0 flex-1 space-y-1">
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-sm text-cyan-300">{row.id}</span>
                        <WaybillChip
                          domain="sales"
                          currentStage={displayStage}
                          amountTHB={amount}
                        />
                        {row.status !== 'rejected' && (
                          <ApproverChip
                            summary={summary}
                            view={canAct ? 'can-act' : 'awaiting'}
                          />
                        )}
                      </div>
                      <div className="text-sm text-slate-400">
                        {chip?.so_number ?? `SO-${row.origin_id}`} · {chip?.customer_name ?? '—'} ·{' '}
                        {(amount ?? 0).toLocaleString('th-TH', { minimumFractionDigits: 2 })} {row.currency}
                      </div>
                      <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs font-mono text-slate-500">
                        <span>
                          <T id="sales.rowSubmitter" locale={locale} values={{ name: row.submitter_name ?? '—' }} />
                        </span>
                        <span>
                          <T id="sales.rowAge" locale={locale} values={{ h: Math.max(0, Math.floor(row.age_hours)) }} />
                        </span>
                        {chip?.due_date && (
                          <span>
                            <T id="sales.rowDue" locale={locale} values={{ date: new Date(chip.due_date).toISOString().slice(0, 10) }} />
                          </span>
                        )}
                      </div>
                    </div>
                    <a
                      href={`/waybill/${row.id}`}
                      className="rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-3 py-1.5 text-xs font-mono text-cyan-200 hover:bg-cyan-500/20"
                    >
                      <T id="sales.rowOpen" locale={locale} />
                    </a>
                  </li>
                );
              })}
            </ul>
          </>
        )}

        {salesRows.length === 0 && scope !== 'mine' && (
          <div className="rounded-2xl border border-dashed border-slate-800 bg-slate-950/40 p-6 text-center text-sm text-slate-500">
            <T id="sales.sectionEmpty" locale={locale} />
          </div>
        )}
      </PageLayout>
    </>
  );
}