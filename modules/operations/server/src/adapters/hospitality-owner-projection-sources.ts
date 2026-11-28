import { finalRootOwnerProjectionSources as organizationProjectionSources } from "@erpx/organization-server/public/final-root-owner-projection-sources";
import "server-only";
import { OPERATIONS_OWNER_PROJECTION_ID_HEAD } from './read-tail-r57-sources';
/** Closed owner projections for hospitality, Files and Runtime native joins. Callers retain original guards, filters and binds in one supplied statement/transaction. No SQL or table input is accepted. */
export const hospitalityOwnerProjectionSources = {
  staff_levels: `${OPERATIONS_OWNER_PROJECTION_ID_HEAD}"code","name","rank","active","version","created_at","updated_at","level_number" FROM (${organizationProjectionSources.staff_levels}) staff_levels`,
} as const;
