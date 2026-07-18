import { NextResponse } from 'next/server';
import { invoke } from '@/ai/router';
import { loadActor } from '@/server/guard';
import { hasPermission } from '@/perm/auth-client';
import { getSecondaryLocaleFromHeaders } from '@/server/locale';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface Body {
  waybillId?: string;
  eventId?: string;
  eventKind?: string;
  fromStage?: string;
  toStage?: string;
  actorName?: string | null;
  lang?: 'en' | 'th' | 'de';
}

function stripThink(s: string): string {
  return (s ?? '').replace(/<\/?think[^>]*>/g, '').trim();
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
  const eventKind = typeof body.eventKind === 'string' ? body.eventKind.trim() : '';
  const fromStage = typeof body.fromStage === 'string' ? body.fromStage.trim() : '';
  const toStage = typeof body.toStage === 'string' ? body.toStage.trim() : '';
  const actorName =
    typeof body.actorName === 'string' && body.actorName.trim().length > 0
      ? body.actorName.trim()
      : 'unknown actor';
  const headerLocale = getSecondaryLocaleFromHeaders(req.headers);
  const lang: 'en' | 'th' | 'de' = headerLocale === 'de' ? 'de' : headerLocale === 'th' ? 'th' : 'en';

  if (!waybillId || !eventKind || !toStage) {
    return NextResponse.json(
      { ok: false, error: 'waybillId, eventKind, toStage are required' },
      { status: 400 },
    );
  }

  const languageHint =
    lang === 'th' ? 'เขียนคำตอบเป็นภาษาไทย' :
    lang === 'de' ? 'Schreiben Sie die Antwort auf Deutsch.' :
    'Write the answer in English.';

  const systemPrompt =
    `You are explaining a single waybill event to a Thai office worker. ` +
    `Event kind=${eventKind}, from stage=${fromStage}, to stage=${toStage}, actor=${actorName}. ` +
    `Output 2 short sentences in the same language as the question: ` +
    `(1) what happened, (2) what the next state will trigger or require. ` +
    `Prose, no bullets. ${languageHint}`;

  const userText =
    lang === 'th'
      ? `ช่วยอธิบายเหตุการณ์นี้ให้พนักงานไทยเข้าใจง่าย`
      : lang === 'de'
        ? `Erklären Sie dieses Waybill-Ereignis in einfacher Sprache.`
        : `Explain this waybill event in plain language.`;

  const result = await invoke(
    'events:explain',
    'chat',
    { text: userText, systemPrompt, temperature: 0.3 },
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

  const text = stripThink(result.text);

  return NextResponse.json({
    ok: true,
    text,
    modelName: result.modelName ?? 'unknown',
    latencyMs: result.latencyMs ?? 0,
  });
}
