import { finalRootOwnerProjectionSources as identity_accessProjectionSources } from "@erpx/server-identity_access/public/final-root-owner-projection-sources";

/** Byte-identical parameterized active-assignment EXISTS head shared by the
 * Support eligibility-flag and assignment-access read witnesses. Only the row
 * alias and the trailing WHERE body vary; this renderer owns the single shared
 * read token and emits text only. Binds, locks, native read kinds and execution
 * stay at the call sites. */
export const readTailR45AssignmentExistsSql = (alias: string, tail: string) =>
    `EXISTS(SELECT 1 FROM (${identity_accessProjectionSources.assignments}) ${alias} WHERE ${tail})`;
