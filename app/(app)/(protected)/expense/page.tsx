import React from 'react';
import Link from 'next/link';
import { redirect } from 'next/navigation';
import { headers } from 'next/headers';
import {
  ArrowRight,
  Clock3,
  FilePenLine,
  Globe2,
  Inbox,
  Landmark,
  Paperclip,
  Receipt,
} from 'lucide-react';
import { loadActor } from '@/server/guard';
import {
  activeStageOf,
  listAllOpenWaybills,
  listMyWaybills,
  loadActiveDraftForSubmitter,
  loadSlipsForExpenses,
  type WaybillInboxRow,
} from '@/waybill/queries';
import { WaybillChip } from '@/components/waybill/WaybillChip';
import { PageLayout } from '@/components/PageLayout';
import { BreadcrumbSetter } from '@/components/breadcrumbs/BreadcrumbSetter';
import { NewExpensePanel } from '@/components/waybill/NewExpensePanel';
import { ListRow } from '@/components/ui/ListRow';
import { Empty } from '@/components/ui/Empty';
import { loadVisionModels } from '@/ai/loadVisionModels';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';
import { formatMoneyServer } from '@/components/i18n/formattersServer';
import { loadActivePermSession } from '@/perm/server';

export const dynamic = 'force-dynamic';

interface PageProps {
  searchParams: Promise<Record<string, string | string[] | undefined>>;
}

function asScope(value: string | string[] | undefined): 'mine' | 'all' | 'queue' {
  const scope = Array.isArray(value) ? value[0] : value;
  if (scope === 'all' || scope === 'queue') return scope;
  return 'mine';
}

function ageHours(value: number): number {
  return Math.max(0, Math.floor(value));
}

export default async function ExpensePage({ searchParams }: PageProps) {
  const sp = await searchParams;
  const h = await headers();
  const session = await loadActivePermSession(
    new Request('http://internal', { headers: h as unknown as HeadersInit }),
  );
  if (!session) redirect('/login');

  const actor = await loadActor();
  if (!actor) redirect('/login');

  const scope = asScope(sp.scope);
  if (scope === 'queue') redirect('/inbox?view=actions&domain=expense');

  const locale = await getSecondaryLocale();
  const [sourceRows, activeDraft, visionModels] = await Promise.all([
    scope === 'all' ? listAllOpenWaybills() : listMyWaybills(actor.id),
    scope === 'mine' ? loadActiveDraftForSubmitter(actor.id) : Promise.resolve(null),
    scope === 'mine' ? loadVisionModels(actor.id) : Promise.resolve([]),
  ]);
  const rows: WaybillInboxRow[] = scope === 'mine'
    ? sourceRows.filter((row) => row.origin === 'expense')
    : sourceRows;
  const expenseRows = rows.filter((row) => row.origin === 'expense');
  const slipMaps = await loadSlipsForExpenses(expenseRows.map((row) => row.origin_id));
  const amounts = await Promise.all(rows.map((row) => formatMoneyServer(
    row.total_amount ? parseFloat(row.total_amount) : null,
    locale,
    row.currency,
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
        subtitle={<T id={scope === 'all' ? 'expense.allOpenSubtitle' : 'expense.submitterSubtitle'} locale={locale} />}
        width="wide"
      >
        {scope === 'mine' ? (
          <>
            {activeDraft && (
              <Link
                href={`/waybill/${activeDraft.waybill_id}`}
                className="panel mb-4 flex min-h-12 flex-wrap items-center gap-2 px-4 py-2.5 text-sm transition hover:border-info/50"
              >
                <FilePenLine className="size-4 shrink-0 text-info" aria-hidden />
                <span className="font-semibold text-ink"><T id="expense.resumeDraft" locale={locale} /></span>
                <span className="font-mono text-info">{activeDraft.waybill_id}</span>
                {activeDraft.vendor_name && <span className="truncate text-ink-2">· {activeDraft.vendor_name}</span>}
                <ArrowRight className="ml-auto size-4 shrink-0 text-info" aria-hidden />
              </Link>
            )}

            <NewExpensePanel currentUserId={actor.id} initialModels={visionModels} />

            <nav className="mb-6 flex flex-wrap items-center gap-2" aria-label="Expense views">
              <Link
                href="/inbox?view=actions&domain=expense"
                className="inline-flex min-h-10 items-center gap-2 rounded-lg border border-rule bg-paper-2 px-3 text-sm font-semibold text-ink-2 transition hover:border-info/50 hover:text-info"
              >
                <Inbox className="size-4" aria-hidden />
                <T id="expense.approvalQueue" locale={locale} />
              </Link>
              <Link
                href="/expense?scope=all"
                className="inline-flex min-h-10 items-center gap-2 rounded-lg border border-rule bg-paper-2 px-3 text-sm font-semibold text-ink-2 transition hover:border-info/50 hover:text-info"
              >
                <Globe2 className="size-4" aria-hidden />
                <T id="expense.tabAll" locale={locale} />
              </Link>
            </nav>
          </>
        ) : (
          <nav className="mb-5 flex flex-wrap items-center gap-2" aria-label="Expense views">
            <Link
              href="/expense"
              className="inline-flex min-h-10 items-center gap-2 rounded-lg border border-accent/40 bg-accent-soft px-3 text-sm font-semibold text-accent-strong"
            >
              <Receipt className="size-4" aria-hidden />
              <T id="expense.newClaimCta" locale={locale} />
            </Link>
            <Link
              href="/inbox?view=actions&domain=expense"
              className="inline-flex min-h-10 items-center gap-2 rounded-lg border border-rule bg-paper-2 px-3 text-sm font-semibold text-ink-2"
            >
              <Inbox className="size-4" aria-hidden />
              <T id="expense.approvalQueue" locale={locale} />
            </Link>
          </nav>
        )}

        <section aria-labelledby="expense-active-title">
          <div className="mb-3 flex flex-wrap items-end justify-between gap-2">
            <div>
              <h2 id="expense-active-title" className="text-lg font-semibold text-ink">
                <T id={scope === 'mine' ? 'expense.activeClaims' : 'expense.sectionOpen'} locale={locale} values={{ n: rows.length }} />
              </h2>
              <p className="mt-0.5 text-sm text-ink-2">
                <T id={scope === 'mine' ? 'expense.activeClaimsHint' : 'expense.allOpenHint'} locale={locale} />
              </p>
            </div>
            <span className="rounded-full border border-rule bg-paper-3 px-2.5 py-1 text-xs font-mono text-mute">{rows.length}</span>
          </div>

          {rows.length > 0 ? (
            <div className="panel overflow-hidden">
              {rows.map((row, index) => {
                const domain = row.origin === 'expense' ? 'expense' : 'procurement';
                const amount = row.total_amount ? parseFloat(row.total_amount) : null;
                const originLabel = row.origin === 'expense'
                  ? `EXP-${row.origin_id}`
                  : row.origin === 'pr' ? `PR-${row.origin_id}` : `PO-${row.origin_id}`;
                const slips = row.origin === 'expense' ? slipMaps.get(row.origin_id) ?? [] : [];
                const hasBank = slips.some((slip) => slip.kind === 'book_bank' || slip.kind === 'book-bank');
                return (
                  <ListRow key={row.id} href={`/waybill/${row.id}`} className="items-start sm:items-center">
                    <span className="mt-0.5 grid size-10 shrink-0 place-items-center rounded-lg border border-rule bg-paper-3 text-accent sm:mt-0">
                      <Receipt className="size-4" aria-hidden />
                    </span>
                    <div className="min-w-0 flex-1">
                      <div className="flex flex-wrap items-center gap-2">
                        <span className="font-mono text-sm font-semibold text-info">{row.id}</span>
                        <WaybillChip domain={domain} currentStage={activeStageOf(row.current_stage)} amountTHB={amount} />
                      </div>
                      <p className="mt-1 truncate text-sm font-semibold text-ink">
                        {row.vendor_name ?? originLabel}
                      </p>
                      <div className="mt-1.5 flex flex-wrap items-center gap-x-3 gap-y-1 text-xs text-mute">
                        <span className="font-mono text-positive">{amounts[index]}</span>
                        <span className="inline-flex items-center gap-1">
                          <Clock3 className="size-3" aria-hidden />
                          <T id="expense.ageHours" locale={locale} values={{ h: ageHours(row.age_hours) }} />
                        </span>
                        {row.origin === 'expense' && (
                          <span className="inline-flex items-center gap-1">
                            <Paperclip className="size-3" aria-hidden />
                            {slips.length} <T id={slips.length === 1 ? 'expense.fileOne' : 'expense.fileMany'} locale={locale} />
                          </span>
                        )}
                        {hasBank && (
                          <span className="inline-flex items-center gap-1 text-info">
                            <Landmark className="size-3" aria-hidden />
                            <T id="expense.bankAttached" locale={locale} />
                          </span>
                        )}
                      </div>
                    </div>
                    <ArrowRight className="mt-3 size-4 shrink-0 text-mute sm:mt-0" aria-hidden />
                  </ListRow>
                );
              })}
            </div>
          ) : (
            <Empty
              icon={Receipt}
              title={<T id={scope === 'mine' ? 'expense.noActiveClaims' : 'expense.noWaybills'} locale={locale} />}
              body={<T id={scope === 'mine' ? 'expense.noActiveClaimsHint' : 'expense.allOpenEmpty'} locale={locale} />}
            />
          )}
        </section>
      </PageLayout>
    </>
  );
}
