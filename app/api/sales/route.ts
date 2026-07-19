import { NextResponse } from 'next/server';
import { z } from 'zod';
import { apiGuard } from '@/server/apiGuard';
import { loadWaybill } from '@/waybill/queries';
import { recordEvent } from '@/waybill/events';
import { withTransaction } from '@/db';
import { hasPermission } from '@folio-lib/perm/server';
import { PERM } from '@folio-lib/perm/taxonomy';

export const dynamic = 'force-dynamic';

const ItemSchema = z.object({
  description: z.string().min(1),
  qty: z.coerce.number().positive(),
  unitPrice: z.coerce.number().nonnegative(),
  vatPct: z.coerce.number().min(0).max(100).optional().default(7),
});

const BodySchema = z.object({
  draftWaybillId: z.string().nullable().optional(),
  customerId: z.coerce.number().int().positive(),
  paymentTerms: z.string().min(1),
  dueDate: z.string().regex(/^\d{4}-\d{2}-\d{2}$/),
  items: z.array(ItemSchema).min(1),
  totals: z.object({
    subtotal: z.coerce.number().nonnegative(),
    vat: z.coerce.number().nonnegative(),
    total: z.coerce.number().nonnegative(),
  }),
});

function round2(n: number): number {
  return Math.round(n * 100) / 100;
}

export async function POST(req: Request) {
  const guard = await apiGuard(req, { perm: 'finance:sales:submit::allow' });
  if (guard.response) return guard.response;
  const actor = guard.actor;
  if (!actor) return NextResponse.json({ ok: false, error: 'unauthorized' }, { status: 401 });

  let body: unknown;
  try { body = await req.json(); } catch { return NextResponse.json({ ok: false, error: 'invalid json' }, { status: 400 }); }
  const parsed = BodySchema.safeParse(body);
  if (!parsed.success) {
    return NextResponse.json({ ok: false, error: parsed.error.issues.map((i) => i.message).join('; ') }, { status: 400 });
  }
  const input = parsed.data;

  let salesOrderId: number;
  let waybillId: string;
  let currentStage: string;

  try {
    const result = await withTransaction(async (q) => {
      let soId: number;
      let wbId: string;
      let stage: string;

      if (input.draftWaybillId) {
        const wb = await loadWaybill(input.draftWaybillId);
        if (!wb || wb.origin !== 'so') throw new Error('not a sales waybill');
        if (wb.submitter_id !== actor.id && !hasPermission(actor, PERM.admin.system.bypass)) {
          throw new Error('not your draft');
        }
        wbId = wb.id;
        soId = wb.origin_id;
        stage = wb.current_stage;
      } else {
        const soRes = await q<{ so_number: string; id: number }>(
          `INSERT INTO sales_orders
             (so_number, customer_id, sales_rep_id, status, payment_terms, due_date,
              subtotal, vat_total, total_amount, branch_id)
           VALUES (
             next_sales_order_number(EXTRACT(YEAR FROM now())::smallint),
             $1, $2, 'so_draft', $3, $4, $5, $6, $7,
             (SELECT id
               FROM finance.branches
               WHERE active
               ORDER BY id
               LIMIT 1)
           )
           RETURNING id, so_number`,
          [
            input.customerId,
            actor.id,
            input.paymentTerms,
            input.dueDate,
            input.totals.subtotal,
            input.totals.vat,
            input.totals.total,
          ],
        );
        soId = soRes.rows[0].id;

        const wbRes = await q<{ id: string }>(
          `INSERT INTO waybills
             (id, origin, origin_id, fiscal_year, waybill_kind, submitter_id,
              total_amount, current_stage, current_owner_role, current_owner_user_id, status)
           VALUES (
             next_waybill_number(EXTRACT(YEAR FROM now())::smallint),
             'so', $1,
             EXTRACT(YEAR FROM now())::smallint,
             'sales', $2,
             $3, 'so_draft', $4, $5, 'open'
           )
           RETURNING id`,
          [
            soId,
            actor.id,
            input.totals.total,
            actor.role_name ?? 'sales_rep',
            actor.id,
          ],
        );
        wbId = wbRes.rows[0].id;
        stage = 'so_draft';
      }

      await q(`DELETE FROM so_items WHERE sales_order_id = $1`, [soId]);

      for (const it of input.items) {
        const subtotal = round2(it.qty * it.unitPrice);
        const vat = round2((subtotal * (it.vatPct ?? 7)) / 100);
        const lineTotal = round2(subtotal + vat);
        await q(
          `INSERT INTO so_items
             (sales_order_id, description, qty, unit_price, vat_amount, line_total)
           VALUES ($1, $2, $3, $4, $5, $6)`,
          [soId, it.description, it.qty, it.unitPrice, vat, lineTotal],
        );
      }

      await q(
        `UPDATE sales_orders
            SET subtotal = $1, vat_total = $2, total_amount = $3,
                payment_terms = $4, due_date = $5,
                customer_id = $6, updated_at = now()
          WHERE id = $7`,
        [
          input.totals.subtotal,
          input.totals.vat,
          input.totals.total,
          input.paymentTerms,
          input.dueDate,
          input.customerId,
          soId,
        ],
      );

      const totalTHB = Number(input.totals.total);
      const nextStage = totalTHB < 5000 ? 'so_credit_check' : 'so_sales_review';
      const eventKind = totalTHB < 5000 ? 'so-auto-approved' : 'so-submitted';

      await q(
        `UPDATE sales_orders SET status = $1, updated_at = now() WHERE id = $2`,
        [nextStage, soId],
      );
      await q(
        `UPDATE waybills SET current_stage = $1, status = 'open', updated_at = now() WHERE id = $2`,
        [nextStage, wbId],
      );

      await recordEvent({
        waybillId: wbId,
        kind: eventKind,
        stageFrom: stage,
        stageTo: nextStage,
        actorId: actor.id,
        actorRole: actor.role_name ?? 'sales_rep',
        payload: {
          decision: 'submit',
          auto_approved: totalTHB < 5000,
          total_amount_thb: totalTHB,
          item_count: input.items.length,
        },
        client: q as never,
      });

      return { soId, wbId, nextStage };
    });

    salesOrderId = result.soId;
    waybillId = result.wbId;
    currentStage = result.nextStage;
  } catch (e: any) {
    return NextResponse.json({ ok: false, error: e?.message ?? 'submit failed' }, { status: 400 });
  }

  return NextResponse.json({ ok: true, waybillId, salesOrderId, currentStage });
}
