import "server-only";
import type { SqlDatabase } from "@erpx/server-runtime/public/runtime/types";
import { readLegacyPeopleTransition, type Lane7Actor } from "./native-lane-7/client";

export async function lockLegacyPeopleAccess(db: SqlDatabase) {
  await db.prepare(`LOCK TABLE hr_people,hr_engagements,people_access_cutoffs,people_lifecycle_cases,people_lifecycle_grants,
    people_package_revisions,people_applied_packages,people_provision_operations,people_provision_grants IN SHARE MODE`).run();
}

/**
 * The clocks consumed by IAM's existing lifecycle/provisioning predicates are
 * now owned by the native lane-7 axum reader. The owner executes the SQL, keeps
 * the Company predicate on every UNION arm and projects the deadline; this
 * function is transport only and fails closed when the native service is
 * unavailable. There is no TypeScript SQL fallback.
 */
export async function nextLegacyPeopleTransition(_db: SqlDatabase, companyId: string, observedAt: number, actor?: Lane7Actor) {
  return readLegacyPeopleTransition(companyId, observedAt, actor);
}
