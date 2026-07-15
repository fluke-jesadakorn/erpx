import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { PERM } from '@folio-lib/perm';


export async function GET(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.ai.assignment.read });
  if (guard.response) return guard.response;
  const r = await query(`SELECT * FROM ai_assignments ORDER BY section_key, task_type, priority`);
  return NextResponse.json({ assignments: r.rows });
}

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.ai.assignment.create });
  if (guard.response) return guard.response;
  const body = await req.json().catch(() => ({}));
  if (!body.section_key || !body.task_type || !body.provider_id || !body.model_id) {
    return NextResponse.json({ error: 'section_key, task_type, provider_id, model_id required' }, { status: 400 });
  }
  const r = await query(
    `INSERT INTO ai_assignments (section_key, task_type, provider_id, model_id, staff_id, params_json, priority, enabled)
     VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id`,
    [body.section_key, body.task_type, body.provider_id, body.model_id, body.staff_id || null, JSON.stringify(body.params_json || {}), body.priority ?? 100, body.enabled !== false],
  );
  return NextResponse.json({ id: r.rows[0].id, ok: true });
}

export async function DELETE(req: Request) {
  const guard = await apiGuard(req, { perm: PERM.ai.assignment.delete });
  if (guard.response) return guard.response;
  const url = new URL(req.url);
  const id = url.searchParams.get('id');
  if (!id) return NextResponse.json({ error: 'id required' }, { status: 400 });
  await query(`DELETE FROM ai_assignments WHERE id = $1`, [id]);
  return NextResponse.json({ ok: true });
}