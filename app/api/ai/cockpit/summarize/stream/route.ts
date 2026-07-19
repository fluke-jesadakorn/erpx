import { invokeStream } from '@folio-lib/ai/router';
import { NextRequest } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { cockpitSummarizePrompt, renderLocaleAwarePrompt } from '@folio-lib/ai/systemPrompts';
import { parseChartBlocks } from '@/components/chat/chartContract';
import { getSecondaryLocaleFromHeaders } from '@folio-lib/server/locale';
import { sseHeaders } from '@folio-lib/ai/streamProtocol';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'tile:cockpit:view::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor!;
  const body = await req.json().catch(() => ({}));
  const { messages, model, lang } = body as { messages?: Array<{ role: 'user' | 'assistant' | 'system'; content: string }>; model?: string; lang?: 'en' | 'th' | 'de' };
  if (!Array.isArray(messages) || messages.length === 0) {
    return new Response(JSON.stringify({ ok: false, error: 'messages required' }), { status: 400 });
  }
  const locale: 'en' | 'th' | 'de' = lang ?? getSecondaryLocaleFromHeaders(req.headers) ?? 'en';
  const systemPrompt = renderLocaleAwarePrompt(cockpitSummarizePrompt, locale);

  const encoder = new TextEncoder();
  const t0 = Date.now();
  const stream = new ReadableStream({
    async start(controller) {
      const send = (event: string, data: unknown) => {
        controller.enqueue(encoder.encode(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`));
      };
      let full = '';
      try {
        const it = invokeStream('cockpit:summarize', 'chat', { messages, systemPrompt, modelOverride: model, temperature: 0.3 }, { actorId: actor.id });
        for await (const chunk of it) {
          full += chunk;
          send('chunk', { delta: chunk });
        }
        const cleaned = full.replace(/<\/?think[^>]*>/g, '').trim();
        const { charts } = parseChartBlocks(cleaned);
        send('meta', { latencyMs: Date.now() - t0, modelName: model ?? null, blocks: { plain: '', charts } });
        controller.close();
      } catch (e: any) {
        send('error', {
          message: e?.message ?? String(e),
          statusCode: e?.statusCode ?? e?.response?.status ?? undefined,
          provider: e?.providerName ?? null,
          model: e?.modelName ?? model ?? null,
          upstreamCode: e?.upstreamCode ?? e?.response?.data?.error?.code ?? null,
          upstreamMessage: e?.upstreamMessage ?? e?.response?.data?.error?.message ?? null,
        });
        controller.close();
      }
    },
  });
  return new Response(stream, { headers: sseHeaders() });
}
