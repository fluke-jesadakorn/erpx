import 'server-only';
import { aiInvoke } from '@folio-lib/ai/router';
import { query } from '@folio-lib/db';
import { renderLocaleAwarePrompt } from '@folio-lib/ai/systemPrompts';

export interface LedgerLineCommentary {
  accountCode: string;
  accountName: string | null;
  accountNameTh: string | null;
  commentary: string;
  generatedAt: string;
}

export async function generateCommentary(args: {
  accountCode: string;
  lang?: 'en' | 'th' | 'de';
  periodLabel?: string;
}): Promise<LedgerLineCommentary | null> {
  const lang = args.lang ?? 'en';

  const acctRes = await query<{ name: string | null; name_th: string | null }>(
    `SELECT name, name_th FROM chart_of_accounts WHERE code = $1`,
    [args.accountCode]
  );
  const acct = acctRes.rows[0];

  const linesRes = await query<{ debit: string; credit: string; description: string | null; entry_date: string }>(
    `SELECT l.debit::text, l.credit::text, l.description, j.entry_date::text
       FROM ledger_lines l
       JOIN journal_entries j ON j.id = l.journal_entry_id
      WHERE l.account_code = $1
   ORDER BY j.entry_date DESC
      LIMIT 30`,
    [args.accountCode]
  );

  if (linesRes.rows.length === 0) return null;

  const debit = linesRes.rows.reduce((s, r) => s + parseFloat(r.debit || '0'), 0);
  const credit = linesRes.rows.reduce((s, r) => s + parseFloat(r.credit || '0'), 0);

  const r = await aiInvoke('ledger:commentary', 'chat', {
    systemPrompt: renderLocaleAwarePrompt(
      `You are a Thai bookkeeper explaining a chart-of-accounts balance to a non-accountant manager. In 2-3 short sentences, describe what the recent activity means and flag anything unusual (large spikes, recurring counterparties). Be specific with numbers. No markdown, no bullets. {{langLine}}`,
      lang
    ),
    text: JSON.stringify({
      accountCode: args.accountCode,
      accountName: acct?.name,
      accountNameTh: acct?.name_th,
      periodLabel: args.periodLabel ?? 'recent',
      totalDebit: debit,
      totalCredit: credit,
      netBalance: debit - credit,
      sampleLines: linesRes.rows.slice(0, 10),
    }),
    temperature: 0.2,
    maxTokens: 500,
  });

  if (!r.ok || !r.text) return null;

  return {
    accountCode: args.accountCode,
    accountName: acct?.name ?? null,
    accountNameTh: acct?.name_th ?? null,
    commentary: r.text,
    generatedAt: new Date().toISOString(),
  };
}