import "server-only";
import {selectProjectionFrom} from './read-tail-r40';
/** Byte-identical closed projection shape shared by both root-owner join sources.
 * The universal projection head (read-tail-r40) now owns the single projection
 * literal; these two closed sources delegate to it and stay byte-identical to
 * their former inline literals. No SQL or relation name comes from a caller. */
const quotedProjection = (columns: string, relation: string) => selectProjectionFrom(columns, relation);
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  staff_project_members: quotedProjection('"project_id","user_id","added_by","created_at"', 'staff_project_members'),
  staff_projects: quotedProjection('"id","company_id","branch_id","department_id","title","description","owner_user_id","version","created_at","updated_at","lifecycle_status"', 'staff_projects'),
} as const;
