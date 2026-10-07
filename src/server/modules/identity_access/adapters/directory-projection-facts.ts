import {identitySnapshotSources as organizationSnapshotSources} from '../../organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import "server-only";
import type { WorkContext } from "@/server/modules/identity_access/public/authorization";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
export function peopleDirectoryAssignmentFacts(database: SqlDatabase) {
    return { readDirectoryFacts1(assignmentId: string, userId: string, role: string, companyId: string, assignmentScope: WorkContext["assignmentScope"], now: number, now2: number) {
            return database.prepare(`
    SELECT 1 FROM assignments a
    ${organizationSources.directory_projection_facts_organization_nodes_c_join} ON c.id=a.company_id AND c.kind='company' AND c.status='active'
    WHERE a.id=? AND a.user_id=? AND a.role=? AND a.company_id=?
      AND a.scope_kind=? AND a.revoked_at IS NULL
      AND iam_assignment_active(a.id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND iam_principal_active(a.user_id,a.company_id,floor(extract(epoch FROM clock_timestamp())*1000)::bigint)
      AND a.valid_from<=? AND a.valid_until>?
  `).bind(assignmentId, userId, role, companyId, assignmentScope, now, now2).first();
        } };
}
