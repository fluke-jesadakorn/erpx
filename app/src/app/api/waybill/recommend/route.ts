import { NextResponse } from 'next/server';
import { invoke } from '@folio-lib/ai/router';
import { loadActor } from '@folio-lib/server/guard';
import { hasPermission } from '@folio-lib/perm/auth-client';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface Body {
  waybillId?: string;
  amount?: number | null;
  currentStage?: string;
  vendorName?: string | null;
}

function stripThink(s: string): string {
  return (s ?? '').replace(/<\/?think[^>]*>/g, '').trim();
}

function extractJson(raw: string): { decision: 'approve' | 'reject'; confidence: number; rationale: string } | null {
  const cleaned = stripThink(raw);
  if (!cleaned) return null;
  const tryParse = (s: string): unknown => {
    try {
      return JSON.parse(s);
    } catch {
      return null;
    }
  };
  const direct = tryParse(cleaned);
  if (direct && typeof direct === 'object') {
    return normalize(direct as Record<string, unknown>);
  }
  const fenced = cleaned.match(/```(?:json)?\s*([\s\S]*?)```/i);
  if (fenced) {
    const obj = tryParse(fenced[1].trim());
    if (obj && typeof obj === 'object') return normalize(obj as Record<string, unknown>);
  }
  const brace = cleaned.match(/\{[\s\S]*\}/);
  if (brace) {
    const obj = tryParse(brace[0]);
    if (obj && typeof obj === 'object') return normalize(obj as Record<string, unknown>);
  }
  return null;
}

function normalize(o: Record<string, unknown>): { decision: 'approve' | 'reject'; confidence: number; rationale: string } | null {
  const rawDecision = String(o.decision ?? '').toLowerCase().trim();
  const decision: 'approve' | 'reject' = rawDecision === 'reject' ? 'reject' : 'approve';
  const confNum = Number(o.confidence);
  const confidence = Number.isFinite(confNum) ? Math.max(0, Math.min(1, confNum)) : 0.5;
  const rationale = typeof o.rationale === 'string' ? o.rationale.trim() : '';
  if (!rationale) return null;
  return { decision, confidence, rationale };
}

export async function POST(req: Request) {
  const actor = await loadActor();
  if (!actor) {
    return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });
  }
  const session = { user: { id: actor.id, name: actor.fullname, role: actor.role_id ?? 'officer::5' }, permissions: actor.permissions };
  const allowed = hasPermission(session, 'finance:expense:view_all::allow')
    || hasPermission(session, 'finance:expense:view_own::allow');
  if (!allowed) {
    return NextResponse.json({ ok: false, error: 'forbidden' }, { status: 403 });
  }

  const body = (await req.json().catch(() => ({}))) as Body;
  const waybillId = typeof body.waybillId === 'string' ? body.waybillId.trim() : '';
  const currentStage = typeof body.currentStage === 'string' ? body.currentStage.trim() : '';
  const vendorName =
    typeof body.vendorName === 'string' && body.vendorName.trim().length > 0
      ? body.vendorName.trim()
      : 'unspecified';
  const amountNum = Number(body.amount);
  const amountText = Number.isFinite(amountNum) ? amountNum.toFixed(2) : 'unspecified';

  if (!waybillId || !currentStage) {
    return NextResponse.json(
      { ok: false, error: 'waybillId and currentStage are required' },
      { status: 400 },
    );
  }

  const systemPrompt =
    `You are a senior reviewer at a Thai finance company. A waybill is at the ${currentStage} stage. ` +
    `The vendor is ${vendorName}, the amount is ${amountText} THB. ` +
    `Based on typical policy approval patterns and policy tiers, suggest approve or reject ` +
    `with confidence 0..1 and a one-sentence rationale. ` +
    `Reply in this JSON format only: ` +
    `{"decision":"approve"|"reject","confidence":0.87,"rationale":"..."}`;

  const userText = `Recommend for waybill ${waybillId} at stage ${currentStage}.`;

  const result = await invoke(
    'am:recommend',
    'chat',
    { text: userText, systemPrompt, temperature: 0.2 },
    { actorId: actor.id },
  );

  if (!result.ok || !result.text) {
    return NextResponse.json({
      ok: false,
      error: result.error || 'AI unavailable',
      modelName: result.modelName,
      latencyMs: result.latencyMs ?? 0,
    });
  }

  const parsed = extractJson(result.text);
  if (!parsed) {
    return NextResponse.json({
      ok: false,
      error: 'Could not parse AI reply',
      modelName: result.modelName,
      latencyMs: result.latencyMs ?? 0,
    });
  }

  return NextResponse.json({
    ok: true,
    decision: parsed.decision,
    confidence: parsed.confidence,
    rationale: parsed.rationale,
    modelName: result.modelName ?? 'unknown',
    latencyMs: result.latencyMs ?? 0,
  });
}
