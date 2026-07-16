import { NextResponse } from 'next/server';
import { query } from '@/db';
import * as ollama from '@/ai/providers/ollama';
import * as openai from '@/ai/providers/openai';
import { decryptKey } from '@/ai/crypto';
import { apiGuard } from '@/server/apiGuard';
import { PERM } from '@/perm';


export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.ai.provider.test });
  if (guard.response) return guard.response;

  const body = await req.json().catch(() => ({}));
  let type = body.type;
  let baseUrl = body.base_url;
  let apiKey: string | null = body.api_key || null;

  if (body.id) {
    const r = await query<{ type: string; base_url: string; api_key_enc: Buffer | null }>(
      'SELECT type, base_url, api_key_enc FROM ai_providers WHERE id = $1', [body.id],
    );
    if (r.rows.length === 0) return NextResponse.json({ ok: false, error: 'provider not found' }, { status: 404 });
    type = r.rows[0].type;
    baseUrl = r.rows[0].base_url;
    if (r.rows[0].api_key_enc) apiKey = await decryptKey(r.rows[0].api_key_enc);
  }

  if (!type || !baseUrl) return NextResponse.json({ ok: false, error: 'type and base_url required' }, { status: 400 });

  try {
    const t0 = Date.now();
    const models = type === 'ollama'
      ? await ollama.ollamaListModels({ baseUrl })
      : await openai.openaiListModels({ baseUrl, apiKey });
    return NextResponse.json({ ok: true, type, baseUrl, modelCount: models.length, sample: models.slice(0, 5), latencyMs: Date.now() - t0 });
  } catch (e: unknown) {
    return NextResponse.json({ ok: false, error: (e as { message?: string }).message || String(e) });
  }
}