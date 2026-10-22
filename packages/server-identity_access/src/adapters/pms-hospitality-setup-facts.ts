import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { all } from "@erpx/server-pos/public/pos-core";
export function pmsHospitalitySetupIdentityAccessFacts(db: SqlDatabase) {
    return {
        roles: (facts: {
            company: string;
            now1: number;
            now2: number;
            branch: string;
        }) => all(db, "SELECT DISTINCT role,user_id FROM assignments WHERE company_id=? AND revoked_at IS NULL AND valid_from<=? AND valid_until>? AND (scope_kind='holding' OR (scope_kind='branch' AND scope_node_id=?))", facts.company, facts.now1, facts.now2, facts.branch)
    };
}
