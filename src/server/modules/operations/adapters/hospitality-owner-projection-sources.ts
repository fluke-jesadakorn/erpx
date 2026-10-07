import { finalRootOwnerProjectionSources as organizationProjectionSources } from "../../organization/public/final-root-owner-projection-sources";
import "server-only";
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  staff_levels: `SELECT "id","company_id","code","name","rank","active","version","created_at","updated_at","level_number" FROM (${organizationProjectionSources.staff_levels}) staff_levels`,
} as const;
