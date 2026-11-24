import "server-only";
import {assetProjectionFromSql} from "./asset-scope-tuple";
/** Canonical owner-local fixed all-rows projection for Assets. The non-enforced
 * metric source and the package-settlement Finance-link source share this owner
 * head: the all-rows head renders byte-identically from the generic
 * `assetProjectionFromSql` projection-from head (r28), so the TS read tail
 * carries no duplicate statement. Callers pass only fixed owner-owned relation
 * names; no SQL or relation name is accepted from a request. */
export const assetAllRowsSql = (table: string) => assetProjectionFromSql("*", ` ${table}`);
