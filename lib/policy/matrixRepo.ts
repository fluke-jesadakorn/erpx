// lib/policy/matrixRepo.ts — server-side loaders for the new /policy matrix.
import 'server-only';
import { query } from '../db';
import { effectOf } from '../perm/grammar';

export const SEED_PERSONA_IDS: readonly string[] = [
  'it_manager',
  'it_supervisor',
  'it_officer',
  'hr_manager',
  'hr_supervisor',
  'hr_officer',
  'accounting_manager',
  'accounting_supervisor',
  'accounting_officer',
  'finance_manager',
  'finance_supervisor',
  'finance_officer',
  'cfo',
  'ceo',
];

export interface MatrixColumn {
  perm: string;
  domain: string;
  subject: string;
  verb: string;
  description: string | null;
}

export interface MatrixTarget {
  id: string;
  kind: 'department' | 'role';
  label: string;
  department_id: string | null;
  significance: boolean;
  member_count: number;
  is_seed_persona: boolean;
  is_system: boolean;
  role_kind: 'hierarchy' | 'system' | null;
  rank: number | null;
}

export interface MatrixGrant {
  target_id: string;
  permission_id: string;
  significance: boolean;
}

export async function loadMatrixColumns(): Promise<MatrixColumn[]> {
  const { rows } = await query<MatrixColumn>(
    `SELECT id AS perm, split_part(id, ':', 1) AS domain,
            split_part(id, ':', 2) AS subject,
            split_part(id, ':', 3) AS verb,
            description
       FROM perm.permissions
       ORDER BY domain, subject, verb, id`,
  );
  return rows;
}

export async function loadMatrixTargets(): Promise<MatrixTarget[]> {
  const { rows } = await query<MatrixTarget>(
    `WITH members AS (
      SELECT department_id, COUNT(*)::int AS count
        FROM perm.user_departments
       GROUP BY department_id
    )
    SELECT d.id::text AS id,
           'department'::text AS kind,
           d.display_name::text AS label,
           NULL::text AS department_id,
           true AS significance,
           COALESCE(m.count, 0) AS member_count,
           false AS is_seed_persona,
           d.is_system,
           NULL::text AS role_kind,
           NULL::smallint AS rank
      FROM perm.departments d
      LEFT JOIN members m ON m.department_id = d.id
    UNION ALL
    SELECT r.id::text AS id,
           'role'::text AS kind,
           COALESCE(r.display_name, r.id)::text AS label,
           r.department_id,
           false AS significance,
           COALESCE(uc.count, 0)::int AS member_count,
           (r.id = ANY($1::text[])) AS is_seed_persona,
           r.is_system,
           r.kind AS role_kind,
           r.rank
      FROM perm.roles r
      LEFT JOIN (
        SELECT role_id, COUNT(*)::int AS count
          FROM perm.user_roles GROUP BY role_id
      ) uc ON uc.role_id = r.id
     WHERE r.kind IN ('hierarchy', 'system')
    ORDER BY kind, id`,
    [SEED_PERSONA_IDS as unknown as string[]],
  );
  return rows;
}

export async function loadMatrixGrants(): Promise<MatrixGrant[]> {
  const rp = await query<MatrixGrant>(
    `SELECT role_id::text AS target_id, permission_id, significance
       FROM perm.role_permissions`,
  );
  const dp = await query<MatrixGrant>(
    `SELECT department_id::text AS target_id, permission_id, significance
       FROM perm.department_permissions`,
  );
  const out: MatrixGrant[] = [];
  for (const r of rp.rows) out.push(r);
  for (const d of dp.rows) out.push(d);
  return out;
}

export async function loadMatrixCells(): Promise<Map<string, Set<string>>> {
  const grants = await loadMatrixGrants();
  const map = new Map<string, Set<string>>();
  for (const g of grants) {
    if (effectOf(g.permission_id) === 'deny') continue;
    if (!map.has(g.target_id)) map.set(g.target_id, new Set());
    map.get(g.target_id)!.add(g.permission_id);
  }
  return map;
}
