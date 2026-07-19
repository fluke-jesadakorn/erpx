import { invoke } from '@folio-lib/ai/router';
import { NextRequest, NextResponse } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { parseChartBlocks } from '@/components/chat/chartContract';
import { parseHtmlBlocks } from '@folio-lib/ai/htmlContract';
import { parseUiBlocks } from '@folio-lib/ai/safeUiContract';
import { parseSqlBlocks, isIntentAsk } from '@folio-lib/ai/sqlContract';
import { askSql } from '@folio-lib/ai/sql';
import { resolveReport } from '@folio-lib/finance/reports';
import { stripThinkTags } from '@folio-lib/ai/think';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const SYS_TILE = `You are Folio AI — a finance assistant for a Thai ERP. Reply in the user's locale.

For financial statements (cash-flow / trial balance / income statement / balance sheet / period summary) emit EXACTLY ONE typed block and no other text:
[SQL]{"intent":"cash_flow","date_from":"YYYY-MM-DD","date_to":"YYYY-MM-DD","question":"..."}[/SQL]
[SQL]{"intent":"trial_balance","date_from":"YYYY-MM-DD","date_to":"YYYY-MM-DD","question":"..."}[/SQL]
[SQL]{"intent":"income_statement","date_from":"YYYY-MM-DD","date_to":"YYYY-MM-DD","question":"..."}[/SQL]
[SQL]{"intent":"balance_sheet","date_from":"YYYY-MM-DD","date_to":"YYYY-MM-DD","question":"..."}[/SQL]
[SQL]{"intent":"period_summary","date_from":"YYYY-MM-DD","date_to":"YYYY-MM-DD","question":"..."}[/SQL]
[SQL]{"intent":"ar_aging","date_from":"YYYY-MM-DD","date_to":"YYYY-MM-DD","question":"..."}[/SQL]
[SQL]{"intent":"ap_aging","date_from":"YYYY-MM-DD","date_to":"YYYY-MM-DD","question":"..."}[/SQL]
[SQL]{"intent":"fx_exposure","date_from":"YYYY-MM-DD","date_to":"YYYY-MM-DD","question":"..."}[/SQL]
[SQL]{"intent":"inventory_valuation","date_from":"YYYY-MM-DD","date_to":"YYYY-MM-DD","question":"..."}[/SQL]
[SQL]{"intent":"gross_margin","date_from":"YYYY-MM-DD","date_to":"YYYY-MM-DD","question":"..."}[/SQL]
[SQL]{"intent":"vat_register","date_from":"YYYY-MM-DD","date_to":"YYYY-MM-DD","question":"..."}[/SQL]
[SQL]{"intent":"wht_register","date_from":"YYYY-MM-DD","date_to":"YYYY-MM-DD","question":"..."}[/SQL]
[SQL]{"intent":"budget_vs_actual","date_from":"YYYY-MM-DD","date_to":"YYYY-MM-DD","question":"..."}[/SQL]

For other data lookups, use [SQL]{"question":"..."}[/SQL].
Optional [CHART], [HTML], [UI] blocks are allowed. {{langLine}}`;

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'ai:chat:use::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor!;
  const body = await req.json().catch(() => ({}));
  const { sectionKey, messages, scope, model, thinking, lang } = body as {
    sectionKey?: string;
    messages?: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>;
    scope?: { tileId?: string; displayName?: string };
    model?: string;
    thinking?: 'low' | 'medium' | 'high';
    lang?: 'en' | 'th' | 'de';
  };
  if (!Array.isArray(messages) || messages.length === 0) {
    return NextResponse.json({ ok: false, error: 'messages required' }, { status: 400 });
  }
  const t0 = Date.now();
  const sysScope = scope?.displayName
    ? `\nYou are currently on the "${scope.displayName}" tile. Bias answers toward ${scope.tileId ?? 'this area'}-related tables.`
    : '';
  const langLine = lang === 'th'
    ? 'ตอบเป็นภาษาไทย'
    : lang === 'de' ? 'Antworten Sie auf Deutsch.' : 'Reply in English.';
  const r = await invoke(sectionKey || 'chat:full', 'chat', {
    messages,
    systemPrompt: SYS_TILE.replace('{{langLine}}', langLine) + sysScope,
    modelOverride: model,
    thinking,
    temperature: 0.3,
  }, { actorId: actor.id });
  if (!r.ok) {
    return NextResponse.json({ ok: false, error: r.error || 'AI failed' }, { status: 502 });
  }
  const text = stripThinkTags(r.text ?? '');
  const { plain, charts } = parseChartBlocks(text);
  const { plain: htmlPlain, htmls } = parseHtmlBlocks(plain);
  const { plain: uiPlain, blocks: uis } = parseUiBlocks(htmlPlain);
  const { asks: sqlAsks } = parseSqlBlocks(uiPlain);
  const resolvedSqls: any[] = [];
  for (const ask of sqlAsks) {
    if (isIntentAsk(ask)) {
      const r2 = await resolveReport({
        intent: ask.intent,
        dateFrom: ask.date_from,
        dateTo: ask.date_to,
        lang: lang ?? 'en',
      });
      if (r2.ok) {
        const kpiRows = r2.kpis.map(k => ({ kpi: k.label, value: k.value }));
        const sectionRows: Array<Record<string, unknown>> = [];
        for (const s of r2.sections) {
          for (const row of s.rows) {
            const obj: Record<string, unknown> = {};
            row.forEach((cell, i) => { obj[s.columns[i] ?? `c${i}`] = cell; });
            sectionRows.push(obj);
          }
        }
        const rows = [...kpiRows, ...sectionRows];
        const kpiList = r2.kpis.map(k => `${k.label}: ${k.value}`).join('; ');
        const sectionTitles = r2.sections.map(s => s.title).join(', ');
        resolvedSqls.push({
          question: ask.question || '',
          sql: `report:${r2.intent}`,
          columns: ['kpi', 'value'],
          rows,
          rowCount: rows.length,
          explanation: `${kpiList} | ${sectionTitles}`,
        });
      } else {
        resolvedSqls.push({
          question: ask.question || '',
          sql: `report:${ask.intent}`,
          columns: ['kpi', 'value'],
          rows: [],
          rowCount: 0,
          explanation: `[rejected] ${r2.reason}`,
        });
      }
      continue;
    }
    const r2 = await askSql({ question: ask.question, lang: lang ?? 'en' });
    if (r2) resolvedSqls.push(r2);
  }
  return NextResponse.json({
    ok: true,
    text: uiPlain,
    modelName: r.modelName ?? null,
    latencyMs: Date.now() - t0,
    charts,
    htmls: htmls.map(h => h.html),
    uis: uis.map((b, i) => ({ id: `ui_${i}`, root: b.root })),
    sqls: resolvedSqls,
  });
}
