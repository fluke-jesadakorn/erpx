import { NextResponse } from 'next/server';
import { query } from '@/db';
import { encryptKey } from '@/ai/crypto';
import { apiGuard } from '@/server/apiGuard';
import { PERM } from '@/perm';


export async function PUT(req: Request, { params }: { params: Promise<{ id: string }> }) {
  const guard = await apiGuard(req, { perm: PERM.ai.provider.update });
  if (guard.response) return guard.response;

  const { id } = await params;
  const body = await req.json().catch(() => ({}));
  const setParts: string[] = [];
  const sqlParams: unknown[] = [];
  let i = 1;
  if (body.name !== undefined) { setParts.push(`name = $${i++}`); sqlParams.push(body.name); }
  if (body.base_url !== undefined) { setParts.push(`base_url = $${i++}`); sqlParams.push(body.base_url); }
  if (body.enabled !== undefined) { setParts.push(`enabled = $${i++}`); sqlParams.push(body.enabled); }
  if (body.notes !== undefined) { setParts.push(`notes = $${i++}`); sqlParams.push(body.notes); }
  if (body.api_key !== undefined) {
    const keyBuf = body.api_key ? await encryptKey(body.api_key) : null;
    setParts.push(`api_key_enc = $${i++}`);
    sqlParams.push(keyBuf);
  }
  if (setParts.length === 0) return NextResponse.json({ error: 'nothing to update' }, { status: 400 });
  sqlParams.push(id);
  await query(`UPDATE ai_providers SET ${setParts.join(', ')} WHERE id = $${i}`, sqlParams);
  return NextResponse.json({ ok: true });
}

export async function DELETE(req: Request, { params }: { params: Promise<{ id: string }> }) {
  const guard = await apiGuard(req, { perm: PERM.ai.provider.delete });
  if (guard.response) return guard.response;
  const { id } = await params;
  await query(`DELETE FROM ai_providers WHERE id = $1`, [id]);
  return NextResponse.json({ ok: true });
}