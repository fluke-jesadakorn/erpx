import { NextRequest, NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { computeRiskScore } from '@/waybill/risk';

export const dynamic = 'force-dynamic';

export async function GET(req: NextRequest, ctx: { params: Promise<{ id: string }> }): Promise<Response> {
  const guard = await apiGuard(req, { perm: 'tile:inbox:view::allow' });
  if (guard.response) return guard.response;
  const { id } = await ctx.params;
  const score = await computeRiskScore(id);
  return NextResponse.json({ ok: true, waybillId: id, riskScore: score });
}