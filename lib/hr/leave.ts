import 'server-only';
import { query, withTransaction } from '../db';
import { generateWaybillId } from '../waybill/number';
import { recordEvent } from '../waybill/events';

export type LeaveType = 'sick' | 'annual' | 'personal';
export type LeaveStatus = 'pending' | 'approved' | 'rejected';

export interface LeaveRequestRow {
  id: string;
  employee_id: number;
  employee_name: string;
  employee_code: string;
  department: string | null;
  position: string;
  leave_type: LeaveType;
  start_date: string;
  end_date: string;
  days: number;
  reason: string | null;
  reject_reason: string | null;
  status: LeaveStatus;
  approved_by: number | null;
  approved_by_name: string | null;
  current_stage: string;
  created_at: string;
}

export interface SubmitLeaveInput {
  employeeId: number;
  leaveType: LeaveType;
  startDate: string;
  endDate: string;
  days: number;
  reason: string | null;
  medicalCertNote?: string | null;
}

export interface LeaveStats {
  total: number;
  pending: number;
  approved: number;
  rejected: number;
}

export interface DeptStat {
  department: string;
  total_days: number;
}

interface LeaveDBRow {
  waybill_id: string;
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
  reject_reason: string | null;
  wb_status: string;
  current_stage: string;
  approver_id: number | null;
  approver_name: string | null;
  created_at: string;
}

const LEAVE_SELECT = `
  SELECT hl.waybill_id,
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
         NULL::text       AS reject_reason,
         w.status         AS wb_status,
         w.current_stage,
         (SELECT actor_id FROM folio.waybill_events
            WHERE waybill_id = hl.waybill_id AND kind = 'advanced' AND actor_id IS NOT NULL
            ORDER BY sequence ASC LIMIT 1) AS approver_id,
         (SELECT fullname FROM folio.users WHERE id =
            (SELECT actor_id FROM folio.waybill_events
               WHERE waybill_id = hl.waybill_id AND kind = 'advanced' AND actor_id IS NOT NULL
               ORDER BY sequence ASC LIMIT 1)) AS approver_name,
         w.created_at::text
    FROM folio.hr_leave hl
    JOIN folio.waybills w ON w.id = hl.waybill_id
    JOIN folio.users    u ON u.id = hl.employee_id
`;

function num(v: string | number | null | undefined): number {
  if (v == null) return 0;
  return typeof v === 'string' ? parseFloat(v) : v;
}

function deriveStatus(wbStatus: string, currentStage: string): LeaveStatus {
  if (wbStatus === 'completed' || currentStage === 'hr_disbursed') return 'approved';
  if (wbStatus === 'rejected' || currentStage === 'rejected') return 'rejected';
  return 'pending';
}

function rowToLeave(r: LeaveDBRow): LeaveRequestRow {
  return {
    id: r.waybill_id,
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
    reject_reason: r.reject_reason,
    status: deriveStatus(r.wb_status, r.current_stage),
    approved_by: r.approver_id,
    approved_by_name: r.approver_name,
    current_stage: r.current_stage,
    created_at: r.created_at,
  };
}

export interface ListLeaveFilter {
  status?: LeaveStatus | string;
  employeeId?: number | string;
}

function statusToWb(status: string): { wb?: string; stage?: string } {
  if (status === 'pending') return { wb: 'open' };
  if (status === 'approved') return { wb: 'completed' };
  if (status === 'rejected') return { wb: 'rejected' };
  return {};
}

export async function listLeaveRequests(filter: ListLeaveFilter = {}): Promise<LeaveRequestRow[]> {
  const where: string[] = [`w.origin = 'hr_leave'`];
  const params: unknown[] = [];
  if (filter.status) {
    const m = statusToWb(String(filter.status));
    if (m.wb) {
      params.push(m.wb);
      where.push(`w.status = $${params.length}`);
    }
  }
  if (filter.employeeId !== undefined && filter.employeeId !== null) {
    params.push(Number(filter.employeeId));
    where.push(`hl.employee_id = $${params.length}`);
  }
  const r = await query<LeaveDBRow>(
    `${LEAVE_SELECT} WHERE ${where.join(' AND ')} ORDER BY w.created_at DESC`,
    params,
  );
  return r.rows.map(rowToLeave);
}

export async function listLeaveStats(): Promise<LeaveStats> {
  const r = await query<{
    total: string;
    pending: string;
    approved: string;
    rejected: string;
  }>(
    `SELECT COUNT(*) AS total,
            COUNT(*) FILTER (WHERE status = 'open')     AS pending,
            COUNT(*) FILTER (WHERE status = 'completed') AS approved,
            COUNT(*) FILTER (WHERE status = 'rejected')  AS rejected
       FROM folio.waybills WHERE origin = 'hr_leave'`,
  );
  const row = r.rows[0] ?? { total: '0', pending: '0', approved: '0', rejected: '0' };
  return {
    total: parseInt(row.total, 10) || 0,
    pending: parseInt(row.pending, 10) || 0,
    approved: parseInt(row.approved, 10) || 0,
    rejected: parseInt(row.rejected, 10) || 0,
  };
}

export async function listDeptStats(): Promise<DeptStat[]> {
  const r = await query<{ department: string | null; total_days: string }>(
    `SELECT u.dept_label AS department,
            SUM(hl.days)::float AS total_days
       FROM folio.hr_leave hl
       JOIN folio.waybills w ON w.id = hl.waybill_id
       JOIN folio.users    u ON u.id = hl.employee_id
      WHERE w.status = 'completed'
      GROUP BY u.dept_label
      ORDER BY total_days DESC`,
  );
  return r.rows.map((row) => ({
    department: row.department ?? '—',
    total_days: typeof row.total_days === 'string' ? parseFloat(row.total_days) : row.total_days,
  }));
}

export interface SubmitLeaveResult {
  id: string;
  waybillId: string;
  originId: number;
}

export async function submitLeave(input: SubmitLeaveInput): Promise<SubmitLeaveResult> {
  const waybillId = await generateWaybillId(new Date().getFullYear());
  return withTransaction(async (q) => {
    const nextRes = await q<{ next_id: number }>(
      `SELECT COALESCE(MAX(origin_id), 0) + 1 AS next_id
         FROM folio.waybills WHERE origin = 'hr_leave'`,
    );
    const originId = nextRes.rows[0]?.next_id ?? 1;
    await q(
      `INSERT INTO folio.waybills
         (id, origin, origin_id, fiscal_year, waybill_kind,
          submitter_id, current_stage, status, created_at, updated_at)
       VALUES
         ($1, 'hr_leave', $2, EXTRACT(YEAR FROM now())::smallint,
          'hr_leave', $3, 'hr_review', 'open', now(), now())`,
      [waybillId, originId, input.employeeId],
    );
    await q(
      `INSERT INTO folio.hr_leave
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
    return { id: waybillId, waybillId, originId };
  });
}

export interface DecideResult {
  id: string;
  status: LeaveStatus;
  employee_id: number;
  leave_type: LeaveType;
  days: number;
  line_user_id: string | null;
  employee_name: string;
  start_date: string;
  end_date: string;
  hr_name: string;
}

async function decide(
  waybillId: string,
  hrActorId: number,
  action: 'approve' | 'reject',
  rejectReason: string | null,
): Promise<DecideResult | null> {
  return withTransaction(async (q) => {
    const wb = await q<{ current_stage: string; status: string }>(
      `SELECT current_stage, status FROM folio.waybills WHERE id = $1`,
      [waybillId],
    );
    if (wb.rows.length === 0) throw new Error('Leave request not found');
    if (wb.rows[0].status !== 'open') {
      throw new Error(`Cannot process: leave request is already "${wb.rows[0].status}"`);
    }
    const stage = wb.rows[0].current_stage;
    let nextStage: string;
    let nextStatus: string;
    if (action === 'reject') {
      nextStage = 'rejected';
      nextStatus = 'rejected';
    } else if (stage === 'hr_review') {
      nextStage = 'hr_authorization';
      nextStatus = 'open';
    } else if (stage === 'hr_authorization') {
      nextStage = 'hr_disbursed';
      nextStatus = 'completed';
    } else {
      throw new Error(`no next stage from ${stage}`);
    }

    await q(
      `UPDATE folio.waybills
          SET current_stage = $1,
              status        = $2,
              updated_at    = now()
        WHERE id = $3`,
      [nextStage, nextStatus, waybillId],
    );

    if (action === 'reject' && rejectReason) {
      await q(
        `UPDATE folio.hr_leave
            SET reason = COALESCE(reason, '') || ' [rejected: ' || $2 || ']'
          WHERE waybill_id = $1`,
        [waybillId, rejectReason],
      );
    }

    await recordEvent({
      waybillId,
      kind: action === 'approve' ? 'advanced' : 'rejected',
      stageFrom: stage,
      stageTo: nextStage,
      actorId: hrActorId,
      actorRole: null,
      payload: action === 'approve' ? { decision: 'approve' } : { reason: rejectReason },
      client: q as never,
    });

    if (nextStatus === 'completed') {
      await q(
        `UPDATE folio.users u
            SET used_sick     = used_sick     + CASE WHEN hl.leave_type = 'sick'     THEN hl.days ELSE 0 END,
                used_annual   = used_annual   + CASE WHEN hl.leave_type = 'annual'   THEN hl.days ELSE 0 END,
                used_personal = used_personal + CASE WHEN hl.leave_type = 'personal' THEN hl.days ELSE 0 END
           FROM folio.hr_leave hl
          WHERE hl.waybill_id = $1
            AND hl.employee_id = u.id`,
        [waybillId],
      );
    }

    const info = await q<{
      leave_type: LeaveType;
      days: string | number;
      start_date: string;
      end_date: string;
      employee_name: string;
      hr_name: string;
      line_user_id: string | null;
    }>(
      `SELECT hl.leave_type, hl.days::float AS days,
              hl.start_date::text, hl.end_date::text,
              u.fullname AS employee_name,
              u.line_user_id,
              (SELECT fullname FROM folio.users WHERE id = $2) AS hr_name
         FROM folio.hr_leave hl
         JOIN folio.users u ON u.id = hl.employee_id
        WHERE hl.waybill_id = $1`,
      [waybillId, hrActorId],
    );
    const row = info.rows[0];
    if (!row) throw new Error('Leave row vanished');
    const daysNum = typeof row.days === 'string' ? parseFloat(row.days) : row.days;
    return {
      id: waybillId,
      status: action === 'approve' ? 'approved' : 'rejected',
      employee_id: 0,
      leave_type: row.leave_type,
      days: daysNum,
      line_user_id: row.line_user_id,
      employee_name: row.employee_name,
      start_date: row.start_date,
      end_date: row.end_date,
      hr_name: row.hr_name,
    };
  });
}

export async function approveLeave(
  waybillId: string,
  hrActorId: number | string,
): Promise<DecideResult | null> {
  const numericActorId = typeof hrActorId === 'string' ? parseInt(hrActorId, 10) : hrActorId;
  return decide(waybillId, numericActorId, 'approve', null);
}

export async function rejectLeave(
  waybillId: string,
  hrActorId: number | string,
  reason: string,
): Promise<DecideResult | null> {
  if (!reason || !reason.trim()) throw new Error('Rejection reason is required');
  const numericActorId = typeof hrActorId === 'string' ? parseInt(hrActorId, 10) : hrActorId;
  return decide(waybillId, numericActorId, 'reject', reason.trim());
}

export async function findLeaveRequestById(id: string): Promise<LeaveRequestRow | null> {
  const r = await query<LeaveDBRow>(`${LEAVE_SELECT} WHERE hl.waybill_id = $1`, [id]);
  if (r.rows.length === 0) return null;
  return rowToLeave(r.rows[0]);
}

export interface LeaveHistoryRow {
  waybill_id: string;
  leave_type: LeaveType;
  status: string;
  days: number | string;
  start_date: string;
  end_date: string;
  reason: string | null;
}

export async function listLeave(filter: {
  employeeId: number;
  limit: number;
}): Promise<LeaveHistoryRow[]> {
  const r = await query<{
    waybill_id: string;
    leave_type: LeaveType;
    wb_status: string;
    current_stage: string;
    days: string | number;
    start_date: string;
    end_date: string;
    reason: string | null;
  }>(
    `SELECT hl.waybill_id, hl.leave_type, w.status AS wb_status, w.current_stage,
            hl.days::float AS days,
            hl.start_date::text, hl.end_date::text, hl.reason
       FROM folio.hr_leave hl
       JOIN folio.waybills w ON w.id = hl.waybill_id
      WHERE hl.employee_id = $1
      ORDER BY w.created_at DESC
      LIMIT $2`,
    [filter.employeeId, filter.limit],
  );
  return r.rows.map((row) => ({
    waybill_id: row.waybill_id,
    leave_type: row.leave_type,
    status: row.wb_status,
    days: row.days,
    start_date: row.start_date,
    end_date: row.end_date,
    reason: row.reason,
  }));
}