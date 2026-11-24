import "server-only";
import {planningPackageFactRows} from "./planning-package-fact-sql";
/** Owner-local production resource-allocation point-lookup statement shared
 * by the Planning capacity minutes reader and the no-active-allocation guard.
 * r22 keeps this name as a thin wrapper; the generalized package fact builder
 * owns the single read literal and emits the identical statement. Only fixed
 * server-owned projection and predicate fragments vary; no bind value or caller
 * SQL is interpolated. */
export const planningResourceAllocationRows = (projection: string, predicate: string) =>
  planningPackageFactRows(projection, 'planning_resource_allocations', predicate);
