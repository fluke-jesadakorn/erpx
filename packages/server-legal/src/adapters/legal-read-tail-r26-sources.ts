import "server-only";
/** Byte-identical Legal version-assertion read fragment shared by owner adapters (round 26).
 * Text only: every call site still prepares and executes its own assertion with unchanged binds. */
/** Optimistic-concurrency version witness; the caller supplies the fixed legal table. */
export const legalVersionExistsSql = (table: string) =>
  `EXISTS(SELECT 1 FROM ${table} WHERE id=? AND version=?)`;
