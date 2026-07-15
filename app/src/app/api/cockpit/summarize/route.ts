import { NextResponse } from 'next/server';
import { invoke } from '@folio-lib/ai/router';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { parseChartBlocks } from '@/components/chat/chartContract';
import { parseExtractBlocks } from '@/components/chat/extractContract';
import { cockpitSummarizePrompt, renderLocaleAwarePrompt } from '@folio-lib/ai/systemPrompts';
import { getSecondaryLocaleFromHeaders } from '@folio-lib/server/locale';

export const dynamic = 'force-dynamic';

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: 'tile:cockpit:view::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor;
  if (!actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });

  let body: any = {};
  try { body = await req.json(); } catch {}
  const { messages } = body;
  if (!Array.isArray(messages) || messages.length === 0) {
    return NextResponse.json({ ok: false, error: 'messages required' }, { status: 400 });
  }

  const lang = getSecondaryLocaleFromHeaders(req.headers);
  const systemPrompt = renderLocaleAwarePrompt(cockpitSummarizePrompt, lang);

  const result = await invoke(
    'cockpit:summarize',
    'chat',
    {
      messages,
      systemPrompt,
      temperature: 0.3,
    },
    { actorId: actor.id },
  );

  if (!result.ok) {
    return NextResponse.json({ ok: false, error: result.error || 'AI call failed' }, { status: 200 });
  }

  const fullText = (result.text || '').replace(/<think>[\s\S]*?<\/think>/g, '').trim();
  const { plain, charts } = parseChartBlocks(fullText);
  const { extracts } = parseExtractBlocks(plain);

  return NextResponse.json({
    ok: true,
    text: fullText,
    plain,
    charts,
    extracts,
    modelName: result.modelName,
    latencyMs: result.latencyMs,
  });
}