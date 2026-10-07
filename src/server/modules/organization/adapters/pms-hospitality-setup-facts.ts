import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { all, one } from "../../pos/public/pos-core";
export function pmsHospitalitySetupOrganizationFacts(db: SqlDatabase) {
    return {
        branches: (facts: {
            company: string;
            branchId: string | null;
            branchId2: string | null;
        }) => all(db, "SELECT id,display_name name FROM organization_nodes WHERE company_id=? AND kind='branch' AND status='active' AND (?::text IS NULL OR id=?) ORDER BY display_name", facts.company, facts.branchId, facts.branchId2),
        companyName: (facts: {
            company: string;
        }) => one(db, 'SELECT display_name FROM organization_nodes WHERE id=?', facts.company)
    };
}
