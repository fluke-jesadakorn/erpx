import 'server-only';
import { hasPolicyReadSnapshot, policyRead } from "@/server/modules/identity_access/public/iam-read-snapshot";
import type { SqlDatabase } from "@/server/modules/runtime/public/runtime/types";
import { createPersonIdentityIds, createCurrentPersonPrincipals } from "../application/authority";
export async function personIdentityIds(db: SqlDatabase, companyId: string, ids: readonly string[]) {
    return createPersonIdentityIds({ aliases: async (companyId, ids) => (await db
            .prepare(`SELECT DISTINCT linked.user_id FROM iam_members original JOIN iam_members linked ON linked.company_id=original.company_id AND linked.person_id=original.person_id WHERE original.company_id=? AND original.user_id IN (${ids.map(() => "?").join(",")})`)
            .bind(companyId, ...ids)
            .all<{
            user_id: string;
        }>()).results })(companyId, ids);
}
export async function currentPersonPrincipals(db: SqlDatabase, companyId: string, ids: readonly string[]) {
    return createCurrentPersonPrincipals({ successors: async (companyId, ids) => {
            const select = `SELECT original.user_id original_id,linked.user_id current_id FROM iam_members original JOIN iam_members linked ON linked.company_id=original.company_id AND linked.person_id=original.person_id AND linked.status='active' WHERE original.company_id=? AND original.status='superseded'`;
            return hasPolicyReadSnapshot()
                ? await policyRead(db, JSON.stringify(["current-principals", companyId]), async () => (await db.prepare(select).bind(companyId).all<{
                    original_id: string;
                    current_id: string;
                }>()).results)
                : (await db.prepare(`${select} AND original.user_id IN (${ids.map(() => "?").join(",")})`)
                    .bind(companyId, ...ids).all<{
                    original_id: string;
                    current_id: string;
                }>()).results;
        } })(companyId, ids);
}
