import 'server-only';
import { deriveSqlInsights, type SqlInsights } from './sqlInsights';
import { templatesFor } from './prompts';

export type SqlLang = 'en' | 'th' | 'de';

export interface SummarizeInput {
  question: string;
  sql: string;
  columns: string[];
  rows: Array<Record<string, unknown>>;
  lang?: SqlLang;
}

export interface SqlSummary {
  text: string;
  insights: SqlInsights;
  kind: 'count' | 'aggregate' | 'entity' | 'list' | 'empty';
}

const NUMBER_HEAD = /\b(how many|count(?:\s+of)?|number of|total\s+number\s+of|headcount|size\s+of)\b/i;
const SUM_HEAD = /\b(total|sum|aggregate|combined|amount)\b/i;
const AVG_HEAD = /\b(average|avg|mean)\b/i;
const LIST_HEAD = /\b(list|show|all|give\s+me|fetch|get|who\s+are|which)\b/i;
const LOOKUP_HEAD = /\b(who\s+is|find|look\s*up|search|show\s+me|info\s+on|details?\s+(?:on|for|about))\b/i;

const THAI_COUNT = /(จำนวน|กี่คน|กี่|นับ|รวมทั้งหมด|ทั้งหมดกี่)/;
const THAI_SUM = /(ยอดรวม|รวม|ค่าใช้จ่ายรวม|มูลค่ารวม|ยอดสุทธิ)/;
const THAI_AVG = /(เฉลี่ย|ค่าเฉลี่ย)/;

const DE_COUNT = /(wieviele|wie\s+viele|anzahl|gesamtanzahl|kopfzahl)/i;
const DE_SUM = /(summe|gesamt|betrag)/i;
const DE_AVG = /(durchschnitt|mittelwert)/i;

function numberFromCell(v: unknown): number | null {
  if (v == null || v === '') return null;
  if (typeof v === 'number') return Number.isFinite(v) ? v : null;
  if (typeof v === 'string') {
    const cleaned = v.replace(/[,_$\s]/g, '');
    const n = Number(cleaned);
    return Number.isFinite(n) ? n : null;
  }
  if (typeof v === 'object' && v && 'toString' in v) {
    const n = Number(String(v));
    return Number.isFinite(n) ? n : null;
  }
  return null;
}

function firstNumberCell(rows: Array<Record<string, unknown>>): { col: string; value: number } | null {
  for (const r of rows) {
    for (const [k, v] of Object.entries(r)) {
      const n = numberFromCell(v);
      if (n != null) return { col: k, value: n };
    }
  }
  return null;
}

function isCountAlias(col: string, sql: string): boolean {
  const c = col.toLowerCase();
  if (/(count|num|qty|quantity|headcount|employees|staffs?|staff|persons?|users?|records?|days?|contracts?|vendors?|customers?|invoices?|orders?|items?|rows?|pages?|chunks?|hours?|minutes?|seconds?|members?|people|active|inactive|pending|approved|rejected|disbursed)/i.test(c)) return true;
  if (/^total_/i.test(c) && /count\s*\(/i.test(sql)) return true;
  if (/^num_/i.test(c)) return true;
  return false;
}

function localeNounForQuestion(q: string): string {
  const m = q.match(/\b(?:how\s+many|count(?:\s+of)?|number\s+of)\s+([a-z][a-z\s\-]{1,40}?)(?:\?|$|\.|,| do| are| have| in| at| with| that| who| which| currently| today| this)/i);
  if (m) return m[1].trim().replace(/\s+/g, ' ');
  return '';
}

function thaiNounForQuestion(q: string): string {
  const m = q.match(/จำนวน\s*([ก-๙a-zA-Z][ก-๙a-zA-Z\s]{1,40}?)(?:\?|$|\.|,| ที่| ใน| ทั้งหมด| ตอนนี้| วันนี้| ปัจจุบัน)/);
  if (m) return m[1].trim().replace(/\s+/g, ' ');
  return '';
}

function germanNounForQuestion(q: string): string {
  const m = q.match(/(?:wieviele|wie\s+viele|anzahl)\s+([a-zäöüß][a-zäöüß\s\-]{1,40}?)(?:\?|$|\.|,| gibt| sind| haben| derzeit| aktuell| heute| im| in| mit)/i);
  if (m) return m[1].trim().replace(/\s+/g, ' ');
  return '';
}

function fmtCount(n: number, lang: SqlLang): string {
  if (lang === 'th') return n.toLocaleString('th-TH');
  if (lang === 'de') return n.toLocaleString('de-DE');
  return n.toLocaleString('en-US');
}

function fmtNumber(n: number, lang: SqlLang): string {
  if (lang === 'th') return n.toLocaleString('th-TH', { maximumFractionDigits: 2 });
  if (lang === 'de') return n.toLocaleString('de-DE', { maximumFractionDigits: 2 });
  return n.toLocaleString('en-US', { maximumFractionDigits: 2 });
}

export function summarizeSql(req: SummarizeInput): SqlSummary {
  const lang = req.lang ?? 'en';
  const tpl = templatesFor(lang);
  const q = (req.question || '').trim();
  const cols = req.columns ?? [];
  const rows = req.rows ?? [];
  const insights = deriveSqlInsights(cols, rows);

  if (rows.length === 0) {
    return { text: tpl.empty, insights, kind: 'empty' };
  }

  const wantCount = NUMBER_HEAD.test(q) || THAI_COUNT.test(q) || DE_COUNT.test(q);
  const wantSum = SUM_HEAD.test(q) || THAI_SUM.test(q) || DE_SUM.test(q);
  const wantAvg = AVG_HEAD.test(q) || THAI_AVG.test(q) || DE_AVG.test(q);
  const wantLookup = LOOKUP_HEAD.test(q);

  if (rows.length === 1 && cols.length <= 2) {
    const row = rows[0];
    const numericCell = firstNumberCell(rows);

    if (numericCell && numericCell.col) {
      const isCountKind = wantCount
        || cols.length === 1
        || isCountAlias(numericCell.col, req.sql);
      if (isCountKind) {
        const n = numericCell.value;
        const noun = lang === 'th' ? thaiNounForQuestion(q) : lang === 'de' ? germanNounForQuestion(q) : localeNounForQuestion(q);
        return {
          text: tpl.count({ noun, formatted: fmtCount(n, lang) }),
          insights,
          kind: 'count',
        };
      }
      if (wantAvg) {
        return {
          text: tpl.average({ formatted: fmtNumber(numericCell.value, lang) }),
          insights,
          kind: 'aggregate',
        };
      }
      if (wantSum) {
        return {
          text: tpl.sum({ formatted: fmtNumber(numericCell.value, lang) }),
          insights,
          kind: 'aggregate',
        };
      }
    }

    if (wantLookup || cols.length <= 4) {
      const bullets = Object.entries(row).slice(0, 4)
        .map(([k, v]) => `${k}: ${v == null ? '—' : String(v)}`)
        .join(', ');
      return {
        text: tpl.result({ bullets }),
        insights,
        kind: 'entity',
      };
    }
  }

  if (wantCount && insights.headline) {
    return { text: insights.headline, insights, kind: 'count' };
  }
  if (wantSum && insights.headline) {
    return { text: insights.headline, insights, kind: 'aggregate' };
  }

  const summary = insights.headline || tpl.fallback({ rows: rows.length });

  return { text: summary, insights, kind: LIST_HEAD.test(q) || rows.length > 1 ? 'list' : 'list' };
}