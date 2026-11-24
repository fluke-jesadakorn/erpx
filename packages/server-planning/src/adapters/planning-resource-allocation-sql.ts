import "server-only";
/** Owner-local parameterized production resource-allocation point-lookup statement shared
 * by the Planning capacity minutes reader and the no-active-allocation guard. Only fixed
 * server-owned projection and predicate fragments vary; no bind value or caller SQL is
 * interpolated. Each emitted statement is byte-identical to the former inline literal it
 * replaces. */
export const planningResourceAllocationRows = (projection: string, predicate: string) =>
  `SELECT ${projection} FROM planning_resource_allocations WHERE ${predicate}`;
