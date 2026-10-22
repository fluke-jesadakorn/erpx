import "server-only";
/** Closed native projections for Legal, Asset, Maintenance and Customer joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const finalRootOwnerProjectionSources = {
  staff_project_members: "SELECT \"project_id\",\"user_id\",\"added_by\",\"created_at\" FROM staff_project_members",
  staff_projects: "SELECT \"id\",\"company_id\",\"branch_id\",\"department_id\",\"title\",\"description\",\"owner_user_id\",\"version\",\"created_at\",\"updated_at\",\"lifecycle_status\" FROM staff_projects",
} as const;
