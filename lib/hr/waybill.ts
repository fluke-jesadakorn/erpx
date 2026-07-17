// lib/hr/waybill.ts — HR leave flow wired through the Waybill system.
//
// Every leave request becomes a Waybill (origin='hr_leave') so it inherits
// the same audit log, HMAC-signed event chain, and route handlers as
// expense/PR/PO. Stages: hr_review → hr_authorization → hr_disbursed.
//
// Public surface:
//   submitLeave({ employeeId, leaveType, startDate, endDate, days, reason, medicalCertNote })
//   approveLeaveWaybill(waybillId, actor)
//   rejectLeaveWaybill(waybillId, actor, reason)

import 'server-only';
import { query, withTransaction } from '../db';
import { generateWaybillId } from '../waybill/number';
import { recordEvent } from '../waybill/events';
import { matchPerm } from '../perm';

export type LeaveType = 'sick' | 'annual' | 'personal';

export interface SubmitLeaveInput {
  employeeId: number;
  leaveType: LeaveType;
  startDate: string;
  endDate: string;
  days: number;
  reason: string | null;
  medicalCertNote?: string | null;
}

export interface SubmitLeaveResult {
  waybillId: string;
  originId: number;
}

export interface HrLeaveRow {
  waybill_id: string;
  origin_id: number;
  employee_id: number;
  employee_name: string;
  employee_code: string;
  department: string | null;
  position: string;
  leave_type: LeaveType;
  start_date: string;
  end_date: string;
  days: string | number;
  reason: string | null;
  medical_cert_note: string | null;
  current_stage: string;
  status: string;
  submitter_name: string | null;
  created_at: string;
  updated_at: string;
}

interface HrLeaveDBRow {
  waybill_id: string;
  origin_id: number;
  employee_id: number;
  employee_name: string;
  employee_code: string;
  department: string | null;
  position: string;
  leave_type: LeaveType;
  start_date: string;
  end_date: string;
  days: string | number;
  reason: string | null;
  medical_cert_note: string | null;
  current_stage: string;
  status: string;
  submitter_name: string | null;
  created_at: string;
  updated_at: string;
}

function num(v: string | number | null | undefined): number {
  if (v == null) return 0;
  return typeof v === 'string' ? parseFloat(v) : v;
}

function rowToLeave(r: HrLeaveDBRow): HrLeaveRow {
  return {
    waybill_id: r.waybill_id,
    origin_id: r.origin_id,
    employee_id: r.employee_id,
    employee_name: r.employee_name,
    employee_code: r.employee_code,
    department: r.department,
    position: r.position,
    leave_type: r.leave_type,
    start_date: r.start_date,
    end_date: r.end_date,
    days: num(r.days),
    reason: r.reason,
    medical_cert_note: r.medical_cert_note,
    current_stage: r.current_stage,
    status: r.status,
    submitter_name: r.submitter_name,
    created_at: r.created_at,
    updated_at: r.updated_at,
  };
}

const HR_LEAVE_SELECT = `
  SELECT hl.waybill_id,
         w.origin_id,
         hl.employee_id,
         u.fullname      AS employee_name,
         u.employee_code,
         u.dept_label    AS department,
         u.position,
         hl.leave_type,
         hl.start_date::text,
         hl.end_date::text,
         hl.days::float   AS days,
         hl.reason,
         hl.medical_cert_note,
         w.current_stage,
         w.status,
         (SELECT fullname FROM users WHERE id = w.submitter_id) AS submitter_name,
         w.created_at::text,
         w.updated_at::text
    FROM hr_leave hl
    JOIN waybills w  ON w.id = hl.waybill_id
    JOIN users    u  ON u.id = hl.employee_id
`;

export async function submitLeave(input: SubmitLeaveInput): Promise<SubmitLeaveResult> {
  const waybillId = await generateWaybillId(new Date().getFullYear());
  return withTransaction(async (q) => {
    const nextRes = await q<{ next_id: number }>(
      `SELECT COALESCE(MAX(origin_id), 0) + 1 AS next_id
         FROM waybills WHERE origin = 'hr_leave'`,
    );
    const originId = nextRes.rows[0]?.next_id ?? 1;
    await q(
      `INSERT INTO waybills
         (id, origin, origin_id, fiscal_year, waybill_kind,
          submitter_id, current_stage, status, created_at, updated_at)
       VALUES
         ($1, 'hr_leave', $2, EXTRACT(YEAR FROM now())::smallint,
          'hr_leave', $3, 'hr_review', 'open', now(), now())`,
      [waybillId, originId, input.employeeId],
    );
    await q(
      `INSERT INTO hr_leave
         (waybill_id, employee_id, leave_type, start_date, end_date, days, reason, medical_cert_note)
       VALUES ($1, $2, $3, $4::date, $5::date, $6, $7, $8)`,
      [
        waybillId,
        input.employeeId,
        input.leaveType,
        input.startDate,
        input.endDate,
        input.days,
        input.reason,
        input.medicalCertNote ?? null,
      ],
    );
    await recordEvent({
      waybillId,
      kind: 'submitted',
      stageFrom: null,
      stageTo: 'hr_review',
      actorId: input.employeeId,
      actorRole: null,
      payload: {
        leaveType: input.leaveType,
        days: input.days,
        startDate: input.startDate,
        endDate: input.endDate,
        reason: input.reason,
      },
      client: q as never,
    });
    return { waybillId, originId };
  });
}

export async function approveLeaveWaybill(
  waybillId: string,
  actor: { id: number; role_name: string; permissions: string[] },
): Promise<HrLeaveRow | null> {
  const wbRow = await query<{ current_stage: string; status: string }>(
    `SELECT current_stage, status FROM waybills WHERE id = $1`,
    [waybillId],
  );
  if (wbRow.rows.length === 0) return null;
  const stage = wbRow.rows[0].current_stage;
  if (wbRow.rows[0].status !== 'open') return null;
  if (stage === 'hr_disbursed' || stage === 'rejected') return null;

  if (!matchPerm(actor.permissions, `stage:${stage}:act::allow`)
      && !matchPerm(actor.permissions, 'admin:system:bypass::allow')) {
    throw new Error('forbidden');
  }

  const nextStage =
    stage === 'hr_review' ? 'hr_authorization'
      : stage === 'hr_authorization' ? 'hr_disbursed'
        : null;
  if (!nextStage) throw new Error(`no next stage from ${stage}`);

  await withTransaction(async (q) => {
    await q(
      `UPDATE waybills
          SET current_stage = $1,
              status        = $2,
              updated_at    = now()
        WHERE id = $3`,
      [nextStage, nextStage === 'hr_disbursed' ? 'completed' : 'open', waybillId],
    );
    await recordEvent({
      waybillId,
      kind: 'advanced',
      stageFrom: stage,
      stageTo: nextStage,
      actorId: actor.id,
      actorRole: actor.role_name,
      payload: { decision: 'approve' },
      client: q as never,
    });
    if (nextStage === 'hr_disbursed') {
      await q(
        `UPDATE users u
            SET used_sick     = used_sick     + CASE WHEN hl.leave_type = 'sick'     THEN hl.days ELSE 0 END,
                used_annual   = used_annual   + CASE WHEN hl.leave_type = 'annual'   THEN hl.days ELSE 0 END,
                used_personal = used_personal + CASE WHEN hl.leave_type = 'personal' THEN hl.days ELSE 0 END
           FROM hr_leave hl
          WHERE hl.waybill_id = $1
            AND hl.employee_id = u.id`,
        [waybillId],
      );
    }
  });

  return getLeaveByWaybill(waybillId);
}

export async function rejectLeaveWaybill(
  waybillId: string,
  actor: { id: number; role_name: string; permissions: string[] },
  reason: string,
): Promise<HrLeaveRow | null> {
  if (!reason || !reason.trim()) throw new Error('Rejection reason is required');
  const wbRow = await query<{ current_stage: string; status: string }>(
    `SELECT current_stage, status FROM waybills WHERE id = $1`,
    [waybillId],
  );
  if (wbRow.rows.length === 0) return null;
  if (wbRow.rows[0].status !== 'open') return null;

  if (!matchPerm(actor.permissions, `stage:${wbRow.rows[0].current_stage}:act::allow`)
      && !matchPerm(actor.permissions, 'admin:system:bypass::allow')) {
    throw new Error('forbidden');
  }

  await withTransaction(async (q) => {
    await q(
      `UPDATE waybills
          SET current_stage = 'rejected',
              status        = 'rejected',
              updated_at    = now()
        WHERE id = $1`,
      [waybillId],
    );
    await q(
      `UPDATE hr_leave SET reason = COALESCE(reason, '') || ' [rejected: ' || $2 || ']'
        WHERE waybill_id = $1`,
      [waybillId, reason.trim()],
    );
    await recordEvent({
      waybillId,
      kind: 'rejected',
      stageFrom: wbRow.rows[0].current_stage,
      stageTo: 'rejected',
      actorId: actor.id,
      actorRole: actor.role_name,
      payload: { reason: reason.trim() },
      client: q as never,
    });
  });
  return getLeaveByWaybill(waybillId);
}

export async function getLeaveByWaybill(waybillId: string): Promise<HrLeaveRow | null> {
  const r = await query<HrLeaveDBRow>(
    `${HR_LEAVE_SELECT} WHERE hl.waybill_id = $1`,
    [waybillId],
  );
  return r.rows[0] ? rowToLeave(r.rows[0]) : null;
}

export interface ListLeaveFilter {
  status?: string;
  employeeId?: number;
  stage?: string;
  limit?: number;
}

export async function listLeave(filter: ListLeaveFilter = {}): Promise<HrLeaveRow[]> {
  const where: string[] = [];
  const params: unknown[] = [];
  if (filter.status) {
    params.push(filter.status);
    where.push(`w.status = $${params.length}`);
  }
  if (filter.stage) {
    params.push(filter.stage);
    where.push(`w.current_stage = $${params.length}`);
  }
  if (filter.employeeId) {
    params.push(filter.employeeId);
    where.push(`hl.employee_id = $${params.length}`);
  }
  params.push(filter.limit ?? 200);
  const r = await query<HrLeaveDBRow>(
    `${HR_LEAVE_SELECT}
     ${where.length ? 'WHERE ' + where.join(' AND ') : ''}
     ORDER BY w.created_at DESC
     LIMIT $${params.length}`,
    params,
  );
  return r.rows.map(rowToLeave);
}

export async function getEmployeeQuota(employeeId: number): Promise<{
  sick: { total: number; used: number; remaining: number };
  annual: { total: number; used: number; remaining: number };
  personal: { total: number; used: number; remaining: number };
} | null> {
  const r = await query<{
    quota_sick: number;
    used_sick: number;
    quota_annual: number;
    used_annual: number;
    quota_personal: number;
    used_personal: number;
  }>(
    `SELECT quota_sick, used_sick, quota_annual, used_annual,
            quota_personal, used_personal
       FROM users WHERE id = $1`,
    [employeeId],
  );
  const row = r.rows[0];
  if (!row) return null;
  return {
    sick:     { total: row.quota_sick,     used: row.used_sick,     remaining: row.quota_sick - row.used_sick },
    annual:   { total: row.quota_annual,   used: row.used_annual,   remaining: row.quota_annual - row.used_annual },
    personal: { total: row.quota_personal, used: row.used_personal, remaining: row.quota_personal - row.used_personal },
  };
}

export async function updateEmployeeQuota(
  employeeId: number,
  patch: {
    quotaSick?: number;
    quotaAnnual?: number;
    quotaPersonal?: number;
  },
  _actorId: number,
): Promise<{
  sick: { from: number; to: number };
  annual: { from: number; to: number };
  personal: { from: number; to: number };
} | null> {
  const existing = await getEmployeeQuota(employeeId);
  if (!existing) return null;
  const newSick     = patch.quotaSick     ?? existing.sick.total;
  const newAnnual   = patch.quotaAnnual   ?? existing.annual.total;
  const newPersonal = patch.quotaPersonal ?? existing.personal.total;
  await query(
    `UPDATE users
        SET quota_sick     = $1,
            used_sick      = LEAST(used_sick, $1),
            quota_annual   = $2,
            used_annual    = LEAST(used_annual, $2),
            quota_personal = $3,
            used_personal  = LEAST(used_personal, $3)
      WHERE id = $4`,
    [newSick, newAnnual, newPersonal, employeeId],
  );
  return {
    sick:     { from: existing.sick.total,     to: newSick },
    annual:   { from: existing.annual.total,   to: newAnnual },
    personal: { from: existing.personal.total, to: newPersonal },
  };
}

export async function listHrLeaveStats(): Promise<{
  total: number;
  pending: number;
  approved: number;
  rejected: number;
}> {
  const r = await query<{
    total: string;
    pending: string;
    approved: string;
    rejected: string;
  }>(
    `SELECT COUNT(*) AS total,
            COUNT(*) FILTER (WHERE w.status = 'open')     AS pending,
            COUNT(*) FILTER (WHERE w.status = 'completed') AS approved,
            COUNT(*) FILTER (WHERE w.status = 'rejected')  AS rejected
       FROM waybills WHERE origin = 'hr_leave'`,
  );
  const row = r.rows[0] ?? { total: '0', pending: '0', approved: '0', rejected: '0' };
  return {
    total:    parseInt(row.total,    10) || 0,
    pending:  parseInt(row.pending,  10) || 0,
    approved: parseInt(row.approved, 10) || 0,
    rejected: parseInt(row.rejected, 10) || 0,
  };
}
