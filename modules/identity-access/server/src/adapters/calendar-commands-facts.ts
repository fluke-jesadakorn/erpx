import "server-only";
import type { SqlDatabase } from "@erpx/platform-runtime-server/public/runtime/types";
import { activeRecipientsSql, samePersonSql } from "./iam-probe-sql";
import { assignmentCompanyUserProbeSql } from "./read-tail-family-r32";
export function calendarCommandsIdentityAccessFacts(db: SqlDatabase) {
    return { read6: (facts: {
            value1: string;
            value2: string;
            value3: number;
        }) => db.prepare(assignmentCompanyUserProbeSql("scope_kind='holding' AND iam_role_allows(role,company_id,'iam.directory.manage') AND iam_assignment_active(id,?) LIMIT 1")).bind(facts.value1, facts.value2, facts.value3).first<Record<string, unknown>>(),
        read10: (facts: {
            value1: string | null;
            value2: string;
            value3: string;
        }) => db.prepare(samePersonSql).bind(facts.value1, facts.value2, facts.value3).first<Record<string, unknown>>(),
        read28: (facts: {
            value1: string;
            value2: string[];
            value3: number;
        }) => db.prepare(activeRecipientsSql(facts.value2, "\n        ")).bind(facts.value1, ...facts.value2, facts.value3, facts.value3).all<{
            user_id: string;
        }>() };
}
