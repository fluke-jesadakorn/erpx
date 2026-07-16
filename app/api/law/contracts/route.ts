import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { deleteContract, listContracts } from '@/law/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const guard = await apiGuard(req);
  if (guard.response) return guard.response;
  const url = new URL(req.url);
  const status = url.searchParams.get('status') || undefined;
  const limit = Number(url.searchParams.get('limit') || 50);
  try {
    const contracts = await listContracts({ status, limit });
    return NextResponse.json({ ok: true, contracts });
  } catch (err) {
    return NextResponse.json({ ok: false, error: (err as Error).message }, { status: 500 });
  }
}

export async function DELETE(req: Request) {
  const guard = await apiGuard(req);
  if (guard.response) return guard.response;
  const id = new URL(req.url).searchParams.get('id');
  if (!id) return NextResponse.json({ ok: false, error: 'id is required' }, { status: 400 });
  try {
    const deleted = await deleteContract(id);
    if (!deleted) return NextResponse.json({ ok: false, error: 'contract not found' }, { status: 404 });
    return NextResponse.json({ ok: true, id });
  } catch (err) {
    return NextResponse.json({ ok: false, error: (err as Error).message }, { status: 500 });
  }
}
