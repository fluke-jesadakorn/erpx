import {identitySnapshotSources as organizationSnapshotSources} from '../../organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";

/** Identity owns assignment validity and access scope, including its historical HR view. */
export async function companyDirectoryMemberIds(db: SqlDatabase, input: { companyId: string; branchId: string | null; includeInactive: boolean }, now: number): Promise<string[]> {
  const members = (await db.prepare(`SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND user_id IS NOT NULL AND ((revoked_at IS NULL AND valid_from<=? AND valid_until>?) OR ?=1) AND (?::text IS NULL OR scope_kind='holding' OR COALESCE(branch_id,(SELECT branch_id ${organizationSources.member_directory_organization_nodes_from} WHERE id=scope_node_id))=?) ORDER BY user_id`)
    .bind(input.companyId, now, now, input.includeInactive ? 1 : 0, input.branchId, input.branchId).all<{ user_id: string }>()).results;
  return members.map(member => member.user_id);
}

/** A caller must supply only the account identifiers visible in its owner query. */
export async function directoryAccounts(control: SqlDatabase, ids: string[]): Promise<{ id: string; name: string; email: string }[]> {
  if (!ids.length) return [];
  return (await control.prepare(`SELECT id,name,email FROM "user" WHERE id IN (${ids.map(() => "?").join(",")})`).bind(...ids).all<{ id: string; name: string; email: string }>()).results;
}
