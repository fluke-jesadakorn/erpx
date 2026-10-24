import 'server-only';
import type {SqlDatabase}from '@erpx/server-runtime/public/runtime/types';
import {assertion}from '@erpx/server-workflow/public/command-assertions';
import {organizationGraphSnapshotSources}from '@erpx/server-people_payroll/public/organization-graph-snapshot-sources';
const graphSql = `SELECT jsonb_build_object(
 'people',(SELECT jsonb_agg(p.snapshot ORDER BY p.id) FROM (${organizationGraphSnapshotSources.people}) p WHERE p.company_id=?),
 'engagements',(SELECT jsonb_agg(e.snapshot ORDER BY e.id) FROM (${organizationGraphSnapshotSources.engagements}) e WHERE e.company_id=?),
 'positions',(SELECT jsonb_agg(to_jsonb(p) ORDER BY p.id) FROM organization_positions p WHERE p.company_id=?),
 'levels',(SELECT jsonb_agg(to_jsonb(l) ORDER BY l.id) FROM staff_levels l WHERE l.company_id=?),
 'nodes',(SELECT jsonb_agg(to_jsonb(n) ORDER BY n.id) FROM organization_nodes n WHERE n.company_id=?)
)::text snapshot`;
export async function organizationGuard(db: SqlDatabase, companyId: string, key: string) {
  const args = Array(5).fill(companyId) as string[];
  const row = await db.prepare(graphSql).bind(...args).first<{snapshot:string}>();
  return assertion(db,key,"organization-snapshot",`((${graphSql})::jsonb = ?::jsonb)`,[...args,row!.snapshot]);
}
