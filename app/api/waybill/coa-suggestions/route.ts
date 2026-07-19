import { NextResponse } from 'next/server';
import { getSemanticSuggestions } from '@/waybill/queries';
import { loadActivePermSession } from '@/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

interface Body {
  description?: string;
}

export interface CoaSuggestionOut {
  code: string;
  name: string | null;
  name_th: string | null;
  normal_side: 'debit' | 'credit';
  similarity: number;
}

function deriveNormalSide(account_type: unknown): 'debit' | 'credit' {
  const t = String(account_type ?? '').toLowerCase();
  if (t === 'asset' || t === 'expense') return 'debit';
  return 'credit';
}

export async function POST(req: Request) {
  const session = await loadActivePermSession(req);
  if (!session) {
    return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });
  }

  const body = (await req.json().catch(() => ({}))) as Body;
  const description = typeof body.description === 'string' ? body.description : '';
  if (!description.trim()) {
    return NextResponse.json({ success: true, suggestions: [] });
  }

  const out = await getSemanticSuggestions(description, session.decoded.user.id);
  if (!out || out.success === false) {
    return NextResponse.json({
      success: false,
      suggestions: [],
      error: out && 'error' in out ? out.error : 'AI unavailable',
    });
  }

  const raw = Array.isArray(out.suggestions) ? out.suggestions : [];
  const suggestions: CoaSuggestionOut[] = raw.map((r: Record<string, unknown>) => {
    const side = r.normal_side === 'credit' || r.normal_side === 'debit'
      ? r.normal_side
      : deriveNormalSide(r.account_type);
    return {
      code: String(r.code ?? ''),
      name: r.name == null ? null : String(r.name),
      name_th: r.name_th == null ? null : String(r.name_th),
      normal_side: side,
      similarity: Number(r.similarity ?? 0),
    };
  });

  return NextResponse.json({ success: true, suggestions });
}
