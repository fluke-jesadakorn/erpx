import "server-only";
import {heads, renderHead} from "./read-head-sources";
/** Canonical owner-local alias-qualified row head shared by the Work/Plan
 * source-image projection and the scoped source CTE in the package-obligations
 * facts read. Both compose byte-identical heads; callers pass only an owner-fixed
 * alias and a closed tail fragment, never request-authored SQL. Each composed
 * string stays byte-identical to its pre-dedupe literal and no lock or write
 * statement is accepted or changed. */
export const aliasRowsSql = (alias: string, tail: string): string => renderHead(heads.aliasRows, {alias, tail});
