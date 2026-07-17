export const SQL_BLOCK = /\[SQL\]([\s\S]*?)\[\/SQL\]/g;

export type ReportIntentKey =
  | 'cash_flow'
  | 'trial_balance'
  | 'income_statement'
  | 'balance_sheet'
  | 'period_summary';

export const REPORT_INTENTS: ReadonlyArray<ReportIntentKey> = [
  'cash_flow',
  'trial_balance',
  'income_statement',
  'balance_sheet',
  'period_summary',
];

export interface SqlAsk { question: string; }

export interface IntentAsk {
  intent: ReportIntentKey;
  date_from: string;
  date_to: string;
  question: string;
}

export type ParsedSqlAsk = SqlAsk | IntentAsk;

const YMD = /^\d{4}-\d{2}-\d{2}$/;

export function isIntentAsk(ask: unknown): ask is IntentAsk {
  if (!ask || typeof ask !== 'object') return false;
  const a = ask as Record<string, unknown>;
  return typeof a.intent === 'string' && (REPORT_INTENTS as ReadonlyArray<string>).includes(a.intent);
}

export function parseSqlBlocks(text: string): { plain: string; asks: ParsedSqlAsk[] } {
  const asks: ParsedSqlAsk[] = [];
  if (!text) return { plain: '', asks };
  const plain = text.replace(SQL_BLOCK, (_m, body: string) => {
    let obj: unknown;
    try { obj = JSON.parse(body); } catch { return ''; }
    if (!obj || typeof obj !== 'object') return '';
    const o = obj as Record<string, unknown>;
    if (isIntentAsk(o)) {
      if (typeof o.date_from === 'string' && YMD.test(o.date_from) && typeof o.date_to === 'string' && YMD.test(o.date_to)) {
        asks.push({
          intent: o.intent as ReportIntentKey,
          date_from: o.date_from,
          date_to: o.date_to,
          question: typeof o.question === 'string' ? o.question : '',
        });
      }
      return '';
    }
    if (typeof o.question === 'string' && o.question.trim()) {
      asks.push({ question: o.question.trim() });
    }
    return '';
  });
  return { plain: plain.trim(), asks };
}
