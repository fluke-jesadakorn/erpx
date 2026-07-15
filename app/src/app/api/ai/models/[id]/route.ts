import { NextResponse } from 'next/server';
import { query } from '@folio-lib/db';
import { apiGuard } from '@folio-lib/server/apiGuard';
import { PERM } from '@folio-lib/perm';


export async function PUT(req: Request, { params }: { params: Promise<{ id: string }> }) {
  const guard = await apiGuard(req, { perm: PERM.ai.model.update });
  if (guard.response) return guard.response;
  const { id } = await params;
  const body = await req.json().catch(() => ({}));
  const setParts: string[] = [];
  const sqlParams: unknown[] = [];
  let i = 1;
  if (body.name !== undefined) { setParts.push(`name = $${i++}`); sqlParams.push(body.name); }
  if (body.enabled !== undefined) { setParts.push(`enabled = $${i++}`); sqlParams.push(body.enabled); }
  if (body.capabilities !== undefined) { setParts.push(`capabilities = $${i++}`); sqlParams.push(body.capabilities); }
  if (body.context_window !== undefined) { setParts.push(`context_window = $${i++}`); sqlParams.push(body.context_window); }
  if (body.defaults_json !== undefined) { setParts.push(`defaults_json = $${i++}`); sqlParams.push(JSON.stringify(body.defaults_json)); }
  if (setParts.length === 0) return NextResponse.json({ error: 'nothing to update' }, { status: 400 });
  sqlParams.push(id);
  await query(`UPDATE ai_models SET ${setParts.join(', ')} WHERE id = $${i}`, sqlParams);
  return NextResponse.json({ ok: true });
}

export async function DELETE(req: Request, { params }: { params: Promise<{ id: string }> }) {
  const guard = await apiGuard(req, { perm: PERM.ai.model.delete });
  if (guard.response) return guard.response;
  const { id } = await params;
  await query(`DELETE FROM ai_models WHERE id = $1`, [id]);
  return NextResponse.json({ ok: true });
}