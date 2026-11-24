import {identitySnapshotSources as organizationSnapshotSources} from '@erpx/server-organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import "server-only";
import {idNameEmailByUserIdsSql} from './iam-probe-sql';
import {distinctAssigneeSql} from './read-tail-family-r29';
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";

/** Identity owns assignment validity and access scope, including its historical HR view. */
export async function companyDirectoryMemberIds(db: SqlDatabase, input: { companyId: string; branchId: string | null; includeInactive: boolean }, now: number): Promise<string[]> {
  const members = (await db.prepare(distinctAssigneeSql(`user_id IS NOT NULL AND ((revoked_at IS NULL AND valid_from<=? AND valid_until>?) OR ?=1) AND (?::text IS NULL OR scope_kind='holding' OR COALESCE(branch_id,(SELECT branch_id ${organizationSources.member_directory_organization_nodes_from} WHERE id=scope_node_id))=?) ORDER BY user_id`))
    .bind(input.companyId, now, now, input.includeInactive ? 1 : 0, input.branchId, input.branchId).all<{ user_id: string }>()).results;
  return members.map(member => member.user_id);
}

/** A caller must supply only the account identifiers visible in its owner query. */
export async function directoryAccounts(control: SqlDatabase, ids: string[]): Promise<{ id: string; name: string; email: string }[]> {
  if (!ids.length) return [];
  return (await control.prepare(idNameEmailByUserIdsSql(ids)).bind(...ids).all<{ id: string; name: string; email: string }>()).results;
}
