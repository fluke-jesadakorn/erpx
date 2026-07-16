import { NextResponse } from 'next/server';
import { query } from '@/db';
import { apiGuard } from '@/server/apiGuard';
import { PERM } from '@/perm';


export async function PUT(req: Request, { params }: { params: Promise<{ id: string }> }) {
  const guard = await apiGuard(req, { perm: PERM.ai.staff.update });
  if (guard.response) return guard.response;
  const { id } = await params;
  const body = await req.json().catch(() => ({}));
  const setParts: string[] = [];
  const sqlParams: unknown[] = [];
  let i = 1;
  for (const k of ['name', 'role_label', 'system_prompt', 'default_provider_id', 'default_model_id', 'enabled'] as const) {
    if (body[k] !== undefined) { setParts.push(`${k} = $${i++}`); sqlParams.push(body[k]); }
  }
  if (body.capabilities !== undefined) { setParts.push(`capabilities = $${i++}`); sqlParams.push(body.capabilities); }
  if (setParts.length === 0) return NextResponse.json({ error: 'nothing to update' }, { status: 400 });
  sqlParams.push(id);
  await query(`UPDATE ai_staff SET ${setParts.join(', ')} WHERE id = $${i}`, sqlParams);
  return NextResponse.json({ ok: true });
}

export async function DELETE(req: Request, { params }: { params: Promise<{ id: string }> }) {
  const guard = await apiGuard(req, { perm: PERM.ai.staff.delete });
  if (guard.response) return guard.response;
  const { id } = await params;
  await query(`DELETE FROM ai_staff WHERE id = $1`, [id]);
  return NextResponse.json({ ok: true });
}