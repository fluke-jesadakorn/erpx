import "server-only";
import {planningSelectColumnsFrom} from "./planning-select-columns-sql";
/** Owner-local parameterized row-listing SQL shared by the Planning command-source
 * read tail. r25 keeps this name as a thin wrapper; the generalized single-line
 * projection/FROM builder owns the single read literal and emits the identical
 * scoped statement. Every argument is a fixed server-owned identifier, predicate
 * or ordering fragment; no bind value or caller SQL is interpolated. */
export const planningScopedBranchList = (columns: string, from: string, scope: string, branch: string, order: string) =>
  planningSelectColumnsFrom(columns, `${from} WHERE ${scope} AND (?::text IS NULL OR ${branch}) ORDER BY ${order}`);
