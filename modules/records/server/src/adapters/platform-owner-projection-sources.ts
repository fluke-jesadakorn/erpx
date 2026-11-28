import "server-only";
/** Native-owned fixed SQL for the Records employee-request projection.
 * Exact bytes live in the Rust-owned contract `owner-projection-head-sources.json`
 * (`include_str!` in adapters/owner_projection_head.rs); consumers keep their own
 * predicates, aliases and binds and no statement token remains in TypeScript. */
import contract from "./owner-projection-head-sources.json" with { type: "json" };
const heads = contract as { readonly employeeRequests: string };
export const employeeRequestsProjectionSql = heads.employeeRequests;
/** Closed owner projections for Platform, Organization and Procurement joins. Original filters, bindings and supplied transactions remain with the caller. */
export const platformOwnerProjectionSources = Object.freeze({
  employee_requests: employeeRequestsProjectionSql,
} as const);
