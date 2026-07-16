import { NextResponse } from 'next/server';
import { query } from '@/db';
import { apiGuard } from '@/server/apiGuard';
import { PERM } from '@/perm';


export async function GET(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.ai.staff.read });
  if (guard.response) return guard.response;
  const r = await query(`SELECT id, name, role_label, system_prompt, capabilities, default_provider_id, default_model_id, enabled FROM ai_staff ORDER BY id`);
  return NextResponse.json({ staff: r.rows });
}

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.ai.staff.create });
  if (guard.response) return guard.response;
  const body = await req.json().catch(() => ({}));
  if (!body.name) return NextResponse.json({ error: 'name required' }, { status: 400 });
  const r = await query(
    `INSERT INTO ai_staff (name, role_label, system_prompt, capabilities, default_provider_id, default_model_id, enabled)
     VALUES ($1,$2,$3,$4,$5,$6,$7) RETURNING id`,
    [body.name, body.role_label || null, body.system_prompt || '', body.capabilities || [], body.default_provider_id || null, body.default_model_id || null, body.enabled !== false],
  );
  return NextResponse.json({ id: r.rows[0].id, ok: true });
}

export async function PUT() {
  return NextResponse.json({ error: 'use /api/ai/staff/[id]' }, { status: 404 });
}

export async function DELETE() {
  return NextResponse.json({ error: 'use /api/ai/staff/[id]' }, { status: 404 });
}