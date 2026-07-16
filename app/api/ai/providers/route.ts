import { NextResponse } from 'next/server';
import { query } from '@/db';
import { encryptKey } from '@/ai/crypto';
import { apiGuard } from '@/server/apiGuard';
import { PERM } from '@/perm';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const SELECT = `SELECT id, name, type, base_url, enabled, preset, notes,
                       CASE WHEN api_key_enc IS NULL THEN false ELSE true END AS has_api_key,
                       created_at, updated_at
                FROM ai_providers ORDER BY id`;

export async function GET(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.ai.provider.read });
  if (guard.response) return guard.response;
  const res = await query(SELECT);
  return NextResponse.json({ providers: res.rows });
}

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.ai.provider.create });
  if (guard.response) return guard.response;

  const body = await req.json().catch(() => ({}));
  if (!body.name || !body.type || !body.base_url) {
    return NextResponse.json({ error: 'name, type, base_url required' }, { status: 400 });
  }
  if (!['ollama', 'openai_compat', 'minimax'].includes(body.type)) {
    return NextResponse.json({ error: 'invalid type' }, { status: 400 });
  }
  const keyBuf = body.api_key ? await encryptKey(body.api_key) : null;
  try {
    const res = await query(
      `INSERT INTO ai_providers (name, type, base_url, api_key_enc, enabled, preset, notes)
       VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id`,
      [body.name, body.type, body.base_url, keyBuf, body.enabled !== false, body.preset || null, body.notes || null],
    );
    return NextResponse.json({ id: res.rows[0].id, ok: true });
  } catch (e: unknown) {
    if ((e as { code?: string }).code === '23505') return NextResponse.json({ error: 'name already exists' }, { status: 409 });
    throw e;
  }
}

export async function PUT() {
  return NextResponse.json({ error: 'use /api/ai/providers/[id]' }, { status: 404 });
}

export async function DELETE() {
  return NextResponse.json({ error: 'use /api/ai/providers/[id]' }, { status: 404 });
}