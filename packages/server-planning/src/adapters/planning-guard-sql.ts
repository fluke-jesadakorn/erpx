import "server-only";
/** Owner-local command guard fragments shared by the Planning read tail.
 * Every argument is a fixed server-owned SQL predicate or operation literal;
 * no bind value or caller SQL is interpolated. Each emitted guard is
 * byte-identical to the former inline statement it replaces. */
export const productionResourcesExists = (predicate: string) =>
  `EXISTS(SELECT 1 FROM production_resources WHERE id=? AND company_id=? AND ${predicate})`;
export const planningAllocatedMinutes =
  "COALESCE((SELECT sum(minutes) FROM planning_resource_allocations WHERE resource_id=? AND work_date=? AND released_at IS NULL),0)";
