import { NextResponse } from 'next/server';
import { loadActor } from '@/server/guard';
import { query } from '@/db';
import { withTransaction } from '@/db';
import { loadWaybill } from '@/waybill/queries';

export const dynamic = 'force-dynamic';

export async function POST(req: Request) {
  const actor = await loadActor();
  if (!actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });

  let body: any = {};
  try { body = await req.json(); } catch (_e) {
    try {
      const fd = await req.formData();
      body = { waybillId: fd.get('waybillId') };
    } catch {}
  }
  const waybillId = body.waybillId;
  if (!waybillId) return NextResponse.json({ ok: false, error: 'waybillId required' }, { status: 400 });

  const wb = await loadWaybill(String(waybillId));
  if (!wb || wb.origin !== 'so') return NextResponse.json({ ok: false, error: 'not a sales waybill' }, { status: 404 });
  if (wb.submitter_id !== actor.id && actor.role_name !== 'admin' && actor.role_name !== 'sales_supervisor') {
    return NextResponse.json({ ok: false, error: 'not your draft' }, { status: 403 });
  }

  await withTransaction(async (q) => {
    await q(`DELETE FROM waybill_events WHERE waybill_id = $1 AND kind = 'so-created'`, [wb.id]);
    await q(`DELETE FROM waybills WHERE id = $1`, [wb.id]).catch(() => null);
    await q(`DELETE FROM journal_entries WHERE so_id = $1`, [wb.origin_id]);
    await q(`DELETE FROM so_items WHERE sales_order_id = $1`, [wb.origin_id]);
    await q(`DELETE FROM sales_orders WHERE id = $1`, [wb.origin_id]);
    await query(`SELECT 1`, []);
  }).catch((e) => {
    console.error('discardSalesDraft error', e);
  });

  return NextResponse.json({ ok: true });
}
