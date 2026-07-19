import React from 'react';
import Link from 'next/link';
import { redirect } from 'next/navigation';
import { loadActor } from '@/server/guard';
import {
  listAllOpenWaybills,
  listMyWaybills,
  listAwaitingForActor,
  activeStageOf,
  loadActiveDraftForSubmitter,
  loadWaybillEvents,
  loadApproverSummariesForRows,
  type WaybillInboxRow,
} from '@/waybill/queries';
import { WaybillChip } from '@/components/waybill/WaybillChip';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NewExpensePanel } from '@/components/waybill/NewExpensePanel';
import { ApproverChip } from '@/components/waybill/ApproverChip';
import { ListRow } from '@/components/ui/ListRow';
import { Empty } from '@/components/ui/Empty';
import { query } from '@/db';
import { stageRoles } from '@/waybill/derive';
import { loadVisionModels } from '@/ai/loadVisionModels';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { formatMoneyServer } from '@/components/i18n/formattersServer';
import { headers } from 'next/headers';
import { loadActivePermSession } from '@/perm/server';
import { loadSlipsForExpenses } from '@/waybill/queries';
import { ReceiptBankCard } from './_components/ReceiptBankCard';
import { CheckCircle2, FilePenLine, Globe2, Inbox, Send } from 'lucide-react';

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

interface ArtifactChipRow {
  expense_id: number;
  pr_id: number | null;
  pr_number: string | null;
  po_id: number | null;
  po_number: string | null;
  jv_id: number | null;
}

async function loadArtifactsForExpenses(
  expenseIds: number[],
): Promise<Map<number, ArtifactChipRow>> {
  const out = new Map<number, ArtifactChipRow>();
  if (expenseIds.length === 0) return out;
  const r = await query<ArtifactChipRow>(
    `SELECT e.id            AS expense_id,
            e.pr_id,
            pr.pr_number,
            e.po_id,
            po.po_number,
            e.journal_entry_id AS jv_id
       FROM expenses e
       LEFT JOIN purchase_requisitions pr ON pr.id = e.pr_id
       LEFT JOIN purchase_orders po ON po.id = e.po_id
       WHERE e.id = ANY($1::int[])`,
    [expenseIds],
  );
  for (const row of r.rows) out.set(row.expense_id, row);
  return out;
}

export default async function ExpenseInboxPage({ searchParams }: PageProps) {
  const sp = await searchParams;

  const h = await headers();
  const out = await loadActivePermSession(
    new Request('http://internal', { headers: h as unknown as HeadersInit }),
  );
  if (!out) redirect('/login');
  const locale = await getSecondaryLocale();

  const actor = await loadActor();
  if (!actor) redirect('/login');

  const scope = asScope(sp.scope, 'mine');
  const role = actor.role_name ?? 'officer';

  let rows: WaybillInboxRow[] = [];
  if (scope === 'all') {
    rows = await listAllOpenWaybills();
  } else if (scope === 'queue') {
    rows = await listAwaitingForActor(actor.id, role);
  } else {
    rows = await listMyWaybills(actor.id);
  }

  const expenseRows = rows.filter((r) => r.origin === 'expense');
  const [artifacts, summariesMap, slipMaps] = await Promise.all([
    loadArtifactsForExpenses(expenseRows.map((r) => r.origin_id)),
    loadApproverSummariesForRows(
      rows,
      (r) => (r.total_amount ? parseFloat(r.total_amount) : null),
    ),
    loadSlipsForExpenses(expenseRows.map((r) => r.origin_id)),
  ]);
  const summaries = summariesMap;

  const activeDraft = scope === 'mine' ? await loadActiveDraftForSubmitter(actor.id) : null;
  const draftEvents = activeDraft ? await loadWaybillEvents(activeDraft.waybill_id) : [];
  const draftEventCount = draftEvents.length;
  const tabHref = (s: string) => `/expense?scope=${s}`;
  const formattedAmounts = await Promise.all(rows.map(r => formatMoneyServer(
    r.total_amount ? parseFloat(r.total_amount) : null,
    locale,
    r.currency,
  )));

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
          { label: <T id="breadcrumbs.home" locale={locale} />, href: '/' },
          { label: <T id="nav.expense" locale={locale} />, href: '/expense' },
        ]}
      />
      <PageLayout
        title={<T id="expense.title" locale={locale} />}
        subtitle={<T id="expense.subtitle" locale={locale} values={{ role, scope }} />}
        width="wide"
      >
        <nav className="glass-toolbar mb-5 flex flex-wrap gap-2 p-2 text-xs font-mono">
          <a
            href={tabHref('mine')}
            aria-current={scope === 'mine' ? 'page' : undefined}
            className={
              'inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 transition-colors ' +
              (scope === 'mine'
                ? 'border-accent/45 bg-accent-soft text-accent'
                : 'border-rule text-ink-2 hover:border-rule hover:text-ink')
            }
          >
            <Send size={13} aria-hidden />
            <T id="expense.tabMine" locale={locale} />
            {rows.length > 0 && <span className="rounded-full bg-paper-2 px-1.5 text-xs">{rows.length}</span>}
          </a>
          <a
            href={tabHref('queue')}
            aria-current={scope === 'queue' ? 'page' : undefined}
            className={
              'inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 transition-colors ' +
              (scope === 'queue'
                ? 'border-accent/45 bg-accent-soft text-accent'
                : 'border-rule text-ink-2 hover:border-rule hover:text-ink')
            }
          >
            <CheckCircle2 size={13} aria-hidden />
            <T id="expense.tabQueue" locale={locale} />
          </a>
          <a
            href={tabHref('all')}
            aria-current={scope === 'all' ? 'page' : undefined}
            className={
              'inline-flex items-center gap-1.5 rounded-lg border px-3 py-1.5 transition-colors ' +
              (scope === 'all'
                ? 'border-accent/45 bg-accent-soft text-accent'
                : 'border-rule text-ink-2 hover:border-rule hover:text-ink')
            }
          >
            <Globe2 size={13} aria-hidden />
            <T id="expense.tabAll" locale={locale} />
          </a>
          <Link
            href="/inbox"
            className="ml-auto inline-flex items-center gap-1.5 rounded-lg border border-rule px-3 py-1.5 font-mono text-ink-2 transition-colors hover:border-rule hover:text-ink"
          >
            <Inbox size={13} aria-hidden />
            <T id="nav.inbox" locale={locale} />
          </Link>
        </nav>

        {scope === 'mine' && (
          <section
            aria-label="draft-status"
            className="panel mb-6 flex flex-wrap items-center gap-3 px-4 py-3 text-xs"
          >
            <FilePenLine size={16} aria-hidden className="text-accent" />
            <span className="font-bold text-ink">
              <T id="expense.draftMode" locale={locale} />
            </span>
            {activeDraft ? (
              <>
                <span className="text-mute">·</span>
                <span className="font-mono text-info">{activeDraft.waybill_id}</span>
                {activeDraft.vendor_name && (
                  <>
                    <span className="text-mute">·</span>
                    <span className="text-ink-2">{activeDraft.vendor_name}</span>
                  </>
                )}
                {activeDraft.total_amount && (
                  <>
                    <span className="text-mute">·</span>
                    <span className="font-mono text-positive">
                      {await formatMoneyServer(activeDraft.total_amount, locale)}
                    </span>
                  </>
                )}
                <span className="text-mute">·</span>
                <span className="text-ink-2">
                  <T id="expense.draftSaved" locale={locale} values={{ age: fmtAge(activeDraft.draft_updated_at ? activeDraft.draft_updated_at.toISOString() : null) }} />
                </span>
                <span className="text-mute">·</span>
                <span className="text-mute">
                  <T id="expense.draftEvents" locale={locale} values={{ n: draftEventCount }} />
                </span>
                <a
                  href={`/waybill/${activeDraft.waybill_id}`}
                  className="ml-auto rounded-lg border border-info/40 bg-info px-3 py-1.5 font-mono text-info hover:bg-info"
                >
                  <T id="expense.openWaybill" locale={locale} />
                </a>
              </>
            ) : (
              <>
                <span className="text-mute">·</span>
                <span className="text-ink-2">
                  <T id="expense.draftEmpty" locale={locale} />
                </span>
              </>
            )}
          </section>
        )}

        {scope === 'mine' && (
          <div className="mb-8">
            <NewExpensePanel
              currentUserId={actor.id}
              initialModels={await loadVisionModels(actor.id)}
            />
          </div>
        )}

        {rows.length > 0 && (
          <>
            <div className="mb-3 flex items-baseline justify-between">
              <h2 className="text-xs font-mono uppercase tracking-widest text-mute">
                {scope === 'mine' ? (
                  <T id="expense.sectionMine" locale={locale} />
                ) : (
                  <T id="expense.sectionOpen" locale={locale} values={{ n: rows.length }} />
                )}
              </h2>
              <span className="text-xs font-mono text-mute">
                <T id="expense.clickToOpen" locale={locale} />
              </span>
            </div>
            <ul className="space-y-2">
              {rows.map((row, index) => {
                const domain = row.origin === 'expense' ? 'expense' : 'procurement';
                const amount = row.total_amount ? parseFloat(row.total_amount) : null;
                const originLabel =
                  row.origin === 'expense'
                    ? `EXP-${row.origin_id}`
                    : row.origin === 'pr'
                    ? `PR-${row.origin_id}`
                    : `PO-${row.origin_id}`;
                const displayStage = activeStageOf(row.current_stage);
                const summary = summaries.get(row.id) ?? null;
                const canAct = !!role && stageRoles(displayStage).includes(role);
                const art = row.origin === 'expense' ? artifacts.get(row.origin_id) : null;
                return (
                  <ListRow
                    key={row.id}
                    className="flex-wrap justify-between"
                  >
                    <div className="min-w-0 flex-1 space-y-1">
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-sm text-info">{row.id}</span>
                        <WaybillChip
                          domain={domain}
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
                      <div className="text-sm text-ink-2">
                        {originLabel} · {row.vendor_name ?? '—'} ·{' '}
                        {formattedAmounts[index]}
                      </div>
                      <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs font-mono text-mute">
                        <span>
                          <T id="expense.submitter" locale={locale} /> {row.submitter_name ?? '—'}
                        </span>
                        <span>
                          <T id="expense.ageHours" locale={locale} values={{ h: Math.max(0, Math.floor(row.age_hours)) }} />
                        </span>
                        {art?.pr_number && (
                          <span className="text-info">
                            PR <Link href={`/pr/${art.pr_id}`} className="hover:underline">{art.pr_number}</Link>
                          </span>
                        )}
                        {art?.po_number && (
                          <span className="text-info">
                            PO <Link href={`/po/${art.po_id}`} className="hover:underline">{art.po_number}</Link>
                          </span>
                        )}
                        {art?.jv_id != null && (
                          <span className="text-positive">GL #{art.jv_id}</span>
                        )}
                      </div>
                    </div>
                    {row.origin === 'expense' && (
                      <div className="basis-full mt-1">
                        <ReceiptBankCard
                          waybillId={row.id}
                          vendorName={row.vendor_name ?? null}
                          totalAmount={row.total_amount ?? null}
                          currency={row.currency}
                          slips={slipMaps.get(row.origin_id) ?? []}
                        />
                      </div>
                    )}
                    <a
                      href={`/waybill/${row.id}`}
                      className="rounded-lg border border-info/40 bg-info px-3 py-1.5 text-xs font-mono text-info hover:bg-info"
                    >
                      <T id="expense.openArrow" locale={locale} />
                    </a>
                  </ListRow>
                );
              })}
            </ul>
          </>
        )}

        {rows.length === 0 && scope !== 'mine' && (
          <Empty
            title={<T id="expense.noWaybills" locale={locale} />}
            body="Items will appear here once available."
          />
        )}
      </PageLayout>
    </>
  );
}
