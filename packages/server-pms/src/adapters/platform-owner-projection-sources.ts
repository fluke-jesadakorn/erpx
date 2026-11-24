import "server-only";
import { PMS_PROPERTIES_PROJECTION_SQL, PMS_RECORDS_PROJECTION_SQL } from './pms-read-tail-r20-sql';
/** Closed owner projections for Platform, Organization and Procurement joins. Original filters, bindings and supplied transactions remain with the caller. */
export const platformOwnerProjectionSources = Object.freeze({
  pms_properties: PMS_PROPERTIES_PROJECTION_SQL,
  pms_records: PMS_RECORDS_PROJECTION_SQL,
} as const);
