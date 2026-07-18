import { NextRequest, NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActor } from '@/server/guard';
import { hasPermission, loadActivePermSession } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function POST(req: NextRequest, ctx: { params: Promise<{ id: string }> }) {
  const actor = await loadActor();
  const session = await loadActivePermSession(req);
  if (!actor || !session) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  const { id } = await ctx.params;
  const body = await req.json();
  const decision = body.decision as 'approved' | 'denied';
  if (decision !== 'approved' && decision !== 'denied') {
    return NextResponse.json({ error: 'invalid decision' }, { status: 400 });
  }
  const allowed = hasPermission(session.session, PERM.access_request.request.resolve);
  if (!allowed) return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  const r = await query(
    `UPDATE access_requests
        SET status = $1,
            resolved_at = NOW(),
            resolved_by_user_id = $2,
            resolved_note = $3
      WHERE id = $4 AND status = 'pending'
      RETURNING id`,
    [decision, actor.id, body.note || null, id],
  );
  return NextResponse.json({ updated: r.rows.length > 0 });
}