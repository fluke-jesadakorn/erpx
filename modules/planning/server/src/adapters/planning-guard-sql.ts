import "server-only";
import {planningCommandExistsGuard} from "./planning-command-guard-sql";
import {planningPackageFactRows} from "./planning-package-fact-sql";
/** Owner-local command guard fragments shared by the Planning read tail.
 * Every argument is a fixed server-owned SQL predicate or operation literal;
 * no bind value or caller SQL is interpolated. Each emitted guard is
 * byte-identical to the former inline statement it replaces. */
/** r20 production-resources guard kept as a thin wrapper; the generalized
 * EXISTS builder owns the single read literal and emits the identical guard. */
export const productionResourcesExists = (predicate: string) =>
  planningCommandExistsGuard("production_resources", `id=? AND company_id=? AND ${predicate}`);
export const planningAllocatedMinutes =
  `COALESCE((${planningPackageFactRows('sum(minutes)', 'planning_resource_allocations', 'resource_id=? AND work_date=? AND released_at IS NULL')}),0)`;
