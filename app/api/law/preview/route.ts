import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { getContract, previewContract } from '@/law/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request) {
  const guard = await apiGuard(req);
  if (guard.response) return guard.response;
  const id = new URL(req.url).searchParams.get('id');
  if (!id) return NextResponse.json({ ok: false, error: 'id is required' }, { status: 400 });
  try {
    const contract = await getContract(id);
    if (!contract) return NextResponse.json({ ok: false, error: 'contract not found' }, { status: 404 });
    return NextResponse.json({ ok: true, ...(await previewContract(id)) });
  } catch (err) {
    return NextResponse.json({ ok: false, error: (err as Error).message }, { status: 500 });
  }
}
