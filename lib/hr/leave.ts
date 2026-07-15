import 'server-only';
import { query, withTransaction } from '../db';

export type LeaveType = 'sick' | 'annual' | 'personal';
export type LeaveStatus = 'pending' | 'approved' | 'rejected';

export interface LeaveRequestRow {
  id: string;
  employee_id: string;
  employee_name: string;
  employee_code: string;
  department: string;
  position: string;
  leave_type: LeaveType;
  start_date: string;
  end_date: string;
  days: number;
  reason: string | null;
  reject_reason: string | null;
  status: LeaveStatus;
  approved_by: string | null;
  approved_by_name: string | null;
  created_at: string;
}

export interface SubmitLeaveInput {
  employeeId: string;
  leaveType: LeaveType;
  startDate: string;
  endDate: string;
  days: number;
  reason: string | null;
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
  id: string;
  employee_id: string;
  employee_name: string;
  employee_code: string;
  department: string;
  position: string;
  leave_type: LeaveType;
  start_date: string;
  end_date: string;
  days: string | number;
  reason: string | null;
  reject_reason: string | null;
  status: LeaveStatus;
  approved_by: string | null;
  approved_by_name: string | null;
  created_at: string;
}

function numberFromDays(v: string | number | null): number {
  if (v === null || v === undefined) return 0;
  if (typeof v === 'number') return v;
  const n = parseFloat(v);
  return Number.isFinite(n) ? n : 0;
}

function rowToLeave(r: LeaveDBRow): LeaveRequestRow {
  return {
    id: r.id,
    employee_id: r.employee_id,
    employee_name: r.employee_name,
    employee_code: r.employee_code,
    department: r.department,
    position: r.position,
    leave_type: r.leave_type,
    start_date: r.start_date,
    end_date: r.end_date,
    days: numberFromDays(r.days),
    reason: r.reason,
    reject_reason: r.reject_reason,
    status: r.status,
    approved_by: r.approved_by,
    approved_by_name: r.approved_by_name,
    created_at: r.created_at,
  };
}

export interface ListLeaveFilter {
  status?: LeaveStatus;
  employeeId?: string;
}

export async function listLeaveRequests(
  filter: ListLeaveFilter = {},
): Promise<LeaveRequestRow[]> {
  const where: string[] = [];
  const params: unknown[] = [];
  if (filter.status) {
    params.push(filter.status);
    where.push(`lr.status = $${params.length}`);
  }
  if (filter.employeeId) {
    params.push(filter.employeeId);
    where.push(`lr.employee_id = $${params.length}`);
  }
  const r = await query<LeaveDBRow>(
    `SELECT lr.id, lr.employee_id,
            e.name as employee_name,
            e.employee_code,
            e.department, e.position,
            lr.leave_type,
            lr.start_date::text, lr.end_date::text,
            lr.days::float as days,
            lr.reason, lr.reject_reason,
            lr.status,
            lr.approved_by,
            appr.name as approved_by_name,
            lr.created_at
       FROM hr.leave_requests lr
       JOIN hr.employees e ON lr.employee_id = e.id
       LEFT JOIN hr.employees appr ON lr.approved_by = appr.id
       ${where.length ? 'WHERE ' + where.join(' AND ') : ''}
      ORDER BY lr.created_at DESC`,
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
            COUNT(*) FILTER (WHERE status = 'pending') AS pending,
            COUNT(*) FILTER (WHERE status = 'approved') AS approved,
            COUNT(*) FILTER (WHERE status = 'rejected') AS rejected
       FROM hr.leave_requests`,
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
  const r = await query<{ department: string; total_days: string }>(
    `SELECT e.department,
            SUM(lr.days)::float AS total_days
       FROM hr.leave_requests lr
       JOIN hr.employees e ON lr.employee_id = e.id
      WHERE lr.status = 'approved'
      GROUP BY e.department
      ORDER BY total_days DESC`,
  );
  return r.rows.map((row) => ({
    department: row.department,
    total_days: typeof row.total_days === 'string' ? parseFloat(row.total_days) : row.total_days,
  }));
}

function leaveColumnFor(type: LeaveType): string {
  if (type === 'sick') return 'used_sick_leave';
  if (type === 'annual') return 'used_annual_leave';
  return 'used_personal_leave';
}

export interface DecideResult {
  id: string;
  status: LeaveStatus;
  employee_id: string;
  leave_type: LeaveType;
  days: number;
  line_user_id: string | null;
  employee_name: string;
  start_date: string;
  end_date: string;
  hr_name: string;
}

export async function submitLeave(input: SubmitLeaveInput): Promise<{ id: string }> {
  const r = await query<{ id: string }>(
    `INSERT INTO hr.leave_requests
       (employee_id, leave_type, start_date, end_date, days, reason, status)
     VALUES ($1, $2, $3::date, $4::date, $5, $6, 'pending')
     RETURNING id`,
    [
      input.employeeId,
      input.leaveType,
      input.startDate,
      input.endDate,
      input.days,
      input.reason,
    ],
  );
  return { id: r.rows[0].id };
}

async function decideAndIncrement(
  requestId: string,
  hrActorId: string,
  action: 'approve' | 'reject',
  rejectReason: string | null,
): Promise<DecideResult | null> {
  return withTransaction(async (q) => {
    const updateRes = await q<{
      employee_id: string;
      leave_type: LeaveType;
      days: string | number;
    }>(
      `UPDATE hr.leave_requests
          SET status = $1,
              approved_by = $2,
              reject_reason = $3,
              updated_at = NOW()
        WHERE id = $4 AND status = 'pending'
        RETURNING employee_id, leave_type, days::float as days`,
      [action === 'approve' ? 'approved' : 'rejected', hrActorId, rejectReason, requestId],
    );
    if (updateRes.rowCount === 0) {
      const check = await q<{ status: LeaveStatus }>(
        `SELECT status FROM hr.leave_requests WHERE id = $1`,
        [requestId],
      );
      if (check.rows.length === 0) throw new Error('Leave request not found');
      throw new Error(`Cannot process: leave request is already "${check.rows[0].status}"`);
    }
    const updated = updateRes.rows[0];
    const column = leaveColumnFor(updated.leave_type);

    if (action === 'approve') {
      const daysNum =
        typeof updated.days === 'string' ? parseFloat(updated.days) : updated.days;
      await q(
        `UPDATE hr.employees
            SET ${column} = ${column} + $1, updated_at = NOW()
          WHERE id = $2`,
        [daysNum, updated.employee_id],
      );
    }

    const info = await q<{
      line_user_id: string | null;
      employee_name: string;
      hr_name: string;
      start_date: string;
      end_date: string;
    }>(
      `SELECT (SELECT line_user_id FROM hr.employees WHERE id = $1) AS line_user_id,
              (SELECT name FROM hr.employees WHERE id = $1) AS employee_name,
              (SELECT name FROM hr.employees WHERE id = $2) AS hr_name,
              start_date::text, end_date::text
         FROM hr.leave_requests WHERE id = $3`,
      [updated.employee_id, hrActorId, requestId],
    );
    const row = info.rows[0];
    const daysNum = typeof updated.days === 'string' ? parseFloat(updated.days) : updated.days;
    return {
      id: requestId,
      status: action === 'approve' ? 'approved' : 'rejected',
      employee_id: updated.employee_id,
      leave_type: updated.leave_type,
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
  requestId: string,
  hrActorId: string,
): Promise<DecideResult | null> {
  return decideAndIncrement(requestId, hrActorId, 'approve', null);
}

export async function rejectLeave(
  requestId: string,
  hrActorId: string,
  reason: string,
): Promise<DecideResult | null> {
  if (!reason || !reason.trim()) throw new Error('Rejection reason is required');
  return decideAndIncrement(requestId, hrActorId, 'reject', reason.trim());
}

export async function findLeaveRequestById(id: string): Promise<LeaveRequestRow | null> {
  const r = await query<LeaveDBRow>(
    `SELECT lr.id, lr.employee_id,
            e.name as employee_name,
            e.employee_code,
            e.department, e.position,
            lr.leave_type,
            lr.start_date::text, lr.end_date::text,
            lr.days::float as days,
            lr.reason, lr.reject_reason,
            lr.status,
            lr.approved_by,
            appr.name as approved_by_name,
            lr.created_at
       FROM hr.leave_requests lr
       JOIN hr.employees e ON lr.employee_id = e.id
       LEFT JOIN hr.employees appr ON lr.approved_by = appr.id
      WHERE lr.id = $1`,
    [id],
  );
  if (r.rows.length === 0) return null;
  return rowToLeave(r.rows[0]);
}
