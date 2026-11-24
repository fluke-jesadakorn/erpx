import "server-only";
import { PMS_PROPERTIES_PROJECTION_SQL, PMS_RECORDS_PROJECTION_SQL } from './pms-read-tail-r20-sql';
import { PMS_POSTINGS_PROJECTION_SQL } from './pms-read-tail-r29-sql';
/** Closed native projections for commercial owner joins. Callers retain their original scope/authority filters and execute the composed statement in their supplied transaction. No SQL or relation name is accepted from a caller. */
export const commercialOwnerProjectionSources = {
  pms_postings: PMS_POSTINGS_PROJECTION_SQL,
  pms_properties: PMS_PROPERTIES_PROJECTION_SQL,
  pms_records: PMS_RECORDS_PROJECTION_SQL,
} as const;
