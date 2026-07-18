import 'server-only';
import { query } from '../db';

export interface EmployeeRow {
  id: string;
  employee_code: string;
  line_user_id: string | null;
  name: string;
  department: string | null;
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
  id: number;
  employee_code: string;
  line_user_id: string | null;
  name: string;
  department: string | null;
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
    id: String(r.id),
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

const USER_SELECT = `
  SELECT u.id, u.employee_code, u.line_user_id,
         u.fullname        AS name,
         COALESCE(d.display_name, u.dept_label) AS department,
         u.position,
         COALESCE(u.position, '') AS job_description,
         COALESCE((
           SELECT ur.role_id FROM perm.user_roles ur
            WHERE ur.user_id = u.id AND ur.role_kind = 'hierarchy'
            LIMIT 1
         ), 'staff') AS role,
         u.quota_sick       AS total_sick_leave,
         u.used_sick        AS used_sick_leave,
         u.quota_annual     AS total_annual_leave,
         u.used_annual      AS used_annual_leave,
         u.quota_personal   AS total_personal_leave,
         u.used_personal    AS used_personal_leave,
         u.created_at::text
    FROM folio.users u
    LEFT JOIN perm.user_departments ud ON ud.user_id = u.id
    LEFT JOIN perm.departments d ON d.id = ud.department_id
`;

export async function listEmployees(): Promise<EmployeeRow[]> {
  const r = await query<EmployeeDBRow>(
    `${USER_SELECT} ORDER BY u.fullname ASC`,
  );
  return r.rows.map(rowToEmployee);
}

export async function listHRUsers(): Promise<HRUserOption[]> {
  const r = await query<EmployeeDBRow>(
    `${USER_SELECT}
      WHERE EXISTS (
        SELECT 1 FROM perm.user_departments hud
         WHERE hud.user_id = u.id AND hud.department_id = 'hr'
      )
      ORDER BY u.fullname ASC`,
  );
  return r.rows.map((row) => ({
    id: String(row.id),
    employee_code: row.employee_code,
    name: row.name,
    position: row.position,
  }));
}

export async function getEmployee(id: string): Promise<EmployeeRow | null> {
  const numericId = parseInt(id, 10);
  if (!Number.isFinite(numericId)) return null;
  const r = await query<EmployeeDBRow>(
    `${USER_SELECT} WHERE u.id = $1`,
    [numericId],
  );
  if (r.rows.length === 0) return null;
  return rowToEmployee(r.rows[0]);
}

export async function updateQuota(
  employeeId: string,
  patch: QuotaPatch,
  _reason: string,
  _hrActorId: string,
): Promise<QuotaChange[]> {
  const numericId = parseInt(employeeId, 10);
  if (!Number.isFinite(numericId)) throw new Error('Invalid employee id');
  const existing = await getEmployee(employeeId);
  if (!existing) throw new Error('Employee not found');

  const newSick = patch.totalSickLeave ?? existing.total_sick_leave;
  const newAnnual = patch.totalAnnualLeave ?? existing.total_annual_leave;
  const newPersonal = patch.totalPersonalLeave ?? existing.total_personal_leave;
  const clampedUsedSick = Math.min(existing.used_sick_leave, newSick);
  const clampedUsedAnnual = Math.min(existing.used_annual_leave, newAnnual);
  const clampedUsedPersonal = Math.min(existing.used_personal_leave, newPersonal);

  await query(
    `UPDATE folio.users
        SET quota_sick     = $1,
            used_sick      = $2,
            quota_annual   = $3,
            used_annual    = $4,
            quota_personal = $5,
            used_personal  = $6
      WHERE id = $7`,
    [newSick, clampedUsedSick, newAnnual, clampedUsedAnnual, newPersonal, clampedUsedPersonal, numericId],
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
