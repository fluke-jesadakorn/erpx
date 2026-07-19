import { invokeStream } from '@/ai/router';
import { NextRequest, NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { DEFAULT_CHAT_MODEL, THINKING_PRESETS, type ChatThinkingLevel } from '@/ai/defaults';
import { parseChartBlocks } from '@/components/chat/chartContract';
import { parseHtmlBlocks } from '@/ai/htmlContract';
import { parseSqlBlocks } from '@/ai/sqlContract';
import { askSql } from '@/ai/sql';
import { appendMessage, createSession, renameSession, rewindSession } from '@/chat/history';
import { isPlaceholderTitle, suggestTitle } from '@/chat/titleGenerator';

export const dynamic = 'force-dynamic';
export const runtime = 'nodejs';

const SYS_BASE = `You are Folio Intelligence, the embedded operating assistant for this company's Folio system. Folio is always the company context.

IN-SCOPE means anything about Folio itself: finance and accounting, expenses, slips, reimbursements, procurement, PR/PO, waybills and approvals, customers, sales and AR, vendors and AP, banking, budgets, tax, inventory, HR and leave, people and org structure, roles/permissions/policies/tiles, contracts and law records, notifications/audit trails, AI providers/models/assignments, pages, workflows, data definitions, operating instructions, investigation, comparisons, forecasts, and scenarios using Folio data.

OUT-OF-SCOPE means a different objective such as general knowledge, weather, news, sports, entertainment, recipes, travel, medical advice, unrelated coding, homework, or creative writing. For these, return a small [HTML] artifact that briefly explains the Folio-only scope and suggests relevant topics. Keep it concise and use the language rule below.

For any question whose answer depends on live company records, output one standalone query request:
[SQL]{"question":"a complete, context-independent description of the data needed"}[/SQL]
The SQL engine sees the live Folio, finance, inventory, law, and permission catalogs, creates raw PostgreSQL, executes it through a read-only connection, and presents the SQL plus results. Never guess a live value. If the user supplies a raw SELECT/WITH query, put that exact query in question so it can be executed.

For conceptual, instructional, or analytical answers that do not require live rows, return a polished interactive artifact:
[HTML]
<section>...</section>
<style>...</style>
<script>...</script>
[/HTML]
The artifact must be self-contained HTML/CSS/vanilla JS, responsive, accessible, and visually refined. Use clear hierarchy, compact cards, useful interaction such as tabs, filters, expand/collapse, or scenario controls, and no external assets or network calls. Every surface must set an explicit, high-contrast foreground and background color. Layouts must wrap without horizontal page scrolling at 320px. Never include placeholder values, undefined functions, fake fetch calls, or code that implies unavailable live data. Do not emit code fences. Do not explain the artifact outside the block unless one short sentence is essential.

LANGUAGE RULE — this overrides the interface locale and all earlier conversation language:
- Detect the language of the latest user message and answer in that same language.
- Apply it to prose, headings, labels, explanations, and generated artifacts.
- If the user explicitly requests a different response language, use the requested language.
- Support any language; do not limit detection to English, Thai, or German.
- If the latest message contains only code, SQL, identifiers, or numbers, reuse the latest natural-language user message; otherwise fall back to English.

Prefer exact Folio terminology. Never invent tables, fields, people, totals, or statuses.`;

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'ai:chat:full::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor!;
  const body = await req.json().catch(() => ({}));
  const { messages, sessionId, sectionKey, model, thinking, scope, editMessageId } = body as {
    messages: Array<{ role: 'user' | 'assistant' | 'system'; content: string }>;
    sessionId?: string;
    sectionKey?: string;
    model?: string;
    thinking?: ChatThinkingLevel;
    scope?: { tileId?: string; displayName?: string };
    editMessageId?: string;
  };
  if (!Array.isArray(messages) || messages.length === 0) {
    return NextResponse.json({ ok: false, error: 'messages required' }, { status: 400 });
  }

  const modelName = model || '';
  const t = thinking && thinking !== 'auto' ? THINKING_PRESETS[thinking] : null;
  const sysScope = scope?.displayName
    ? `\n\nYou are currently on the "${scope.displayName}" page${scope.tileId ? ` (${scope.tileId})` : ''}. Bias answers toward ${scope.tileId ?? 'this area'}-related tables, but cross-table [SQL] queries are still allowed.`
    : '';
  const systemPrompt = `${SYS_BASE}${sysScope}`;

  const sid = sessionId || (await createSession(actor.id, deriveTitle(messages), modelName || DEFAULT_CHAT_MODEL)).id;

  const lastUser = messages[messages.length - 1];
  if (editMessageId) await rewindSession(actor.id, sid, editMessageId);
  let userMessageId: string | undefined;
  if (lastUser?.role === 'user') {
    const saved = await appendMessage(actor.id, sid, { role: 'user', content: lastUser.content });
    userMessageId = saved.id;
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
        if (lastUser?.role === 'user' && isRawSql(lastUser.content)) {
          usedModel = 'Raw SQL';
          full = `[SQL]${JSON.stringify({ question: lastUser.content })}[/SQL]`;
        } else if (lastUser?.role === 'user' && needsLiveData(lastUser.content)) {
          usedModel = 'Live Folio SQL';
          full = `[SQL]${JSON.stringify({ question: lastUser.content })}[/SQL]`;
        } else {
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
        }
        const latencyMs = Date.now() - t0;

        const { plain: afterChart, charts } = parseChartBlocks(full);
        const { plain: afterHtml, htmls } = parseHtmlBlocks(afterChart);
        const { plain: afterSql, asks: sqlAsks } = parseSqlBlocks(afterHtml);
        const resolvedSqls: any[] = [];
        for (const ask of sqlAsks) {
          const r = await askSql({ question: ask.question });
          if (r) resolvedSqls.push(r);
        }

        const artifacts = htmls.map(h => h.html);
        if (afterSql && artifacts.length === 0 && resolvedSqls.length === 0 && charts.length === 0) {
          artifacts.push(plainArtifact(afterSql));
        }
        const blocks = {
          plain: artifacts.length > 0 && resolvedSqls.length === 0 && charts.length === 0 ? '' : afterSql,
          charts,
          htmls: artifacts,
          sqls: resolvedSqls,
        };
        const assistant = await appendMessage(actor.id, sid, {
          role: 'assistant', content: full, blocks, modelName: usedModel, latencyMs,
        });
        send('meta', {
          sessionId: sid,
          userMessageId,
          assistantMessageId: assistant.id,
          modelName: usedModel,
          latencyMs,
          blocks,
          fellBack: attemptedFallback,
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

function isRawSql(text: string): boolean {
  return /^(SELECT|WITH)\b/i.test(text.trim()) || /```sql[\s\S]*?(SELECT|WITH)\b/i.test(text);
}

function needsLiveData(text: string): boolean {
  return /\b(live|real[- ]?time|right now|currently|today|latest|actuals?|actual data|current[- ](?:week|month|quarter|year)|current (?:balance|status|total|value|records?|data|queue|expenses?|approvals?|receivables?|payables?)|this (?:week|month|quarter|year)|open receivables?|open payables?|my (?:queue|expenses?|approvals?))\b|ข้อมูล(?:จริง|ปัจจุบัน)|ตอนนี้|วันนี้|ล่าสุด|เดือนนี้|ไตรมาสนี้|ปีนี้|aktuell|echtzeit|heute|diese(?:n|m|s)? (?:woche|monat|quartal|jahr)/i.test(text);
}

function plainArtifact(text: string): string {
  const safe = text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  return `<article class="answer"><span>FOLIO</span><div>${safe.replace(/\n/g, '<br>')}</div></article><style>*{box-sizing:border-box}body{margin:0;padding:22px;background:#081119;color:#edf6fb;font:14px/1.65 Inter,system-ui,sans-serif}.answer{max-width:880px;margin:auto;padding:26px;border:1px solid #243c4d;border-radius:20px;background:linear-gradient(145deg,#122532,#0b171f);box-shadow:0 24px 70px #0007}.answer>span{display:block;margin-bottom:14px;color:#64ddff;font:700 10px ui-monospace,monospace;letter-spacing:.18em}.answer>div{white-space:normal;color:#d8e6ee}</style><script>document.documentElement.dataset.ready='true'</script>`;
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
