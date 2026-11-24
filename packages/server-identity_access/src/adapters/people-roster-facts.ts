import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
/** One owner-local fixed projection for the People roster role-authorization read.
 * The two facts ports previously repeated the same SQL literal; one projection
 * now serves both call sites so they cannot drift apart. The native identity
 * role authority owns the same authorization shape. */
const rosterRoleAllowsSql = "SELECT 1 WHERE iam_role_allows(?,?,?)";
export function rosterIdentityFacts(database: SqlDatabase) {
    return { readRescheduleRosterShift(fact0: string, fact1: string, fact2: "roster.manage") { return database.prepare(rosterRoleAllowsSql).bind(fact0, fact1, fact2).first<Record<string, unknown>>(); },
        readRosterCommand(fact0: string, fact1: string, fact2: string) { return database.prepare(rosterRoleAllowsSql).bind(fact0, fact1, fact2).first<Record<string, unknown>>(); } };
}
