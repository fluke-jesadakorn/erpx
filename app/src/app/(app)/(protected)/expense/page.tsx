import React from 'react';
import Link from 'next/link';
import { redirect } from 'next/navigation';
import { loadActor } from '@folio-lib/server/guard';
import {
  listAllOpenWaybills,
  listMyWaybills,
  listAwaitingForActor,
  activeStageOf,
  loadActiveDraftForSubmitter,
  loadWaybillEvents,
  loadApproverSummariesForRows,
  type WaybillInboxRow,
} from '@folio-lib/waybill/queries';
import { WaybillChip } from '@/components/waybill/WaybillChip';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NewExpensePanel } from '@/components/waybill/NewExpensePanel';
import { PerTileChat } from '@/components/chat/PerTileChat';
import { ApproverChip } from '@/components/waybill/ApproverChip';
import { query } from '@folio-lib/db';
import { stageRoles } from '@folio-lib/waybill/derive';
import { loadVisionModels } from '@folio-lib/ai/loadVisionModels';
import { Bilingual } from '@/components/i18n/Bilingual';
import { headers } from 'next/headers';
import { loadActivePermSession } from '@folio-lib/perm/server';
import { loadSlipsForExpenses } from '@folio-lib/waybill/queries';
import { BookBankMini } from './_components/BookBankMini';
import { DocList } from './_components/DocList';

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

  return (
    <>
      <BreadcrumbSetter
        crumbs={[
          { label: 'Hub', href: '/' },
          { label: 'Expense', href: '/expense' },
        ]}
      />
      <PageLayout
        title="Expense · ใบส่งของ"
        subtitle={`My open Waybills · role=${role} · scope=${scope}`}
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
            <Bilingual en="Mine" th="ของฉัน" de="Meine" />
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
            <Bilingual en="My queue" th="งานรอฉัน" de="Meine Warteschlange" />
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
            <Bilingual en="All open" th="ทั้งหมดที่เปิดอยู่" de="Alle offenen" />
          </a>
          <Link
            href="/inbox"
            className="ml-auto inline-flex items-center gap-1.5 rounded-lg border border-slate-700 px-3 py-1.5 font-mono text-slate-400 transition-colors hover:border-slate-500 hover:text-slate-200"
          >
            <span aria-hidden>📥</span>
            <Bilingual en="Inbox" th="กล่องขาเข้า" de="Posteingang" />
          </Link>
        </nav>

        {scope === 'mine' && (
          <section
            aria-label="Draft status · โหมดร่าง"
            className="mb-6 flex flex-wrap items-center gap-3 rounded-2xl border border-slate-800 bg-slate-950/55 px-4 py-3 text-xs"
          >
            <span aria-hidden className="text-base">📝</span>
            <span className="font-bold text-slate-200">
              <Bilingual en="Draft mode" th="โหมดร่าง" de="Entwurfsmodus" />
            </span>
            {activeDraft ? (
              <>
                <span className="text-slate-500">·</span>
                <span className="font-mono text-cyan-300">{activeDraft.waybill_id}</span>
                {activeDraft.vendor_name && (
                  <>
                    <span className="text-slate-500">·</span>
                    <span className="text-slate-300">{activeDraft.vendor_name}</span>
                  </>
                )}
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
                  saved {fmtAge(activeDraft.draft_updated_at ? activeDraft.draft_updated_at.toISOString() : null)}
                </span>
                <span className="text-slate-500">·</span>
                <span className="text-slate-500">{draftEventCount} events</span>
                <a
                  href={`/waybill/${activeDraft.waybill_id}`}
                  className="ml-auto rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-3 py-1.5 font-mono text-cyan-200 hover:bg-cyan-500/20"
                >
                  Open waybill →
                </a>
              </>
            ) : (
              <>
                <span className="text-slate-500">·</span>
                <span className="text-slate-400">
                  no draft yet — upload a receipt below to auto-reserve{' '}
                  <code className="text-cyan-300">WB-YYYY-NNNNNN</code> (auto-saves every 10s, 24h TTL)
                </span>
              </>
            )}
          </section>
        )}

        {scope === 'mine' && (
          <div className="mb-8">
            <NewExpensePanel
              currentUserId={actor.id}
              initialModels={await loadVisionModels()}
            />
          </div>
        )}

        {rows.length > 0 && (
          <>
            <div className="mb-3 flex items-baseline justify-between">
              <h2 className="text-xs font-mono uppercase tracking-widest text-slate-500">
                {scope === 'mine' ? (
                  <Bilingual
                    en="My other in-flight waybills"
                    th="Waybill อื่น ๆ ที่กำลังดำเนินการ"
                    de="Weitere laufende Belege"
                  />
                ) : (
                  <Bilingual
                    en={`Open waybills (${rows.length})`}
                    th={`Waybills ที่เปิดอยู่ (${rows.length})`}
                    de={`Offene Belege (${rows.length})`}
                  />
                )}
              </h2>
              <span className="text-xs font-mono text-slate-500">
                <Bilingual en="click to open" th="คลิกเพื่อเปิด" de="zum Öffnen klicken" />
              </span>
            </div>
            <ul className="space-y-2">
              {rows.map((row) => {
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
                  <li
                    key={row.id}
                    className="flex flex-wrap items-center justify-between gap-3 rounded-2xl border border-slate-800 bg-slate-950/60 p-4"
                  >
                    <div className="min-w-0 flex-1 space-y-1">
                      <div className="flex items-center gap-2">
                        <span className="font-mono text-sm text-cyan-300">{row.id}</span>
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
                      <div className="text-sm text-slate-400">
                        {originLabel} · {row.vendor_name ?? '—'} ·{' '}
                        {(amount ?? 0).toLocaleString('th-TH', { minimumFractionDigits: 2 })} {row.currency}
                      </div>
                      <div className="flex flex-wrap gap-x-3 gap-y-1 text-xs font-mono text-slate-500">
                        <span>
                          <Bilingual en="submitter:" th="ผู้ส่ง:" de="Einreicher:" /> {row.submitter_name ?? '—'}
                        </span>
                        <span>
                          <Bilingual
                            en={`age ${Math.max(0, Math.floor(row.age_hours))}h`}
                            th={`อายุ ${Math.max(0, Math.floor(row.age_hours))} ชม.`}
                            de={`Alter ${Math.max(0, Math.floor(row.age_hours))} Std.`}
                          />
                        </span>
                        {art?.pr_number && (
                          <span className="text-cyan-300">
                            PR <Link href={`/pr/${art.pr_id}`} className="hover:underline">{art.pr_number}</Link>
                          </span>
                        )}
                        {art?.po_number && (
                          <span className="text-cyan-300">
                            PO <Link href={`/po/${art.po_id}`} className="hover:underline">{art.po_number}</Link>
                          </span>
                        )}
                        {art?.jv_id != null && (
                          <span className="text-emerald-300">GL #{art.jv_id}</span>
                        )}
                      </div>
                    </div>
                    {row.origin === 'expense' && (
                      <div className="mt-2 space-y-1">
                        <BookBankMini
                          slips={slipMaps.get(row.origin_id) ?? []}
                          waybillId={row.id}
                          currentStage={row.current_stage}
                        />
                        <DocList waybillId={row.id} currentStage={row.current_stage} />
                      </div>
                    )}
                    <a
                      href={`/waybill/${row.id}`}
                      className="rounded-lg border border-cyan-500/40 bg-cyan-500/10 px-3 py-1.5 text-xs font-mono text-cyan-200 hover:bg-cyan-500/20"
                    >
                      <Bilingual en="Open →" th="เปิด →" de="Öffnen →" />
                    </a>
                  </li>
                );
              })}
            </ul>
          </>
        )}

        {rows.length === 0 && scope !== 'mine' && (
          <div className="rounded-2xl border border-dashed border-slate-800 bg-slate-950/40 p-6 text-center text-sm text-slate-500">
            <Bilingual en="No Waybills in this scope." th="ไม่มี Waybill ในขอบเขตนี้" de="Keine Belege in diesem Bereich." />
          </div>
        )}
      </PageLayout>

      <PerTileChat
        tileId="expense"
        sectionKey="chat:expense"
        displayName="Expense"
        lang="en"
        expenseDraftId={activeDraft?.waybill_id}
      />
    </>
  );
}
