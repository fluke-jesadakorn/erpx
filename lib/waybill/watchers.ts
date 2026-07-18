import 'server-only';
import { query } from '../db';
import { STAGE_TO_ROLE, stageDepartment } from '../perm/stages';

export interface WaybillWatcherRow {
  id: number;
  waybill_id: string;
  stage_key: string;
  user_id: number;
  notified_at: Date | null;
  created_at: Date;
}

export async function addWatcher(args: {
  waybillId: string;
  stageKey: string;
  userId: number;
}): Promise<WaybillWatcherRow> {
  const ins = await query<WaybillWatcherRow>(
    `INSERT INTO waybill_watchers (waybill_id, stage_key, user_id)
     VALUES ($1, $2, $3)
     ON CONFLICT (waybill_id, stage_key, user_id) DO NOTHING
     RETURNING id, waybill_id, stage_key, user_id, notified_at, created_at`,
    [args.waybillId, args.stageKey, args.userId],
  );
  if (ins.rows[0]) return ins.rows[0];
  const sel = await query<WaybillWatcherRow>(
    `SELECT id, waybill_id, stage_key, user_id, notified_at, created_at
       FROM waybill_watchers
      WHERE waybill_id = $1 AND stage_key = $2 AND user_id = $3`,
    [args.waybillId, args.stageKey, args.userId],
  );
  return sel.rows[0];
}

export async function removeWatcher(args: {
  waybillId: string;
  stageKey: string;
  userId: number;
}): Promise<void> {
  await query(
    `DELETE FROM waybill_watchers
      WHERE waybill_id = $1 AND stage_key = $2 AND user_id = $3`,
    [args.waybillId, args.stageKey, args.userId],
  );
}

export async function markWatchersNotified(args: {
  waybillId: string;
  stageKey: string;
  client?: typeof query;
}): Promise<number> {
  const q = args.client ?? query;
  const r = await q<{ id: number }>(
    `UPDATE waybill_watchers
        SET notified_at = now()
      WHERE waybill_id = $1 AND stage_key = $2 AND notified_at IS NULL
      RETURNING id`,
    [args.waybillId, args.stageKey],
  );
  return r.rows.length;
}

export async function listWatchersForWaybill(waybillId: string): Promise<WaybillWatcherRow[]> {
  const r = await query<WaybillWatcherRow>(
    `SELECT id, waybill_id, stage_key, user_id, notified_at, created_at
       FROM waybill_watchers
      WHERE waybill_id = $1
   ORDER BY id ASC`,
    [waybillId],
  );
  return r.rows;
}

export interface WaybillWatchingRow extends WaybillWatcherRow {
  current_stage: string;
  vendor_name: string | null;
  total_amount: string | null;
  currency: string;
}

export async function listWatchingForUser(
  userId: string | number,
): Promise<WaybillWatchingRow[]> {
  const r = await query<WaybillWatchingRow>(
    `SELECT w.id, w.waybill_id, w.stage_key, w.user_id, w.notified_at, w.created_at,
            wb.current_stage, wb.vendor_name, wb.total_amount, wb.currency
       FROM waybill_watchers w
       JOIN waybills wb ON wb.id = w.waybill_id
      WHERE w.user_id = $1
   ORDER BY w.created_at DESC`,
    [userId],
  );
  return r.rows;
}

export interface WaybillWaitingOnRow {
  id: string;
  origin: 'expense' | 'pr' | 'po';
  origin_id: number;
  waybill_kind: 'reimbursement' | 'procurement';
  current_stage: string;
  vendor_name: string | null;
  total_amount: string | null;
  currency: string;
  submitter_name: string | null;
  age_hours: number;
}

export async function listWaitingOnForUser(
  userId: string | number,
): Promise<WaybillWaitingOnRow[]> {
  const userRes = await query<{ role_id: string; dept_id: string | null; dept_group_id: string | null }>(
    `SELECT COALESCE((
              SELECT ur.role_id FROM perm.user_roles ur
                WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
                LIMIT 1
            ), '') AS role_id,
            ud.department_id AS dept_id,
            ud.department_id AS dept_group_id
       FROM users u
       LEFT JOIN perm.user_departments ud ON ud.user_id = u.id
      WHERE u.id = $1`,
    [userId],
  );
  const user = userRes.rows[0];
  if (!user || !user.role_id) return [];

  const roleName = user.role_id;
  const stages = Object.entries(STAGE_TO_ROLE)
    .filter(([stage, roles]) => roles.includes(roleName)
      && (!stageDepartment(stage) || stageDepartment(stage) === user.dept_id))
    .map(([s]) => s);
  if (stages.length === 0) return [];

  const r = await query<WaybillWaitingOnRow>(
    `SELECT wb.id, wb.origin, wb.origin_id, wb.waybill_kind, wb.current_stage,
            wb.vendor_name, wb.total_amount, wb.currency,
            sub.fullname AS submitter_name,
            EXTRACT(EPOCH FROM (now() - wb.updated_at)) / 3600.0 AS age_hours
       FROM waybills wb
       LEFT JOIN users sub ON sub.id = wb.submitter_id
      WHERE wb.status = 'open'
        AND wb.current_stage = ANY($1::text[])
        AND (
          wb.current_stage NOT IN ('submission','department_approval','dept_verification','dept_authorization')
          OR EXISTS (
            SELECT 1 FROM perm.user_departments sud
             WHERE sud.user_id = sub.id AND sud.department_id = $2
          )
        )
   ORDER BY wb.updated_at DESC
      LIMIT 100`,
    [stages, user.dept_group_id],
  );
  return r.rows;
}
