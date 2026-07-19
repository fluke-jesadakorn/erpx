import { NextResponse } from 'next/server';
import { query } from '@/db';
import { apiGuard } from '@/server/apiGuard';
import { PERM } from '@/perm';
import { VISION_MODELS_CACHE_TAG } from '@/ai/loadVisionModels';
import { revalidateTag } from 'next/cache';


export async function GET(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.ai.model.read });
  if (guard.response) return guard.response;
  const r = await query(`SELECT id, name, provider_id, capabilities, context_window, enabled FROM ai_models ORDER BY id`);
  return NextResponse.json({ models: r.rows });
}

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.ai.model.create });
  if (guard.response) return guard.response;
  const body = await req.json().catch(() => ({}));
  if (!body.name || !body.provider_id) return NextResponse.json({ error: 'name and provider_id required' }, { status: 400 });
  const r = await query(
    `INSERT INTO ai_models (name, provider_id, capabilities, context_window, enabled, defaults_json)
     VALUES ($1, $2, $3, $4, $5, $6) RETURNING id`,
    [body.name, body.provider_id, body.capabilities || [], body.context_window || null, body.enabled !== false, JSON.stringify(body.defaults_json || {})],
  );
  revalidateTag(VISION_MODELS_CACHE_TAG, 'max');
  return NextResponse.json({ id: r.rows[0].id, ok: true });
}
