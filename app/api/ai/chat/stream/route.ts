import { invokeStream } from '@folio-lib/ai/router';
import { NextRequest } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { sseHeaders } from '@folio-lib/ai/streamProtocol';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'ai:chat:use::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor!;
  const body = await req.json().catch(() => ({}));
  const { sectionKey, messages, systemPrompt, model, temperature } = body as {
    sectionKey?: string;
    messages?: Array<{ role: 'system' | 'user' | 'assistant'; content: string }>;
    systemPrompt?: string;
    model?: string;
    temperature?: number;
  };
  if (!sectionKey || !Array.isArray(messages) || messages.length === 0) {
    return new Response(JSON.stringify({ ok: false, error: 'sectionKey and messages required' }), {
      status: 400,
      headers: { 'Content-Type': 'application/json' },
    });
  }

  const encoder = new TextEncoder();
  const t0 = Date.now();
  const stream = new ReadableStream({
    async start(controller) {
      const send = (event: string, data: unknown) => {
        controller.enqueue(encoder.encode(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`));
      };
      try {
        const it = invokeStream(
          sectionKey,
          'chat',
          {
            messages,
            systemPrompt,
            modelOverride: model,
            temperature: typeof temperature === 'number' ? temperature : 0.3,
          },
          { actorId: actor.id },
        );
        for await (const chunk of it) {
          send('chunk', { delta: chunk });
        }
        send('meta', { latencyMs: Date.now() - t0, modelName: model ?? null });
        controller.close();
      } catch (e: any) {
        send('error', {
          message: e?.message ?? String(e),
          statusCode: e?.statusCode ?? e?.response?.status ?? undefined,
          upstreamCode: e?.response?.data?.error?.code ?? null,
          upstreamMessage: e?.response?.data?.error?.message ?? null,
        });
        controller.close();
      }
    },
  });
  return new Response(stream, { headers: sseHeaders() });
}
