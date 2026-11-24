import "server-only";
import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";

/** Shared parameterized eligibility fragments for the Support read tail.
 * One owner-local rendering keeps each call site byte-identical; the native
 * readers remain the authority. */
export const supportEligibleFlagSql = (predicate: string, alias: string) =>
    `CASE WHEN EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) m WHERE ${predicate} AND m.user_id=?) THEN 1 ELSE 0 END ${alias}`;

export const supportReviewerCountSql = (predicate: string) =>
    `(SELECT COUNT(DISTINCT m.user_id) FROM (${identity_accessProjectionSources.assignments}) m WHERE ${predicate})`;
