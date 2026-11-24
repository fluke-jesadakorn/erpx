import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { roleAllowsSql } from "./iam-probe-sql";
/** The roster role-authorization read uses the owner's single fixed role probe.
 * Both facts ports share that literal with the calendar/collaboration/legal
 * probes so the authorization shape cannot drift across the owner. */
export function rosterIdentityFacts(database: SqlDatabase) {
    return { readRescheduleRosterShift(fact0: string, fact1: string, fact2: "roster.manage") { return database.prepare(roleAllowsSql).bind(fact0, fact1, fact2).first<Record<string, unknown>>(); },
        readRosterCommand(fact0: string, fact1: string, fact2: string) { return database.prepare(roleAllowsSql).bind(fact0, fact1, fact2).first<Record<string, unknown>>(); } };
}
