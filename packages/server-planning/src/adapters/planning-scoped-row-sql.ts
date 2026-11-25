import "server-only";
import {planningSelectColumnsFrom} from "./planning-select-columns-sql";
/** Owner-local parameterized single-row SQL shared by the Planning command-source
 * read tail. r24 keeps this name as a thin wrapper; the generalized single-line
 * projection/FROM builder owns the single read literal and emits the identical
 * statement. Every argument is a fixed server-owned identifier or fragment; no
 * bind value or caller SQL is interpolated. */
export const planningScopedBranchRow = (columns: string, from: string, id: string, company: string, branch: string) =>
  planningSelectColumnsFrom(columns, `${from} WHERE ${id}=? AND ${company}=? AND (?::text IS NULL OR ${branch}=?)`);
