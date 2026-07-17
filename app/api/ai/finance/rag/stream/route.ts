import { invoke } from '@folio-lib/ai/router';
import { NextRequest } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { searchVendors } from '@folio-lib/finance/rag';
import { sseHeaders } from '@folio-lib/ai/streamProtocol';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'finance:vendor:view::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor!;
  const body = await req.json().catch(() => ({}));
  const { question, model, lang } = body as { question?: string; model?: string; lang?: 'en' | 'th' | 'de' };
  if (!question || !question.trim()) {
    return new Response(JSON.stringify({ ok: false, error: 'question required' }), { status: 400 });
  }
  const locale = lang ?? 'en';

  const hits = await searchVendors({ query: question, k: 8 });
  const context = hits.map((h, i) =>
    `[${i + 1}] vendor=${h.vendor_name ?? '?'}, amount=${h.amount_thb ?? '?'}, date=${h.transaction_date ?? '?'}\n    ${(h.description ?? '').slice(0, 280)}`,
  ).join('\n');
  const langLine = locale === 'th'
    ? 'ตอบเป็นภาษาไทย ใช้เฉพาะข้อมูลจากบริบท อ้างอิง [n] เมื่อจำเป็น'
    : locale === 'de'
      ? 'Antworten Sie auf Deutsch. Verwenden Sie nur den bereitgestellten Kontext. Zitieren Sie [n] wenn nötig.'
      : 'Reply in English using only the context. Cite [n] when relevant.';
  const systemPrompt = `You are a Thai-ERP finance analyst. ${langLine} Keep the answer under 200 words.`;

  const encoder = new TextEncoder();
  const t0 = Date.now();
  const stream = new ReadableStream({
    async start(controller) {
      const send = (event: string, data: unknown) => {
        controller.enqueue(encoder.encode(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`));
      };
      try {
        const r = await invoke('finance:rag', 'chat', {
          systemPrompt,
          text: `Context:\n${context || '(no matching expense history)'}\n\nQuestion: ${question}`,
          modelOverride: model,
          temperature: 0.1,
          maxTokens: 600,
        }, { actorId: actor.id });
        if (!r.ok || !r.text) {
          send('error', { message: r.error || 'AI call failed', statusCode: r.statusCode, upstreamCode: r.upstreamCode });
          controller.close();
          return;
        }
        controller.enqueue(encoder.encode(`data: ${JSON.stringify({ chunk: r.text })}\n\n`));
        send('meta', { latencyMs: Date.now() - t0, modelName: r.modelName ?? null, hits });
        controller.close();
      } catch (e: any) {
        send('error', { message: e?.message ?? String(e) });
        controller.close();
      }
    },
  });
  return new Response(stream, { headers: sseHeaders() });
}
