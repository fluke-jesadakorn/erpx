import React from 'react';
import type { JournalLineRow } from '@folio-lib/waybill/queries';
import { fmtMoney } from './ui';
import { Bilingual } from '@/components/i18n/Bilingual';

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
  locale,
}: {
  account: { code: string; name: string | null; name_th: string | null };
  debit: number;
  credit: number;
  description: string | null;
  locale: Locale;
}) {
  return (
    <tr className="align-top font-mono text-slate-300">
      <td className="border-b border-slate-800/40 px-2 py-2 text-cyan-300">{account.code}</td>
      <td className="border-b border-slate-800/40 px-2 py-2 text-slate-200">
        {pickAccountName(account, locale)}
      </td>
      <td className="border-b border-slate-800/40 px-2 py-2 text-right text-emerald-200 tabular-nums">
        {debit > 0 ? fmtMoney(debit, '').trim() : ''}
      </td>
      <td className="border-b border-slate-800/40 px-2 py-2 text-right text-amber-200 tabular-nums">
        {credit > 0 ? fmtMoney(credit, '').trim() : ''}
      </td>
      <td className="border-b border-slate-800/40 px-2 py-2 text-slate-500">{description ?? ''}</td>
    </tr>
  );
}

export function LinesTable({
  lines,
  locale,
}: {
  lines: JournalLineRow[];
  locale: Locale;
}) {
  if (lines.length === 0) {
    return (
      <p className="glass-panel rounded-md border p-3 text-sm italic text-slate-500">
        {<Bilingual en="no ledger lines" th="ไม่มีรายการบัญชี" de="keine Buchungszeilen" locale={locale} />}
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
          <tr className="text-left font-mono text-sm uppercase tracking-widest text-slate-500">
            <th className="border-b border-slate-800/60 px-2 py-1.5">code</th>
            <th className="border-b border-slate-800/60 px-2 py-1.5">
              {<Bilingual en="account" th="ชื่อบัญชี" de="Konto" locale={locale} />}
            </th>
            <th className="border-b border-slate-800/60 px-2 py-1.5 text-right">
              {<Bilingual en="debit" th="เดบิต" de="Soll" locale={locale} />}
            </th>
            <th className="border-b border-slate-800/60 px-2 py-1.5 text-right">
              {<Bilingual en="credit" th="เครดิต" de="Haben" locale={locale} />}
            </th>
            <th className="border-b border-slate-800/60 px-2 py-1.5">
              {<Bilingual en="description" th="รายละเอียด" de="Beschreibung" locale={locale} />}
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
              locale={locale}
            />
          ))}
        </tbody>
        <tfoot>
          <tr className="font-mono text-sm font-bold uppercase tracking-wider">
            <td className="border-t border-slate-700/60 px-2 py-2 text-slate-400" colSpan={2}>
              {<Bilingual en="total" th="รวม" de="Summe" locale={locale} />}
            </td>
            <td className="border-t border-slate-700/60 px-2 py-2 text-right text-emerald-200 tabular-nums">
              {fmtMoney(totalDebit, '').trim()}
            </td>
            <td className="border-t border-slate-700/60 px-2 py-2 text-right text-amber-200 tabular-nums">
              {fmtMoney(totalCredit, '').trim()}
            </td>
            <td className="border-t border-slate-700/60 px-2 py-2">
              {balanced ? (
                <span className="inline-flex items-center gap-1 rounded-full border border-emerald-500/40 bg-emerald-500/15 px-2 py-0.5 text-emerald-200">
                  <span aria-hidden>✓</span>
                  <span>
                    {<Bilingual en="balanced" th="สมดุล" de="ausgeglichen" locale={locale} />}
                  </span>
                </span>
              ) : (
                <span className="inline-flex items-center gap-1 rounded-full border border-rose-500/40 bg-rose-500/15 px-2 py-0.5 text-rose-200">
                  <span aria-hidden>⚠</span>
                  <span>
                    {<Bilingual en="unbalanced" th="ไม่สมดุล" de="nicht ausgeglichen" locale={locale} />}
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

export function GlLinesView({
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
  return (
    <div className="space-y-4">
      {draft && (
        <section className="space-y-2 rounded-xl border border-cyan-500/40 bg-cyan-950/15 p-3">
          <header className="flex flex-wrap items-baseline justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="rounded-md border border-cyan-400/40 bg-cyan-500/15 px-2 py-0.5 text-sm font-mono font-bold uppercase text-cyan-200">
                📝 DRAFT
              </span>
              <span className="font-mono text-sm text-slate-500">JE #{draft.journal_id}</span>
            </div>
            <span className="font-mono text-sm text-slate-500">
              entry_date: <span className="text-cyan-300">{draft.entry_date as any}</span>
            </span>
          </header>
          {draft.description && <p className="text-sm text-slate-300">{draft.description}</p>}
          <LinesTable lines={draft.lines} locale={locale} />
        </section>
      )}

      {posted && (
        <section className="space-y-2 rounded-xl border border-emerald-500/40 bg-emerald-950/15 p-3">
          <header className="flex flex-wrap items-baseline justify-between gap-2">
            <div className="flex items-center gap-2">
              <span className="rounded-md border border-emerald-400/40 bg-emerald-500/15 px-2 py-0.5 text-sm font-mono font-bold uppercase text-emerald-200">
                ✓ POSTED
              </span>
              <span className="font-mono text-sm text-slate-500">JE #{posted.journal_id}</span>
            </div>
            <span className="font-mono text-sm text-slate-500">
              entry_date: <span className="text-emerald-300">{posted.entry_date as any}</span>
            </span>
          </header>
          {posted.description && <p className="text-sm text-slate-300">{posted.description}</p>}
          <LinesTable lines={posted.lines} locale={locale} />
        </section>
      )}

      {totalsOverride && (
        <p className="text-sm font-mono text-slate-400">
          Σ {fmtMoney(totalsOverride.totalDebit, '').trim()} / {fmtMoney(totalsOverride.totalCredit, '').trim()}{' '}
          {totalsOverride.balanced ? '✓ balanced' : '⚠ unbalanced'}
        </p>
      )}
    </div>
  );
}