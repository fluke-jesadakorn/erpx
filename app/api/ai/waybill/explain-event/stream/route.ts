import { invoke } from '@folio-lib/ai/router';
import { NextRequest } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { getSecondaryLocaleFromHeaders } from '@folio-lib/server/locale';
import { sseHeaders } from '@folio-lib/ai/streamProtocol';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function stripThink(s: string): string {
  return (s ?? '').replace(/<\/?think[^>]*>/g, '').trim();
}

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'finance:expense:view_all::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor!;
  const body = await req.json().catch(() => ({}));
  const { waybillId, eventKind, fromStage, toStage, actorName, model } = body as {
    waybillId?: string;
    eventKind?: string;
    fromStage?: string;
    toStage?: string;
    actorName?: string | null;
    model?: string;
  };
  if (!waybillId || !eventKind || !fromStage || !toStage) {
    return new Response(JSON.stringify({ ok: false, error: 'waybillId, eventKind, fromStage, toStage required' }), { status: 400 });
  }
  const locale = getSecondaryLocaleFromHeaders(req.headers);
  const languageHint = locale === 'th'
    ? 'เขียนคำตอบเป็นภาษาไทย'
    : locale === 'de'
      ? 'Schreiben Sie die Antwort auf Deutsch.'
      : 'Write the answer in English.';
  const systemPrompt = `You are explaining a single waybill event to a Thai office worker. Event kind=${eventKind}, from stage=${fromStage}, to stage=${toStage}, actor=${actorName || 'unknown'}. Output 2 short sentences in the same language as the question: (1) what happened, (2) what the next state will trigger or require. Prose, no bullets. ${languageHint}`;
  const userText = locale === 'th'
    ? 'ช่วยอธิบายเหตุการณ์นี้ให้พนักงานไทยเข้าใจง่าย'
    : locale === 'de'
      ? 'Erklären Sie dieses Waybill-Ereignis in einfacher Sprache.'
      : 'Explain this waybill event in plain language.';

  const encoder = new TextEncoder();
  const t0 = Date.now();
  const stream = new ReadableStream({
    async start(controller) {
      const send = (event: string, data: unknown) => {
        controller.enqueue(encoder.encode(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`));
      };
      try {
        const r = await invoke('events:explain', 'chat', { text: userText, systemPrompt, modelOverride: model, temperature: 0.3 }, { actorId: actor.id });
        if (!r.ok || !r.text) {
          send('error', { message: r.error || 'AI call failed', statusCode: r.statusCode });
          controller.close();
          return;
        }
        const text = stripThink(r.text);
        controller.enqueue(encoder.encode(`data: ${JSON.stringify({ chunk: text })}\n\n`));
        send('meta', { latencyMs: Date.now() - t0, modelName: r.modelName ?? null });
        controller.close();
      } catch (e: any) {
        send('error', { message: e?.message ?? String(e) });
        controller.close();
      }
    },
  });
  return new Response(stream, { headers: sseHeaders() });
}
