import 'server-only';
import { query, withTransaction } from '../db';
import { recordEvent } from './events';
import { normalizeStage, stagePrimaryRole } from '../perm/stages';
import { pipsForDomain, domainForOrigin, type WaybillDomain } from './derive';

interface WaybillForRecall {
  id: string;
  origin: 'expense' | 'pr' | 'po';
  origin_id: number;
  waybill_kind: 'reimbursement' | 'procurement';
  current_stage: string;
  status: string;
}

export async function reCallWaybillAction(args: {
  waybillId: string;
  targetStage: string;
  actorId: number;
  actorRole: string;
  reason: string;
}): Promise<{ ok: boolean; error?: string }> {
  const wbRes = await query<WaybillForRecall>(
    `SELECT id, origin, origin_id, waybill_kind, current_stage, status
       FROM waybills WHERE id = $1`,
    [args.waybillId],
  );
  const wb = wbRes.rows[0];
  if (!wb) return { ok: false, error: 'Waybill not found' };

  const targetCanon = normalizeStage(args.targetStage);
  if (!targetCanon) {
    return { ok: false, error: `stage '${args.targetStage}' not recognized` };
  }

  const domain: WaybillDomain = domainForOrigin(wb.origin);
  const pips = pipsForDomain(domain);
  const curIdx = pips.findIndex((p) => p.key === wb.current_stage);
  const targetIdx = pips.findIndex((p) => p.key === targetCanon);
  if (curIdx < 0) return { ok: false, error: `stage '${wb.current_stage}' not in pipeline` };
  if (targetIdx < 0) return { ok: false, error: `stage '${targetCanon}' not in pipeline` };
  if (targetIdx >= curIdx) {
    return {
      ok: false,
      error: `target stage '${targetCanon}' must precede current '${wb.current_stage}'`,
    };
  }
  if (wb.status !== 'open') {
    return { ok: false, error: `cannot recall waybill with status '${wb.status}'` };
  }

  const newOwner = stagePrimaryRole(targetCanon);

  await withTransaction(async (q) => {
    if (wb.origin === 'expense') {
      await q(
        `UPDATE expenses SET status = $1, updated_at = now() WHERE id = $2`,
        [targetCanon, wb.origin_id],
      );
    } else if (wb.origin === 'pr') {
      await q(
        `UPDATE purchase_requisitions SET status = $1, updated_at = now() WHERE id = $2`,
        [targetCanon, wb.origin_id],
      );
    } else if (wb.origin === 'po') {
      await q(
        `UPDATE purchase_orders SET status = $1, updated_at = now() WHERE id = $2`,
        [targetCanon, wb.origin_id],
      );
    }
    await q(
      `UPDATE waybills
          SET current_stage = $1,
              current_owner_role = $2,
              status = 'open',
              updated_at = now()
        WHERE id = $3`,
      [targetCanon, newOwner, wb.id],
    );
    await recordEvent({
      waybillId: wb.id,
      kind: 'authorization-overridden',
      stageFrom: wb.current_stage,
      stageTo: targetCanon,
      actorId: args.actorId,
      actorRole: args.actorRole,
      payload: {
        reason: args.reason,
        reCalledBy: args.actorId,
        reCalledByRole: args.actorRole,
      },
      client: q as never,
    });
  });

  return { ok: true };
}