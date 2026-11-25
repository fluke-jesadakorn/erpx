import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import type { WorkContext } from "@erpx/server-identity_access/public/authorization";
import { nativeLane24BranchOptions } from './native-lane-24/client';

/**
 * PMS branch option facts. The axum lane-24 owner owns the SQL, the Company
 * bind, the projection and the response shape; the caller WorkContext selects
 * the registered Holding. No local SQL and no fallback remain here.
 */
export function pmsOrganizationFacts(_db: SqlDatabase) {
    return {
        readOrganizationNodes: (facts: {
            companyId: string;
            fact1: readonly string[];
            actor: WorkContext;
        }) => nativeLane24BranchOptions(facts.actor, facts.companyId, facts.actor.branchId),
        // Advisory lock reference read stays owner-local: it participates in a
        // caller transaction and is outside this native read family.
        readOrganizationNodes2: (facts: {
            branchId: string;
            companyId: string;
        }) => _db.prepare("SELECT 1 FROM organization_nodes WHERE id=? AND company_id=? AND kind='branch' AND status='active' FOR SHARE").bind(facts.branchId, facts.companyId).first()
    };
}
