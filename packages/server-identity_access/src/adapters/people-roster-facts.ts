import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
export function rosterIdentityFacts(database: SqlDatabase) {
    return { readRescheduleRosterShift(fact0: string, fact1: string, fact2: "roster.manage") { return database.prepare("SELECT 1 WHERE iam_role_allows(?,?,?)").bind(fact0, fact1, fact2).first<Record<string, unknown>>(); },
        readRosterCommand(fact0: string, fact1: string, fact2: string) { return database.prepare("SELECT 1 WHERE iam_role_allows(?,?,?)").bind(fact0, fact1, fact2).first<Record<string, unknown>>(); } };
}
