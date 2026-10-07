import {identitySnapshotSources as organizationSnapshotSources} from '../../organization/public/identity-snapshot-sources';
const organizationSources=organizationSnapshotSources();
import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
export function attachmentServiceIdentityAccessFacts(db: SqlDatabase) {
    return { readActiveWorkContexts2: (facts: {
            sessionId: string;
            companyId: string;
        }) => {
            return db.prepare(`SELECT assignment_id,assignment_version,revision,expires_at,user_id
      FROM active_work_contexts WHERE session_id=? AND company_id=? FOR UPDATE`).bind(facts.sessionId, facts.companyId).first<{
                assignment_id: string | null;
                assignment_version: number | null;
                revision: number;
                expires_at: number;
                user_id: string;
            }>();
        },
        readAssignments3: (facts: {
            assignmentId: string;
            assignmentVersion: number;
            userId: string;
            role: string;
            companyId: string;
            branchId: string | null;
            departmentId: string | null;
        }) => {
            return db.prepare(`SELECT id FROM assignments a WHERE a.id=? AND a.version=? AND a.user_id=? AND a.role=? AND a.company_id=?
      AND a.revoked_at IS NULL AND a.valid_from<=floor(extract(epoch FROM clock_timestamp())*1000)::bigint AND a.valid_until>floor(extract(epoch FROM clock_timestamp())*1000)::bigint
      AND (a.scope_kind='holding' OR (a.scope_kind='branch' AND a.scope_node_id=?) OR (a.scope_kind='department' AND a.scope_node_id=?))
      AND EXISTS(SELECT 1 ${organizationSources.attachments_service_facts_organization_nodes_company_from} WHERE company.id=a.company_id AND company.status='active')
      AND (a.scope_kind='holding' OR EXISTS(SELECT 1 ${organizationSources.attachments_service_facts_organization_nodes_node_from} WHERE node.id=a.scope_node_id AND node.company_id=a.company_id AND node.status='active'
        AND (node.branch_id IS NULL OR EXISTS(SELECT 1 ${organizationSources.attachments_service_facts_organization_nodes_branch_from} WHERE branch.id=node.branch_id AND branch.company_id=a.company_id AND branch.status='active'))))
      FOR SHARE`).bind(facts.assignmentId, facts.assignmentVersion, facts.userId, facts.role, facts.companyId, facts.branchId, facts.departmentId).first<{
                id: string;
            }>();
        } };
}
