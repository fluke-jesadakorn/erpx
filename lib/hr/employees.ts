import 'server-only';
import { query } from '../db';

export interface EmployeeRow {
  id: string;
  employee_code: string;
  line_user_id: string | null;
  name: string;
  department: string;
  position: string;
  role: string;
  job_description: string;
  total_sick_leave: number;
  used_sick_leave: number;
  total_annual_leave: number;
  used_annual_leave: number;
  total_personal_leave: number;
  used_personal_leave: number;
  created_at: string;
}

export interface HRUserOption {
  id: string;
  employee_code: string;
  name: string;
  position: string;
}

export interface QuotaPatch {
  totalSickLeave?: number;
  totalAnnualLeave?: number;
  totalPersonalLeave?: number;
}

export interface QuotaChange {
  label: string;
  from: number;
  to: number;
}

interface EmployeeDBRow {
  id: string;
  employee_code: string;
  line_user_id: string | null;
  name: string;
  department: string;
  position: string;
  role: string;
  job_description: string;
  total_sick_leave: number | null;
  used_sick_leave: number | null;
  total_annual_leave: number | null;
  used_annual_leave: number | null;
  total_personal_leave: number | null;
  used_personal_leave: number | null;
  created_at: string;
}

function num(v: number | string | null | undefined, d: number): number {
  if (v === null || v === undefined) return d;
  const n = typeof v === 'string' ? parseInt(v, 10) : v;
  return Number.isFinite(n) ? n : d;
}

function rowToEmployee(r: EmployeeDBRow): EmployeeRow {
  return {
    id: r.id,
    employee_code: r.employee_code,
    line_user_id: r.line_user_id,
    name: r.name,
    department: r.department,
    position: r.position,
    role: r.role,
    job_description: r.job_description,
    total_sick_leave: num(r.total_sick_leave, 30),
    used_sick_leave: num(r.used_sick_leave, 0),
    total_annual_leave: num(r.total_annual_leave, 10),
    used_annual_leave: num(r.used_annual_leave, 0),
    total_personal_leave: num(r.total_personal_leave, 6),
    used_personal_leave: num(r.used_personal_leave, 0),
    created_at: r.created_at,
  };
}

export async function listEmployees(): Promise<EmployeeRow[]> {
  const r = await query<EmployeeDBRow>(
    `SELECT id, employee_code, line_user_id, name, department, position, role,
            job_description,
            total_sick_leave, used_sick_leave,
            total_annual_leave, used_annual_leave,
            total_personal_leave, used_personal_leave,
            created_at
       FROM hr.employees
      ORDER BY name ASC`,
  );
  return r.rows.map(rowToEmployee);
}

export async function listHRUsers(): Promise<HRUserOption[]> {
  const r = await query<{
    id: string;
    employee_code: string;
    name: string;
    position: string;
  }>(
    `SELECT id, employee_code, name, position
       FROM hr.employees
      WHERE role = 'hr'
      ORDER BY name ASC`,
  );
  return r.rows;
}

export async function getEmployee(id: string): Promise<EmployeeRow | null> {
  const r = await query<EmployeeDBRow>(
    `SELECT id, employee_code, line_user_id, name, department, position, role,
            job_description,
            total_sick_leave, used_sick_leave,
            total_annual_leave, used_annual_leave,
            total_personal_leave, used_personal_leave,
            created_at
       FROM hr.employees WHERE id = $1`,
    [id],
  );
  if (r.rows.length === 0) return null;
  return rowToEmployee(r.rows[0]);
}

export async function updateQuota(
  employeeId: string,
  patch: QuotaPatch,
  _reason: string,
  hrActorId: string,
): Promise<QuotaChange[]> {
  const existing = await getEmployee(employeeId);
  if (!existing) throw new Error('Employee not found');

  const newSick = patch.totalSickLeave ?? existing.total_sick_leave;
  const newAnnual = patch.totalAnnualLeave ?? existing.total_annual_leave;
  const newPersonal = patch.totalPersonalLeave ?? existing.total_personal_leave;
  const clampedUsedSick = Math.min(existing.used_sick_leave, newSick);
  const clampedUsedAnnual = Math.min(existing.used_annual_leave, newAnnual);
  const clampedUsedPersonal = Math.min(existing.used_personal_leave, newPersonal);

  await query(
    `UPDATE hr.employees
        SET total_sick_leave     = $1,
            used_sick_leave      = $2,
            total_annual_leave   = $3,
            used_annual_leave    = $4,
            total_personal_leave = $5,
            used_personal_leave  = $6,
            updated_at           = NOW()
      WHERE id = $7`,
    [newSick, clampedUsedSick, newAnnual, clampedUsedAnnual, newPersonal, clampedUsedPersonal, employeeId],
  );

  const changes: QuotaChange[] = [];
  if (newSick !== existing.total_sick_leave)
    changes.push({ label: 'ลาป่วย', from: existing.total_sick_leave, to: newSick });
  if (newAnnual !== existing.total_annual_leave)
    changes.push({ label: 'ลาพักร้อน', from: existing.total_annual_leave, to: newAnnual });
  if (newPersonal !== existing.total_personal_leave)
    changes.push({ label: 'ลากิจ', from: existing.total_personal_leave, to: newPersonal });

  return changes;
}
