import { invokeStream } from '@/ai/router';
import { NextRequest, NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { DEFAULT_CHAT_MODEL, THINKING_PRESETS, type ChatThinkingLevel } from '@/ai/defaults';
import { parseChartBlocks } from '@/components/chat/chartContract';
import { parseHtmlBlocks } from '@/ai/htmlContract';
import { parseSqlBlocks } from '@/ai/sqlContract';
import { askSql } from '@/ai/sql';
import { appendMessage, createSession, renameSession } from '@/chat/history';
import { isPlaceholderTitle, suggestTitle } from '@/chat/titleGenerator';

export const dynamic = 'force-dynamic';
export const runtime = 'nodejs';

const SYS_BASE = `You are Folio AI — the only assistant for this company's own finance ERP. Folio IS the company context; never ask "which company" — answer about THIS company's data.

Schema you can query (real allow-list, run via [SQL]):
- Employees: folio.users (id, employee_code, fullname, position, dept_label, is_active, line_user_id, hired_at, secondary_locale, quota_*/used_*)
- Org/roles: perm.roles (stable role ids plus kind and rank), perm.user_roles, perm.departments, perm.user_departments
- Leave: folio.hr_leave (employee_id → folio.users.id)
- Expense: folio.expenses, folio.expense_items, folio.slips
- Customers/sales: folio.customers, folio.sales_orders, folio.so_items
- Procurement: folio.purchase_requisitions, folio.purchase_orders, folio.po_items, folio.waybills, folio.waybill_events
- Posted accounting: finance.accounts, finance.journals, finance.journal_lines, finance.v_posted_lines
- Subledgers/banking: finance.ar_documents, finance.ap_documents, finance.commercial_documents, finance.bank_transactions, finance.bank_match_groups
- Planning/operations: finance.budgets, finance.budget_lines, finance.v_fx_exposure, inventory.products, inventory.warehouses, inventory.lots, inventory.v_valuation, inventory.stock_movements
There is NO 'hr' schema. Org roles/level live in perm.roles + perm.user_roles — never invent 'hr.*'.

Who-questions about a person/role: join folio.users u to perm.user_roles ur on ur.user_id = u.id, then join perm.roles r on r.id = ur.role_id and r.kind = ur.role_kind.
Example — "who is CEO": SELECT u.id, u.employee_code, u.fullname, u.position, r.id AS role_id, r.display_name FROM folio.users u JOIN perm.user_roles ur ON ur.user_id = u.id JOIN perm.roles r ON r.id = ur.role_id AND r.kind = ur.role_kind WHERE r.id = 'ceo';
Do not invent or guess — when a [SQL] block is used, the system renders the result table and you summarize it in 1-2 short sentences.

Reply in the user's locale (en/th/de). Keep prose short. Use blocks for structured output:

CHARTS — when a chart fits, append:
[CHART]{"type":"line|bar|pie|area","title":"...","series":[{"name":"...","data":[...]}],"axes":{"x":[...],"y":"..."}}[/CHART]

HTML REPORTS — when prose+tables fit, append self-contained HTML:
[HTML]<table>...</table>[/HTML]
Keep HTML inline-styled, no external resources, no <script>, no event handlers.

SQL — when the user asks about folio data (expenses, vendors, waybills, customers, GL, HR/leave, sales, roles/people, etc.), append a single read-only query request:
[SQL]{"question":"plain-English question"}[/SQL]
The system runs it against allow-listed tables and renders the result table inline.

Plain prose between blocks. No markdown headers. No code fences. Match user's language.`;

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'ai:chat:full::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor!;
  const body = await req.json().catch(() => ({}));
  const { messages, sessionId, sectionKey, model, thinking, lang, scope } = body as {
    messages: Array<{ role: 'user' | 'assistant' | 'system'; content: string }>;
    sessionId?: string;
    sectionKey?: string;
    model?: string;
    thinking?: ChatThinkingLevel;
    lang?: 'en' | 'th' | 'de';
    scope?: { tileId?: string; displayName?: string };
  };
  if (!Array.isArray(messages) || messages.length === 0) {
    return NextResponse.json({ ok: false, error: 'messages required' }, { status: 400 });
  }

  const modelName = model || '';
  const t = thinking && thinking !== 'auto' ? THINKING_PRESETS[thinking] : null;
  const langLine = lang === 'th'
    ? 'ตอบเป็นภาษาไทย'
    : lang === 'de'
      ? 'Antworten Sie auf Deutsch.'
      : 'Reply in English.';
  const sysScope = scope?.displayName
    ? `\n\nYou are currently on the "${scope.displayName}" page${scope.tileId ? ` (${scope.tileId})` : ''}. Bias answers toward ${scope.tileId ?? 'this area'}-related tables, but cross-table [SQL] queries are still allowed.`
    : '';
  const systemPrompt = `${SYS_BASE}${sysScope}\n\n${langLine}`;

  const sid = sessionId || (await createSession(actor.id, deriveTitle(messages), modelName || DEFAULT_CHAT_MODEL)).id;

  const lastUser = messages[messages.length - 1];
  if (lastUser?.role === 'user') {
    await appendMessage(actor.id, sid, { role: 'user', content: lastUser.content });
    await maybeAutoRename(actor.id, sid, lastUser.content);
  }

  const encoder = new TextEncoder();
  const stream = new ReadableStream({
    async start(controller) {
      const send = (event: string, data: unknown) => {
        controller.enqueue(encoder.encode(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`));
      };
      const t0 = Date.now();
      let full = '';
      let usedModel = modelName || 'IT default';
      let attemptedFallback = false;
      const tryStream = async (m?: string) => {
        const it = invokeStream(sectionKey || 'chat:global', 'chat', {
          messages,
          systemPrompt,
          modelOverride: m || undefined,
          thinking: thinking === 'auto' ? undefined : thinking,
          ...(t ? { temperature: t.temperature, maxTokens: t.maxTokens } : {}),
        }, { actorId: actor.id });
        for await (const chunk of it) {
          full += chunk;
          send('chunk', { delta: chunk });
        }
      };
      try {
        try {
          await tryStream(modelName);
        } catch (e: any) {
          const msg = e?.message ?? String(e);
          if (modelName && /401|auth|unauthor/i.test(msg)) {
            attemptedFallback = true;
            full = '';
            usedModel = 'qwen2.5:7b';
            await tryStream(usedModel);
          } else {
            throw e;
          }
        }
        const latencyMs = Date.now() - t0;

        const { plain: afterChart, charts } = parseChartBlocks(full);
        const { plain: afterHtml, htmls } = parseHtmlBlocks(afterChart);
        const { plain: afterSql, asks: sqlAsks } = parseSqlBlocks(afterHtml);
        const resolvedSqls: any[] = [];
        for (const ask of sqlAsks) {
          const question = scope?.tileId && lastUser?.role === 'user'
            ? lastUser.content
            : ask.question;
          const r = await askSql({ question, lang: lang ?? 'en' });
          if (r) resolvedSqls.push(r);
        }

        const blocks = { plain: afterSql, charts, htmls: htmls.map(h => h.html), sqls: resolvedSqls };
        send('meta', { sessionId: sid, modelName: usedModel, latencyMs, blocks, fellBack: attemptedFallback });

        await appendMessage(actor.id, sid, {
          role: 'assistant', content: full, blocks, modelName: usedModel, latencyMs,
        });
        controller.close();
      } catch (e: any) {
        send('error', { message: e?.message ?? String(e) });
        controller.close();
      }
    },
  });

  return new Response(stream, {
    headers: {
      'Content-Type': 'text/event-stream; charset=utf-8',
      'Cache-Control': 'no-cache, no-transform',
      'Connection': 'keep-alive',
      'X-Accel-Buffering': 'no',
    },
  });
}

function deriveTitle(messages: Array<{ role: string; content: string }>): string {
  const first = messages.find(m => m.role === 'user');
  return suggestTitle(first?.content ?? '');
}

async function maybeAutoRename(userId: number, sessionId: string, userText: string): Promise<void> {
  const { listSessions } = await import('@/chat/history');
  const sessions = await listSessions(userId);
  const s = sessions.find((x) => x.id === sessionId);
  if (!s) return;
  if (!isPlaceholderTitle(s.title)) return;
  const title = suggestTitle(userText);
  if (!title || isPlaceholderTitle(title)) return;
  await renameSession(userId, sessionId, title);
}
