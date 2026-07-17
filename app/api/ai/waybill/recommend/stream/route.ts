import { invoke } from '@folio-lib/ai/router';
import { NextRequest } from 'next/server';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { sseHeaders } from '@folio-lib/ai/streamProtocol';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

function stripThink(s: string): string {
  return (s ?? '').replace(/<\/?think[^>]*>/g, '').trim();
}

function extractJson(raw: string) {
  const cleaned = stripThink(raw);
  if (!cleaned) return null;
  const tryParse = (s: string) => { try { return JSON.parse(s); } catch { return null; } };
  const direct = tryParse(cleaned);
  if (direct && typeof direct === 'object') return direct;
  const fenced = cleaned.match(/```(?:json)?\s*([\s\S]*?)```/i);
  if (fenced) { const obj = tryParse(fenced[1].trim()); if (obj && typeof obj === 'object') return obj; }
  const brace = cleaned.match(/\{[\s\S]*\}/);
  if (brace) { const obj = tryParse(brace[0]); if (obj && typeof obj === 'object') return obj; }
  return null;
}

export async function POST(req: NextRequest) {
  const guard = await apiGuard(req, { perm: 'finance:expense:view_all::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor!;
  const body = await req.json().catch(() => ({}));
  const { waybillId, amount, currentStage, vendorName, model } = body as {
    waybillId?: string;
    amount?: number | null;
    currentStage?: string;
    vendorName?: string | null;
    model?: string;
  };
  if (!waybillId || !currentStage) {
    return new Response(JSON.stringify({ ok: false, error: 'waybillId and currentStage required' }), { status: 400 });
  }
  const amountText = Number.isFinite(Number(amount)) ? Number(amount).toFixed(2) : 'unspecified';
  const systemPrompt = `You are a senior reviewer at a Thai finance company. A waybill is at the ${currentStage} stage. The vendor is ${vendorName || 'unspecified'}, the amount is ${amountText} THB. Based on typical policy approval patterns and policy tiers, suggest approve or reject with confidence 0..1 and a one-sentence rationale. Reply in JSON: {"decision":"approve"|"reject","confidence":0.87,"rationale":"..."}`;
  const userText = `Recommend for waybill ${waybillId} at stage ${currentStage}.`;

  const encoder = new TextEncoder();
  const t0 = Date.now();
  const stream = new ReadableStream({
    async start(controller) {
      const send = (event: string, data: unknown) => {
        controller.enqueue(encoder.encode(`event: ${event}\ndata: ${JSON.stringify(data)}\n\n`));
      };
      try {
        const r = await invoke('am:recommend', 'chat', { text: userText, systemPrompt, modelOverride: model, temperature: 0.2 }, { actorId: actor.id });
        if (!r.ok || !r.text) {
          send('error', { message: r.error || 'AI call failed', statusCode: r.statusCode });
          controller.close();
          return;
        }
        const parsed = extractJson(r.text);
        controller.enqueue(encoder.encode(`data: ${JSON.stringify({ chunk: r.text })}\n\n`));
        send('meta', { latencyMs: Date.now() - t0, modelName: r.modelName ?? null, decision: parsed });
        controller.close();
      } catch (e: any) {
        send('error', { message: e?.message ?? String(e) });
        controller.close();
      }
    },
  });
  return new Response(stream, { headers: sseHeaders() });
}
