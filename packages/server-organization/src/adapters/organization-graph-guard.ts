import 'server-only';
import type {SqlDatabase}from '@erpx/server-runtime/public/runtime/types';
import {assertion}from '@erpx/server-workflow/public/command-assertions';
import {organizationGraphSnapshotSources}from '@erpx/server-people_payroll/public/organization-graph-snapshot-sources';
import {companySnapshotAggregate} from './organization-column-alias-families-r28';
const graphSql = `SELECT jsonb_build_object(
 'people',(SELECT jsonb_agg(p.snapshot ORDER BY p.id) FROM (${organizationGraphSnapshotSources.people}) p WHERE p.company_id=?),
 'engagements',(SELECT jsonb_agg(e.snapshot ORDER BY e.id) FROM (${organizationGraphSnapshotSources.engagements}) e WHERE e.company_id=?),
 'positions',${companySnapshotAggregate('organization_positions','p')},
 'levels',${companySnapshotAggregate('staff_levels','l')},
 'nodes',${companySnapshotAggregate('organization_nodes','n')}
)::text snapshot`;
export async function organizationGuard(db: SqlDatabase, companyId: string, key: string) {
  const args = Array(5).fill(companyId) as string[];
  const row = await db.prepare(graphSql).bind(...args).first<{snapshot:string}>();
  return assertion(db,key,"organization-snapshot",`((${graphSql})::jsonb = ?::jsonb)`,[...args,row!.snapshot]);
}
