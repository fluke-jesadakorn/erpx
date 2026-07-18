// GET /api/policy/matrix — payload for the /policy RBAC matrix UI.
// Returns the stage-chain permissions (stage:*:act::allow) plus the personas
// in `perm.roles`, with the current grant set per role.
//
// Behind perm: rbac:matrix:view.

import { NextResponse } from 'next/server';
import { query } from '@/db';
import { loadActivePermSession, hasPermission, PERM } from '@/perm/server';

export const runtime = 'nodejs';
export const dynamic = 'force-dynamic';

const STAGE_PERMS: string[] = [
  PERM.stage.department_approval.act,
  PERM.stage.accounting_review.act,
  PERM.stage.accounting_approval.act,
  PERM.stage.executive_approval.act,
  PERM.stage.payment.act,
  PERM.stage.settlement.act,
];

export async function GET(_req: Request) {
  const out = await loadActivePermSession(_req);
  if (!out) return NextResponse.json({ error: 'unauthorized' }, { status: 401 });
  if (!hasPermission(out.session, PERM.rbac.matrix.view))
    return NextResponse.json({ error: 'forbidden' }, { status: 403 });

  const personasRes = await query<{
    id: string;
    display_name: string;
    sort_order: number;
    rank: number | null;
  }>(
    `SELECT id, display_name, sort_order, rank
       FROM perm.roles
      ORDER BY rank NULLS LAST, sort_order, display_name`,
  );

  const grantsRes = await query<{ role_id: string; permission_id: string }>(
    `SELECT role_id, permission_id
       FROM perm.role_permissions
      WHERE permission_id = ANY($1::text[])`,
    [STAGE_PERMS],
  );

  const usersRes = await query<{ role_id: string; count: string }>(
    `SELECT ur.role_id, COUNT(*)::text AS count
       FROM perm.user_roles ur
      GROUP BY ur.role_id`,
  );

  const grantsByRole: Record<string, Set<string>> = {};
  for (const g of grantsRes.rows) {
    if (!grantsByRole[g.role_id]) grantsByRole[g.role_id] = new Set();
    grantsByRole[g.role_id].add(g.permission_id);
  }
  const userCountByRole: Record<string, number> = {};
  for (const r of usersRes.rows) userCountByRole[r.role_id] = Number(r.count);

  return NextResponse.json({
    stages: STAGE_PERMS.map((id) => {
      const [, stage] = id.match(/^stage:([^:]+):/) ?? [];
      return { perm: id, stage: stage ?? id };
    }),
    personas: personasRes.rows.map((p) => ({
      ...p,
      user_count: userCountByRole[p.id] ?? 0,
      grants: Array.from(grantsByRole[p.id] ?? []),
    })),
  });
}
