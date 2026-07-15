import { NextResponse } from 'next/server';
import { invoke } from '@folio-lib/ai/router';
import { loadActor } from '@folio-lib/server/guard';
import { buildTileSystemPrompt } from '@folio-lib/ai/systemPrompts';
import { getSecondaryLocaleFromHeaders } from '@folio-lib/server/locale';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface Body {
  tileId?: string;
  tileDisplayName?: string;
  roleName?: string;
  fullname?: string;
  lang?: 'en' | 'th' | 'de';
}

export async function POST(req: Request) {
  const actor = await loadActor();
  if (!actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });

  const body = (await req.json().catch(() => ({}))) as Body;
  const tileId = body.tileId ?? '';
  const tileDisplayName = body.tileDisplayName ?? tileId;
  if (!tileId) {
    return NextResponse.json({ ok: false, error: 'tileId is required' }, { status: 400 });
  }

  const locale = getSecondaryLocaleFromHeaders(req.headers);

  const systemPrompt = buildTileSystemPrompt(tileId, tileDisplayName, {
    roleName: body.roleName,
    lang: locale,
  });

  const greeting = body.fullname ? `Hi, my name is ${body.fullname}. ` : '';
  const userText = `${greeting}Tell me about the ${tileDisplayName} tile.`;

  const result = await invoke(
    'tile:explainer',
    'chat',
    { text: userText, systemPrompt },
    { actorId: actor.id },
  );

  return NextResponse.json(result);
}