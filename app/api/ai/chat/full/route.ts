import { invokeStream } from '@/ai/router';
import { NextRequest, NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { DEFAULT_CHAT_MODEL, DEFAULT_THINKING, THINKING_PRESETS } from '@/ai/defaults';
import { parseChartBlocks } from '@/components/chat/chartContract';
import { parseHtmlBlocks } from '@/ai/htmlContract';
import { parseSqlBlocks } from '@/ai/sqlContract';
import { askSql } from '@/ai/sql';
import { appendMessage, createSession } from '@/chat/history';

export const dynamic = 'force-dynamic';
export const runtime = 'nodejs';

const SYS_BASE = `You are Folio AI — a finance assistant for a Thai ERP.
Reply in the user's locale (en/th/de). Keep prose short. Use blocks for structured output:

CHARTS — when a chart fits, append:
[CHART]{"type":"line|bar|pie|area","title":"...","series":[{"name":"...","data":[...]}],"axes":{"x":[...],"y":"..."}}[/CHART]

HTML REPORTS — when prose+tables fit, append self-contained HTML:
[HTML]<table>...</table>[/HTML]
Keep HTML inline-styled, no external resources, no <script>, no event handlers.

SQL — when the user asks about folio data (expenses, vendors, waybills, customers, GL, HR, sales, etc.), append a single read-only query request:
[SQL]{"question":"plain-English question"}[/SQL]
The system runs it against allow-listed tables and renders the result table inline.

Plain prose between blocks. No markdown headers. No code fences. Match user's language.`;

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'ai:chat:full::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor!;
  const body = await req.json().catch(() => ({}));
  const { messages, sessionId, model, thinking, lang, scope } = body as {
    messages: Array<{ role: 'user' | 'assistant' | 'system'; content: string }>;
    sessionId?: string;
    model?: string;
    thinking?: 'low' | 'medium' | 'high';
    lang?: 'en' | 'th' | 'de';
    scope?: { tileId?: string; displayName?: string };
  };
  if (!Array.isArray(messages) || messages.length === 0) {
    return NextResponse.json({ ok: false, error: 'messages required' }, { status: 400 });
  }

  const modelName = model || DEFAULT_CHAT_MODEL;
  const t = THINKING_PRESETS[thinking ?? 'high'] ?? DEFAULT_THINKING;
  const langLine = lang === 'th'
    ? 'ตอบเป็นภาษาไทย'
    : lang === 'de'
      ? 'Antworten Sie auf Deutsch.'
      : 'Reply in English.';
  const sysScope = scope?.displayName
    ? `\n\nYou are currently on the "${scope.displayName}" page${scope.tileId ? ` (${scope.tileId})` : ''}. Bias answers toward ${scope.tileId ?? 'this area'}-related tables, but cross-table [SQL] queries are still allowed.`
    : '';
  const systemPrompt = `${SYS_BASE}${sysScope}\n\n${langLine}`;

  const sid = sessionId || (await createSession(actor.id, deriveTitle(messages), modelName)).id;

  const lastUser = messages[messages.length - 1];
  if (lastUser?.role === 'user') {
    await appendMessage(actor.id, sid, { role: 'user', content: lastUser.content });
  }

  const encoder = new TextEncoder();
  const stream = new ReadableStream({
    async start(controller) {
      const send = (event: string, data: unknown) => {
        controller.enqueue(encoder.encode(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`));
      };
      const t0 = Date.now();
      let full = '';
      let usedModel = modelName;
      let attemptedFallback = false;
      const tryStream = async (m: string) => {
        const it = invokeStream('chat:full', 'chat', {
          messages,
          systemPrompt,
          modelOverride: m,
          temperature: t.temperature,
          maxTokens: t.maxTokens,
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
          if (modelName === 'MiniMax-M3' && /401|auth|unauthor/i.test(msg)) {
            attemptedFallback = true;
            full = '';
            usedModel = 'qwen2.5:7b';
            await tryStream(usedModel);
          } else {
            throw e;
          }
        }
        const latencyMs = Date.now() - t0;

        const { asks: sqlAsks } = parseSqlBlocks(full);
        const resolvedSqls: any[] = [];
        for (const ask of sqlAsks) {
          const r = await askSql({ question: ask.question, lang: lang ?? 'en' });
          if (r) resolvedSqls.push(r);
        }

        const { charts } = parseChartBlocks(full);
        const { htmls } = parseHtmlBlocks(full);

        const blocks = { plain: '', charts, htmls: htmls.map(h => h.html), sqls: resolvedSqls };
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
  const text = first?.content ?? 'New chat';
  return text.length > 60 ? text.slice(0, 57) + '…' : text;
}