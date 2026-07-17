// lib/policy/matrixRepo.ts — server-side loaders for the new /policy matrix.
import 'server-only';
import { query } from '../db';
import { effectOf } from '../perm/grammar';

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
  significance: boolean;
  member_count: number;
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
    `WITH dept AS (
      SELECT DISTINCT split_part(id, ':', 3) AS dept_id, true AS significance
        FROM perm.permissions
       WHERE id LIKE 'user:dept:%::allow'
    ),
    members AS (
      SELECT split_part(permission_id, ':', 3) AS dept_id,
             COUNT(*)::int AS count
        FROM perm.user_permissions
       WHERE permission_id LIKE 'user:dept:%::allow'
         AND revoked_at IS NULL
         AND (ends_at IS NULL OR ends_at > now())
       GROUP BY 1
    )
    SELECT d.dept_id::text AS id,
           'department'::text AS kind,
           initcap(d.dept_id)::text AS label,
           d.significance,
           COALESCE(m.count, 0) AS member_count
      FROM dept d
      LEFT JOIN members m ON m.dept_id = d.dept_id
    UNION ALL
    SELECT r.id::text AS id,
           'role'::text AS kind,
           COALESCE(r.display_name, r.id)::text AS label,
           false AS significance,
           COALESCE(uc.count, 0)::int AS member_count
      FROM perm.roles r
      LEFT JOIN (
        SELECT role_id, COUNT(*)::int AS count
          FROM perm.user_roles GROUP BY role_id
      ) uc ON uc.role_id = r.id
     WHERE r.is_system = false OR r.id IN ('ceo::1','cfo::2','manager::3','supervisor::4','officer::5','hr_manager::3','hr::5','accounting_manager::3','account_officer::5','account_supervisor::4','finance::2','admin::2','it::2','sales_rep::3','sales_supervisor::2')
    ORDER BY kind, id`,
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
