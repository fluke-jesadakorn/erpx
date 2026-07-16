import React from 'react';
import type { JournalLineRow } from '@/waybill/queries';
import { fmtMoney } from './ui';
import { T } from '@/components/i18n/TServer';
import { getSecondaryLocale } from '@/server/locale';

type Locale = 'th' | 'de';

function pickAccountName(
  account: { name: string | null; name_th: string | null },
  locale: Locale,
): string {
  const th = account.name_th ?? null;
  const en = account.name ?? null;
  if (locale === 'de') return th ?? en ?? '—';
  if (locale === 'th') return th ?? en ?? '—';
  return en ?? th ?? '—';
}

export function summarizeLines(lines: JournalLineRow[]): { totalDebit: number; totalCredit: number; balanced: boolean } {
  const totalDebit = lines.reduce((s, l) => s + l.debit, 0);
  const totalCredit = lines.reduce((s, l) => s + l.credit, 0);
  return { totalDebit, totalCredit, balanced: Math.abs(totalDebit - totalCredit) < 0.005 };
}

export function LineRow({
  account,
  debit,
  credit,
  description,
}: {
  account: { code: string; name: string | null; name_th: string | null };
  debit: number;
  credit: number;
  description: string | null;
  locale: Locale;
}) {
  return (
    <tr className="align-top font-mono text-ink-2">
      <td className="border-b border-rule px-2 py-2 text-info">{account.code}</td>
      <td className="border-b border-rule px-2 py-2 text-ink">
        {pickAccountName(account, 'th')}
      </td>
      <td className="border-b border-rule px-2 py-2 text-right text-positive tabular-nums">
        {debit > 0 ? fmtMoney(debit, '').trim() : ''}
      </td>
      <td className="border-b border-rule px-2 py-2 text-right text-caution tabular-nums">
        {credit > 0 ? fmtMoney(credit, '').trim() : ''}
      </td>
      <td className="border-b border-rule px-2 py-2 text-ink-2">{description ?? ''}</td>
    </tr>
  );
}

export async function LinesTable({
  lines,
}: {
  lines: JournalLineRow[];
  locale: Locale;
}) {
  const localeResolved = await getSecondaryLocale();
  void localeResolved;

  if (lines.length === 0) {
    return (
      <p className="glass-panel rounded-md border p-3 text-sm italic text-ink-2">
        <T id="waybill.gl.noLines" locale={localeResolved} />
      </p>
    );
  }
  const totalDebit = lines.reduce((s, l) => s + l.debit, 0);
  const totalCredit = lines.reduce((s, l) => s + l.credit, 0);
  const balanced = Math.abs(totalDebit - totalCredit) < 0.005;
  return (
    <div className="overflow-x-auto">
      <table className="w-full min-w-[640px] border-collapse text-xs">
        <thead>
          <tr className="text-left font-mono text-sm uppercase tracking-widest text-ink-2">
            <th className="border-b border-rule px-2 py-1.5">
              <T id="waybill.gl.code" locale={localeResolved} />
            </th>
            <th className="border-b border-rule px-2 py-1.5">
              <T id="waybill.gl.account" locale={localeResolved} />
            </th>
            <th className="border-b border-rule px-2 py-1.5 text-right">
              <T id="waybill.gl.debit" locale={localeResolved} />
            </th>
            <th className="border-b border-rule px-2 py-1.5 text-right">
              <T id="waybill.gl.credit" locale={localeResolved} />
            </th>
            <th className="border-b border-rule px-2 py-1.5">
              <T id="waybill.gl.description" locale={localeResolved} />
            </th>
          </tr>
        </thead>
        <tbody>
          {lines.map((l, i) => (
            <LineRow
              key={i}
              account={{ code: l.account_code, name: l.account_name, name_th: l.account_name_th }}
              debit={l.debit}
              credit={l.credit}
              description={l.description}
              locale="th"
            />
          ))}
        </tbody>
        <tfoot>
          <tr className="font-mono text-sm font-bold uppercase tracking-wider">
            <td className="border-t border-rule-strong px-2 py-2 text-ink-2" colSpan={2}>
              <T id="waybill.gl.total" locale={localeResolved} />
            </td>
            <td className="border-t border-rule-strong px-2 py-2 text-right text-positive tabular-nums">
              {fmtMoney(totalDebit, '').trim()}
            </td>
            <td className="border-t border-rule-strong px-2 py-2 text-right text-caution tabular-nums">
              {fmtMoney(totalCredit, '').trim()}
            </td>
            <td className="border-t border-rule-strong px-2 py-2">
              {balanced ? (
                <span className="inline-flex items-center gap-1 rounded-full border border-positive/40 bg-positive-soft px-2 py-0.5 text-positive-strong">
                  <span aria-hidden>✓</span>
                  <span>
                    <T id="waybill.gl.balanced" locale={localeResolved} />
                  </span>
                </span>
              ) : (
                <span className="inline-flex items-center gap-1 rounded-full border border-critical/40 bg-critical-soft px-2 py-0.5 text-critical-strong">
                  <span aria-hidden>⚠</span>
                  <span>
                    <T id="waybill.gl.unbalanced" locale={localeResolved} />
                  </span>
                </span>
              )}
            </td>
          </tr>
        </tfoot>
      </table>
    </div>
  );
}

export interface DraftBlock {
  journal_id: number;
  entry_date: Date | string | null;
  description: string | null;
  lines: JournalLineRow[];
}

export interface PostedBlock {
  journal_id: number;
  entry_date: Date | string | null;
  description: string | null;
  finalized_by: number | null;
  finalized_by_name: string | null;
  finalized_at: Date | string | null;
  lines: JournalLineRow[];
}

export interface EventRef {
  actor_id: number | null;
  actor_name: string | null;
  occurred_at: Date | string;
}

export interface JournalStepView {
  draft: DraftBlock | null;
  posted: PostedBlock | null;
  posted_event: EventRef | null;
  confirmed_event: EventRef | null;
}

export interface GlJournalEntry {
  draft: DraftBlock | null;
  posted: PostedBlock | null;
  posted_event: EventRef | null;
  confirmed_event: EventRef | null;
}

export async function GlLinesView({
  draft,
  posted,
  locale,
  totalsOverride,
}: {
  draft: DraftBlock | null;
  posted: PostedBlock | null;
  locale: Locale;
  totalsOverride?: { totalDebit: number; totalCredit: number; balanced: boolean };
}) {
  const localeResolved = await getSecondaryLocale();
  void locale;
  return (
    <div className="space-y-4">
      {draft && (
        <section className="space-y-2 rounded-xl border border-info/40 bg-info-soft/40 p-3">
          <header className="flex flex-wrap items-baseline justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="rounded-md border border-info/50 bg-info/15 px-2 py-0.5 text-sm font-mono font-bold uppercase text-info-strong">
                📝 DRAFT
              </span>
              <span className="font-mono text-sm text-ink-2">JE #{draft.journal_id}</span>
            </div>
            <span className="font-mono text-sm text-ink-2">
              entry_date: <span className="text-info">{draft.entry_date as any}</span>
            </span>
          </header>
          {draft.description && <p className="text-sm text-ink">{draft.description}</p>}
          <LinesTable lines={draft.lines} locale={localeResolved} />
        </section>
      )}

      {posted && (
        <section className="space-y-2 rounded-xl border border-positive/40 bg-positive-soft/40 p-3">
          <header className="flex flex-wrap items-baseline justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="rounded-md border border-positive/50 bg-positive/15 px-2 py-0.5 text-sm font-mono font-bold uppercase text-positive-strong">
                ✓ POSTED
              </span>
              <span className="font-mono text-sm text-ink-2">JE #{posted.journal_id}</span>
            </div>
            <span className="font-mono text-sm text-ink-2">
              entry_date: <span className="text-positive">{posted.entry_date as any}</span>
            </span>
          </header>
          {posted.description && <p className="text-sm text-ink">{posted.description}</p>}
          <LinesTable lines={posted.lines} locale={localeResolved} />
        </section>
      )}

      {totalsOverride && (
        <p className="text-sm font-mono text-ink-2">
          Σ {fmtMoney(totalsOverride.totalDebit, '').trim()} / {fmtMoney(totalsOverride.totalCredit, '').trim()}{' '}
          {totalsOverride.balanced ? '✓ balanced' : '⚠ unbalanced'}
        </p>
      )}
    </div>
  );
}
