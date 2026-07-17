import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { suggestSoCoa, type SoCoaSuggestion } from '@folio-lib/sales/coa';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface Body {
  soId?: number;
}

export interface SoCoaSuggestionOut {
  itemId: number;
  code: string;
  name: string | null;
  nameTh: string | null;
  normal_side: 'debit' | 'credit';
  similarity: number;
}

function normalizeSide(side: string | null | undefined): 'debit' | 'credit' {
  return side === 'debit' ? 'debit' : 'credit';
}

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: 'tile:sales:view::allow' });
  if (guard.response) return guard.response;

  const body = (await req.json().catch(() => ({}))) as Body;
  const soId = Number(body.soId);
  if (!Number.isFinite(soId) || soId <= 0) {
    return NextResponse.json({ success: false, suggestions: [], error: 'invalid input' }, { status: 400 });
  }

  const list = await suggestSoCoa(soId);
  const suggestions: SoCoaSuggestionOut[] = list.map((s: SoCoaSuggestion) => ({
    itemId: s.itemId,
    code: s.code,
    name: s.name,
    nameTh: s.nameTh,
    normal_side: normalizeSide(s.normalSide),
    similarity: s.similarity,
  }));

  return NextResponse.json({ success: true, suggestions });
}