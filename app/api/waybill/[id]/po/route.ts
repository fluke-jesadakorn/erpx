import { NextResponse } from 'next/server';
import { apiGuard } from '@/server/apiGuard';
import { loadWaybill } from '@/waybill/queries';
import { matchPerm } from '@/perm/grammar';
import { ensurePoPdf } from '@/finance/poPdf';
import { get } from '@/slips/storage';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

export async function GET(req: Request, ctx: { params: Promise<{ id: string }> }): Promise<Response> {
  const guard = await apiGuard(req);
  if (guard.response) return guard.response;
  const { id } = await ctx.params;
  const wb = await loadWaybill(id);
  if (!wb) return NextResponse.json({ error: 'not found' }, { status: 404 });
  const actor = guard.actor;
  const allowed = actor.id === wb.submitter_id
    || matchPerm(actor.permissions, 'finance:expense:view_all::allow')
    || matchPerm(actor.permissions, 'finance:pr:approve::allow')
    || matchPerm(actor.permissions, 'finance:po:approve::allow')
    || matchPerm(actor.permissions, 'finance:report:executive::allow')
    || matchPerm(actor.permissions, 'admin:system:bypass::allow');
  if (!allowed) return NextResponse.json({ error: 'forbidden' }, { status: 403 });
  try {
    const key = await ensurePoPdf(id, actor.fullname ?? actor.role_name ?? `User ${actor.id}`);
    const file = await get(key);
    return new Response(new Uint8Array(file), {
      headers: {
        'Content-Type': 'application/pdf',
        'Content-Length': String(file.length),
        'Content-Disposition': `attachment; filename="PO-${id}.pdf"`,
        'Cache-Control': 'private, no-store',
      },
    });
  } catch (error) {
    const message = error instanceof Error ? error.message : String(error);
    return NextResponse.json({ error: message }, { status: 409 });
  }
}
