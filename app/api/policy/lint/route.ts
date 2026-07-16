import { NextResponse } from 'next/server';
import { lintPolicy } from '@/policy/lint';

export const dynamic = 'force-dynamic';

export async function POST(req: Request) {
  const body = await req.json().catch(() => null);
  const policyId = String(body?.policyId ?? '');
  if (!policyId) {
    return NextResponse.json({ ok: false, error: 'invalid policyId' }, { status: 400 });
  }
  const r = await lintPolicy(policyId);
  if (!r) return NextResponse.json({ ok: false, error: 'policy not found or AI failed' }, { status: 404 });
  return NextResponse.json({ ok: true, lint: r });
}