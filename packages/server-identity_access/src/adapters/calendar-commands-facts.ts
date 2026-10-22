import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function calendarCommandsIdentityAccessFacts(db: SqlDatabase) {
    return { read6: (facts: {
            value1: string;
            value2: string;
            value3: number;
        }) => db.prepare(`SELECT 1 FROM assignments WHERE company_id=? AND user_id=? AND scope_kind='holding' AND iam_role_allows(role,company_id,'iam.directory.manage') AND iam_assignment_active(id,?) LIMIT 1`).bind(facts.value1, facts.value2, facts.value3).first<Record<string, unknown>>(),
        read10: (facts: {
            value1: string | null;
            value2: string;
            value3: string;
        }) => db.prepare("SELECT 1 WHERE iam_same_person(?,?,?)").bind(facts.value1, facts.value2, facts.value3).first<Record<string, unknown>>(),
        read28: (facts: {
            value1: string;
            value2: string[];
            value3: number;
        }) => db.prepare(`SELECT DISTINCT user_id FROM assignments WHERE company_id=? AND user_id IN (${facts.value2.map(() => "?").join(",")})
        AND revoked_at IS NULL AND iam_assignment_active(id,?) AND iam_principal_active(user_id,company_id,?)`).bind(facts.value1, ...facts.value2, facts.value3, facts.value3).all<{
            user_id: string;
        }>() };
}
