import "server-only";
import type { SqlDatabase } from "../../runtime/public/runtime/types";
import { pmsSourcesSql } from "../../pms/public/pms/source-query";
import { pmsSourceReadableSql } from "./pms-source-authorization";
export function pmsSocialRecipients(db: SqlDatabase, id: string, companyId: string, actorId: string, review: string | null, recipient: string | null) {
    return db.prepare(`SELECT DISTINCT a.user_id FROM assignments a
      JOIN (${pmsSourcesSql}) pr ON pr.id=? AND pr.company_id=a.company_id
      WHERE a.company_id=? AND ${pmsSourceReadableSql("pr", "a.user_id", "a.id")}
      AND NOT iam_same_person(a.user_id,?,a.company_id)
      AND ((?::text IS NOT NULL AND iam_role_allows(a.role,a.company_id,?)) OR (?::text IS NULL AND iam_same_person(a.user_id,?,a.company_id)))`).bind(id, companyId, actorId, review, review, review, recipient).all<{
        user_id: string;
    }>().then(rows => rows.results);
}
